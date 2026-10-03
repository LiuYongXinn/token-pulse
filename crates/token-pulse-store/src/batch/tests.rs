use super::*;
use crate::{FileRegistration, SessionRegistration, SourceRecord};
use token_pulse_core::domain::{PhysicalPosition, UsageObservation};

pub(crate) fn setup() -> (tempfile::TempDir, Database) {
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
pub(crate) fn fixture() -> WriteBatch {
    let usage = UsageVector {
        input_total: Some(100),
        cache_write_input: None,
        cached_input: Some(60),
        output_total: Some(10),
        reasoning_output: Some(2),
        reported_total: Some(110),
    };
    let observation = UsageObservation {
        request_usage: None,
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
        model_context_window: None,
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
        canonical: vec![],
    }
}
#[test]
fn live_canonical_progress_and_consumption_rollback_with_the_physical_checkpoint() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute(
            "INSERT INTO canonical_usage_sequence VALUES('ledger',0,'observation','event')",
            [],
        )?;
        conn.execute(
            "INSERT INTO file_usage_cursors VALUES('ledger','generation',1,'aligned')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let mut b = fixture();
    b.expected_offset = 100;
    b.expected_checkpoint_revision = 1;
    b.next_offset = 200;
    b.observed_size = 200;
    b.observations[0].observation_id = "next-observation".into();
    let last = UsageVector {
        input_total: Some(20),
        cache_write_input: None,
        cached_input: Some(5),
        output_total: Some(5),
        reasoning_output: Some(1),
        reported_total: Some(25),
    };
    let cumulative = UsageVector {
        input_total: Some(120),
        cache_write_input: None,
        cached_input: Some(65),
        output_total: Some(15),
        reasoning_output: Some(3),
        reported_total: Some(135),
    };
    if let NormalizedObservation::Usage(u) = &mut b.observations[0].record {
        u.physical_position.byte_offset = 100;
        u.physical_position.byte_end = 200;
        u.last = Some(last);
        u.cumulative = Some(cumulative);
    }
    b.events[0].event_id = "next-event".into();
    b.events[0].origin_observation_id = "next-observation".into();
    b.events[0].usage = last;
    b.streams[0].observation_id = "next-observation".into();
    b.streams[0].baseline = cumulative;
    b.streams[0].expected_state_revision = Some(1);
    b.canonical = vec![CanonicalProgressWrite {
        ledger_id: "ledger".into(),
        expected_cursor: 1,
        expected_length: 1,
        steps: vec![CanonicalStep::Append {
            observation_id: "next-observation".into(),
        }],
        requires_rebuild: false,
    }];
    let failed = b.clone();
    assert_eq!(
        db.write(move |conn| commit_batch(conn, failed, |stage| {
            if stage == CommitStage::BeforeCommit {
                Err(ErrorCode::DiskFull.into())
            } else {
                Ok(())
            }
        }))
        .unwrap_err()
        .code,
        ErrorCode::DiskFull
    );
    let assert_old = || {
        db.snapshot(|tx, r| {
            assert_eq!(r.data, 1);
            assert_eq!(
                tx.query_row("SELECT COUNT(*) FROM canonical_usage_sequence", [], |r| r
                    .get::<_, i64>(
                    0
                ))?,
                1
            );
            assert_eq!(
                tx.query_row("SELECT next_ordinal FROM file_usage_cursors", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
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
            Ok(())
        })
        .unwrap()
    };
    assert_old();
    let mut stale = b.clone();
    stale.canonical[0].expected_length = 2;
    assert_eq!(
        db.commit(stale).unwrap_err().code,
        ErrorCode::CheckpointConflict
    );
    assert_old();
    db.commit(b).unwrap();
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 2);
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM canonical_usage_sequence", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            2
        );
        assert_eq!(
            tx.query_row("SELECT next_ordinal FROM file_usage_cursors", [], |r| r
                .get::<_, i64>(0))?,
            2
        );
        assert_eq!(
            tx.query_row("SELECT committed_offset FROM file_generations", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            200
        );
        assert_eq!(
            tx.query_row(
                "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "135"
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn unchanged_poll_keeps_checkpoint_revision_but_size_context_and_stale_inputs_are_checked() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let mut empty = fixture();
    empty.expected_offset = 100;
    empty.expected_checkpoint_revision = 1;
    empty.observations.clear();
    empty.events.clear();
    empty.streams.clear();
    for _ in 0..4 {
        let receipt = db.commit(empty.clone()).unwrap();
        assert_eq!(receipt.checkpoint_revision, 1);
        assert_eq!(receipt.data_revision, 1);
        assert!(!receipt.usage_changed);
    }
    empty.reader_context.metadata.model = Some("synthetic-reader-state-change".into());
    assert_eq!(db.commit(empty.clone()).unwrap().checkpoint_revision, 2);
    empty.expected_checkpoint_revision = 2;
    empty.observed_size = 150;
    assert_eq!(db.commit(empty.clone()).unwrap().checkpoint_revision, 3);
    assert_eq!(
        db.commit(empty.clone()).unwrap_err().code,
        ErrorCode::CheckpointConflict
    );
    empty.expected_checkpoint_revision = 3;
    assert_eq!(db.commit(empty).unwrap().checkpoint_revision, 3);
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 1);
        assert_eq!(
            tx.query_row("SELECT committed_offset FROM file_generations", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            100
        );
        assert_eq!(
            tx.query_row("SELECT observed_size FROM file_generations", [], |r| r
                .get::<_, i64>(0))?,
            150
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn current_episode_is_explicit_even_when_batch_updates_arrive_in_reverse_order() {
    let (_dir, db) = setup();
    let mut batch = fixture();
    batch.next_offset = 200;
    batch.observed_size = 200;
    batch.streams[0].episode_id = "z-old".into();
    let usage = UsageVector {
        input_total: Some(10),
        cache_write_input: None,
        cached_input: Some(4),
        output_total: Some(1),
        reasoning_output: Some(0),
        reported_total: Some(11),
    };
    let mut observation = batch.observations[0].clone();
    observation.observation_id = "second".into();
    if let NormalizedObservation::Usage(u) = &mut observation.record {
        u.physical_position.byte_offset = 100;
        u.physical_position.byte_end = 200;
        u.last = Some(usage);
        u.cumulative = Some(usage);
    }
    batch.observations.push(observation);
    let mut event = batch.events[0].clone();
    event.event_id = "reset-event".into();
    event.origin_observation_id = "second".into();
    event.episode_id = "a-new".into();
    event.usage = usage;
    event.calculation_method = "episode_reset".into();
    batch.events.push(event);
    let mut stream = batch.streams[0].clone();
    stream.episode_id = "a-new".into();
    stream.observation_id = "second".into();
    stream.baseline = usage;
    batch.streams.insert(0, stream);
    db.commit(batch).unwrap();
    let loaded = db.session_accounting("session").unwrap();
    assert_eq!(loaded.state.streams["stream"].episode_id, "a-new");
    assert_eq!(loaded.state.streams["stream"].cumulative, usage);
    assert_eq!(loaded.state.streams["stream"].last_snapshot, Some(usage));
}
fn assert_state(db: &Database, committed: bool) {
    db.snapshot(|tx, revision| {
        assert_eq!(revision.data, i64::from(committed));
        assert_eq!(
            tx.query_row(
                "SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            i64::from(committed) * 2
        );
        for table in [
            "observations",
            "usage_events",
            "stream_states",
            "event_provenance",
            "stream_frontiers",
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
