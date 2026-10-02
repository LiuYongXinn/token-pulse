//! Real temporary rollout trees, bounded batches and native watcher behavior. No benchmarks.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use token_pulse_collector::service::{CollectorOptions, CollectorService};
use token_pulse_core::{
    numeric::EpochMs,
    protocol::{CoverageState, DateRange, DimensionSelection, UsageFilter},
};
use token_pulse_store::{Database, SourceRecord};

fn filter() -> UsageFilter {
    UsageFilter {
        range: DateRange {
            start_ms: EpochMs::new(0).unwrap(),
            end_ms: EpochMs::new(5000).unwrap(),
            timezone: "UTC".into(),
        },
        sources: DimensionSelection::All {},
        models: DimensionSelection::All {},
        projects: DimensionSelection::All {},
        sessions: DimensionSelection::All {},
    }
}
fn call() -> &'static [u8] {
    b"{\"timestamp\":\"1970-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"total_tokens\":1}}}}\n"
}
fn log(path: &Path, id: &str) {
    let mut bytes =
        serde_json::to_vec(&serde_json::json!({"type":"session_meta","payload":{"id":id}}))
            .unwrap();
    bytes.push(b'\n');
    bytes.extend(call());
    fs::write(path, bytes).unwrap();
}
fn setup() -> (tempfile::TempDir, tempfile::TempDir, Database, PathBuf) {
    let logs = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    fs::create_dir(logs.path().join("sessions")).unwrap();
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
    let path = logs.path().join("sessions/a.jsonl");
    log(&path, "scan-a");
    (logs, data, db, path)
}
fn service(db: &Database, watcher: bool) -> CollectorService {
    CollectorService::start(
        db.clone(),
        CollectorOptions {
            watcher,
            active_poll: Duration::from_millis(70),
            manifest_poll: Duration::from_secs(3600),
        },
    )
    .unwrap()
}
fn wait(db: &Database, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "scan evidence condition timed out: {:?}, total {}", db.snapshot(|tx,_|Ok(tx.query_row("SELECT state,invalidated,discovery_complete,issue_code,(SELECT COUNT(*) FROM source_scan_files),(SELECT COUNT(*) FROM source_scan_files WHERE file_generation_id IS NULL),(SELECT COUNT(*) FROM source_files),readability FROM source_scan_state JOIN sources USING(source_id)",[],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?,r.get::<_,i64>(6)?,r.get::<_,String>(7)?)))?)).unwrap(), total(db)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn complete(db: &Database) -> bool {
    matches!(
        db.usage_coverage(&filter()).unwrap().state,
        CoverageState::Complete
    )
}
fn total(db: &Database) -> String {
    db.usage_totals(&filter())
        .unwrap()
        .total_tokens
        .as_str()
        .into()
}
fn append(path: &Path, bytes: &[u8]) {
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

#[test]
fn real_startup_suspend_resume_shutdown_and_restart_require_new_complete_proof() {
    let (_logs, _data, db, path) = setup();
    let original = fs::read(&path).unwrap();
    let collector = service(&db, false);
    wait(&db, || complete(&db));
    assert_eq!(total(&db), "1");
    assert_eq!(fs::read(&path).unwrap(), original);
    collector.suspend();
    wait(&db, || collector.status().suspended);
    assert!(!complete(&db));
    append(&path, call());
    assert_eq!(total(&db), "1");
    collector.resume();
    wait(&db, || complete(&db) && total(&db) == "2");
    collector.shutdown();
    assert!(!complete(&db));
    let restarted = service(&db, false);
    wait(&db, || complete(&db));
    assert_eq!(total(&db), "2");
    restarted.shutdown();
    assert!(!complete(&db));
}
#[test]
fn incomplete_final_line_is_a_real_gap_until_native_watcher_sees_newline() {
    let (_logs, _data, db, path) = setup();
    append(&path, &call()[..call().len() - 1]);
    let collector = service(&db, true);
    wait(&db, || {
        total(&db) == "1"
            && db
                .usage_coverage(&filter())
                .unwrap()
                .pending_file_count
                .as_str()
                == "1"
    });
    assert!(!complete(&db));
    append(&path, b"\n");
    wait(&db, || complete(&db) && total(&db) == "2");
    collector.shutdown();
}
#[test]
fn archive_moves_prune_old_queue_paths_with_and_without_native_watcher() {
    for watcher in [false, true] {
        let (logs, _data, db, path) = setup();
        let original = fs::read(&path).unwrap();
        let collector = service(&db, watcher);
        wait(&db, || complete(&db));
        fs::create_dir(logs.path().join("archived_sessions")).unwrap();
        let archived = logs.path().join("archived_sessions").join("a.jsonl");
        fs::rename(&path, &archived).unwrap();
        wait(&db, || {
            db.snapshot(|tx, _| {
                Ok(tx.query_row(
                    "SELECT COUNT(*) FROM source_files WHERE canonical_path=?1",
                    [archived.to_str().unwrap()],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap()
                == 1
                && complete(&db)
        });
        std::thread::sleep(Duration::from_millis(500));
        assert!(collector.status().error.is_none());
        wait(&db, || complete(&db));
        assert_eq!(total(&db), "1");
        assert_eq!(fs::read(&archived).unwrap(), original);
        db.snapshot(|tx, _| {
            assert_eq!(
                tx.query_row("SELECT COUNT(*) FROM source_files", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
            Ok(())
        })
        .unwrap();
        collector.shutdown();
    }
}
#[test]
fn all_129_enumerated_files_and_later_membership_are_read_before_completion() {
    let (logs, _data, db, _path) = setup();
    for i in 1..129 {
        log(
            &logs.path().join(format!("sessions/{i}.jsonl")),
            &format!("scan-{i}"),
        );
    }
    let collector = CollectorService::start(
        db.clone(),
        CollectorOptions {
            watcher: false,
            active_poll: Duration::from_secs(60),
            manifest_poll: Duration::from_secs(3600),
        },
    )
    .unwrap();
    // Check the manifest and its completion in one read snapshot: a subsequent run
    // may legitimately start between separate queries.
    wait(&db, || {
        db.snapshot(|tx,_|Ok(tx.query_row("SELECT EXISTS(SELECT 1 FROM source_scan_state WHERE source_id='source' AND state='ready' AND (SELECT COUNT(*) FROM source_scan_files)=129 AND (SELECT COALESCE(SUM(total_tokens),0) FROM active_usage_events)=129)",[],|r|r.get::<_,bool>(0))?)).unwrap()
    });
    let b = logs.path().join("sessions/new.jsonl");
    log(&b, "scan-new");
    collector.reconcile();
    wait(&db, || complete(&db) && total(&db) == "130");
    collector.shutdown();
}
#[test]
fn removed_rollout_tree_invalidates_native_scan_without_discarding_past_consumption() {
    let (logs, _data, db, path) = setup();
    let collector = service(&db, true);
    wait(&db, || complete(&db));
    fs::remove_file(&path).unwrap();
    fs::remove_dir(logs.path().join("sessions")).unwrap();
    wait(&db, || {
        db.snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT COUNT(*) FROM source_files WHERE status='missing'",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap()
            == 1
    });
    assert!(!complete(&db));
    assert_eq!(total(&db), "1");
    assert_eq!(
        db.usage_coverage(&filter())
            .unwrap()
            .pending_file_count
            .as_str(),
        "1"
    );
    collector.shutdown();
}
#[test]
fn invalid_generation_retains_old_facts_and_cannot_claim_complete() {
    let (_logs, _data, db, path) = setup();
    let collector = service(&db, true);
    wait(&db, || complete(&db));
    fs::write(&path, b"{}").unwrap();
    wait(&db, || collector.status().error.is_some());
    assert!(!complete(&db));
    assert_eq!(total(&db), "1");
    collector.shutdown();
}
