use super::*;
use crate::leases::cursor::QueryBinding;
use std::{thread, time::Instant};
fn grow(db: &Database) {
    db.write(|conn|{let tx=conn.transaction()?;tx.execute_batch("CREATE TABLE wal_probe(value BLOB); INSERT INTO wal_probe VALUES(zeroblob(100000)); UPDATE app_state SET data_revision=1;")?;tx.commit()?;Ok(())}).unwrap();
}
fn wal_size(db: &Database) -> u64 {
    let mut name = db.path().as_os_str().to_owned();
    name.push("-wal");
    std::fs::metadata(Path::new(&name))
        .map(|m| m.len())
        .unwrap_or(0)
}
#[test]
fn a_pinned_reader_defers_without_waiting_and_release_allows_verified_truncation() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.snapshot(|tx, revision| {
        assert_eq!(revision.data, 0);
        grow(&db);
        let before = wal_size(&db);
        assert!(before > 100000);
        let started = Instant::now();
        let result = db.checkpoint_wal()?;
        assert_eq!(result.state, CheckpointState::Deferred);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(wal_size(&db), before);
        assert_eq!(
            tx.query_row("SELECT data_revision FROM app_state", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        Ok(())
    })
    .unwrap();
    let result = db.checkpoint_wal().unwrap();
    assert_eq!(result.state, CheckpointState::Complete);
    assert_eq!(result.log_frames, Some(0));
    assert_eq!(result.checkpointed_frames, Some(0));
    assert_eq!(wal_size(&db), 0);
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 1);
        assert_eq!(
            tx.query_row("SELECT length(value) FROM wal_probe", [], |r| r
                .get::<_, i64>(0))?,
            100000
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn background_checkpoint_recovers_new_leases_after_actual_wal_pressure() {
    let dir = tempfile::tempdir().unwrap();
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
    let binding = QueryBinding::new("main", &"synthetic").unwrap();
    let old = db.leases().open(&binding).unwrap();
    grow(&db);
    let deadline = Instant::now() + Duration::from_secs(3);
    while wal_size(&db) > 32 * 1024 {
        assert!(Instant::now() < deadline, "WAL pressure did not recover");
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        db.leases()
            .read(&old, &binding, |_, r| Ok(r.data))
            .unwrap_err()
            .code,
        ErrorCode::SnapshotExpired
    );
    let fresh = db.leases().open(&binding).unwrap();
    assert_eq!(
        db.leases()
            .read(&fresh, &binding, |tx, r| {
                assert_eq!(r.data, 1);
                Ok(
                    tx.query_row("SELECT length(value) FROM wal_probe", [], |r| {
                        r.get::<_, i64>(0)
                    })?,
                )
            })
            .unwrap(),
        100000
    );
}
