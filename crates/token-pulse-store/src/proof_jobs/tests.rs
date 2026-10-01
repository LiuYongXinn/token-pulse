use super::*;
use crate::{
    batch::tests::{fixture, setup},
    jobs::JobAdvance,
};
use token_pulse_core::jobs::JobProgress;

#[test]
fn stale_versions_reject_baseline_and_prepared_append_without_touching_old_facts() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let old = db.usage_totals(&crate::query::tests::filter()).unwrap();
    db.write(|conn| {
        conn.execute(
            "UPDATE ledger_generations SET accounting_version='previous-engine'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.session_accounting("session").err().unwrap().code,
        ErrorCode::CandidateObsolete
    );
    let mut prepared = fixture();
    prepared.expected_offset = 100;
    prepared.expected_checkpoint_revision = 1;
    assert_eq!(
        db.commit(prepared).unwrap_err().code,
        ErrorCode::CandidateObsolete
    );
    assert_eq!(
        serde_json::to_value(db.usage_totals(&crate::query::tests::filter()).unwrap()).unwrap(),
        serde_json::to_value(old).unwrap()
    );
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT committed_offset FROM file_generations", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            100
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM observations", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM stream_states", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
    db.write(|conn| {
        conn.execute(
            "UPDATE ledger_generations SET parser_version='previous-parser'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(db.enqueue_accounting_upgrade(2).unwrap().is_none());
    assert_eq!(
        db.session_accounting("session").err().unwrap().code,
        ErrorCode::CandidateObsolete
    );
    let job = db
        .create_job(
            "parser-probe".into(),
            JobRequest {
                kind: JobKind::Rebuild,
                scope: JobScope::Sessions {
                    session_keys: vec!["session".into()],
                },
                request_key: "parser-probe".into(),
            },
            2,
        )
        .unwrap();
    let checkpoint = db.get_job(&job.job_id).unwrap().checkpoint;
    db.advance_job(
        job.job_id.clone(),
        JobAdvance {
            expected: JobState::Queued,
            next: JobState::Running,
            progress: JobProgress::default(),
            checkpoint,
            error: None,
            at_ms: 3,
        },
    )
    .unwrap();
    assert_eq!(
        db.prepare_rebuild(job.job_id, 4).unwrap_err().code,
        ErrorCode::UnsupportedFormat
    );
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM ledger_generations WHERE state='candidate'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn upgrade_freezes_committed_prefix_and_respects_busy_jobs_and_cancellation() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    assert!(db.enqueue_accounting_upgrade(1).unwrap().is_none());
    db.write(|conn| {
        conn.execute(
            "UPDATE ledger_generations SET accounting_version='previous-engine'",
            [],
        )?;
        conn.execute("UPDATE file_generations SET observed_size=101", [])?;
        Ok(())
    })
    .unwrap();
    let job = db.enqueue_accounting_upgrade(2).unwrap().unwrap();
    assert!(
        db.get_job(&job.job_id)
            .unwrap()
            .request
            .request_key
            .starts_with("accounting-upgrade-rebuild:")
    );
    assert!(db.enqueue_accounting_upgrade(3).unwrap().is_none());
    assert!(db.enqueue_proof_rebuild(3).unwrap().is_none());
    db.cancel_job(job.job_id, 4).unwrap();
    assert!(db.enqueue_accounting_upgrade(5).unwrap().is_none());
    // Changed real source availability is new evidence; stable input alone is not.
    db.write(|conn| {
        conn.execute("UPDATE sources SET enabled=0", [])?;
        Ok(())
    })
    .unwrap();
    assert!(db.enqueue_accounting_upgrade(6).unwrap().is_some());
}

#[test]
fn upgrade_obsolete_retries_are_bounded_and_changed_input_can_retry() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute(
            "UPDATE ledger_generations SET accounting_version='previous-engine'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    for retry in 0..3 {
        let job = db.enqueue_accounting_upgrade(2).unwrap().unwrap();
        assert!(
            db.get_job(&job.job_id)
                .unwrap()
                .request
                .request_key
                .ends_with(&format!(":{retry}"))
        );
        let stored = db.get_job(&job.job_id).unwrap();
        db.advance_job(
            job.job_id,
            JobAdvance {
                expected: JobState::Queued,
                next: JobState::Failed,
                progress: JobProgress::default(),
                checkpoint: stored.checkpoint,
                error: Some(ErrorCode::CandidateObsolete),
                at_ms: 3,
            },
        )
        .unwrap();
    }
    assert!(db.enqueue_accounting_upgrade(4).unwrap().is_none());
    db.write(|conn| {
        conn.execute(
            "UPDATE file_generations SET observed_size=observed_size+1",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let next = db.enqueue_accounting_upgrade(5).unwrap().unwrap();
    assert!(
        db.get_job(&next.job_id)
            .unwrap()
            .request
            .request_key
            .ends_with(":0")
    );
}
