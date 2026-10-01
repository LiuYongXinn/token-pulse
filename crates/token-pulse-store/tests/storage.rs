use token_pulse_store::{Database, ErrorCode, SourceRecord, rusqlite::Connection};

fn source(id: &str) -> SourceRecord {
    SourceRecord {
        source_id: id.into(),
        root_path: "synthetic-read-only-source".into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    }
}

#[test]
fn migration_reopens_and_readers_cannot_write() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.add_source(source("one")).unwrap();
    db.snapshot(|tx, rev| {
        assert_eq!(rev.settings, 1);
        assert!(tx.execute("DELETE FROM sources", []).is_err());
        let tables: i64 = tx.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(tables, 31);
        let fk: i64 = tx.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
        assert_eq!(fk, 1);
        Ok(())
    })
    .unwrap();
    drop(db);
    let reopened = Database::open(dir.path()).unwrap();
    reopened
        .snapshot(|tx, _| {
            assert_eq!(
                tx.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))?,
                1
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn real_read_snapshot_survives_a_concurrent_commit() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.add_source(source("before")).unwrap();
    db.snapshot(|tx, revision| {
        assert_eq!(revision.settings, 1);
        let writer = db.clone();
        std::thread::spawn(move || writer.add_source(source("after")).unwrap())
            .join()
            .unwrap();
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            tx.query_row("SELECT settings_revision FROM app_state", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, revision| {
        assert_eq!(revision.settings, 2);
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))?,
            2
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn exact_sql_aggregate_preserves_unknown_and_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(tx.query_row("SELECT sum_token_decimal(v) FROM (SELECT 9223372036854775807 AS v UNION ALL SELECT 9223372036854775807)", [], |r|r.get::<_,String>(0))?, "18446744073709551614");
        assert_eq!(tx.query_row("SELECT sum_token_decimal(NULL)", [], |r|r.get::<_,Option<String>>(0))?, None);
        assert_eq!(tx.query_row("SELECT sum_money_atoms(v) FROM (SELECT '9007199254740993' AS v UNION ALL SELECT '1')", [], |r|r.get::<_,String>(0))?, "9007199254740994");
        assert!(tx.query_row("SELECT sum_money_atoms(v) FROM (SELECT '170141183460469231731687303715884105727' AS v UNION ALL SELECT '1')", [], |r|r.get::<_,String>(0)).is_err());
        Ok(())
    }).unwrap();
}
#[test]
fn unknown_newer_schema_and_corruption_do_not_create_fresh_history() {
    let dir = tempfile::tempdir().unwrap();
    let connection = Connection::open(dir.path().join("token-pulse.db")).unwrap();
    connection.execute_batch("PRAGMA user_version=99").unwrap();
    drop(connection);
    assert_eq!(
        Database::open(dir.path()).err().unwrap().code,
        ErrorCode::MigrationFailed
    );
    let corrupt = tempfile::tempdir().unwrap();
    let path = corrupt.path().join("token-pulse.db");
    std::fs::write(&path, b"corrupt database preserved").unwrap();
    assert_eq!(
        Database::open(corrupt.path()).err().unwrap().code,
        ErrorCode::DbCorrupt
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt database preserved");
}
