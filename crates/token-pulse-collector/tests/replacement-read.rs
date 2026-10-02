//! Necessary-observation staging against real temporary files; no active publication or benchmarks.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use token_pulse_collector::{collect_file, replacement::read_replacement_file};
use token_pulse_store::{Database, ErrorCode, SourceRecord, source_management::SourceMutation};
fn call(value: u8) -> Vec<u8> {
    let mut b=serde_json::to_vec(&serde_json::json!({"timestamp":"1970-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":value}}}})).unwrap();
    b.push(b'\n');
    b
}
fn log(id: &str, count: usize, value: u8) -> Vec<u8> {
    let mut b = serde_json::to_vec(&serde_json::json!({"type":"session_meta","payload":{"id":id}}))
        .unwrap();
    b.push(b'\n');
    for _ in 0..count {
        b.extend(call(value));
    }
    b
}
fn setup() -> (tempfile::TempDir, tempfile::TempDir, Database, PathBuf) {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    fs::create_dir(logs.path().join("sessions")).unwrap();
    let path = logs.path().join("sessions").join("change.jsonl");
    fs::write(&path, log("old", 3, 2)).unwrap();
    let db = Database::open(data.path()).unwrap();
    db.add_source(SourceRecord {
        source_id: "source".into(),
        root_path: logs.path().to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    collect_file(&db, "source", &path, 1).unwrap();
    (data, logs, db, path)
}
fn old(db: &Database, path: &Path) -> (i64, i64, i64, String, i64, i64) {
    let (data, price, settings, total) = db
        .snapshot(|tx, r| {
            Ok((
                r.data,
                r.price,
                r.settings,
                tx.query_row(
                    "SELECT COALESCE(sum_token_decimal(total_tokens),'0') FROM active_usage_events",
                    [],
                    |r| r.get::<_, String>(0),
                )?,
            ))
        })
        .unwrap();
    let cp = db
        .file_checkpoint("source", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    (
        data,
        price,
        settings,
        total,
        cp.committed_offset,
        cp.checkpoint_revision,
    )
}
fn count(db: &Database, table: &str) -> i64 {
    db.snapshot(|tx, _| {
        Ok(tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
    })
    .unwrap()
}
fn append(path: &Path, bytes: &[u8]) {
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

#[test]
fn real_truncate_replace_and_same_size_rewrite_only_stage_new_generation() {
    for change in ["truncate", "replace", "same_size"] {
        let (_data, logs, db, path) = setup();
        let before = old(&db, &path);
        let original = fs::read(&path).unwrap();
        if change == "replace" {
            fs::rename(&path, logs.path().join("old-copy.jsonl")).unwrap();
        }
        let bytes = log(
            if change == "truncate" { "old" } else { "new" },
            if change == "truncate" { 1 } else { 3 },
            8,
        );
        if change == "same_size" {
            assert_eq!(bytes.len(), original.len());
        }
        fs::write(&path, &bytes).unwrap();
        let receipt = read_replacement_file(&db, "source", &path, 2).unwrap();
        assert!(receipt.read_complete);
        assert!(!receipt.has_more);
        assert_eq!(old(&db, &path), before);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(count(&db, "sessions"), 1);
        assert_eq!(count(&db, "observations"), 4);
        let c = db.file_read_candidate(&receipt.generation_id).unwrap();
        assert_eq!(c.state, "ready");
        assert_eq!(c.checkpoint.committed_offset, bytes.len() as i64);
        assert_ne!(receipt.generation_id, c.base.generation_id);
        let proposed:String=db.snapshot(|tx,_|Ok(tx.query_row("SELECT json_extract(normalized_json,'$.provider_session_id') FROM file_candidate_observations WHERE byte_offset=0",[],|r|r.get(0))?)).unwrap();
        assert_eq!(proposed, if change == "truncate" { "old" } else { "new" });
    }
}
#[test]
fn unchanged_file_does_not_manufacture_replacement_from_a_writer_conflict() {
    let (_data, _logs, db, path) = setup();
    let before = old(&db, &path);
    assert_eq!(
        read_replacement_file(&db, "source", &path, 2)
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(count(&db, "file_read_candidates"), 0);
    assert_eq!(old(&db, &path), before);
}
#[test]
fn bounded_read_restarts_from_durable_candidate_context_without_touching_old_accounting() {
    let (data, _logs, db, path) = setup();
    let before = old(&db, &path);
    let bytes = log("new", 601, 8);
    fs::write(&path, &bytes).unwrap();
    let r = read_replacement_file(&db, "source", &path, 2).unwrap();
    assert!(!r.read_complete);
    assert!(r.has_more);
    assert_eq!(count(&db, "file_candidate_observations"), 500);
    assert_eq!(old(&db, &path), before);
    let generation = r.generation_id;
    drop(db);
    let db = Database::open(data.path()).unwrap();
    let r = read_replacement_file(&db, "source", &path, 3).unwrap();
    assert_eq!(r.generation_id, generation);
    assert!(r.read_complete);
    assert_eq!(count(&db, "file_candidate_observations"), 602);
    assert_eq!(old(&db, &path), before);
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
#[test]
fn incomplete_tail_and_later_append_continue_same_candidate_including_after_ready() {
    let (_data, _logs, db, path) = setup();
    let mut bytes = log("new", 1, 8);
    let next = call(8);
    bytes.extend(&next[..next.len() - 1]);
    fs::write(&path, bytes).unwrap();
    let r = read_replacement_file(&db, "source", &path, 2).unwrap();
    assert!(!r.read_complete);
    assert!(!r.has_more);
    let generation = r.generation_id;
    append(&path, b"\n");
    let r = read_replacement_file(&db, "source", &path, 3).unwrap();
    assert!(r.read_complete);
    assert_eq!(r.generation_id, generation);
    let revision = r.checkpoint_revision;
    let same = read_replacement_file(&db, "source", &path, 4).unwrap();
    assert_eq!(same.checkpoint_revision, revision);
    assert!(same.read_complete);
    append(&path, &call(8));
    let r = read_replacement_file(&db, "source", &path, 5).unwrap();
    assert!(r.read_complete);
    assert_eq!(r.generation_id, generation);
    assert!(r.checkpoint_revision > revision);
    assert_eq!(count(&db, "file_candidate_observations"), 4);
}
#[test]
fn another_rewrite_fails_candidate_and_retry_creates_new_owner_without_erasing_old_results() {
    let (_data, _logs, db, path) = setup();
    let before = old(&db, &path);
    fs::write(&path, log("new", 601, 8)).unwrap();
    let first = read_replacement_file(&db, "source", &path, 2).unwrap();
    assert!(first.has_more);
    fs::write(&path, log("alt", 1, 9)).unwrap();
    assert_eq!(
        read_replacement_file(&db, "source", &path, 3)
            .err()
            .unwrap()
            .code,
        ErrorCode::CheckpointConflict
    );
    assert_eq!(
        db.file_read_candidate(&first.generation_id).unwrap().state,
        "failed"
    );
    assert_eq!(old(&db, &path), before);
    let next = read_replacement_file(&db, "source", &path, 4).unwrap();
    assert!(next.read_complete);
    assert_ne!(next.generation_id, first.generation_id);
    assert_eq!(old(&db, &path), before);
}
#[test]
fn parser_body_and_unknown_raw_record_stay_out_of_persistence_on_readonly_source() {
    let (_data, _logs, db, path) = setup();
    let mut bytes = log("new", 1, 8);
    bytes.extend(b"{\"type\":\"response_item\",\"payload\":{\"content\":\"SYNTHETIC_CHAT_BODY_DO_NOT_SAVE\"}}\n{\"type\":\"unknown_layout\",\"secret\":\"SYNTHETIC_CHAT_BODY_DO_NOT_SAVE\"}\n");
    fs::write(&path, &bytes).unwrap();
    let original_permissions = fs::metadata(&path).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let result = read_replacement_file(&db, "source", &path, 2);
    let remains_readonly = fs::metadata(&path).unwrap().permissions().readonly();
    fs::set_permissions(&path, original_permissions).unwrap();
    let r = result.unwrap();
    assert!(remains_readonly);
    assert!(r.read_complete);
    assert_eq!(count(&db, "file_candidate_observations"), 2);
    assert_eq!(count(&db, "file_candidate_diagnostics"), 1);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    db.snapshot(|tx, _| {
        for (table, col) in [
            ("file_candidate_observations", "normalized_json"),
            ("file_candidate_diagnostics", "diagnostic_json"),
        ] {
            let mut s = tx.prepare(&format!("SELECT {col} FROM {table}"))?;
            for row in s.query_map([], |r| r.get::<_, String>(0))? {
                assert!(!row?.contains("SYNTHETIC_CHAT_BODY_DO_NOT_SAVE"));
            }
        }
        Ok(())
    })
    .unwrap();
}
#[test]
fn paused_and_outside_rollout_paths_are_rejected_before_candidate_creation() {
    let (_data, logs, db, path) = setup();
    let outside = logs.path().join("outside.jsonl");
    fs::write(&outside, log("new", 1, 8)).unwrap();
    assert_eq!(
        read_replacement_file(&db, "source", &outside, 2)
            .err()
            .unwrap()
            .code,
        ErrorCode::PermissionDenied
    );
    let revision = db.snapshot(|_, r| Ok(r.settings)).unwrap();
    db.mutate_sources(SourceMutation::Pause("source".into()), revision, 3)
        .unwrap();
    fs::write(&path, log("new", 1, 8)).unwrap();
    assert_eq!(
        read_replacement_file(&db, "source", &path, 4)
            .err()
            .unwrap()
            .code,
        ErrorCode::PermissionDenied
    );
    assert_eq!(count(&db, "file_read_candidates"), 0);
}
#[test]
fn large_effective_metadata_uses_smaller_transaction_slices_and_eventually_seals() {
    let (_data, _logs, db, path) = setup();
    let header = serde_json::json!({"type":"session_meta","payload":{"id":"new","cwd":format!("E:\\synthetic\\{}","a".repeat(32000))}});
    let mut bytes = serde_json::to_vec(&header).unwrap();
    bytes.push(b'\n');
    bytes.extend(
        serde_json::to_vec(
            &serde_json::json!({"type":"turn_context","payload":{"model":"m".repeat(32000)}}),
        )
        .unwrap(),
    );
    bytes.push(b'\n');
    for _ in 0..600 {
        bytes.extend(call(8));
    }
    fs::write(&path, &bytes).unwrap();
    let first = read_replacement_file(&db, "source", &path, 2).unwrap();
    assert!(first.has_more);
    assert!(count(&db, "file_candidate_observations") < 500);
    for _ in 0..10 {
        if read_replacement_file(&db, "source", &path, 3)
            .unwrap()
            .read_complete
        {
            assert_eq!(count(&db, "file_candidate_observations"), 602);
            return;
        }
    }
    panic!("bounded candidate did not converge");
}
#[test]
fn oversized_line_skip_survives_a_batch_boundary_without_persisting_raw_bytes() {
    let (_data, _logs, db, path) = setup();
    let before = old(&db, &path);
    let mut bytes = log("new", 0, 8);
    bytes.extend(std::iter::repeat_n(b'x', 17 * 1024 * 1024));
    bytes.push(b'\n');
    bytes.extend(call(8));
    fs::write(&path, bytes).unwrap();
    let first = read_replacement_file(&db, "source", &path, 2).unwrap();
    assert!(first.has_more);
    assert!(!first.read_complete);
    let c = db.file_read_candidate(&first.generation_id).unwrap();
    assert!(c.checkpoint.context.oversized_line.is_some());
    assert_eq!(count(&db, "file_candidate_observations"), 1);
    let second = read_replacement_file(&db, "source", &path, 3).unwrap();
    assert_eq!(second.generation_id, first.generation_id);
    assert!(second.read_complete);
    assert_eq!(count(&db, "file_candidate_observations"), 2);
    assert_eq!(count(&db, "file_candidate_diagnostics"), 1);
    assert_eq!(old(&db, &path), before);
}
