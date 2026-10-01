//! Synthetic rollout inputs with independently listed proof and source expectations.
use std::{fs, path::Path};
use token_pulse_collector::collect_file;
use token_pulse_core::{
    domain::ObservationQuality,
    jobs::{JobProgress, JobRequest, JobScope},
    protocol::{JobKind, JobState},
};
use token_pulse_store::{
    Database, ErrorCode, SourceRecord,
    batch::{PendingEvidence, PendingWrite},
    jobs::JobAdvance,
    rebuild::CandidateBatch,
};

fn header(id: &str) -> String {
    serde_json::json!({"timestamp":"1970-01-01T00:00:00Z","type":"session_meta","payload":{"id":id}}).to_string()+"\n"
}
fn usage(second: u32, last: i64, total: i64) -> String {
    let v = |n| serde_json::json!({"input_tokens":n,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0,"total_tokens":n});
    serde_json::json!({"timestamp":format!("1970-01-01T00:00:{second:02}Z"),"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":v(last),"total_token_usage":v(total)}}}).to_string()+"\n"
}
fn source(db: &Database, root: &Path) {
    fs::create_dir_all(root.join("sessions")).unwrap();
    db.add_source(SourceRecord {
        source_id: "local".into(),
        root_path: root.to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
}
fn start(db: &Database) {
    db.create_job(
        "proof".into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::All {},
            request_key: "proof".into(),
        },
        1,
    )
    .unwrap();
    db.advance_job(
        "proof".into(),
        JobAdvance {
            expected: JobState::Queued,
            next: JobState::Running,
            progress: JobProgress::default(),
            checkpoint: db.get_job("proof").unwrap().checkpoint,
            error: None,
            at_ms: 2,
        },
    )
    .unwrap();
    db.prepare_rebuild("proof".into(), 3).unwrap();
}
fn pending(
    db: &Database,
    ledger: &str,
    observation: &str,
    id: &str,
) -> Result<(), token_pulse_store::StoreError> {
    db.stage_candidate_batch(CandidateBatch {
        job_id: "proof".into(),
        events: vec![],
        streams: vec![],
        provenance: vec![],
        contexts: vec![],
        pending: vec![PendingWrite {
            pending_id: id.into(),
            ledger_id: ledger.into(),
            observation_id: observation.into(),
            quality: ObservationQuality::Duplicate,
            reason_code: "verified_mirror".into(),
            vector: None,
            evidence: PendingEvidence::default(),
        }],
    })
}
#[test]
fn internally_computed_proof_is_stable_invisible_and_cannot_authorize_unrelated_observations() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let a = usage(1, 110, 110);
    let b = usage(2, 25, 135);
    let c = usage(3, 13, 148);
    let primary = logs.path().join("sessions/primary.jsonl");
    let mirror = logs.path().join("sessions/mirror.jsonl");
    let unrelated = logs.path().join("sessions/unrelated.jsonl");
    fs::write(&primary, header("same") + &a + &b).unwrap();
    collect_file(&db, "local", &primary, 5000).unwrap();
    fs::write(&mirror, header("same") + &a + &b + &c).unwrap();
    collect_file(&db, "local", &mirror, 5001).unwrap();
    fs::write(&unrelated, header("different") + &usage(4, 7, 7)).unwrap();
    collect_file(&db, "local", &unrelated, 5002).unwrap();
    let trusted: String = db
        .snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT session_key FROM active_usage_events WHERE total_tokens=110",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    start(&db);
    let result = db.prepare_canonical_replay("proof").unwrap();
    let group = result
        .plan
        .groups
        .iter()
        .find(|g| g.session_key == trusted)
        .unwrap();
    assert_eq!(
        (
            group.primary_record_count,
            group.record_count,
            group.member_sequence_keys.len(),
            group.alias_session_keys.len()
        ),
        (2, 3, 2, 1)
    );
    assert_eq!(
        group.origin_sequence_for(0),
        Some(group.primary_sequence_key.as_str())
    );
    assert_eq!(
        group.origin_sequence_for(2),
        Some(group.longest_sequence_key.as_str())
    );
    assert_eq!(
        result.sequences[&group.primary_sequence_key]
            .physical
            .records
            .len(),
        2
    );
    let again = db.prepare_canonical_replay("proof").unwrap();
    assert_eq!(again.plan, result.plan);
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM session_aliases", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM candidate_session_aliases", [], |r| {
                r.get::<_, i64>(0)
            })?,
            1
        );
        assert_eq!(
            tx.query_row(
                "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "142"
        );
        Ok(())
    })
    .unwrap();
    let m = db.get_rebuild_manifest("proof").unwrap();
    let ledger = &m
        .ledgers
        .iter()
        .find(|l| l.session_key == trusted)
        .unwrap()
        .candidate_ledger_id;
    let foreign = &result.sequences[&group.longest_sequence_key].observation_ids[0];
    pending(&db, ledger, foreign, "proved").unwrap();
    let other = result
        .plan
        .groups
        .iter()
        .find(|g| g.session_key != trusted)
        .unwrap();
    let unproved = &result.sequences[&other.primary_sequence_key].observation_ids[0];
    assert_eq!(
        pending(&db, ledger, unproved, "unproved").unwrap_err().code,
        ErrorCode::CheckpointConflict
    );
    assert_eq!(
        db.prepare_canonical_replay("proof").err().unwrap().code,
        ErrorCode::RevisionConflict
    );
    db.fail_rebuild("proof".into(), ErrorCode::JobCancelled, 6000)
        .unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM session_aliases", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            tx.query_row(
                "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "142"
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        pending(&db, ledger, foreign, "after-cancel")
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
}
#[test]
fn competing_tails_and_input_advance_do_not_persist_alias_authority() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let prefix = header("same") + &usage(1, 110, 110);
    let primary = logs.path().join("sessions/primary.jsonl");
    fs::write(&primary, &prefix).unwrap();
    collect_file(&db, "local", &primary, 5000).unwrap();
    for (name, tail) in [("one", usage(2, 25, 135)), ("two", usage(2, 13, 123))] {
        let path = logs.path().join(format!("sessions/{name}.jsonl"));
        fs::write(&path, prefix.clone() + &tail).unwrap();
        collect_file(&db, "local", &path, 5001).unwrap();
    }
    start(&db);
    let result = db.prepare_canonical_replay("proof").unwrap();
    assert_eq!(result.plan.groups.len(), 1);
    assert_eq!(result.plan.groups[0].record_count, 1);
    assert_eq!(result.plan.isolated.len(), 2);
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM candidate_session_aliases", [], |r| {
                r.get::<_, i64>(0)
            })?,
            0
        );
        Ok(())
    })
    .unwrap();
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(&primary)
        .unwrap()
        .write_all(usage(3, 7, 117).as_bytes())
        .unwrap();
    collect_file(&db, "local", &primary, 5002).unwrap();
    assert_eq!(
        db.prepare_canonical_replay("proof").err().unwrap().code,
        ErrorCode::CandidateObsolete
    );
}
