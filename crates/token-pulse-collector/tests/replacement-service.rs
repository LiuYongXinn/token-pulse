//! Real Win32 watcher/polling and independent durable jobs; synthetic logs, no benchmarks.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use token_pulse_collector::{
    collect_file,
    jobs::JobService,
    replacement::read_replacement_file,
    service::{CollectorOptions, CollectorService},
};
use token_pulse_core::{
    numeric::EpochMs,
    protocol::{CoverageState, DateRange, DimensionSelection, UsageFilter},
};
use token_pulse_store::{Database, SourceRecord};
fn call(value: u8) -> Vec<u8> {
    let mut bytes=serde_json::to_vec(&serde_json::json!({"timestamp":"1970-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":value}}}})).unwrap();
    bytes.push(b'\n');
    bytes
}
fn log(id: &str, count: usize, value: u8) -> Vec<u8> {
    let mut bytes =
        serde_json::to_vec(&serde_json::json!({"type":"session_meta","payload":{"id":id}}))
            .unwrap();
    bytes.push(b'\n');
    for _ in 0..count {
        bytes.extend(call(value));
    }
    bytes
}
fn setup() -> (tempfile::TempDir, tempfile::TempDir, Database, PathBuf) {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    fs::create_dir(logs.path().join("sessions")).unwrap();
    let path = logs.path().join("sessions").join("change.jsonl");
    fs::write(&path, log("old", 3, 2)).unwrap();
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
    (data, logs, db, path)
}
fn start_collector(db: &Database, watcher: bool) -> CollectorService {
    CollectorService::start(
        db.clone(),
        CollectorOptions {
            watcher,
            active_poll: if watcher {
                Duration::from_secs(3600)
            } else {
                Duration::from_millis(70)
            },
            manifest_poll: Duration::from_secs(3600),
        },
    )
    .unwrap()
}
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
fn total(db: &Database) -> String {
    db.usage_totals(&filter())
        .unwrap()
        .total_tokens
        .as_str()
        .into()
}
fn complete(db: &Database) -> bool {
    matches!(
        db.usage_coverage(&filter()).unwrap().state,
        CoverageState::Complete
    )
}
fn wait(db: &Database, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "replacement condition timed out; total {}, coverage {:?}",
            total(db),
            db.usage_coverage(&filter()).unwrap()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn append(path: &Path, bytes: &[u8]) {
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}
fn checkpoint(db: &Database, path: &Path) -> (String, i64, i64) {
    let c = db
        .file_checkpoint("source", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    (
        c.file_generation_id,
        c.committed_offset,
        c.checkpoint_revision,
    )
}

#[test]
fn native_watcher_and_polling_automatically_publish_truncate_replace_and_append_with_complete_new_proof()
 {
    for watcher in [false, true] {
        let (_data, logs, db, path) = setup();
        let collector = start_collector(&db, watcher);
        let jobs = JobService::start(db.clone()).unwrap();
        wait(&db, || complete(&db) && total(&db) == "6");
        let bytes = log("old", 1, 8);
        fs::write(&path, &bytes).unwrap();
        wait(&db, || complete(&db) && total(&db) == "8");
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::rename(&path, logs.path().join("old-unscanned.jsonl")).unwrap();
        let bytes = log("new", 3, 8);
        fs::write(&path, &bytes).unwrap();
        wait(&db, || complete(&db) && total(&db) == "24");
        assert_eq!(fs::read(&path).unwrap(), bytes);
        append(&path, &call(8));
        wait(&db, || complete(&db) && total(&db) == "32");
        db.snapshot(|tx,_| {assert_eq!(tx.query_row("SELECT COUNT(*) FROM file_read_candidates WHERE state='published'",[],|r|r.get::<_,i64>(0))?,2);assert_eq!(tx.query_row("SELECT COUNT(*) FROM file_read_candidates WHERE state IN ('reading','ready','claimed')",[],|r|r.get::<_,i64>(0))?,0);assert!(tx.query_row("SELECT e.file_generation_id=f.current_generation_id AND e.checkpoint_revision=g.checkpoint_revision AND e.upper_bound=g.committed_offset FROM source_scan_files e JOIN source_files f ON f.file_id=e.file_id JOIN file_generations g ON g.file_generation_id=f.current_generation_id",[],|r|r.get::<_,bool>(0))?);Ok(())}).unwrap();
        collector.shutdown();
        jobs.shutdown();
    }
}
#[test]
fn startup_and_resume_replace_across_read_and_registration_batches_without_manual_requests() {
    let (_data, _logs, db, path) = setup();
    collect_file(&db, "source", &path, 1).unwrap();
    fs::write(&path, log("new", 601, 8)).unwrap();
    let collector = start_collector(&db, false);
    let jobs = JobService::start(db.clone()).unwrap();
    wait(&db, || complete(&db) && total(&db) == "4808");
    collector.suspend();
    wait(&db, || collector.status().suspended);
    assert!(!complete(&db));
    let previous = checkpoint(&db, &path);
    fs::write(&path, log("again", 5, 9)).unwrap();
    assert_eq!(total(&db), "4808");
    assert_eq!(checkpoint(&db, &path), previous);
    collector.resume();
    wait(&db, || complete(&db) && total(&db) == "45");
    collector.shutdown();
    jobs.shutdown();
}
#[test]
fn unfinished_replacement_retains_old_results_and_resumes_same_candidate_after_restart_and_newline()
{
    let (data, _logs, db, path) = setup();
    collect_file(&db, "source", &path, 1).unwrap();
    let before = checkpoint(&db, &path);
    let mut bytes = log("new", 601, 8);
    let tail = call(8);
    bytes.extend(&tail[..tail.len() - 1]);
    fs::write(&path, bytes).unwrap();
    let collector = start_collector(&db, false);
    wait(&db, || {
        db.snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT COUNT(*) FROM file_candidate_observations",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap()
            == 602
    });
    assert_eq!(total(&db), "6");
    assert!(!complete(&db));
    assert_eq!(checkpoint(&db, &path), before);
    let file = db
        .file_checkpoint("source", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    let generation = db
        .active_file_read_candidate(&file.file_id)
        .unwrap()
        .unwrap()
        .checkpoint
        .file_generation_id;
    collector.shutdown();
    drop(collector);
    drop(db);
    append(&path, b"\n");
    let db = Database::open(data.path()).unwrap();
    let collector = start_collector(&db, false);
    let jobs = JobService::start(db.clone()).unwrap();
    wait(&db, || complete(&db) && total(&db) == "4816");
    assert_eq!(
        db.file_read_candidate(&generation).unwrap().state,
        "published"
    );
    collector.shutdown();
    jobs.shutdown();
}
#[test]
fn claimed_file_waits_for_job_without_advancing_old_checkpoint_or_requeueing_then_gets_fresh_confirmation()
 {
    let (_data, _logs, db, path) = setup();
    collect_file(&db, "source", &path, 1).unwrap();
    let before = checkpoint(&db, &path);
    fs::write(&path, log("new", 3, 8)).unwrap();
    let read = read_replacement_file(&db, "source", &path, 2).unwrap();
    let job = db
        .enqueue_file_candidate_rebuild(read.generation_id.clone(), read.checkpoint_revision, 3)
        .unwrap();
    let collector = start_collector(&db, false);
    wait(&db, || {
        db.snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT COALESCE(MAX(discovery_complete),0) FROM source_scan_state",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap()
            == 1
    });
    assert_eq!(total(&db), "6");
    assert!(!complete(&db));
    assert_eq!(checkpoint(&db, &path), before);
    assert_eq!(
        db.get_job(&job.job_id).unwrap().job.state,
        token_pulse_core::protocol::JobState::Queued
    );
    assert_eq!(
        db.snapshot(|tx, _| Ok(
            tx.query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get::<_, i64>(0))?
        ))
        .unwrap(),
        1
    );
    let jobs = JobService::start(db.clone()).unwrap();
    wait(&db, || complete(&db) && total(&db) == "24");
    assert_eq!(
        db.file_read_candidate(&read.generation_id).unwrap().state,
        "published"
    );
    collector.shutdown();
    jobs.shutdown();
}
#[test]
fn another_rewrite_during_unfinished_read_releases_old_candidate_and_eventually_publishes_only_latest_usage()
 {
    let (_data, _logs, db, path) = setup();
    let collector = start_collector(&db, false);
    let jobs = JobService::start(db.clone()).unwrap();
    wait(&db, || complete(&db) && total(&db) == "6");
    let file = db
        .file_checkpoint("source", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    let mut bytes = log("new", 1, 8);
    bytes.extend(b"{");
    fs::write(&path, bytes).unwrap();
    wait(&db, || {
        db.active_file_read_candidate(&file.file_id)
            .unwrap()
            .is_some()
    });
    let generation = db
        .active_file_read_candidate(&file.file_id)
        .unwrap()
        .unwrap()
        .checkpoint
        .file_generation_id;
    assert_eq!(total(&db), "6");
    assert!(!complete(&db));
    fs::write(&path, log("new", 1, 9)).unwrap();
    wait(&db, || complete(&db) && total(&db) == "9");
    assert_eq!(db.file_read_candidate(&generation).unwrap().state, "failed");
    assert_eq!(
        db.snapshot(|tx, _| Ok(tx.query_row(
            "SELECT COUNT(*) FROM file_read_candidates WHERE state='published'",
            [],
            |r| r.get::<_, i64>(0)
        )?))
        .unwrap(),
        1
    );
    collector.shutdown();
    jobs.shutdown();
}
#[test]
fn frozen_dependency_reads_wait_and_resume_after_failure() {
    use token_pulse_core::{jobs::JobProgress, protocol::JobState};
    use token_pulse_store::{ErrorCode, jobs::JobAdvance};
    let (_data, logs, db, path) = setup();
    collect_file(&db, "source", &path, 1).unwrap();
    let other = logs.path().join("sessions").join("other.jsonl");
    fs::write(&other, log("other", 1, 2)).unwrap();
    collect_file(&db, "source", &other, 1).unwrap();
    let before = checkpoint(&db, &other);
    fs::write(&path, log("new", 3, 8)).unwrap();
    let read = read_replacement_file(&db, "source", &path, 2).unwrap();
    let job = db
        .enqueue_file_candidate_rebuild(read.generation_id.clone(), read.checkpoint_revision, 3)
        .unwrap();
    db.advance_job(
        job.job_id.clone(),
        JobAdvance {
            expected: JobState::Queued,
            next: JobState::Running,
            progress: JobProgress::default(),
            checkpoint: db.get_job(&job.job_id).unwrap().checkpoint,
            error: None,
            at_ms: 4,
        },
    )
    .unwrap();
    db.register_file_candidate_inputs(read.generation_id, job.job_id.clone(), -1, 4)
        .unwrap();
    let manifest = db.prepare_rebuild(job.job_id.clone(), 4).unwrap();
    assert_eq!(manifest.files.len(), 2);
    append(&other, &call(2));
    let collector = start_collector(&db, false);
    wait(&db, || {
        db.snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT COALESCE(MAX(discovery_complete),0) FROM source_scan_state",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap()
            == 1
    });
    assert_eq!(checkpoint(&db, &other), before);
    assert_eq!(total(&db), "8");
    assert!(!complete(&db));
    db.fail_rebuild(job.job_id, ErrorCode::JobInterrupted, 5)
        .unwrap();
    let jobs = JobService::start(db.clone()).unwrap();
    wait(&db, || complete(&db) && total(&db) == "28");
    assert!(checkpoint(&db, &other).2 > before.2);
    collector.shutdown();
    jobs.shutdown();
}
