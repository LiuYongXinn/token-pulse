use super::*;
use crate::{
    SessionRegistration,
    batch::{self, ObservationWrite},
    file_candidate::{BeginFileCandidate, FileCandidateBatch},
    jobs::JobAdvance,
};
use token_pulse_core::{
    domain::{EffectiveMetadata, NormalizedObservation, PhysicalPosition, ReaderContext},
    jobs::JobProgress,
};

fn advance(db: &Database, expected: JobState, next: JobState) {
    db.advance_job(
        "replacement-job".into(),
        JobAdvance {
            expected,
            next,
            progress: JobProgress::default(),
            checkpoint: db.get_job("replacement-job").unwrap().checkpoint,
            error: None,
            at_ms: 4,
        },
    )
    .unwrap();
}
fn owned(db: &Database, key: &str, provider: &str, empty: bool) {
    owned_with_diagnostics(db, key, provider, empty, vec![]);
}
fn owned_with_diagnostics(
    db: &Database,
    key: &str,
    provider: &str,
    empty: bool,
    diagnostics: Vec<batch::DiagnosticWrite>,
) {
    db.begin_file_read_candidate(BeginFileCandidate {
        generation_id: "replacement".into(),
        file_id: "file".into(),
        expected_generation_id: "generation".into(),
        expected_checkpoint_revision: 1,
        identity: "new-identity".into(),
        observed_size: if empty { 0 } else { 20 },
        at_ms: 2,
    })
    .unwrap();
    let mut observations = vec![];
    let metadata = EffectiveMetadata {
        parent_provider_id: Some("new-parent-provider".into()),
        ..Default::default()
    };
    if !empty {
        observations.push(ObservationWrite {
            observation_id: "new-head".into(),
            session_key: Some(key.into()),
            payload_fingerprint: "head".into(),
            record: NormalizedObservation::SessionMetadata {
                physical_position: PhysicalPosition {
                    file_generation_id: "replacement".into(),
                    byte_offset: 0,
                    byte_end: 10,
                },
                provider_session_id: provider.into(),
                metadata: metadata.clone(),
                created_at_ms: None,
            },
        });
        let mut usage = batch::tests::fixture().observations.remove(0);
        usage.observation_id = "new-usage".into();
        usage.session_key = Some(key.into());
        let NormalizedObservation::Usage(u) = &mut usage.record else {
            unreachable!()
        };
        u.session_key = key.into();
        u.physical_position = PhysicalPosition {
            file_generation_id: "replacement".into(),
            byte_offset: 10,
            byte_end: 20,
        };
        u.effective_metadata = metadata.clone();
        observations.push(usage);
    }
    db.stage_file_candidate_batch(FileCandidateBatch {
        generation_id: "replacement".into(),
        expected_offset: 0,
        expected_checkpoint_revision: 0,
        next_offset: if empty { 0 } else { 20 },
        observed_size: if empty { 0 } else { 20 },
        anchors: vec![],
        context: ReaderContext {
            session_key: if empty { None } else { Some(key.into()) },
            metadata,
            ..Default::default()
        },
        observations,
        diagnostics,
        at_ms: 2,
    })
    .unwrap();
    db.seal_file_read_candidate("replacement".into(), 1, 3)
        .unwrap();
    db.create_job(
        "replacement-job".into(),
        token_pulse_core::jobs::JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::Sources {
                source_ids: vec!["source".into()],
            },
            request_key: "replacement-job".into(),
        },
        3,
    )
    .unwrap();
    db.claim_file_candidate_for_rebuild("replacement".into(), "replacement-job".into(), 1, 3)
        .unwrap();
    advance(db, JobState::Queued, JobState::Running);
    db.register_file_candidate_inputs("replacement".into(), "replacement-job".into(), -1, 4)
        .unwrap();
}
fn planned(db: &Database) -> RebuildManifest {
    db.prepare_rebuild("replacement-job".into(), 4).unwrap()
}
fn derived(db: &Database, m: &RebuildManifest, key: &str) {
    let ledger = &m
        .ledgers
        .iter()
        .find(|l| l.session_key == key)
        .unwrap()
        .candidate_ledger_id;
    let mut event = batch::tests::fixture().events.remove(0);
    event.event_id = "new-event".into();
    event.ledger_id = ledger.clone();
    event.origin_observation_id = "new-usage".into();
    let mut stream = batch::tests::fixture().streams.remove(0);
    stream.ledger_id = ledger.clone();
    stream.observation_id = "new-usage".into();
    db.stage_candidate_batch(CandidateBatch {
        job_id: m.job_id.clone(),
        events: vec![event],
        streams: vec![stream],
        pending: vec![],
        contexts: vec![],
        provenance: vec![],
    })
    .unwrap();
}
fn ready(db: &Database) {
    advance(db, JobState::Running, JobState::Validating);
    db.validate_candidate("replacement-job").unwrap();
    advance(db, JobState::Validating, JobState::Publishing);
}
fn old(tx: &Transaction<'_>) -> StoreResult<()> {
    assert_eq!(
        tx.query_row(
            "SELECT current_generation_id FROM source_files WHERE file_id='file'",
            [],
            |r| r.get::<_, String>(0)
        )?,
        "generation"
    );
    assert_eq!(
        tx.query_row(
            "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
            [],
            |r| r.get::<_, String>(0)
        )?,
        "110"
    );
    assert!(tx.query_row(
        "SELECT active_ledger_id IS NULL FROM sessions WHERE session_key='new'",
        [],
        |r| r.get::<_, bool>(0)
    )?);
    Ok(())
}
#[test]
fn replacement_publication_rollback_and_real_snapshot_keep_files_ledgers_identity_audits_and_revisions_together()
 {
    let (_dir, db) = batch::tests::setup();
    db.commit(batch::tests::fixture()).unwrap();
    owned(&db, "new", "new-provider", false);
    let m = planned(&db);
    derived(&db, &m, "new");
    ready(&db);
    let old_revision = db
        .snapshot(|tx, r| {
            old(tx)?;
            Ok(r)
        })
        .unwrap();
    assert_eq!(
        db.write(
            |conn| super::super::publish(conn, "replacement-job", 5, || Err(
                ErrorCode::DiskFull.into()
            ))
        )
        .unwrap_err()
        .code,
        ErrorCode::DiskFull
    );
    db.snapshot(|tx, r| {
        old(tx)?;
        assert_eq!(r.data, old_revision.data);
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM rebuild_audits", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            tx.query_row("SELECT state FROM file_read_candidates", [], |r| r
                .get::<_, String>(0))?,
            "claimed"
        );
        Ok(())
    })
    .unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let reader = db.clone();
    let handle = std::thread::spawn(move || {
        reader
            .snapshot(|tx, r| {
                old(tx)?;
                entered_tx.send(()).unwrap();
                done_rx.recv().unwrap();
                old(tx)?;
                assert_eq!(r.data, old_revision.data);
                Ok(())
            })
            .unwrap()
    });
    entered_rx.recv().unwrap();
    db.publish_candidate("replacement-job".into(), 5).unwrap();
    done_tx.send(()).unwrap();
    handle.join().unwrap();
    db.snapshot(|tx,r|{
        assert_eq!(r.data,old_revision.data+1);assert_eq!((r.price,r.settings),(old_revision.price,old_revision.settings));
        assert_eq!(tx.query_row("SELECT current_generation_id FROM source_files WHERE file_id='file'",[],|r|r.get::<_,String>(0))?,"replacement");
        assert_eq!(tx.query_row("SELECT committed_offset FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get::<_,i64>(0))?,100);
        assert_eq!(tx.query_row("SELECT checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get::<_,i64>(0))?,1);
        assert!(tx.query_row("SELECT created_at_ms IS NULL AND active_ledger_id IS NOT NULL AND identity_status='confirmed' FROM sessions WHERE session_key='new'",[],|r|r.get::<_,bool>(0))?);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM active_usage_events WHERE session_key='session'",[],|r|r.get::<_,i64>(0))?,0);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM rebuild_audits WHERE old_ledger_id IS NULL",[],|r|r.get::<_,i64>(0))?,1);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM rebuild_audits WHERE committed_data_revision=?1",[r.data],|r|r.get::<_,i64>(0))?,2);Ok(())
    }).unwrap();
    assert_eq!(
        db.file_read_candidate("replacement").unwrap().state,
        "published"
    );
}
#[test]
fn replacement_manifest_includes_old_and_proposed_parent_edges_across_sources_excludes_unowned_null_identity()
 {
    let (_dir, db) = batch::tests::setup();
    db.commit(batch::tests::fixture()).unwrap();
    for (key, provider, parent) in [
        ("old-parent", "old-parent-provider", None),
        ("new-parent", "new-parent-provider", None),
        ("mirror", "provider-session", None),
        ("child", "child-provider", Some("provider-session")),
    ] {
        db.ensure_session(SessionRegistration {
            session_key: key.into(),
            provider_session_id: Some(provider.into()),
            parent_key: None,
            parent_provider_id: parent.map(Into::into),
            created_at_ms: Some(0),
            ledger_id: format!("ledger-{key}"),
            registered_at_ms: 1,
        })
        .unwrap();
    }
    db.write(|conn|{conn.execute("UPDATE sessions SET parent_provider_id='old-parent-provider',parent_key='old-parent' WHERE session_key='session'",[])?;conn.execute("INSERT INTO sessions(session_key,provider,provider_session_id,identity_status,parent_provider_id,created_at_ms) VALUES('foreign','codex','provider-session','candidate','new-parent-provider',100),('new','codex','new-provider','candidate','stale-parent-provider',123)",[])?;Ok(())}).unwrap();
    owned(&db, "new", "new-provider", false);
    let m = planned(&db);
    assert_eq!(
        m.ledgers
            .iter()
            .map(|l| l.session_key.as_str())
            .collect::<Vec<_>>(),
        [
            "child",
            "mirror",
            "new",
            "new-parent",
            "old-parent",
            "session"
        ]
    );
    assert_eq!(
        m.files
            .iter()
            .map(|f| f.generation_id.as_str())
            .collect::<Vec<_>>(),
        ["replacement"]
    );
    assert_eq!(m.replacements[0].sessions[0].created_at_ms, None);
    assert_eq!(
        m.ledgers
            .iter()
            .find(|l| l.session_key == "new")
            .unwrap()
            .created_at_ms,
        Some(123)
    );
    db.ensure_session(SessionRegistration {
        session_key: "late-peer".into(),
        provider_session_id: Some("new-provider".into()),
        parent_key: None,
        parent_provider_id: None,
        created_at_ms: None,
        ledger_id: "late-ledger".into(),
        registered_at_ms: 5,
    })
    .unwrap();
    assert_eq!(
        db.replay_records("replacement-job", "new", None)
            .err()
            .unwrap()
            .code,
        ErrorCode::CandidateObsolete
    );
}
#[test]
fn replacement_frozen_input_rejects_header_cursor_context_old_pointer_and_active_ledger_changes() {
    for mutation in [
        "UPDATE file_rebuild_sessions SET parent_provider_id=NULL",
        "UPDATE file_rebuild_candidates SET after_offset=-1",
        "UPDATE file_generations SET reader_context_json=json_set(reader_context_json,'$.requires_sequence_rebuild',json('true')) WHERE file_generation_id='replacement'",
        "UPDATE file_generations SET checkpoint_revision=2 WHERE file_generation_id='generation'",
        "UPDATE source_files SET canonical_path='moved.jsonl'",
        "UPDATE sources SET enabled=0",
        "UPDATE sessions SET active_ledger_id=NULL WHERE session_key='session'",
        "UPDATE jobs SET resume_json=json_set(resume_json,'$.candidate_ledger_ids',json('[]')) WHERE job_id='replacement-job'",
    ] {
        let (_dir, db) = batch::tests::setup();
        db.commit(batch::tests::fixture()).unwrap();
        owned(&db, "new", "new-provider", false);
        let m = planned(&db);
        derived(&db, &m, "new");
        let mutation = mutation.to_owned();
        db.write(move |conn| {
            conn.execute_batch(&mutation)?;
            Ok(())
        })
        .unwrap();
        advance(&db, JobState::Running, JobState::Validating);
        assert_eq!(
            db.validate_candidate("replacement-job").unwrap_err().code,
            ErrorCode::CandidateObsolete
        );
        assert_eq!(
            db.file_read_candidate("replacement").unwrap().state,
            "claimed"
        );
    }
}
#[test]
fn replacement_diagnostics_publish_atomically_and_eof_does_not_restore_source_scan_proof() {
    let (_dir, db) = batch::tests::setup();
    let diagnostic = |id: &str, generation: &str| batch::DiagnosticWrite {
        diagnostic_id: id.into(),
        source_id: Some("source".into()),
        file_generation_id: Some(generation.into()),
        byte_offset: Some(0),
        session_key: None,
        code: ErrorCode::UnsupportedFormat,
        severity: "warning".into(),
        metadata: batch::DiagnosticMetadata {
            parser_version: Some(PARSER_VERSION.into()),
            ..Default::default()
        },
        dedup_key: id.into(),
        observed_at_ms: 2,
    };
    let mut initial = batch::tests::fixture();
    initial
        .diagnostics
        .push(diagnostic("old-format", "generation"));
    db.commit(initial).unwrap();
    let scan = db
        .begin_source_scan("source".into(), "synthetic".into(), 2)
        .unwrap();
    db.record_source_scan_files(scan.clone(), vec![("synthetic.jsonl".into(), 100)])
        .unwrap();
    db.finish_source_scan(scan.clone(), None, 2).unwrap();
    db.confirm_source_scan_file(scan, "synthetic.jsonl".into(), "generation".into(), 1, 2)
        .unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT state FROM source_scan_state", [], |r| r
                .get::<_, String>(0))?,
            "ready"
        );
        Ok(())
    })
    .unwrap();
    owned_with_diagnostics(
        &db,
        "new",
        "new-provider",
        false,
        vec![diagnostic("new-format", "replacement")],
    );
    let m = planned(&db);
    derived(&db, &m, "new");
    ready(&db);
    db.write(|conn| {
        super::super::publish(conn, "replacement-job", 5, || {
            Err(ErrorCode::DiskFull.into())
        })
    })
    .unwrap_err();
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM diagnostics WHERE resolved_at_ms IS NULL",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        assert!(tx.query_row(
            "SELECT resolved_at_ms IS NULL FROM diagnostics WHERE diagnostic_id='old-format'",
            [],
            |r| r.get::<_, bool>(0)
        )?);
        Ok(())
    })
    .unwrap();
    db.publish_candidate("replacement-job".into(), 5).unwrap();
    db.snapshot(|tx,_|{
        assert_eq!(tx.query_row("SELECT resolved_at_ms FROM diagnostics WHERE diagnostic_id='old-format'",[],|r|r.get::<_,i64>(0))?,5);
        assert_eq!(tx.query_row("SELECT file_generation_id FROM diagnostics WHERE diagnostic_id='new-format' AND resolved_at_ms IS NULL",[],|r|r.get::<_,String>(0))?,"replacement");
        assert_eq!(tx.query_row("SELECT state FROM source_scan_state",[],|r|r.get::<_,String>(0))?,"incomplete");
        assert!(tx.query_row("SELECT file_generation_id IS NULL AND checkpoint_revision IS NULL AND checked_at_ms IS NULL FROM source_scan_files",[],|r|r.get::<_,bool>(0))?);Ok(())
    }).unwrap();
}
#[test]
fn empty_sessionless_replacement_manifest_freezes_without_a_dummy_ledger_and_blocks_late_registration()
 {
    let (_dir, db) = batch::tests::setup();
    db.commit(batch::tests::fixture()).unwrap();
    db.write(|conn| {
        conn.execute("DELETE FROM file_session_bindings", [])?;
        Ok(())
    })
    .unwrap();
    owned(&db, "unused", "unused", true);
    let m = planned(&db);
    assert!(m.ledgers.is_empty());
    assert_eq!(m.replacements.len(), 1);
    assert_eq!(db.get_rebuild_manifest("replacement-job").unwrap(), m);
    assert_eq!(
        db.register_file_candidate_inputs("replacement".into(), "replacement-job".into(), -1, 5)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    assert_eq!(
        db.claim_file_candidate_for_rebuild("replacement".into(), "replacement-job".into(), 1, 5)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    ready(&db);
    db.publish_candidate("replacement-job".into(), 5).unwrap();
    assert_eq!(
        db.file_read_candidate("replacement").unwrap().state,
        "published"
    );
}
#[test]
fn incomplete_registration_cannot_freeze_and_v1_manifest_still_replays_published_input() {
    let (_dir, db) = batch::tests::setup();
    db.commit(batch::tests::fixture()).unwrap();
    owned(&db, "new", "new-provider", false);
    db.write(|conn| {
        conn.execute("UPDATE file_rebuild_candidates SET materialized=0", [])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.prepare_rebuild("replacement-job".into(), 5)
            .unwrap_err()
            .code,
        ErrorCode::CandidateObsolete
    );
    assert!(
        db.get_job("replacement-job")
            .unwrap()
            .checkpoint
            .candidate_ledger_ids
            .is_empty()
    );
    db.fail_rebuild("replacement-job".into(), ErrorCode::JobInterrupted, 6)
        .unwrap();
    let m = super::super::tests::planned(&db);
    db.write(move |conn| {
        let mut value = serde_json::to_value(&m)?;
        value["version"] = serde_json::json!(1);
        value.as_object_mut().unwrap().remove("replacements");
        let text = serde_json::to_string(&value)?;
        conn.execute("DELETE FROM rebuild_manifests WHERE job_id='rebuild'", [])?;
        conn.execute(
            "UPDATE ledger_generations SET input_manifest_json=?1 WHERE ledger_id=?2",
            params![text, m.ledgers[0].candidate_ledger_id],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(db.get_rebuild_manifest("rebuild").unwrap().version, 1);
    assert_eq!(
        db.replay_records("rebuild", "session", None).unwrap().len(),
        1
    );
}
