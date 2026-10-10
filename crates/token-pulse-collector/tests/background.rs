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
fn title_index_is_imported_and_renames_are_polled_without_log_writes() {
    let (logs, _data, db, path) = setup();
    let index = logs.path().join("session_index.jsonl");
    fs::write(&index, "{\"id\":\"background-synthetic\",\"thread_name\":\"原会话标题\",\"updated_at\":\"2026-10-07T12:00:00Z\"}\n").unwrap();
    let original = fs::read(&path).unwrap();
    let collector = CollectorService::start(
        db.clone(),
        CollectorOptions {
            watcher: false,
            active_poll: Duration::from_millis(50),
            manifest_poll: Duration::from_secs(10),
        },
    )
    .unwrap();
    let label = || {
        db.snapshot(|tx, _| Ok(tx.query_row("SELECT display_name FROM session_labels WHERE provider_session_id='background-synthetic'", [], |r| r.get::<_, String>(0))?)).ok()
    };
    wait_for(|| total(&db) == 1 && label().as_deref() == Some("原会话标题"));
    let before = db.usage_revision().unwrap();
    fs::write(&index, "{\"id\":\"background-synthetic\",\"thread_name\":\"修改后的会话标题\",\"updated_at\":\"2026-10-07T12:01:00Z\"}\n").unwrap();
    wait_for(|| label().as_deref() == Some("修改后的会话标题"));
    let after = db.usage_revision().unwrap();
    assert_eq!(before.data_revision, after.data_revision);
    assert_eq!(before.price_revision, after.price_revision);
    assert!(after.usage_view_revision.value() > before.usage_view_revision.value());
    assert_eq!(total(&db), 1);
    assert_eq!(fs::read(path).unwrap(), original);
    fs::remove_file(index).unwrap();
    collector.reconcile();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(label().as_deref(), Some("修改后的会话标题"));
    assert!(collector.status().error.is_none());
    collector.shutdown();
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

#[test]
fn malformed_title_index_reports_and_resolves_without_changing_usage() {
    use token_pulse_core::{diagnostics::DiagnosticsRequest, error::ErrorCode};
    let (logs, _data, db, _) = setup();
    let index = logs.path().join("session_index.jsonl");
    let entry = "{\"id\":\"background-synthetic\",\"thread_name\":\"valid title\",\"updated_at\":\"2026-10-07T12:00:00Z\"}\n";
    fs::write(&index, format!("bad record\n{entry}")).unwrap();
    let collector = CollectorService::start(
        db.clone(),
        CollectorOptions {
            watcher: false,
            active_poll: Duration::from_millis(50),
            manifest_poll: Duration::from_secs(10),
        },
    )
    .unwrap();
    let issues = || {
        db.diagnostics(&DiagnosticsRequest {
            source_id: Some("source".into()),
        })
        .unwrap()
        .issues
    };
    wait_for(|| {
        total(&db) == 1
            && issues()
                .iter()
                .any(|i| i.code == Some(ErrorCode::TitleIndexInvalid))
    });
    let issue = issues()
        .into_iter()
        .find(|i| i.code == Some(ErrorCode::TitleIndexInvalid))
        .unwrap();
    assert!(issue.path.unwrap().ends_with("session_index.jsonl"));
    assert_eq!(issue.byte_offset.unwrap().as_str(), "0");
    let revision = db.usage_revision().unwrap();
    fs::write(&index, entry).unwrap();
    wait_for(|| {
        !issues()
            .iter()
            .any(|i| i.code == Some(ErrorCode::TitleIndexInvalid))
    });
    let after = db.usage_revision().unwrap();
    assert_eq!(revision.data_revision, after.data_revision);
    assert_eq!(revision.price_revision, after.price_revision);
    assert!(after.usage_view_revision.value() > revision.usage_view_revision.value());
    assert_eq!(total(&db), 1);
    collector.shutdown();
}
