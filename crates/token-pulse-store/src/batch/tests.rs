use super::*;
use crate::{FileRegistration, SessionRegistration, SourceRecord};
use token_pulse_core::domain::{PhysicalPosition, UsageObservation};

fn setup() -> (tempfile::TempDir, Database) {
    let directory = tempfile::tempdir().unwrap();
    let db = Database::open(directory.path()).unwrap();
    db.add_source(SourceRecord {
        source_id: "source".into(),
        root_path: "synthetic".into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    db.ensure_session(SessionRegistration {
        session_key: "session".into(),
        provider_session_id: Some("provider-session".into()),
        parent_key: None,
        parent_provider_id: None,
        created_at_ms: None,
        ledger_id: "ledger".into(),
        registered_at_ms: 1,
    })
    .unwrap();
    db.register_file(FileRegistration {
        file_id: "file".into(),
        source_id: "source".into(),
        canonical_path: "synthetic.jsonl".into(),
        file_identity: Some("identity".into()),
        file_generation_id: "generation".into(),
        observed_size: 100,
        created_at_ms: 1,
        reader_context: ReaderContext::default(),
    })
    .unwrap();
    (directory, db)
}
fn fixture() -> WriteBatch {
    let usage = UsageVector {
        input_total: Some(100),
        cached_input: Some(60),
        output_total: Some(10),
        reasoning_output: Some(2),
        reported_total: Some(110),
    };
    let observation = UsageObservation {
        physical_position: PhysicalPosition {
            file_generation_id: "generation".into(),
            byte_offset: 0,
            byte_end: 100,
        },
        session_key: "session".into(),
        event_time_ms: Some(1000),
        request_identity: None,
        stream_hint: Some("stream".into()),
        last: Some(usage),
        cumulative: Some(usage),
        effective_metadata: EffectiveMetadata::default(),
        explicit_episode_start: true,
    };
    WriteBatch {
        file_generation_id: "generation".into(),
        expected_offset: 0,
        expected_checkpoint_revision: 0,
        next_offset: 100,
        observed_size: 100,
        anchors: vec![],
        reader_context: ReaderContext::default(),
        ledgers: vec![LedgerExpectation {
            session_key: "session".into(),
            ledger_id: "ledger".into(),
        }],
        observations: vec![ObservationWrite {
            observation_id: "observation".into(),
            session_key: Some("session".into()),
            record: NormalizedObservation::Usage(observation),
            payload_fingerprint: "same-fingerprint-may-recur".into(),
        }],
        events: vec![EventWrite {
            event_id: "event".into(),
            ledger_id: "ledger".into(),
            origin_observation_id: "observation".into(),
            occurred_at_ms: 1000,
            semantic_key: None,
            episode_id: "episode".into(),
            model: None,
            project_id: None,
            turn_id: None,
            usage,
            calculation_method: "last_new_stream".into(),
        }],
        streams: vec![StreamWrite {
            ledger_id: "ledger".into(),
            stream_key: "stream".into(),
            episode_id: "episode".into(),
            baseline: usage,
            observation_id: "observation".into(),
            quality: ObservationQuality::Confirmed,
            expected_state_revision: None,
        }],
        provenance: vec![],
        pending: vec![],
        contexts: vec![],
        diagnostics: vec![],
    }
}
fn assert_state(db: &Database, committed: bool) {
    db.snapshot(|tx, revision| {
        assert_eq!(revision.data, i64::from(committed));
        for table in [
            "observations",
            "usage_events",
            "stream_states",
            "event_provenance",
        ] {
            assert_eq!(
                tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
                i64::from(committed),
                "{table}"
            );
        }
        assert_eq!(
            tx.query_row("SELECT committed_offset FROM file_generations", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            if committed { 100 } else { 0 }
        );
        assert_eq!(
            tx.query_row(
                "SELECT checkpoint_revision FROM file_generations",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            i64::from(committed)
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn each_failure_boundary_is_all_or_nothing_after_reopen() {
    for failure in [
        CommitStage::Observations,
        CommitStage::Events,
        CommitStage::Streams,
        CommitStage::Checkpoint,
        CommitStage::BeforeCommit,
        CommitStage::AfterCommit,
    ] {
        let (directory, db) = setup();
        let result = db.write(move |conn| {
            commit_batch(conn, fixture(), |stage| {
                if stage == failure {
                    Err(ErrorCode::DbWriteFailed.into())
                } else {
                    Ok(())
                }
            })
        });
        assert!(result.is_err());
        assert_state(&db, failure == CommitStage::AfterCommit);
        drop(db);
        let reopened = Database::open(directory.path()).unwrap();
        assert_state(&reopened, failure == CommitStage::AfterCommit);
    }
}
#[test]
fn successful_batch_and_stale_retry_do_not_double_count() {
    let (_directory, db) = setup();
    let receipt = db.commit(fixture()).unwrap();
    assert_eq!(
        receipt,
        CommitReceipt {
            data_revision: 1,
            checkpoint_revision: 1,
            usage_changed: true
        }
    );
    assert_state(&db, true);
    assert_eq!(
        db.commit(fixture()).unwrap_err().code,
        ErrorCode::CheckpointConflict
    );
    assert_state(&db, true);
}
#[test]
fn sqlite_constraint_failure_rolls_back_observations_and_checkpoint() {
    let (_directory, db) = setup();
    let mut batch = fixture();
    batch.events[0].event_id = "same".into();
    let mut second = batch.events[0].clone();
    second.semantic_key = Some("second-call".into());
    batch.events.push(second);
    assert_eq!(db.commit(batch).unwrap_err().code, ErrorCode::DbWriteFailed);
    assert_state(&db, false);
}
#[test]
fn cross_session_ledger_and_composite_pointer_are_rejected() {
    let (_directory, db) = setup();
    db.ensure_session(SessionRegistration {
        session_key: "other".into(),
        provider_session_id: Some("other-id".into()),
        parent_key: None,
        parent_provider_id: None,
        created_at_ms: None,
        ledger_id: "other-ledger".into(),
        registered_at_ms: 2,
    })
    .unwrap();
    let mut batch = fixture();
    batch.ledgers.push(LedgerExpectation {
        session_key: "other".into(),
        ledger_id: "other-ledger".into(),
    });
    batch.events[0].ledger_id = "other-ledger".into();
    assert_eq!(
        db.commit(batch).unwrap_err().code,
        ErrorCode::CheckpointConflict
    );
    assert_state(&db, false);
    let invalid = db.write(|conn| {
        conn.execute(
            "UPDATE sessions SET active_ledger_id='other-ledger' WHERE session_key='session'",
            [],
        )?;
        Ok(())
    });
    assert!(invalid.is_err());
    let invalid = db.write(|conn| {
        conn.execute(
            "UPDATE source_files SET current_generation_id='does-not-exist'",
            [],
        )?;
        Ok(())
    });
    assert!(invalid.is_err());
}
#[test]
fn sqlite_disk_full_does_not_advance_batch() {
    let (_directory, db) = setup();
    db.write(|conn| {
        let pages: i64 = conn.pragma_query_value(None, "page_count", |r| r.get(0))?;
        conn.pragma_update(None, "max_page_count", pages)?;
        Ok(())
    })
    .unwrap();
    let mut batch = fixture();
    batch.observations[0].payload_fingerprint = "f".repeat(1024 * 1024);
    assert_eq!(db.commit(batch).unwrap_err().code, ErrorCode::DiskFull);
    assert_state(&db, false);
}
#[test]
fn same_fingerprint_different_physical_calls_are_preserved() {
    let (_directory, db) = setup();
    let mut first = fixture();
    first.next_offset = 50;
    if let NormalizedObservation::Usage(u) = &mut first.observations[0].record {
        u.physical_position.byte_end = 50;
    }
    db.commit(first).unwrap();
    let mut second = fixture();
    second.expected_offset = 50;
    second.expected_checkpoint_revision = 1;
    second.observations[0].observation_id = "second-observation".into();
    second.events[0].event_id = "second-event".into();
    second.events[0].origin_observation_id = "second-observation".into();
    second.streams.clear();
    if let NormalizedObservation::Usage(u) = &mut second.observations[0].record {
        u.physical_position.byte_offset = 50;
    }
    db.commit(second).unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "220"
        );
        Ok(())
    })
    .unwrap();
}
