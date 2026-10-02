use super::*;
use crate::batch::tests::{fixture, setup};
use std::sync::{Arc, atomic::AtomicBool};

fn request(key: &str) -> PriceRevalueRequest {
    PriceRevalueRequest {
        scope: JobScope::All {},
        basis: PriceBasis::EventTime {},
        expected_price_revision: integer(0).unwrap(),
        request_key: key.into(),
    }
}
fn populated() -> (tempfile::TempDir, Database) {
    let (dir, db) = setup();
    db.commit(fixture()).unwrap();
    (dir, db)
}
#[test]
fn creation_is_idempotent_validated_and_has_no_consumption_effect() {
    let (_dir, db) = populated();
    let before=db.snapshot(|tx,r|Ok((r.data,r.price,r.settings,tx.query_row("SELECT committed_offset FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get::<_,i64>(0))?))).unwrap();
    let job = db
        .create_price_revalue_job("job".into(), request("request"), 1)
        .unwrap();
    assert_eq!(job.total_ledgers.as_str(), "1");
    assert_eq!(job.total_events.as_str(), "1");
    assert_eq!(
        db.create_price_revalue_job("other".into(), request("request"), 2)
            .unwrap()
            .job_id,
        "job"
    );
    let mut changed = request("request");
    changed.basis = PriceBasis::SpecifiedTime {
        specified_at_ms: EpochMs::new(2).unwrap(),
    };
    assert_eq!(
        db.create_price_revalue_job("other".into(), changed, 2)
            .unwrap_err()
            .code,
        ErrorCode::RequestKeyConflict
    );
    let mut stale = request("stale");
    stale.expected_price_revision = integer(1).unwrap();
    assert_eq!(
        db.create_price_revalue_job("stale".into(), stale, 2)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    let mut invalid = request("invalid");
    invalid.scope = JobScope::Sources {
        source_ids: vec!["unknown".into()],
    };
    assert_eq!(
        db.create_price_revalue_job("invalid".into(), invalid, 2)
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    db.snapshot(|tx,r| {assert_eq!((r.data,r.price,r.settings,tx.query_row("SELECT committed_offset FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get::<_,i64>(0))?),before);Ok(())}).unwrap();
}
#[test]
fn manual_work_has_priority_and_progress_is_monotonic_and_atomic() {
    let (_dir, db) = populated();
    let auto = db.enqueue_automatic_price_revalue(1).unwrap().unwrap();
    db.create_price_revalue_job("manual".into(), request("manual"), 2)
        .unwrap();
    assert_eq!(
        db.claim_price_revalue_job(3).unwrap().unwrap().job.job_id,
        "manual"
    );
    assert!(db.claim_price_revalue_job(3).unwrap().is_none());
    assert_eq!(
        db.finish_price_revalue_job("manual".into(), None, 3)
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    db.progress_price_revalue("manual".into(), "ledger".into(), 1, 1, false, 4)
        .unwrap();
    assert_eq!(
        db.progress_price_revalue("manual".into(), "ledger".into(), 0, 1, false, 4)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    assert_eq!(
        db.get_price_revalue_job("manual")
            .unwrap()
            .job
            .processed_events
            .as_str(),
        "1"
    );
    db.progress_price_revalue("manual".into(), "ledger".into(), 1, 1, true, 5)
        .unwrap();
    assert_eq!(
        db.finish_price_revalue_job("manual".into(), None, 6)
            .unwrap()
            .state,
        PriceRevalueState::Succeeded
    );
    assert_eq!(
        db.claim_price_revalue_job(7).unwrap().unwrap().job.job_id,
        auto.job_id
    );
}
#[test]
fn queued_and_running_cancel_have_different_transitions_and_no_hot_retry() {
    let (_dir, db) = populated();
    let auto = db.enqueue_automatic_price_revalue(1).unwrap().unwrap();
    assert_eq!(
        db.cancel_price_revalue_job(auto.job_id.clone(), 2).unwrap(),
        CancelJobResult::Accepted
    );
    assert_eq!(
        db.get_price_revalue_job(&auto.job_id).unwrap().job.state,
        PriceRevalueState::Cancelled
    );
    assert!(db.enqueue_automatic_price_revalue(3).unwrap().is_none());
    assert_eq!(
        db.cancel_price_revalue_job(auto.job_id, 4).unwrap(),
        CancelJobResult::AlreadyFinished
    );
    db.create_price_revalue_job("manual".into(), request("manual"), 5)
        .unwrap();
    db.claim_price_revalue_job(6).unwrap();
    db.cancel_price_revalue_job("manual".into(), 7).unwrap();
    assert_eq!(
        db.progress_price_revalue("manual".into(), "ledger".into(), 1, 1, true, 8)
            .unwrap_err()
            .code,
        ErrorCode::JobCancelled
    );
    assert_eq!(
        db.finish_price_revalue_job("manual".into(), Some(ErrorCode::CandidateObsolete), 9)
            .unwrap()
            .state,
        PriceRevalueState::Cancelled
    );
}
#[test]
fn interruption_survives_reopen_and_auto_resumes_only_uncached_work() {
    let (dir, db) = populated();
    let old = db.enqueue_automatic_price_revalue(1).unwrap().unwrap();
    db.claim_price_revalue_job(2).unwrap();
    assert_eq!(db.interrupt_price_revalue_jobs(3).unwrap(), 1);
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.get_price_revalue_job(&old.job_id).unwrap().job.error,
        Some(ErrorCode::JobInterrupted)
    );
    let new = db.enqueue_automatic_price_revalue(4).unwrap().unwrap();
    assert_eq!(new.job_id, old.job_id);
    assert_eq!(new.state, PriceRevalueState::Queued);
    db.claim_price_revalue_job(5).unwrap();
    db.cancel_price_revalue_job(new.job_id.clone(), 6).unwrap();
    db.interrupt_price_revalue_jobs(7).unwrap();
    assert_eq!(
        db.get_price_revalue_job(&new.job_id).unwrap().job.state,
        PriceRevalueState::Cancelled
    );
    assert!(db.enqueue_automatic_price_revalue(8).unwrap().is_none());
}
#[test]
fn ready_cache_skips_auto_and_status_counts_only_event_time() {
    let (_dir, db) = populated();
    assert_eq!(
        db.price_revalue_status().unwrap().uncached_ledgers.as_str(),
        "1"
    );
    db.build_event_valuation(
        "ledger",
        &PriceBasis::SpecifiedTime {
            specified_at_ms: EpochMs::new(1).unwrap(),
        },
        1,
    )
    .unwrap();
    assert_eq!(
        db.price_revalue_status().unwrap().uncached_ledgers.as_str(),
        "1"
    );
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 2)
        .unwrap();
    assert!(db.enqueue_automatic_price_revalue(3).unwrap().is_none());
    assert_eq!(
        db.price_revalue_status().unwrap().uncached_ledgers.as_str(),
        "0"
    );
}
#[test]
fn empty_manual_job_succeeds_and_automatic_does_not_invent_work() {
    let (_dir, db) = setup();
    assert!(db.enqueue_automatic_price_revalue(1).unwrap().is_none());
    db.create_price_revalue_job("empty".into(), request("empty"), 2)
        .unwrap();
    db.claim_price_revalue_job(3).unwrap();
    db.progress_price_revalue("empty".into(), "ledger".into(), 0, 0, true, 4)
        .unwrap();
    let finished = db
        .finish_price_revalue_job("empty".into(), None, 5)
        .unwrap();
    assert_eq!(finished.total_events.as_str(), "0");
    assert_eq!(finished.state, PriceRevalueState::Succeeded);
    assert!(!finished.can_cancel);
}
#[test]
fn failed_auto_waits_for_new_evidence_or_manual_retry() {
    let (_dir, db) = populated();
    let job = db.enqueue_automatic_price_revalue(1).unwrap().unwrap();
    db.claim_price_revalue_job(2).unwrap();
    db.finish_price_revalue_job(job.job_id.clone(), Some(ErrorCode::CandidateObsolete), 3)
        .unwrap();
    assert!(db.enqueue_automatic_price_revalue(4).unwrap().is_none());
    db.write(|conn| {
        conn.execute(
            "UPDATE ledger_usage_versions SET revision=revision+1 WHERE ledger_id='ledger'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert_ne!(
        db.enqueue_automatic_price_revalue(5)
            .unwrap()
            .unwrap()
            .job_id,
        job.job_id
    );
}
#[test]
fn pinned_builder_keeps_requested_price_even_when_current_revision_advances() {
    let (_dir, db) = populated();
    db.write(|conn| {
        conn.execute(
            "UPDATE app_state SET price_revision=1 WHERE singleton=1",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let result = db
        .build_event_valuation_at_revision_interruptible(
            "ledger",
            &PriceBasis::EventTime {},
            0,
            1,
            &stop,
            |_, _| {},
        )
        .unwrap();
    assert_eq!(result.price_revision, 0);
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT price_revision FROM valuation_sets WHERE valuation_set_id=?1",
                [result.set_id],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.build_event_valuation_at_revision_interruptible(
            "ledger",
            &PriceBasis::EventTime {},
            2,
            2,
            &stop,
            |_, _| {}
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
}
