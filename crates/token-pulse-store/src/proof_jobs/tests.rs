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
            "UPDATE ledger_generations SET accounting_version='accounting-v1'",
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
            "UPDATE ledger_generations SET accounting_version='accounting-v1'",
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
            "UPDATE ledger_generations SET accounting_version='accounting-v1'",
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

#[test]
fn legacy_partial_facts_remain_readable_raw_cached_and_inside_old_transactions() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        let tx=conn.transaction()?;
        tx.execute("UPDATE ledger_generations SET accounting_version='accounting-v1'",[])?;
        tx.execute("UPDATE usage_events SET output_tokens_total=NULL,reasoning_output_tokens=NULL,source_total_tokens=10,total_tokens=10",[])?;
        tx.commit()?;Ok(())
    }).unwrap();
    let f = crate::query::tests::filter();
    let raw = db.usage_totals(&f).unwrap();
    assert_eq!(raw.total_tokens.as_str(), "10");
    assert_eq!(raw.input_total.value.as_ref().unwrap().as_str(), "100");
    assert!(raw.output_total.value.is_none());
    assert_eq!(
        db.pricing_summary(&f, &token_pulse_core::protocol::PriceBasis::EventTime {})
            .unwrap()
            .unpriced_total_tokens
            .as_str(),
        "10"
    );
    db.build_hourly_rollup("ledger", 2).unwrap();
    let mut hour = f.clone();
    hour.range.end_ms = EpochMs::new(3_600_000).unwrap();
    assert_eq!(
        serde_json::to_value(db.usage_totals(&hour).unwrap()).unwrap(),
        serde_json::to_value(&raw).unwrap()
    );
    db.snapshot(|tx,_| {
        let frozen=crate::query::totals(tx,&hour)?;
        db.write(|conn| {
            let tx=conn.transaction()?;
            tx.execute("UPDATE ledger_generations SET accounting_version=?1",[ACCOUNTING_VERSION])?;
            tx.execute("UPDATE usage_events SET output_tokens_total=10,reasoning_output_tokens=2,source_total_tokens=110,total_tokens=110",[])?;
            tx.commit()?;Ok(())
        })?;
        assert_eq!(serde_json::to_value(crate::query::totals(tx,&hour)?).unwrap(),serde_json::to_value(frozen).unwrap());Ok(())
    }).unwrap();
    assert_eq!(db.usage_totals(&hour).unwrap().total_tokens.as_str(), "110");
}

#[test]
fn current_invalid_batches_and_unknown_engine_versions_are_rejected_without_upgrade() {
    let (_dir, db) = setup();
    let mut batch = fixture();
    batch.events[0].usage = token_pulse_core::domain::UsageVector {
        cached_input: Some(7),
        reasoning_output: Some(8),
        reported_total: Some(10),
        ..Default::default()
    };
    assert_eq!(db.commit(batch).unwrap_err().code, ErrorCode::InvalidUsage);
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM observations", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            tx.query_row("SELECT committed_offset FROM file_generations", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            0
        );
        Ok(())
    })
    .unwrap();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute(
            "UPDATE ledger_generations SET accounting_version='accounting-v999'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(db.enqueue_accounting_upgrade(2).unwrap().is_none());
    assert_eq!(
        db.usage_totals(&crate::query::tests::filter())
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedFormat
    );
    assert_eq!(
        db.session_accounting("session").err().unwrap().code,
        ErrorCode::CandidateObsolete
    );
}
