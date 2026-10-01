use super::*;
use crate::{
    SessionRegistration,
    batch::tests::{fixture, setup},
    jobs::JobAdvance,
};
use token_pulse_core::{
    domain::ObservationQuality,
    jobs::{CancelJobResult, JobCheckpoint, JobProgress, JobRequest},
};

fn change(db: &Database, id: &str, expected: JobState, next: JobState) {
    let stored = db.get_job(id).unwrap();
    db.advance_job(
        id.into(),
        JobAdvance {
            expected,
            next,
            progress: JobProgress::default(),
            checkpoint: stored.checkpoint,
            error: None,
            at_ms: 2,
        },
    )
    .unwrap();
}
fn planned(db: &Database) -> RebuildManifest {
    db.create_job(
        "rebuild".into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::Sessions {
                session_keys: vec!["session".into()],
            },
            request_key: "rebuild-key".into(),
        },
        1,
    )
    .unwrap();
    change(db, "rebuild", JobState::Queued, JobState::Running);
    db.prepare_rebuild("rebuild".into(), 2).unwrap()
}
fn candidate(m: &RebuildManifest) -> CandidateBatch {
    let b = fixture();
    let ledger = &m
        .ledgers
        .iter()
        .find(|l| l.session_key == "session")
        .unwrap()
        .candidate_ledger_id;
    CandidateBatch {
        job_id: m.job_id.clone(),
        events: b
            .events
            .into_iter()
            .map(|mut e| {
                e.ledger_id = ledger.clone();
                e.event_id = "candidate-event".into();
                e
            })
            .collect(),
        streams: b
            .streams
            .into_iter()
            .map(|mut s| {
                s.ledger_id = ledger.clone();
                s
            })
            .collect(),
        provenance: vec![],
        pending: vec![],
        contexts: vec![],
    }
}
#[test]
fn mirror_publication_failure_rolls_back_aliases_observation_keys_bindings_and_readers() {
    use crate::{FileRegistration, batch::ObservationWrite};
    use token_pulse_core::domain::{
        EffectiveMetadata, NormalizedObservation, PhysicalPosition, ReaderContext,
    };
    let (_dir, db) = setup();
    let with_head =
        |mut b: crate::batch::WriteBatch, session: &str, generation: &str, observation: &str| {
            b.file_generation_id = generation.into();
            b.observations[0].observation_id = observation.into();
            b.observations[0].session_key = Some(session.into());
            if let NormalizedObservation::Usage(u) = &mut b.observations[0].record {
                u.session_key = session.into();
                u.physical_position.file_generation_id = generation.into();
                u.physical_position.byte_offset = 10;
            }
            b.observations.insert(
                0,
                ObservationWrite {
                    observation_id: format!("head-{generation}"),
                    session_key: Some(session.into()),
                    payload_fingerprint: format!("header-{generation}"),
                    record: NormalizedObservation::SessionMetadata {
                        physical_position: PhysicalPosition {
                            file_generation_id: generation.into(),
                            byte_offset: 0,
                            byte_end: 10,
                        },
                        provider_session_id: "provider-session".into(),
                        metadata: EffectiveMetadata::default(),
                        created_at_ms: Some(0),
                    },
                },
            );
            b.reader_context = ReaderContext {
                session_key: Some(session.into()),
                ..Default::default()
            };
            b
        };
    db.commit(with_head(fixture(), "session", "generation", "observation"))
        .unwrap();
    db.ensure_session(SessionRegistration {
        session_key: "mirror".into(),
        provider_session_id: Some("provider-session".into()),
        parent_key: None,
        parent_provider_id: None,
        created_at_ms: None,
        ledger_id: "mirror-ledger".into(),
        registered_at_ms: 1,
    })
    .unwrap();
    db.register_file(FileRegistration {
        file_id: "mirror-file".into(),
        source_id: "source".into(),
        canonical_path: "mirror.jsonl".into(),
        file_identity: Some("mirror-identity".into()),
        file_generation_id: "mirror-generation".into(),
        observed_size: 100,
        created_at_ms: 1,
        reader_context: ReaderContext::default(),
    })
    .unwrap();
    let mut mirror = with_head(
        fixture(),
        "mirror",
        "mirror-generation",
        "mirror-observation",
    );
    mirror.events.clear();
    mirror.streams.clear();
    mirror.ledgers = vec![crate::batch::LedgerExpectation {
        session_key: "mirror".into(),
        ledger_id: "mirror-ledger".into(),
    }];
    mirror.pending.push(PendingWrite {
        pending_id: "mirror-pending".into(),
        ledger_id: "mirror-ledger".into(),
        observation_id: "mirror-observation".into(),
        quality: ObservationQuality::Pending,
        reason_code: "lineage_pending".into(),
        vector: None,
        evidence: Default::default(),
    });
    db.commit(mirror).unwrap();
    let m = planned(&db);
    let proof = db.prepare_canonical_replay("rebuild").unwrap();
    assert_eq!(proof.plan.groups[0].alias_session_keys, ["mirror"]);
    let ledger = &m
        .ledgers
        .iter()
        .find(|l| l.session_key == "session")
        .unwrap()
        .candidate_ledger_id;
    let mut b = candidate(&m);
    b.pending.push(PendingWrite {
        pending_id: "candidate-mirror".into(),
        ledger_id: ledger.clone(),
        observation_id: "mirror-observation".into(),
        quality: ObservationQuality::Duplicate,
        reason_code: "verified_mirror".into(),
        vector: None,
        evidence: Default::default(),
    });
    b.provenance.push(ProvenanceWrite {
        event_id: "candidate-event".into(),
        observation_id: "mirror-observation".into(),
        relation: "mirror".into(),
    });
    db.stage_candidate_batch(b).unwrap();
    db.stage_candidate_ordinals(
        "rebuild".into(),
        ledger.clone(),
        0,
        vec!["observation".into()],
    )
    .unwrap();
    db.finish_candidate_alignment("rebuild".into()).unwrap();
    assert_eq!(
        db.stage_candidate_batch(candidate(&m)).unwrap_err().code,
        ErrorCode::RevisionConflict
    );
    change(&db, "rebuild", JobState::Running, JobState::Validating);
    db.validate_candidate("rebuild").unwrap();
    change(&db, "rebuild", JobState::Validating, JobState::Publishing);
    assert_eq!(
        db.write(|conn| publish(conn, "rebuild", 3, || Err(ErrorCode::DiskFull.into())))
            .unwrap_err()
            .code,
        ErrorCode::DiskFull
    );
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 2);
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM session_aliases", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            tx.query_row(
                "SELECT session_key FROM observations WHERE observation_id='mirror-observation'",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "mirror"
        );
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM file_session_bindings WHERE session_key='session'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM rebuild_audits", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        Ok(())
    })
    .unwrap();
    use std::sync::mpsc;
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let reader = db.clone();
    let handle = std::thread::spawn(move || {
        reader.snapshot(|tx,r|{assert_eq!(r.data,2);ready_tx.send(()).unwrap();done_rx.recv().unwrap();assert_eq!(tx.query_row("SELECT COUNT(*) FROM session_aliases",[],|r|r.get::<_,i64>(0))?,0);assert_eq!(tx.query_row("SELECT session_key FROM observations WHERE observation_id='mirror-observation'",[],|r|r.get::<_,String>(0))?,"mirror");Ok(())}).unwrap()
    });
    ready_rx.recv().unwrap();
    db.publish_candidate("rebuild".into(), 4).unwrap();
    done_tx.send(()).unwrap();
    handle.join().unwrap();
    db.snapshot(|tx,r|{assert_eq!(r.data,3);assert_eq!(tx.query_row("SELECT canonical_session_key FROM session_aliases WHERE alias_session_key='mirror'",[],|r|r.get::<_,String>(0))?,"session");assert_eq!(tx.query_row("SELECT session_key FROM observations WHERE observation_id='mirror-observation'",[],|r|r.get::<_,String>(0))?,"session");assert_eq!(tx.query_row("SELECT json_extract(normalized_json,'$.session_key') FROM observations WHERE observation_id='mirror-observation'",[],|r|r.get::<_,String>(0))?,"session");assert_eq!(tx.query_row("SELECT COUNT(*) FROM file_session_bindings WHERE session_key='session'",[],|r|r.get::<_,i64>(0))?,2);assert_eq!(tx.query_row("SELECT COUNT(*) FROM file_session_bindings WHERE session_key='mirror'",[],|r|r.get::<_,i64>(0))?,1);assert_eq!(tx.query_row("SELECT COUNT(*) FROM active_usage_events",[],|r|r.get::<_,i64>(0))?,1);assert_eq!(tx.query_row("SELECT COUNT(*) FROM rebuild_audits",[],|r|r.get::<_,i64>(0))?,2);Ok(())}).unwrap();
    let mut stale = fixture();
    stale.file_generation_id = "mirror-generation".into();
    stale.expected_offset = 100;
    stale.expected_checkpoint_revision = 1;
    stale.observations.clear();
    stale.events.clear();
    stale.streams.clear();
    stale.ledgers = vec![crate::batch::LedgerExpectation {
        session_key: "mirror".into(),
        ledger_id: m
            .ledgers
            .iter()
            .find(|l| l.session_key == "mirror")
            .unwrap()
            .candidate_ledger_id
            .clone(),
    }];
    stale.reader_context = ReaderContext {
        session_key: Some("mirror".into()),
        ..Default::default()
    };
    assert_eq!(
        db.commit(stale).unwrap_err().code,
        ErrorCode::CheckpointConflict
    );
    db.snapshot(|tx,_|{assert_eq!(tx.query_row("SELECT checkpoint_revision FROM file_generations WHERE file_generation_id='mirror-generation'",[],|r|r.get::<_,i64>(0))?,2);assert_eq!(tx.query_row("SELECT json_extract(reader_context_json,'$.session_key') FROM file_generations WHERE file_generation_id='mirror-generation'",[],|r|r.get::<_,String>(0))?,"session");Ok(())}).unwrap();
}
#[test]
fn candidate_is_invisible_and_publication_preserves_a_real_old_read_snapshot() {
    use std::sync::mpsc;
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let m = planned(&db);
    db.stage_candidate_batch(candidate(&m)).unwrap();
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 1);
        let ids: Vec<String> = super::ids(
            tx,
            "SELECT event_id FROM active_usage_events WHERE session_key=?1",
            "session",
        )?;
        assert_eq!(ids, ["event"]);
        Ok(())
    })
    .unwrap();
    change(&db, "rebuild", JobState::Running, JobState::Validating);
    db.validate_candidate("rebuild").unwrap();
    change(&db, "rebuild", JobState::Validating, JobState::Publishing);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (published_tx, published_rx) = mpsc::channel();
    let reader = db.clone();
    let handle = std::thread::spawn(move || {
        reader
            .snapshot(|tx, r| {
                assert_eq!(r.data, 1);
                entered_tx.send(()).unwrap();
                published_rx.recv().unwrap();
                let ids = super::ids(
                    tx,
                    "SELECT event_id FROM active_usage_events WHERE session_key=?1",
                    "session",
                )?;
                assert_eq!(ids, ["event"]);
                Ok(())
            })
            .unwrap()
    });
    entered_rx.recv().unwrap();
    assert_eq!(db.publish_candidate("rebuild".into(), 3).unwrap(), 2);
    published_tx.send(()).unwrap();
    handle.join().unwrap();
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 2);
        let ids = super::ids(
            tx,
            "SELECT event_id FROM active_usage_events WHERE session_key=?1",
            "session",
        )?;
        assert_eq!(ids, ["candidate-event"]);
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM rebuild_audits", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.get_job("rebuild").unwrap().job.state,
        JobState::Succeeded
    );
    assert_eq!(
        db.cancel_job("rebuild".into(), 4).unwrap(),
        CancelJobResult::AlreadyFinished
    );
}
#[test]
fn publish_error_rolls_back_every_pointer_audit_revision_and_job() {
    let (dir, db) = setup();
    db.commit(fixture()).unwrap();
    let m = planned(&db);
    db.stage_candidate_batch(candidate(&m)).unwrap();
    change(&db, "rebuild", JobState::Running, JobState::Validating);
    change(&db, "rebuild", JobState::Validating, JobState::Publishing);
    assert_eq!(
        db.write(|conn| publish(conn, "rebuild", 3, || Err(ErrorCode::DiskFull.into())))
            .unwrap_err()
            .code,
        ErrorCode::DiskFull
    );
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 1);
        assert_eq!(
            super::ids(
                tx,
                "SELECT event_id FROM active_usage_events WHERE session_key=?1",
                "session"
            )?,
            ["event"]
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM rebuild_audits", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.get_job("rebuild").unwrap().job.state,
        JobState::Publishing
    );
    db.fail_rebuild("rebuild".into(), ErrorCode::DiskFull, 4)
        .unwrap();
    assert_eq!(db.get_job("rebuild").unwrap().job.state, JobState::Failed);
}
#[test]
fn incomplete_candidate_and_changed_input_never_replace_old_results() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let m = planned(&db);
    change(&db, "rebuild", JobState::Running, JobState::Validating);
    assert_eq!(
        db.validate_candidate("rebuild").unwrap_err().code,
        ErrorCode::InvalidUsage
    );
    db.fail_rebuild("rebuild".into(), ErrorCode::InvalidUsage, 3)
        .unwrap();
    let mut r = JobRequest {
        kind: JobKind::Rebuild,
        scope: JobScope::All {},
        request_key: "second".into(),
    };
    r.validate().unwrap();
    db.create_job("second".into(), r, 4).unwrap();
    change(&db, "second", JobState::Queued, JobState::Running);
    let mut m2 = db.prepare_rebuild("second".into(), 5).unwrap();
    m2.job_id = "second".into();
    db.stage_candidate_batch(candidate(&m2)).unwrap();
    let mut append = fixture();
    append.expected_offset = 100;
    append.next_offset = 100;
    append.expected_checkpoint_revision = 1;
    append.observed_size = 101; // A newly observed partial tail changes the frozen input upper bound.
    append.observations.clear();
    append.events.clear();
    append.streams.clear();
    db.commit(append).unwrap();
    change(&db, "second", JobState::Running, JobState::Validating);
    assert_eq!(
        db.validate_candidate("second").unwrap_err().code,
        ErrorCode::CandidateObsolete
    );
    assert_eq!(m.files[0].committed_offset, 100);
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 1);
        assert_eq!(
            super::ids(
                tx,
                "SELECT event_id FROM active_usage_events WHERE session_key=?1",
                "session"
            )?,
            ["event"]
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn dependency_closure_includes_mirrors_parent_children_and_late_related_identity_invalidates() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    for (key, provider, parent) in [
        ("mirror", "provider-session", None),
        ("child", "child-id", Some("provider-session")),
        ("unrelated", "unrelated", None),
    ] {
        db.ensure_session(SessionRegistration {
            session_key: key.into(),
            provider_session_id: Some(provider.into()),
            parent_key: None,
            parent_provider_id: parent.map(Into::into),
            created_at_ms: None,
            ledger_id: format!("ledger-{key}"),
            registered_at_ms: 1,
        })
        .unwrap();
    }
    let m = planned(&db);
    assert_eq!(
        m.ledgers
            .iter()
            .map(|l| l.session_key.as_str())
            .collect::<Vec<_>>(),
        ["child", "mirror", "session"]
    );
    db.stage_candidate_batch(candidate(&m)).unwrap();
    db.ensure_session(SessionRegistration {
        session_key: "late-child".into(),
        provider_session_id: Some("late".into()),
        parent_key: None,
        parent_provider_id: Some("child-id".into()),
        created_at_ms: None,
        ledger_id: "late-ledger".into(),
        registered_at_ms: 2,
    })
    .unwrap();
    change(&db, "rebuild", JobState::Running, JobState::Validating);
    assert_eq!(
        db.validate_candidate("rebuild").unwrap_err().code,
        ErrorCode::CandidateObsolete
    );
}
#[test]
fn cancellation_failure_and_unattributed_anchor_are_distinct_from_missing_results() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let m = planned(&db);
    let mut batch = candidate(&m);
    batch.pending.push(PendingWrite {
        pending_id: "anchor".into(),
        ledger_id: m.ledgers[0].candidate_ledger_id.clone(),
        observation_id: "observation".into(),
        quality: ObservationQuality::Unattributed,
        reason_code: "last_new_stream".into(),
        vector: Some(fixture().events[0].usage),
        evidence: Default::default(),
    });
    db.stage_candidate_batch(batch).unwrap();
    change(&db, "rebuild", JobState::Running, JobState::Validating);
    db.validate_candidate("rebuild").unwrap();
    assert_eq!(
        db.cancel_job("rebuild".into(), 3).unwrap(),
        CancelJobResult::Accepted
    );
    assert_eq!(
        db.publish_candidate("rebuild".into(), 4).unwrap_err().code,
        ErrorCode::JobCancelled
    );
    db.fail_rebuild("rebuild".into(), ErrorCode::JobCancelled, 5)
        .unwrap();
    assert_eq!(
        db.get_job("rebuild").unwrap().job.state,
        JobState::Cancelled
    );
    db.snapshot(|tx, _| {
        assert_eq!(
            super::ids(
                tx,
                "SELECT event_id FROM active_usage_events WHERE session_key=?1",
                "session"
            )?,
            ["event"]
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn candidate_staging_cannot_write_an_active_ledger_or_overwrite_a_baseline() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let m = planned(&db);
    let mut invalid = candidate(&m);
    invalid.events[0].ledger_id = "ledger".into();
    assert_eq!(
        db.stage_candidate_batch(invalid).unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    db.stage_candidate_batch(candidate(&m)).unwrap();
    assert_eq!(
        db.stage_candidate_batch(candidate(&m)).unwrap_err().code,
        ErrorCode::RevisionConflict
    );
    let checkpoint: JobCheckpoint = db.get_job("rebuild").unwrap().checkpoint;
    assert_eq!(checkpoint.candidate_ledger_ids.len(), 1);
}
