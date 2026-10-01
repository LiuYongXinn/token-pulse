use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use token_pulse_collector::{collect_file, jobs::JobService, replay::execute_rebuild_controlled};
use token_pulse_core::{
    jobs::{JobRequest, JobScope},
    protocol::{JobKind, JobState},
};
use token_pulse_store::{Database, ErrorCode, SourceRecord};
#[test]
fn background_worker_drains_persisted_queue_and_reports_completion_without_source_writes() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    fs::create_dir_all(logs.path().join("sessions")).unwrap();
    db.add_source(SourceRecord {
        source_id: "local".into(),
        root_path: logs.path().to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    let path = logs.path().join("sessions/source.jsonl");
    let bytes = include_bytes!("../../../fixtures/codex-rollout-v1.jsonl");
    fs::write(&path, bytes).unwrap();
    collect_file(&db, "local", &path, 5000).unwrap();
    for key in ["first", "second"] {
        db.create_job(
            key.into(),
            JobRequest {
                kind: JobKind::Rebuild,
                scope: JobScope::All {},
                request_key: key.into(),
            },
            1,
        )
        .unwrap();
    }
    let finished = Arc::new(AtomicUsize::new(0));
    let count = finished.clone();
    let service = JobService::start_with_notify(
        db.clone(),
        Arc::new(move || {
            count.fetch_add(1, Ordering::Release);
        }),
    )
    .unwrap();
    service.wake();
    let until = Instant::now() + Duration::from_secs(3);
    while finished.load(Ordering::Acquire) < 2 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    service.shutdown();
    assert_eq!(finished.load(Ordering::Acquire), 2);
    for key in ["first", "second"] {
        assert_eq!(db.get_job(key).unwrap().job.state, JobState::Succeeded);
    }
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(db.next_queued_rebuild().unwrap().is_none());
    db.snapshot(|tx, _| {
        let total: String = tx.query_row(
            "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(total, "120");
        Ok(())
    })
    .unwrap();
}
#[test]
fn shutdown_interruption_is_distinct_from_user_cancellation() {
    let data = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    db.create_job(
        "interrupt".into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::All {},
            request_key: "interrupt".into(),
        },
        1,
    )
    .unwrap();
    assert_eq!(
        execute_rebuild_controlled(&db, "interrupt", || Some(ErrorCode::JobInterrupted), || 2)
            .unwrap_err()
            .code,
        ErrorCode::JobInterrupted
    );
    assert_eq!(
        db.get_job("interrupt").unwrap().job.state,
        JobState::Interrupted
    );
    assert_eq!(
        db.get_job("interrupt").unwrap().job.error.unwrap().code,
        ErrorCode::JobInterrupted
    );
}

#[test]
fn rebuilding_an_empty_database_succeeds_without_manufacturing_a_revision() {
    let data = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    db.create_job(
        "empty".into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::All {},
            request_key: "empty".into(),
        },
        1,
    )
    .unwrap();
    assert_eq!(
        execute_rebuild_controlled(&db, "empty", || None, || 2).unwrap(),
        0
    );
    let job = db.get_job("empty").unwrap().job;
    assert_eq!(job.state, JobState::Succeeded);
    assert!(job.discovery_complete);
    assert_eq!(job.accepted_events.as_str(), "0");
}
