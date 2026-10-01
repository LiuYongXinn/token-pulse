use std::{
    fs,
    io::Write,
    time::{Duration, Instant},
};
use token_pulse_collector::service::{CollectorOptions, CollectorService};
use token_pulse_store::{Database, SourceRecord};

fn call() -> &'static [u8] {
    b"{\"timestamp\":\"1970-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"total_tokens\":1}}}}\n"
}
fn setup() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    Database,
    std::path::PathBuf,
) {
    let logs = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    fs::create_dir(logs.path().join("sessions")).unwrap();
    fs::create_dir(logs.path().join("archived_sessions")).unwrap();
    let path = logs.path().join("sessions/a.jsonl");
    let mut bytes =
        b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"background-synthetic\"}}\n".to_vec();
    bytes.extend(call());
    fs::write(&path, bytes).unwrap();
    let db = Database::open(data.path()).unwrap();
    db.add_source(SourceRecord {
        source_id: "source".into(),
        root_path: logs.path().to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    (logs, data, db, path)
}
fn total(db: &Database) -> i64 {
    db.snapshot(|tx, _| {
        Ok(tx.query_row(
            "SELECT COALESCE(SUM(total_tokens),0) FROM active_usage_events",
            [],
            |r| r.get(0),
        )?)
    })
    .unwrap()
}
fn wait_for(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !condition() {
        assert!(Instant::now() < deadline, "background condition timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn startup_periodic_reconciliation_and_resume_collect_without_watcher() {
    let (_logs, _data, db, path) = setup();
    let collector = CollectorService::start(
        db.clone(),
        CollectorOptions {
            watcher: false,
            active_poll: Duration::from_millis(50),
            manifest_poll: Duration::from_millis(100),
        },
    )
    .unwrap();
    wait_for(|| total(&db) == 1);
    collector.suspend();
    wait_for(|| collector.status().suspended);
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(call())
        .unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(total(&db), 1);
    collector.resume();
    wait_for(|| total(&db) == 2);
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(call())
        .unwrap();
    wait_for(|| total(&db) == 3);
    let before = Instant::now();
    drop(collector);
    assert!(before.elapsed() < Duration::from_secs(3));
    let restarted = CollectorService::start(
        db.clone(),
        CollectorOptions {
            watcher: false,
            active_poll: Duration::from_millis(50),
            manifest_poll: Duration::from_millis(100),
        },
    )
    .unwrap();
    wait_for(|| restarted.status().commits > 0);
    assert_eq!(total(&db), 3);
}

#[test]
fn actual_native_filesystem_watcher_collects_when_polling_is_far_in_future() {
    let (_logs, _data, db, path) = setup();
    let collector = CollectorService::start(
        db.clone(),
        CollectorOptions {
            watcher: true,
            active_poll: Duration::from_secs(3600),
            manifest_poll: Duration::from_secs(3600),
        },
    )
    .unwrap();
    wait_for(|| total(&db) == 1);
    db.snapshot(|tx, _| {
        let json: String = tx.query_row(
            "SELECT capabilities_json FROM sources WHERE source_id='source'",
            [],
            |r| r.get(0),
        )?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        assert_eq!(value["watcher"], "available");
        Ok(())
    })
    .unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(call())
        .unwrap();
    wait_for(|| total(&db) == 2);
    assert!(collector.status().last_commit_at_ms.is_some());
}
