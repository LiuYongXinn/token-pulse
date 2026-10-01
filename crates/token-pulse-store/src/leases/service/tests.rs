use super::*;
use crate::Database;
use serde_json::json;
fn binding(owner: &str) -> QueryBinding {
    QueryBinding::new(owner, &json!({"filter":"synthetic","sort":"recent"})).unwrap()
}
fn read(db: &Database, handle: &LeaseHandle, binding: &QueryBinding) -> StoreResult<i64> {
    db.leases().read(handle, binding, |tx, _| {
        Ok(tx.query_row("SELECT data_revision FROM app_state", [], |r| r.get(0))?)
    })
}
fn wait_expired(db: &Database, handle: &LeaseHandle, binding: &QueryBinding) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        // Inspect actor reservation; reading would reset idle expiry.
        if db.leases().worker(handle, binding).is_err() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "lease did not release its transaction"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_real_lease_keeps_rows_schema_and_revisions_after_concurrent_writer_commit() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.write(|conn| { conn.execute_batch("CREATE TABLE lease_probe(id INTEGER PRIMARY KEY,value TEXT); INSERT INTO lease_probe VALUES(1,'old'); UPDATE app_state SET data_revision=10,price_revision=20,settings_revision=30;")?; Ok(()) }).unwrap();
    let b = binding("main");
    let handle = db.leases().open(&b).unwrap();
    assert_eq!(
        handle.revision,
        Revision {
            data: 10,
            price: 20,
            settings: 30
        }
    );
    let first = db
        .leases()
        .read(&handle, &b, |tx, r| {
            Ok((
                r,
                tx.query_row("SELECT value FROM lease_probe WHERE id=1", [], |row| {
                    row.get::<_, String>(0)
                })?,
            ))
        })
        .unwrap();
    let writer = db.clone();
    thread::spawn(move || writer.write(|conn| { let tx=conn.transaction()?;tx.execute_batch("UPDATE lease_probe SET value='new'; INSERT INTO lease_probe VALUES(2,'later'); UPDATE app_state SET data_revision=11,price_revision=21,settings_revision=31;")?;tx.commit()?;Ok(()) })).join().unwrap().unwrap();
    let again = db
        .leases()
        .read(&handle, &b, |tx, r| {
            Ok((
                r,
                tx.query_row("SELECT value FROM lease_probe WHERE id=1", [], |row| {
                    row.get::<_, String>(0)
                })?,
            ))
        })
        .unwrap();
    assert_eq!(first, again);
    assert_eq!(first.1, "old");
    assert_eq!(read(&db, &handle, &b).unwrap(), 10);
    assert_eq!(
        db.leases()
            .read(&handle, &b, |tx, _| Ok(tx.query_row(
                "SELECT COUNT(*) FROM lease_probe",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        1
    );
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 11);
        assert_eq!(r.price, 21);
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM lease_probe", [], |r| r
                .get::<_, i64>(0))?,
            2
        );
        Ok(())
    })
    .unwrap();
    db.leases().release(&handle, &b).unwrap();
    assert_eq!(
        read(&db, &handle, &b).unwrap_err().code,
        ErrorCode::SnapshotExpired
    );
    let current = db.leases().open(&b).unwrap();
    assert_eq!(read(&db, &current, &b).unwrap(), 11);
}

#[test]
fn two_dedicated_slots_leave_normal_readers_available_and_reject_cross_window_use() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let b = binding("main");
    let a = db.leases().open(&b).unwrap();
    let c = db.leases().open(&b).unwrap();
    assert_eq!(
        db.leases().open(&b).unwrap_err().code,
        ErrorCode::SnapshotExpired
    );
    assert_ne!(a.snapshot_id, c.snapshot_id);
    db.snapshot(|_, revision| {
        assert_eq!(revision.data, 0);
        Ok(())
    })
    .unwrap();
    assert_eq!(
        read(&db, &a, &binding("mini")).unwrap_err().code,
        ErrorCode::CursorInvalid
    );
    assert_eq!(
        db.leases().release(&a, &binding("mini")).unwrap_err().code,
        ErrorCode::CursorInvalid
    );
    assert_eq!(read(&db, &a, &b).unwrap(), 0);
    let forged = LeaseHandle {
        snapshot_id: [0; 16],
        revision: a.revision,
    };
    assert_eq!(
        read(&db, &forged, &b).unwrap_err().code,
        ErrorCode::SnapshotExpired
    );
    assert!(
        db.leases()
            .read(&a, &b, |tx, _| {
                tx.execute("UPDATE app_state SET data_revision=99", [])?;
                Ok(())
            })
            .is_err()
    );
    assert_eq!(read(&db, &a, &b).unwrap(), 0);
    db.leases().release(&a, &b).unwrap();
    let replacement = db.leases().open(&b).unwrap();
    assert_ne!(replacement.snapshot_id, a.snapshot_id);
    assert_eq!(read(&db, &c, &b).unwrap(), 0);
}

#[test]
fn idle_reaping_and_absolute_lifetime_close_real_transactions_without_a_client_request() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open_testing_leases(
        dir.path(),
        Duration::from_millis(280),
        Duration::from_millis(100),
        64 * 1024 * 1024,
    )
    .unwrap();
    let b = binding("main");
    let idle = db.leases().open(&b).unwrap();
    wait_expired(&db, &idle, &b);
    assert_eq!(
        read(&db, &idle, &b).unwrap_err().code,
        ErrorCode::SnapshotExpired
    );
    let active = db.leases().open(&b).unwrap();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(220) {
        assert_eq!(read(&db, &active, &b).unwrap(), 0);
        thread::sleep(Duration::from_millis(35));
    }
    // These successful reads cannot extend the original 280 ms deadline.
    wait_expired(&db, &active, &b);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        read(&db, &active, &b).unwrap_err().code,
        ErrorCode::SnapshotExpired
    );
    let after = db.leases().open(&b).unwrap();
    assert_eq!(read(&db, &after, &b).unwrap(), 0);
}

#[test]
fn wal_pressure_expires_the_lease_and_preserves_writer_results() {
    let dir = tempfile::tempdir().unwrap();
    // Finish fixture migrations and close the sole connection before opening
    // application readers, so this pressure test begins with an empty WAL.
    let path = dir.path().join("token-pulse.db");
    let mut bootstrap = Connection::open(&path).unwrap();
    crate::database::configure(&bootstrap).unwrap();
    crate::migration::migrate(&mut bootstrap, &path).unwrap();
    bootstrap.close().unwrap();
    let db = Database::open_testing_leases(
        dir.path(),
        Duration::from_secs(30),
        Duration::from_secs(10),
        32 * 1024,
    )
    .unwrap();
    let b = binding("main");
    let handle = db.leases().open(&b).unwrap();
    assert_eq!(read(&db, &handle, &b).unwrap(), 0);
    db.write(|conn| {conn.execute_batch("CREATE TABLE large_probe(value BLOB); INSERT INTO large_probe VALUES(zeroblob(100000)); UPDATE app_state SET data_revision=1;")?;Ok(())}).unwrap();
    wait_expired(&db, &handle, &b);
    assert_eq!(
        read(&db, &handle, &b).unwrap_err().code,
        ErrorCode::SnapshotExpired
    );
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 1);
        assert_eq!(
            tx.query_row("SELECT length(value) FROM large_probe", [], |r| r
                .get::<_, i64>(0))?,
            100000
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn in_flight_cancel_interrupts_sql_and_slot_reuse_does_not_inherit_an_open_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let b = binding("main");
    let handle = db.leases().open(&b).unwrap();
    let (started, receive) = mpsc::sync_channel(1);
    let reader = db.clone();
    let reader_binding = b.clone();
    let running = thread::spawn(move || {
        reader.leases().read(&handle,&reader_binding,move |tx,_| {
        started.send(()).unwrap();
        Ok(tx.query_row("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT SUM(x) FROM n",[],|r|r.get::<_,i64>(0))?)
    })
    });
    receive.recv_timeout(Duration::from_secs(2)).unwrap();
    let now = Instant::now();
    db.leases().release(&handle, &b).unwrap();
    assert_eq!(
        running.join().unwrap().unwrap_err().code,
        ErrorCode::SnapshotExpired
    );
    assert!(now.elapsed() < Duration::from_secs(2));
    let next = db.leases().open(&b).unwrap();
    assert_eq!(read(&db, &next, &b).unwrap(), 0);
    db.leases().release(&next, &b).unwrap();
    // Drop while the other actor has an active transaction, too.
    let _another = db.leases().open(&b).unwrap();
    let now = Instant::now();
    drop(db);
    assert!(now.elapsed() < Duration::from_secs(2));
}

#[test]
fn an_active_sql_query_cannot_outlive_the_absolute_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open_testing_leases(
        dir.path(),
        Duration::from_millis(60),
        Duration::from_secs(10),
        64 * 1024 * 1024,
    )
    .unwrap();
    let b = binding("main");
    let handle = db.leases().open(&b).unwrap();
    let began = Instant::now();
    let error=db.leases().read(&handle,&b,|tx,_|Ok(tx.query_row("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT SUM(x) FROM n",[],|r|r.get::<_,i64>(0))?)).unwrap_err();
    assert_eq!(error.code, ErrorCode::SnapshotExpired);
    assert!(began.elapsed() < Duration::from_secs(2));
    wait_expired(&db, &handle, &b);
    let next = db.leases().open(&b).unwrap();
    assert_eq!(read(&db, &next, &b).unwrap(), 0);
}
