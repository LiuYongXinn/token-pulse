use token_pulse_core::{
    jobs::*,
    numeric::DecimalInt,
    protocol::{JobKind, JobState},
};
use token_pulse_store::{Database, ErrorCode};

fn request(key: &str) -> JobRequest {
    JobRequest {
        kind: JobKind::Rebuild,
        scope: JobScope::All {},
        request_key: key.into(),
    }
}
fn advance(db: &Database, id: &str, from: JobState, to: JobState) -> Result<(), ErrorCode> {
    db.advance_job(
        id.into(),
        token_pulse_store::jobs::JobAdvance {
            expected: from,
            next: to,
            progress: JobProgress {
                phase: format!("{to:?}"),
                ..Default::default()
            },
            checkpoint: JobCheckpoint::default(),
            error: if to == JobState::Failed {
                Some(ErrorCode::DbWriteFailed)
            } else {
                None
            },
            at_ms: 2,
        },
    )
    .map(|_| ())
    .map_err(|e| e.code)
}
#[test]
fn canonical_requests_are_durable_and_conflicting_keys_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let mut r = request("one");
    r.scope = JobScope::Sources {
        source_ids: vec!["b".into(), "a".into(), "b".into()],
    };
    let first = db.create_job("job-one".into(), r.clone(), 1).unwrap();
    r.scope = JobScope::Sources {
        source_ids: vec!["a".into(), "b".into()],
    };
    assert_eq!(
        db.create_job("ignored-id".into(), r.clone(), 2)
            .unwrap()
            .job_id,
        first.job_id
    );
    r.kind = JobKind::Import;
    assert_eq!(
        db.create_job("other-id".into(), r, 3).unwrap_err().code,
        ErrorCode::RequestKeyConflict
    );
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(db.get_job("job-one").unwrap().job.state, JobState::Queued);
    assert_eq!(db.list_jobs(100).unwrap().len(), 1);
    assert_eq!(db.list_jobs(101).unwrap_err().code, ErrorCode::InvalidQuery);
}
#[test]
fn queued_cancel_is_final_and_running_cancel_waits_for_safe_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.create_job("queued".into(), request("q"), 1).unwrap();
    assert_eq!(
        db.cancel_job("queued".into(), 2).unwrap(),
        CancelJobResult::Accepted
    );
    assert_eq!(db.get_job("queued").unwrap().job.state, JobState::Cancelled);
    assert_eq!(
        db.cancel_job("queued".into(), 3).unwrap(),
        CancelJobResult::AlreadyFinished
    );
    assert_eq!(
        advance(&db, "queued", JobState::Queued, JobState::Running),
        Err(ErrorCode::RevisionConflict)
    );
    db.create_job("running".into(), request("r"), 1).unwrap();
    advance(&db, "running", JobState::Queued, JobState::Running).unwrap();
    db.cancel_job("running".into(), 2).unwrap();
    assert_eq!(
        db.get_job("running").unwrap().job.state,
        JobState::Cancelling
    );
    assert_eq!(
        advance(&db, "running", JobState::Running, JobState::Validating),
        Err(ErrorCode::JobCancelled)
    );
    assert_eq!(
        db.checkpoint_job(
            "running".into(),
            JobState::Running,
            JobProgress::default(),
            JobCheckpoint::default(),
            3
        )
        .unwrap_err()
        .code,
        ErrorCode::JobCancelled
    );
    advance(&db, "running", JobState::Cancelling, JobState::Cancelled).unwrap();
    assert_eq!(
        db.get_job("running").unwrap().job.error.unwrap().code,
        ErrorCode::JobCancelled
    );
}
#[test]
fn publishing_cancel_and_worker_compete_without_overwriting_each_other() {
    use std::sync::{Arc, Barrier};
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    for i in 0..20 {
        let id = format!("race-{i}");
        db.create_job(id.clone(), request(&id), 1).unwrap();
        advance(&db, &id, JobState::Queued, JobState::Running).unwrap();
        advance(&db, &id, JobState::Running, JobState::Validating).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let worker = {
            let db = db.clone();
            let b = barrier.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                b.wait();
                advance(&db, &id, JobState::Validating, JobState::Publishing)
            })
        };
        let cancel = {
            let db = db.clone();
            let b = barrier.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                b.wait();
                db.cancel_job(id, 3).unwrap()
            })
        };
        barrier.wait();
        let result = worker.join().unwrap();
        let cancel = cancel.join().unwrap();
        match (result, cancel) {
            (Ok(()), CancelJobResult::TooLate) => {
                assert_eq!(db.get_job(&id).unwrap().job.state, JobState::Publishing);
                advance(&db, &id, JobState::Publishing, JobState::Succeeded).unwrap();
            }
            (Err(ErrorCode::JobCancelled), CancelJobResult::Accepted) => {
                assert_eq!(db.get_job(&id).unwrap().job.state, JobState::Cancelling);
                advance(&db, &id, JobState::Cancelling, JobState::Cancelled).unwrap();
            }
            other => panic!("invalid race result {other:?}"),
        }
    }
}
#[test]
fn progress_is_lossless_monotonic_and_checkpoints_are_typed() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.create_job("progress".into(), request("p"), 1).unwrap();
    advance(&db, "progress", JobState::Queued, JobState::Running).unwrap();
    let progress = JobProgress {
        phase: "replay".into(),
        discovered_files: DecimalInt::parse("9").unwrap(),
        processed_files: DecimalInt::parse("2").unwrap(),
        processed_bytes: DecimalInt::parse("92233720368547758080").unwrap(),
        ..Default::default()
    };
    let checkpoint = JobCheckpoint {
        batch_position: DecimalInt::parse("2").unwrap(),
        candidate_ledger_ids: vec!["candidate-a".into()],
        ..Default::default()
    };
    db.checkpoint_job(
        "progress".into(),
        JobState::Running,
        progress.clone(),
        checkpoint.clone(),
        3,
    )
    .unwrap();
    assert_eq!(
        db.get_job("progress").unwrap().job.processed_bytes,
        progress.processed_bytes
    );
    assert_eq!(db.get_job("progress").unwrap().checkpoint, checkpoint);
    let mut invalid = progress.clone();
    invalid.processed_bytes = DecimalInt::parse("1").unwrap();
    assert_eq!(
        db.checkpoint_job(
            "progress".into(),
            JobState::Running,
            invalid,
            checkpoint.clone(),
            4
        )
        .unwrap_err()
        .code,
        ErrorCode::RevisionConflict
    );
    let mut invalid = progress.clone();
    invalid.processed_files = DecimalInt::parse("10").unwrap();
    assert_eq!(
        db.checkpoint_job(
            "progress".into(),
            JobState::Running,
            invalid,
            checkpoint.clone(),
            4
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    let mut invalid = checkpoint;
    invalid.version = 2;
    assert_eq!(
        db.checkpoint_job(
            "progress".into(),
            JobState::Running,
            progress.clone(),
            invalid,
            4
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.get_job("progress").unwrap().job.processed_bytes,
        progress.processed_bytes
    );
}
#[test]
fn restart_marks_unfinished_work_interrupted_and_keeps_old_active_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.ensure_session(token_pulse_store::SessionRegistration {
        session_key: "session".into(),
        provider_session_id: None,
        parent_key: None,
        parent_provider_id: None,
        created_at_ms: None,
        ledger_id: "old-active".into(),
        registered_at_ms: 1,
    })
    .unwrap();
    db.create_job("unfinished".into(), request("unfinished"), 1)
        .unwrap();
    advance(&db, "unfinished", JobState::Queued, JobState::Running).unwrap();
    db.create_job("failed".into(), request("failed"), 1)
        .unwrap();
    advance(&db, "failed", JobState::Queued, JobState::Failed).unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(db.interrupt_unfinished_jobs(5).unwrap(), 1);
    assert_eq!(
        db.get_job("unfinished").unwrap().job.state,
        JobState::Interrupted
    );
    assert_eq!(
        db.get_job("unfinished").unwrap().job.error.unwrap().code,
        ErrorCode::JobInterrupted
    );
    assert_eq!(db.get_job("failed").unwrap().job.state, JobState::Failed);
    assert_eq!(db.interrupt_unfinished_jobs(6).unwrap(), 0);
    db.snapshot(|tx, _| {
        let ledger: String = tx.query_row(
            "SELECT active_ledger_id FROM sessions WHERE session_key='session'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(ledger, "old-active");
        Ok(())
    })
    .unwrap();
}

#[test]
fn durable_queue_is_bounded_but_an_idempotent_retry_does_not_consume_another_slot() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    for n in 0..32 {
        let id = format!("job-{n}");
        db.create_job(id.clone(), request(&id), n).unwrap();
    }
    assert_eq!(
        db.create_job("overflow".into(), request("overflow"), 33)
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.create_job("retry".into(), request("job-0"), 33)
            .unwrap()
            .job_id,
        "job-0"
    );
    db.cancel_job("job-0".into(), 34).unwrap();
    db.create_job("new".into(), request("new"), 35).unwrap();
}
