use std::{fs, io::Write};
use token_pulse_collector::collect_file;
use token_pulse_store::{Database, ErrorCode, SourceRecord};

fn source(db: &Database, id: &str, root: &std::path::Path, enabled: bool) {
    fs::create_dir_all(root.join("sessions")).unwrap();
    fs::create_dir_all(root.join("archived_sessions")).unwrap();
    db.add_source(SourceRecord {
        source_id: id.into(),
        root_path: root.to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled,
        created_at_ms: 1,
    })
    .unwrap();
}
fn total(db: &Database) -> String {
    db.snapshot(|tx, _| {
        Ok(tx.query_row(
            "SELECT COALESCE(sum_token_decimal(total_tokens),'0') FROM active_usage_events",
            [],
            |r| r.get(0),
        )?)
    })
    .unwrap()
}
fn count(db: &Database, table: &str) -> i64 {
    db.snapshot(|tx, _| {
        Ok(tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
    })
    .unwrap()
}

#[test]
fn request_usage_checkpoint_across_batches_and_restart_retains_one_read_only_consumption() {
    use serde_json::json;
    use token_pulse_core::domain::NormalizedObservation;
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, "local", logs.path(), true);
    let path = logs.path().join("sessions/request.jsonl");
    let usage = json!({"input_tokens":272001,"cached_input_tokens":100,"cache_write_input_tokens":50,"output_tokens":10,"reasoning_output_tokens":2,"total_tokens":272011});
    let mut records = vec![
        json!({"type":"session_meta","payload":{"id":"thread","model_provider":"synthetic"}}),
        json!({"type":"turn_context","payload":{"turn_id":"turn","model":"synthetic-model"}}),
    ];
    records.extend((0..497).map(
        |_| json!({"type":"response_item","payload":{"text":"SYNTHETIC_BODY_MUST_NOT_SURVIVE"}}),
    ));
    records.push(json!({"type":"token_usage_record","payload":{"thread_id":"thread","turn_id":"turn","root_turn_id":"root","session_id":"runtime-session","response_id":"response","usage":usage,"turn_token_usage":usage,"thread_token_usage":usage}}));
    records.push(json!({"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":usage,"total_token_usage":usage,"model_context_window":1000000}}}));
    let original: Vec<u8> = records
        .into_iter()
        .flat_map(|mut r| {
            r["timestamp"] = "2026-10-03T00:00:00Z".into();
            let mut line = serde_json::to_vec(&r).unwrap();
            line.push(b'\n');
            line
        })
        .collect();
    fs::write(&path, &original).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let first = collect_file(&db, "local", &path, 5000).unwrap();
    assert!(first.has_more);
    assert_eq!(total(&db), "0");
    let checkpoint = db
        .file_checkpoint("local", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    assert_eq!(
        checkpoint
            .context
            .pending_request_usage
            .as_ref()
            .unwrap()
            .usage
            .input_total,
        Some(272001)
    );
    drop(db);
    let db = Database::open(data.path()).unwrap();
    let second = collect_file(&db, "local", &path, 5001).unwrap();
    assert!(!second.has_more);
    assert_eq!(total(&db), "272011");
    assert_eq!(count(&db, "usage_events"), 1);
    db.snapshot(|tx, _| {
        let mut statement = tx.prepare("SELECT normalized_json FROM observations")?;
        let mut verified = false;
        for row in statement.query_map([], |r| r.get::<_, String>(0))? {
            let text = row?;
            assert!(
                !text.contains("SYNTHETIC_BODY_MUST_NOT_SURVIVE")
                    && !text.contains("runtime-session")
            );
            if let NormalizedObservation::Usage(u) = serde_json::from_str(&text)? {
                assert_eq!(
                    u.request_usage.as_ref().unwrap().usage.input_total,
                    Some(272001)
                );
                assert_eq!(u.request_usage.as_ref().unwrap().response_id, "response");
                assert!(u.request_identity.is_none());
                verified = true;
            }
        }
        assert!(verified);
        Ok(())
    })
    .unwrap();
    assert!(
        db.file_checkpoint("local", path.to_str().unwrap(), None)
            .unwrap()
            .unwrap()
            .context
            .pending_request_usage
            .is_none()
    );
    collect_file(&db, "local", &path, 5002).unwrap();
    assert_eq!(total(&db), "272011");
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(fs::metadata(&path).unwrap().permissions().readonly());
}

#[test]
fn import_restart_append_and_archive_use_real_database_without_double_counting() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let path = logs.path().join("sessions/rollout.jsonl");
    let original = include_bytes!("../../../fixtures/codex-rollout-v1.jsonl");
    let db = Database::open(data.path()).unwrap();
    source(&db, "local", logs.path(), true);
    fs::write(&path, original).unwrap();
    let first = collect_file(&db, "local", &path, 5000).unwrap();
    assert!(first.commit.usage_changed);
    assert!(!first.has_more);
    assert_eq!(total(&db), "120");
    assert_eq!(count(&db, "observations"), 6);
    assert_eq!(count(&db, "usage_events"), 1);
    assert_eq!(count(&db, "context_snapshots"), 1);
    assert_eq!(count(&db, "diagnostics"), 2);
    let checkpoint = db
        .file_checkpoint("local", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    assert_eq!(checkpoint.committed_offset, original.len() as i64);
    assert_eq!(checkpoint.checkpoint_revision, 1);
    assert_eq!(fs::read(&path).unwrap(), original);
    db.snapshot(|tx, _| {
        let mut s = tx.prepare("SELECT normalized_json FROM observations")?;
        for row in s.query_map([], |r| r.get::<_, String>(0))? {
            assert!(!row?.contains("SYNTHETIC_BODY_MUST_NOT_SURVIVE"));
        }
        Ok(())
    })
    .unwrap();
    drop(db);
    let db = Database::open(data.path()).unwrap();
    let repeat = collect_file(&db, "local", &path, 6000).unwrap();
    assert!(!repeat.commit.usage_changed);
    assert_eq!(total(&db), "120");
    let append=br#"{"timestamp":"1970-01-01T00:00:04Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":20,"cached_input_tokens":5,"output_tokens":5,"reasoning_output_tokens":1,"total_tokens":25},"total_token_usage":{"input_tokens":120,"cached_input_tokens":45,"output_tokens":25,"reasoning_output_tokens":6,"total_tokens":145}}}}"#;
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(append).unwrap();
    drop(file);
    collect_file(&db, "local", &path, 7000).unwrap();
    assert_eq!(total(&db), "120");
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"\n")
        .unwrap();
    collect_file(&db, "local", &path, 8000).unwrap();
    assert_eq!(total(&db), "145");
    let archived = logs.path().join("archived_sessions/archived.jsonl");
    fs::rename(&path, &archived).unwrap();
    collect_file(&db, "local", &archived, 9000).unwrap();
    assert_eq!(count(&db, "source_files"), 1);
    assert_eq!(total(&db), "145");
    let cp = db
        .file_checkpoint("local", archived.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    let session = cp.context.session_key.unwrap();
    let state = db.session_accounting(&session).unwrap();
    assert_eq!(state.state.streams.len(), 1);
    let baseline = state.state.streams.values().next().unwrap();
    assert_eq!(baseline.cumulative.reported_total, Some(145));
    assert_eq!(baseline.last_snapshot.unwrap().reported_total, Some(25));
}

#[test]
fn mirror_and_missing_parent_sequences_stay_pending_until_full_proof_publication() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let mirror = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, "local", logs.path(), true);
    source(&db, "mirror", mirror.path(), true);
    let original = include_bytes!("../../../fixtures/codex-rollout-v1.jsonl");
    let a = logs.path().join("sessions/a.jsonl");
    let b = mirror.path().join("sessions/b.jsonl");
    fs::write(&a, original).unwrap();
    fs::write(&b, original).unwrap();
    collect_file(&db, "local", &a, 5000).unwrap();
    collect_file(&db, "mirror", &b, 6000).unwrap();
    assert_eq!(total(&db), "120");
    let cp = db
        .file_checkpoint("mirror", b.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    assert!(cp.context.requires_sequence_rebuild);
    let child = logs.path().join("sessions/child.jsonl");
    let bytes=b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"child\",\"forked_from_id\":\"missing-parent\"}}\n{\"timestamp\":\"1970-01-01T00:00:03Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"total_tokens\":110}}}}\n";
    fs::write(&child, bytes).unwrap();
    collect_file(&db, "local", &child, 7000).unwrap();
    assert_eq!(total(&db), "120");
    db.snapshot(|tx, _| {
        let lineage: i64 = tx.query_row(
            "SELECT COUNT(*) FROM pending_usage WHERE reason_code='lineage_pending'",
            [],
            |r| r.get(0),
        )?;
        assert!(lineage >= 3);
        Ok(())
    })
    .unwrap();
}

#[test]
fn invalid_generation_preserves_old_results_and_disabled_source_is_not_read() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let path = logs.path().join("sessions/a.jsonl");
    let db = Database::open(data.path()).unwrap();
    source(&db, "local", logs.path(), true);
    source(&db, "disabled", logs.path(), false);
    fs::write(
        &path,
        include_bytes!("../../../fixtures/codex-rollout-v1.jsonl"),
    )
    .unwrap();
    collect_file(&db, "local", &path, 5000).unwrap();
    let before = db
        .file_checkpoint("local", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    fs::write(&path, b"{}\n").unwrap();
    assert_eq!(
        collect_file(&db, "local", &path, 6000).err().unwrap().code,
        ErrorCode::CheckpointConflict
    );
    assert_eq!(total(&db), "120");
    assert_eq!(
        db.file_checkpoint("local", path.to_str().unwrap(), None)
            .unwrap()
            .unwrap()
            .committed_offset,
        before.committed_offset
    );
    assert_eq!(
        collect_file(
            &db,
            "disabled",
            &logs.path().join("nonexistent.jsonl"),
            7000
        )
        .err()
        .unwrap()
        .code,
        ErrorCode::PermissionDenied
    );
}
