use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::tests::filter,
};
fn populated() -> (tempfile::TempDir, Database) {
    let (d, db) = setup();
    db.commit(fixture()).unwrap();
    (d, db)
}
fn request() -> BeginFileCandidate {
    BeginFileCandidate {
        generation_id: "replacement".into(),
        file_id: "file".into(),
        expected_generation_id: "generation".into(),
        expected_checkpoint_revision: 1,
        identity: "new-physical-identity".into(),
        observed_size: 100,
        at_ms: 2,
    }
}
fn batch() -> FileCandidateBatch {
    let mut o = fixture().observations.remove(0);
    o.observation_id = "new-observation".into();
    o.session_key = Some("new-session".into());
    if let NormalizedObservation::Usage(u) = &mut o.record {
        u.physical_position.file_generation_id = "replacement".into();
        u.session_key = "new-session".into();
    }
    FileCandidateBatch {
        generation_id: "replacement".into(),
        expected_offset: 0,
        expected_checkpoint_revision: 0,
        next_offset: 100,
        observed_size: 100,
        anchors: vec![],
        context: ReaderContext::default(),
        observations: vec![o],
        diagnostics: vec![],
        at_ms: 3,
    }
}
fn active(db: &Database) -> (i64, i64, i64, String, i64, i64, String) {
    db.snapshot(|tx,r|Ok((r.data,r.price,r.settings,tx.query_row("SELECT current_generation_id FROM source_files WHERE file_id='file'",[],|r|r.get(0))?,tx.query_row("SELECT committed_offset FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get(0))?,tx.query_row("SELECT checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get(0))?,tx.query_row("SELECT sum_token_decimal(total_tokens) FROM active_usage_events",[],|r|r.get(0))?))).unwrap()
}
#[test]
fn isolated_candidate_reopens_without_registering_sessions_or_changing_active_facts() {
    let (d, db) = populated();
    let before = active(&db);
    let c = db.begin_file_read_candidate(request()).unwrap();
    assert_eq!(c.state, "reading");
    assert_eq!(c.checkpoint.committed_offset, 0);
    assert!(c.checkpoint.context.session_key.is_none());
    assert_eq!(db.stage_file_candidate_batch(batch()).unwrap(), 1);
    db.seal_file_read_candidate("replacement".into(), 1, 4)
        .unwrap();
    assert_eq!(active(&db), before);
    db.snapshot(|tx, _| {
        for table in ["sessions", "observations", "usage_events", "stream_states"] {
            assert_eq!(
                tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
        }
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM file_candidate_observations",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.usage_coverage(&filter())
            .unwrap()
            .pending_file_count
            .as_str(),
        "1"
    );
    drop(db);
    let db = Database::open(d.path()).unwrap();
    let c = db.file_read_candidate("replacement").unwrap();
    assert_eq!(c.state, "ready");
    assert_eq!(c.checkpoint.committed_offset, 100);
    assert_eq!(
        c.checkpoint.file_identity.as_deref(),
        Some("new-physical-identity")
    );
    assert_eq!(active(&db), before);
}
#[test]
fn duplicate_begin_reuses_exact_candidate_and_refuses_other_owner_or_payload() {
    let (_d, db) = populated();
    db.begin_file_read_candidate(request()).unwrap();
    db.stage_file_candidate_batch(batch()).unwrap();
    assert_eq!(
        db.begin_file_read_candidate(request())
            .unwrap()
            .checkpoint
            .checkpoint_revision,
        1
    );
    let mut r = request();
    r.generation_id = "another".into();
    assert_eq!(
        db.begin_file_read_candidate(r).err().unwrap().code,
        ErrorCode::RevisionConflict
    );
    let mut r = request();
    r.identity = "changed".into();
    assert_eq!(
        db.begin_file_read_candidate(r).err().unwrap().code,
        ErrorCode::RevisionConflict
    );
    let mut r = request();
    r.expected_checkpoint_revision = 0;
    assert_eq!(
        db.begin_file_read_candidate(r).err().unwrap().code,
        ErrorCode::CheckpointConflict
    );
}
#[test]
fn partial_final_line_and_oversized_skip_do_not_seal_before_eof() {
    let (_d, db) = populated();
    db.begin_file_read_candidate(request()).unwrap();
    let mut b = batch();
    b.next_offset = 0;
    b.observations.clear();
    b.context.oversized_line = Some(token_pulse_core::domain::OversizedLineState {
        start_offset: 0,
        scan_offset: 100,
        anchors: vec![],
    });
    db.stage_file_candidate_batch(b).unwrap();
    assert_eq!(
        db.seal_file_read_candidate("replacement".into(), 1, 4)
            .unwrap_err()
            .code,
        ErrorCode::CheckpointConflict
    );
    let mut b = batch();
    b.expected_checkpoint_revision = 1;
    b.next_offset = 90;
    b.observations.clear();
    b.context = ReaderContext::default();
    db.stage_file_candidate_batch(b).unwrap();
    assert_eq!(
        db.seal_file_read_candidate("replacement".into(), 2, 5)
            .unwrap_err()
            .code,
        ErrorCode::CheckpointConflict
    );
    let mut b = batch();
    b.expected_checkpoint_revision = 2;
    b.expected_offset = 90;
    b.observations.clear();
    db.stage_file_candidate_batch(b).unwrap();
    db.seal_file_read_candidate("replacement".into(), 3, 6)
        .unwrap();
}
#[test]
fn old_checkpoint_path_source_or_parser_changes_obsolete_candidate_without_altering_old_results() {
    for sql in [
        "UPDATE file_generations SET checkpoint_revision=2 WHERE file_generation_id='generation'",
        "UPDATE source_files SET canonical_path='moved.jsonl'",
        "UPDATE sources SET enabled=0",
        "UPDATE sources SET root_path='changed'",
        "UPDATE file_generations SET parser_version='changed' WHERE file_generation_id='generation'",
    ] {
        let (_d, db) = populated();
        db.begin_file_read_candidate(request()).unwrap();
        db.write(move |conn| {
            conn.execute(sql, [])?;
            Ok(())
        })
        .unwrap();
        let before = active(&db);
        assert_eq!(
            db.stage_file_candidate_batch(batch()).unwrap_err().code,
            ErrorCode::CandidateObsolete
        );
        assert_eq!(active(&db), before);
        db.fail_file_read_candidate("replacement".into(), ErrorCode::CandidateObsolete, 5)
            .unwrap();
        assert_eq!(
            db.file_read_candidate("replacement").unwrap().state,
            "failed"
        );
    }
}
#[test]
fn candidate_checkpoint_cas_and_sealed_state_prevent_repeated_staging() {
    let (_d, db) = populated();
    db.begin_file_read_candidate(request()).unwrap();
    db.stage_file_candidate_batch(batch()).unwrap();
    assert_eq!(
        db.stage_file_candidate_batch(batch()).unwrap_err().code,
        ErrorCode::CheckpointConflict
    );
    db.seal_file_read_candidate("replacement".into(), 1, 4)
        .unwrap();
    assert_eq!(
        db.stage_file_candidate_batch(batch()).unwrap_err().code,
        ErrorCode::RevisionConflict
    );
}
#[test]
fn shrinking_candidate_bound_is_a_new_change_even_when_committed_offset_still_fits() {
    let (_d, db) = populated();
    db.begin_file_read_candidate(request()).unwrap();
    let mut b = batch();
    b.observations.clear();
    b.next_offset = 0;
    b.observed_size = 90;
    assert_eq!(
        db.stage_file_candidate_batch(b).unwrap_err().code,
        ErrorCode::CandidateObsolete
    );
    let c = db.file_read_candidate("replacement").unwrap();
    assert_eq!(c.checkpoint.observed_size, 100);
    assert_eq!(c.checkpoint.checkpoint_revision, 0);
}
#[test]
fn staged_observations_diagnostics_and_candidate_checkpoint_rollback_together() {
    let (_d, db) = populated();
    db.begin_file_read_candidate(request()).unwrap();
    let before = active(&db);
    db.write(|conn|{conn.execute_batch("CREATE TRIGGER refuse_candidate_update BEFORE UPDATE ON file_read_candidates BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;Ok(())}).unwrap();
    let mut b = batch();
    b.diagnostics.push(DiagnosticWrite {
        diagnostic_id: "candidate-error".into(),
        source_id: Some("source".into()),
        file_generation_id: Some("replacement".into()),
        byte_offset: Some(0),
        session_key: Some("new-session".into()),
        code: ErrorCode::UnsupportedFormat,
        severity: "warning".into(),
        metadata: Default::default(),
        dedup_key: "candidate-error".into(),
        observed_at_ms: 3,
    });
    assert!(db.stage_file_candidate_batch(b).is_err());
    let c = db.file_read_candidate("replacement").unwrap();
    assert_eq!(c.checkpoint.committed_offset, 0);
    assert_eq!(c.checkpoint.checkpoint_revision, 0);
    assert_eq!(active(&db), before);
    db.snapshot(|tx, _| {
        for table in ["file_candidate_observations", "file_candidate_diagnostics"] {
            assert_eq!(
                tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
                0
            );
        }
        Ok(())
    })
    .unwrap();
}
#[test]
fn bad_positions_overlap_anchor_and_batch_limits_are_rejected_before_progress() {
    let (_d, db) = populated();
    db.begin_file_read_candidate(request()).unwrap();
    let mut b = batch();
    b.observations[0].session_key = Some("other".into());
    assert_eq!(
        db.stage_file_candidate_batch(b).unwrap_err().code,
        ErrorCode::CheckpointConflict
    );
    let mut b = batch();
    b.observations.push(b.observations[0].clone());
    assert_eq!(
        db.stage_file_candidate_batch(b).unwrap_err().code,
        ErrorCode::CheckpointConflict
    );
    let mut b = batch();
    b.anchors.push(ContentAnchor {
        byte_offset: 99,
        byte_length: 2,
        sha256: "a".repeat(64),
    });
    assert_eq!(
        db.stage_file_candidate_batch(b).unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    let mut b = batch();
    b.observations = vec![b.observations[0].clone(); 501];
    assert_eq!(
        db.stage_file_candidate_batch(b).unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.file_read_candidate("replacement")
            .unwrap()
            .checkpoint
            .checkpoint_revision,
        0
    );
}
#[test]
fn failure_releases_owner_and_new_candidate_can_start_while_old_snapshot_keeps_old_state() {
    let (_d, db) = populated();
    db.begin_file_read_candidate(request()).unwrap();
    let before = active(&db);
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT state FROM file_read_candidates", [], |r| r
                .get::<_, String>(0))?,
            "reading"
        );
        db.fail_file_read_candidate("replacement".into(), ErrorCode::JobCancelled, 4)?;
        let mut r = request();
        r.generation_id = "retry".into();
        db.begin_file_read_candidate(r)?;
        assert_eq!(
            tx.query_row("SELECT state FROM file_read_candidates", [], |r| r
                .get::<_, String>(0))?,
            "reading"
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM file_read_candidates", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(active(&db), before);
    assert_eq!(db.file_read_candidate("retry").unwrap().state, "reading");
}
