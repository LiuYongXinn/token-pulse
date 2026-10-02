use super::*;
use crate::{
    FileRegistration,
    batch::{self, DiagnosticMetadata, DiagnosticWrite},
    jobs::JobAdvance,
};
use token_pulse_core::{
    domain::EffectiveMetadata,
    jobs::{JobProgress, JobRequest, JobScope},
    protocol::{JobKind, JobState},
};

fn setup(state: &str) -> (tempfile::TempDir, Database, String) {
    let (dir, db) = batch::tests::setup();
    db.commit(batch::tests::fixture()).unwrap();
    let root = dir.path().join("source");
    let path = root
        .join("sessions")
        .join("old.jsonl")
        .to_str()
        .unwrap()
        .to_owned();
    let path_value = path.clone();
    let root = root.to_str().unwrap().to_owned();
    db.write(move |conn| {
        conn.execute("UPDATE sources SET root_path=?1", [root])?;
        conn.execute("UPDATE source_files SET canonical_path=?1", [path_value])?;
        Ok(())
    })
    .unwrap();
    candidate(&db, "replacement", "file", state == "reading");
    (dir, db, path)
}
fn candidate(db: &Database, generation: &str, file: &str, reading: bool) {
    let old = db
        .snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT current_generation_id FROM source_files WHERE file_id=?1",
                [file],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap();
    db.begin_file_read_candidate(BeginFileCandidate {
        generation_id: generation.into(),
        file_id: file.into(),
        expected_generation_id: old,
        expected_checkpoint_revision: 1,
        identity: "new-physical".into(),
        observed_size: if reading { 20 } else { 10 },
        at_ms: 2,
    })
    .unwrap();
    let metadata = EffectiveMetadata {
        model: Some("white-model".into()),
        ..Default::default()
    };
    db.stage_file_candidate_batch(FileCandidateBatch {
        generation_id: generation.into(),
        expected_offset: 0,
        expected_checkpoint_revision: 0,
        next_offset: 10,
        observed_size: if reading { 20 } else { 10 },
        anchors: vec![ContentAnchor {
            byte_offset: 0,
            byte_length: 10,
            sha256: "b".repeat(64),
        }],
        context: ReaderContext {
            session_key: Some("new".into()),
            metadata: metadata.clone(),
            ..Default::default()
        },
        observations: vec![ObservationWrite {
            observation_id: format!("head-{generation}"),
            session_key: Some("new".into()),
            record: NormalizedObservation::SessionMetadata {
                physical_position: PhysicalPosition {
                    file_generation_id: generation.into(),
                    byte_offset: 0,
                    byte_end: 10,
                },
                provider_session_id: "new-provider".into(),
                metadata,
                created_at_ms: None,
            },
            payload_fingerprint: "necessary-head".into(),
        }],
        diagnostics: vec![DiagnosticWrite {
            diagnostic_id: format!("diagnostic-{generation}"),
            source_id: Some("source".into()),
            file_generation_id: Some(generation.into()),
            byte_offset: Some(0),
            session_key: None,
            code: ErrorCode::UnsupportedFormat,
            severity: "warning".into(),
            metadata: DiagnosticMetadata::default(),
            dedup_key: format!("diagnostic-{generation}"),
            observed_at_ms: 2,
        }],
        at_ms: 2,
    })
    .unwrap();
    if !reading {
        db.seal_file_read_candidate(generation.into(), 1, 3)
            .unwrap();
    }
}
fn request(db: &Database, old_path: &str) -> FileCandidateRelocation {
    let c = db.file_read_candidate("replacement").unwrap();
    FileCandidateRelocation {
        generation_id: "replacement".into(),
        expected_checkpoint_revision: c.checkpoint.checkpoint_revision,
        expected_path: old_path.into(),
        path: Path::new(&c.base.source_root)
            .join("archived_sessions")
            .join("new.jsonl")
            .to_str()
            .unwrap()
            .into(),
        identity: "new-physical".into(),
        at_ms: 4,
    }
}
fn running(db: &Database, manifest: bool) {
    db.create_job(
        "owner".into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::All {},
            request_key: "owner".into(),
        },
        3,
    )
    .unwrap();
    db.claim_file_candidate_for_rebuild("replacement".into(), "owner".into(), 1, 3)
        .unwrap();
    if manifest {
        db.advance_job(
            "owner".into(),
            JobAdvance {
                expected: JobState::Queued,
                next: JobState::Running,
                progress: JobProgress::default(),
                checkpoint: db.get_job("owner").unwrap().checkpoint,
                error: None,
                at_ms: 3,
            },
        )
        .unwrap();
        db.register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 3)
            .unwrap();
        db.prepare_rebuild("owner".into(), 3).unwrap();
    }
}
fn old_total(db: &Database) -> String {
    db.snapshot(|tx, _| {
        Ok(tx.query_row(
            "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
            [],
            |r| r.get(0),
        )?)
    })
    .unwrap()
}
#[test]
fn reading_and_ready_relocation_preserves_spool_context_anchors_and_old_active_input_but_rejects_stale_reader()
 {
    for state in ["reading", "ready"] {
        let (_dir, db, path) = setup(state);
        let before = db.file_read_candidate("replacement").unwrap();
        let revisions = db
            .snapshot(|_, r| Ok((r.data, r.price, r.settings)))
            .unwrap();
        let movement = request(&db, &path);
        let destination = movement.path.clone();
        db.relocate_file_candidate(movement).unwrap();
        let after = db.file_read_candidate("replacement").unwrap();
        assert_eq!(after.state, before.state);
        assert_eq!(after.base.path, destination);
        assert_eq!(
            after.checkpoint.committed_offset,
            before.checkpoint.committed_offset
        );
        assert_eq!(after.checkpoint.context, before.checkpoint.context);
        assert_eq!(after.checkpoint.anchors, before.checkpoint.anchors);
        assert_eq!(
            after.checkpoint.checkpoint_revision,
            before.checkpoint.checkpoint_revision + 1
        );
        db.validate_file_read_candidate("replacement").unwrap();
        db.snapshot(|tx,r|{assert_eq!((r.data,r.price,r.settings),revisions);assert_eq!(tx.query_row("SELECT COUNT(*) FROM file_candidate_observations",[],|r|r.get::<_,i64>(0))?,1);assert_eq!(tx.query_row("SELECT COUNT(*) FROM file_candidate_diagnostics",[],|r|r.get::<_,i64>(0))?,1);assert_eq!(tx.query_row("SELECT current_generation_id FROM source_files",[],|r|r.get::<_,String>(0))?,"generation");assert_eq!(tx.query_row("SELECT committed_offset FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get::<_,i64>(0))?,100);Ok(())}).unwrap();
        assert_eq!(old_total(&db), "110");
        let mut stale = request(&db, &path);
        stale.expected_checkpoint_revision = before.checkpoint.checkpoint_revision;
        assert_eq!(
            db.relocate_file_candidate(stale).unwrap_err().code,
            ErrorCode::CandidateObsolete
        );
    }
}
#[test]
fn candidate_identity_lookup_is_source_scoped_and_rejects_multiple_logical_owners() {
    let (_dir, db, _path) = setup("ready");
    assert!(
        db.file_candidate_at_identity("source", "new-physical")
            .unwrap()
            .is_some()
    );
    assert!(
        db.file_candidate_at_identity("other", "new-physical")
            .unwrap()
            .is_none()
    );
    assert!(
        db.file_candidate_at_identity("source", "unknown")
            .unwrap()
            .is_none()
    );
    let c = db.file_read_candidate("replacement").unwrap();
    db.register_file(FileRegistration {
        file_id: "other-file".into(),
        source_id: "source".into(),
        canonical_path: Path::new(&c.base.source_root)
            .join("sessions")
            .join("other.jsonl")
            .to_str()
            .unwrap()
            .into(),
        file_identity: Some("other-original".into()),
        file_generation_id: "other-generation".into(),
        observed_size: 0,
        created_at_ms: 1,
        reader_context: ReaderContext::default(),
    })
    .unwrap();
    db.write(|conn|{conn.execute("UPDATE file_generations SET checkpoint_revision=1 WHERE file_generation_id='other-generation'",[])?;Ok(())}).unwrap();
    candidate(&db, "other-replacement", "other-file", false);
    assert_eq!(
        db.file_candidate_at_identity("source", "new-physical")
            .err()
            .unwrap()
            .code,
        ErrorCode::AmbiguousUsage
    );
}
#[test]
fn claimed_relocation_fails_the_owner_and_entire_candidate_ledger_group_atomically_without_publishing()
 {
    for manifest in [false, true] {
        let (_dir, db, path) = setup("ready");
        running(&db, manifest);
        db.relocate_file_candidate(request(&db, &path)).unwrap();
        assert_eq!(db.get_job("owner").unwrap().job.state, JobState::Failed);
        assert_eq!(
            db.get_job("owner").unwrap().job.error.map(|e| e.code),
            Some(ErrorCode::CandidateObsolete)
        );
        assert_eq!(
            db.file_read_candidate("replacement").unwrap().state,
            "failed"
        );
        assert_eq!(old_total(&db), "110");
        db.snapshot(|tx, r| {
            assert_eq!(r.data, 1);
            assert_eq!(
                tx.query_row(
                    "SELECT state FROM file_generations WHERE file_generation_id='replacement'",
                    [],
                    |r| r.get::<_, String>(0)
                )?,
                "invalid"
            );
            assert_eq!(
                tx.query_row(
                    "SELECT COUNT(*) FROM ledger_generations WHERE state='candidate'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                0
            );
            assert_eq!(
                tx.query_row("SELECT COUNT(*) FROM rebuild_audits", [], |r| r
                    .get::<_, i64>(0))?,
                0
            );
            if manifest {
                assert!(tx.query_row(
                    "SELECT active_ledger_id IS NULL FROM sessions WHERE session_key='new'",
                    [],
                    |r| r.get::<_, bool>(0)
                )?);
            }
            Ok(())
        })
        .unwrap();
    }
}
#[test]
fn relocation_writer_failure_rolls_back_job_claim_ledger_failures_and_path_together() {
    let (_dir, db, path) = setup("ready");
    running(&db, true);
    db.write(|conn|{conn.execute_batch("CREATE TRIGGER fail_move BEFORE UPDATE OF canonical_path ON source_files BEGIN SELECT RAISE(ABORT,'synthetic movement failure'); END;")?;Ok(())}).unwrap();
    assert!(db.relocate_file_candidate(request(&db, &path)).is_err());
    assert_eq!(db.get_job("owner").unwrap().job.state, JobState::Running);
    assert_eq!(
        db.file_read_candidate("replacement").unwrap().state,
        "claimed"
    );
    assert_eq!(old_total(&db), "110");
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM ledger_generations WHERE state='candidate'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            2
        );
        assert_eq!(
            tx.query_row("SELECT canonical_path FROM source_files", [], |r| r
                .get::<_, String>(0))?,
            path
        );
        Ok(())
    })
    .unwrap();
    db.write(|conn| {
        conn.execute_batch("DROP TRIGGER fail_move")?;
        Ok(())
    })
    .unwrap();
    db.relocate_file_candidate(request(&db, &path)).unwrap();
    assert_eq!(db.get_job("owner").unwrap().job.state, JobState::Failed);
}
#[test]
fn already_failed_candidate_can_identify_move_without_reusing_failed_checkpoint_or_mutating_manifest()
 {
    let (_dir, db, path) = setup("ready");
    running(&db, true);
    let original = db.get_rebuild_manifest("owner").unwrap();
    db.fail_rebuild("owner".into(), ErrorCode::CandidateObsolete, 4)
        .unwrap();
    let before = db.file_read_candidate("replacement").unwrap();
    assert!(
        db.file_candidate_at_identity("source", "new-physical")
            .unwrap()
            .is_some()
    );
    db.relocate_file_candidate(request(&db, &path)).unwrap();
    let after = db.file_read_candidate("replacement").unwrap();
    assert_eq!(after.state, "failed");
    assert_eq!(after.base, before.base);
    assert_eq!(
        after.checkpoint.checkpoint_revision,
        before.checkpoint.checkpoint_revision
    );
    assert_eq!(db.get_rebuild_manifest("owner").unwrap(), original);
    assert_eq!(old_total(&db), "110");
    assert!(
        db.file_candidate_at_identity("source", "new-physical")
            .unwrap()
            .is_none()
    );
}
#[test]
fn relocation_rejects_outside_rollouts_traversal_identity_conflict_and_revision_overflow() {
    let (_dir, db, path) = setup("reading");
    for target in [
        Path::new(&path)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("auth.json"),
        Path::new(&path).parent().unwrap().join("..\\outside.jsonl"),
        Path::new(&path)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("other.jsonl"),
    ] {
        let mut request = request(&db, &path);
        request.path = target.to_str().unwrap().into();
        assert_eq!(
            db.relocate_file_candidate(request).unwrap_err().code,
            ErrorCode::InvalidQuery
        );
        assert_eq!(
            db.file_read_candidate("replacement").unwrap().base.path,
            path
        );
    }
    let mut wrong = request(&db, &path);
    wrong.identity = "other-identity".into();
    assert_eq!(
        db.relocate_file_candidate(wrong).unwrap_err().code,
        ErrorCode::CandidateObsolete
    );
    db.write(|conn|{conn.execute("UPDATE file_generations SET checkpoint_revision=9223372036854775807 WHERE file_generation_id='replacement'",[])?;Ok(())}).unwrap();
    assert_eq!(
        db.relocate_file_candidate(request(&db, &path))
            .unwrap_err()
            .code,
        ErrorCode::NumericOverflow
    );
    assert_eq!(old_total(&db), "110");
}

#[test]
fn occupied_target_and_changed_base_reject_relocation_without_moving_or_failing_owner() {
    let (_dir, db, path) = setup("ready");
    running(&db, true);
    let target = request(&db, &path).path;
    db.register_file(FileRegistration {
        file_id: "occupied-file".into(),
        source_id: "source".into(),
        canonical_path: target,
        file_identity: Some("other-physical".into()),
        file_generation_id: "occupied-generation".into(),
        observed_size: 0,
        created_at_ms: 3,
        reader_context: ReaderContext::default(),
    })
    .unwrap();
    assert_eq!(
        db.relocate_file_candidate(request(&db, &path))
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    assert_eq!(db.get_job("owner").unwrap().job.state, JobState::Running);
    db.write(|conn| {
        conn.execute("UPDATE file_generations SET checkpoint_revision=checkpoint_revision+1 WHERE file_generation_id='generation'", [])?;
        Ok(())
    }).unwrap();
    assert_eq!(
        db.relocate_file_candidate(request(&db, &path))
            .unwrap_err()
            .code,
        ErrorCode::CandidateObsolete
    );
    assert!(
        db.file_candidate_at_identity("source", "new-physical")
            .unwrap()
            .is_none()
    );
    assert_eq!(db.get_job("owner").unwrap().job.state, JobState::Running);
    assert_eq!(
        db.file_read_candidate("replacement").unwrap().base.path,
        path
    );
    assert_eq!(old_total(&db), "110");
}
