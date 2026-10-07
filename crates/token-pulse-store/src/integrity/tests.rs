use super::*;
use crate::{
    Database,
    batch::tests::{fixture, setup},
    leases::cursor::QueryBinding,
    query::tests::filter,
};
use std::time::{Duration, Instant};

fn wait_failed(db: &Database) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while db.integrity_error().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(db.integrity_error(), Some(ErrorCode::DbCorrupt));
}

#[test]
fn critical_fact_corruption_is_rejected_before_desktop_open_returns() {
    let (directory, db) = setup();
    db.commit(fixture()).unwrap();
    let path = db.path().to_owned();
    drop(db);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "PRAGMA ignore_check_constraints=ON; UPDATE usage_events SET total_tokens=-1;",
        )
        .unwrap();
    drop(connection);
    assert!(
        matches!(Database::open_desktop(directory.path()), Err(e) if e.code==ErrorCode::DbCorrupt)
    );
}

#[test]
fn deferred_detail_corruption_blocks_statistics_writes_and_pagination() {
    let (directory, db) = setup();
    db.commit(fixture()).unwrap();
    let path = db.path().to_owned();
    drop(db);
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA ignore_check_constraints=ON; INSERT INTO valuation_sets VALUES('bad',0,'event_time',NULL,'ready',0); INSERT INTO event_valuations SELECT 'bad',event_id,NULL,NULL,NULL,'invalid' FROM usage_events LIMIT 1;").unwrap();
    connection
        .execute_batch("PRAGMA ignore_check_constraints=OFF")
        .unwrap();
    assert_ne!(
        connection
            .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(connection);
    let db = Database::open_desktop(directory.path()).unwrap();
    let read_only = Connection::open_with_flags(
        db.path(),
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    crate::database::configure(&read_only).unwrap();
    read_only.pragma_update(None, "query_only", true).unwrap();
    assert_ne!(
        read_only
            .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok",
        "read-only verification must see corruption"
    );
    wait_failed(&db);
    assert!(matches!(db.usage_totals(&filter()), Err(e) if e.code==ErrorCode::DbCorrupt));
    assert!(matches!(db.write(|_| Ok(())), Err(e) if e.code==ErrorCode::DbCorrupt));
    let binding = QueryBinding::new("main", &filter()).unwrap();
    assert!(matches!(db.leases().open(&binding), Err(e) if e.code==ErrorCode::DbCorrupt));
}

#[test]
fn checksum_mismatch_cannot_enter_deferred_validation() {
    let (directory, db) = setup();
    let path = db.path().to_owned();
    drop(db);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE schema_migrations SET checksum='wrong' WHERE version=16",
            [],
        )
        .unwrap();
    drop(connection);
    assert!(
        matches!(Database::open_desktop(directory.path()), Err(e) if e.code==ErrorCode::MigrationFailed)
    );
}

#[test]
fn valid_desktop_open_preserves_results_and_shutdown_cancels_verification() {
    let (directory, db) = setup();
    db.commit(fixture()).unwrap();
    let before = db.usage_totals(&filter()).unwrap();
    drop(db);
    let db = Database::open_desktop(directory.path()).unwrap();
    assert_eq!(
        serde_json::to_value(db.usage_totals(&filter()).unwrap()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    drop(db);
    let db = Database::open(directory.path()).unwrap();
    assert_eq!(db.integrity_error(), None);
}

#[test]
fn active_lease_cannot_publish_a_late_response_after_integrity_failure() {
    let (_directory, db) = setup();
    let health = Health::default();
    let mut leases = crate::leases::LeaseService::new(db.path()).unwrap();
    leases.set_health(health.clone());
    let binding = QueryBinding::new("main", &filter()).unwrap();
    let lease = leases.open(&binding).unwrap();
    let failing = health.clone();
    assert!(
        matches!(leases.read(&lease, &binding, move |_, _| { failing.0.store(true, Ordering::Release); Ok(7) }), Err(e) if e.code==ErrorCode::DbCorrupt)
    );
    assert!(
        matches!(leases.read(&lease, &binding, |_, _| Ok(7)), Err(e) if e.code==ErrorCode::DbCorrupt)
    );
    leases.release(&lease, &binding).unwrap();
}
