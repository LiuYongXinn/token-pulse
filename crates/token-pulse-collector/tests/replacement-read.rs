//! Replacement staging and verified publication against real temporary files; no benchmarks.
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
fn anchored_log(id: &str, parent: Option<&str>, values: &[u8]) -> Vec<u8> {
    let mut payload = serde_json::json!({"id":id,"timestamp":"2026-10-01T00:00:00Z"});
    if let Some(parent) = parent {
        payload["forked_from_id"] = serde_json::json!(parent);
    }
    let mut bytes =
        serde_json::to_vec(&serde_json::json!({"type":"session_meta","payload":payload})).unwrap();
    bytes.push(b'\n');
    let mut cumulative = 0u32;
    for value in values {
        cumulative += u32::from(*value);
        bytes.extend(counted_call(*value, cumulative));
    }
    bytes
}
fn counted_call(value: u8, cumulative: u32) -> Vec<u8> {
    let vector = |n: u32| serde_json::json!({"input_tokens":n,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0,"total_tokens":n});
    let mut bytes=serde_json::to_vec(&serde_json::json!({"timestamp":"1970-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":vector(u32::from(value)),"total_token_usage":vector(cumulative)}}})).unwrap();
    bytes.push(b'\n');
    bytes
}
fn manual_rebuild(db: &Database, id: &str) {
    db.create_job(
        id.into(),
        token_pulse_core::jobs::JobRequest {
            kind: token_pulse_core::protocol::JobKind::Rebuild,
            scope: token_pulse_core::jobs::JobScope::All {},
            request_key: id.into(),
        },
        3,
    )
    .unwrap();
    token_pulse_collector::replay::execute_rebuild(db, id, || false, || 4).unwrap();
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
fn queue_replacement(db: &Database, path: &Path) -> (String, String) {
    let r = read_replacement_file(db, "source", path, 2).unwrap();
    assert!(r.read_complete);
    let job = db
        .enqueue_file_candidate_rebuild(r.generation_id.clone(), r.checkpoint_revision, 3)
        .unwrap();
    (job.job_id, r.generation_id)
}
#[test]
fn replacement_executor_publishes_rewrite_truncate_and_new_provider_then_appends_without_recounting()
 {
    use token_pulse_collector::replay::execute_rebuild;
    for (provider, count) in [("old", 1), ("old", 3), ("new", 3)] {
        let (data, _logs, db, path) = setup();
        let before = old(&db, &path);
        let bytes = log(provider, count, 8);
        fs::write(&path, &bytes).unwrap();
        let (job, generation) = queue_replacement(&db, &path);
        let revision = execute_rebuild(&db, &job, || false, || 4).unwrap();
        let after = old(&db, &path);
        assert_eq!(revision, before.0 + 1);
        assert_eq!(after.0, revision);
        assert_eq!((after.1, after.2), (before.1, before.2));
        assert_eq!(after.3, (count * 8).to_string());
        assert_eq!(after.4, bytes.len() as i64);
        assert_eq!(
            db.file_read_candidate(&generation).unwrap().state,
            "published"
        );
        assert_eq!(
            db.get_job(&job).unwrap().job.state,
            token_pulse_core::protocol::JobState::Succeeded
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        db.snapshot(|tx, _| {
            assert_eq!(
                tx.query_row(
                    "SELECT COUNT(*) FROM sessions WHERE active_ledger_id IS NULL",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                0
            );
            assert_eq!(
                tx.query_row(
                    "SELECT COUNT(*) FROM file_generations WHERE state='current'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                1
            );
            assert_eq!(
                tx.query_row(
                    "SELECT COUNT(*) FROM file_generations WHERE state='retired'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                1
            );
            if provider == "new" {
                assert_eq!(
                    tx.query_row(
                        "SELECT COUNT(*) FROM rebuild_audits WHERE old_ledger_id IS NULL",
                        [],
                        |r| r.get::<_, i64>(0)
                    )?,
                    1
                );
            }
            Ok(())
        })
        .unwrap();
        drop(db);
        let db = Database::open(data.path()).unwrap();
        collect_file(&db, "source", &path, 5).unwrap();
        assert_eq!(old(&db, &path).3, (count * 8).to_string());
        append(&path, &call(8));
        collect_file(&db, "source", &path, 6).unwrap();
        assert_eq!(old(&db, &path).3, ((count + 1) * 8).to_string());
    }
}
#[test]
fn replacement_executor_rejects_missing_rewritten_paused_and_cancelled_inputs_preserving_old_facts()
{
    use token_pulse_collector::replay::execute_rebuild;
    for scenario in ["missing", "rewrite", "pause", "cancel"] {
        let (_data, _logs, db, path) = setup();
        let before = old(&db, &path);
        fs::write(&path, log("new", 3, 8)).unwrap();
        let (job, generation) = queue_replacement(&db, &path);
        if scenario == "missing" {
            fs::remove_file(&path).unwrap();
        }
        if scenario == "rewrite" {
            fs::write(&path, log("new", 3, 9)).unwrap();
        }
        if scenario == "pause" {
            db.mutate_sources(SourceMutation::Pause("source".into()), before.2, 3)
                .unwrap();
        }
        let expected = old(&db, &path);
        let result = execute_rebuild(&db, &job, || scenario == "cancel", || 4);
        assert!(result.is_err(), "{scenario}");
        assert_eq!(old(&db, &path), expected);
        assert_eq!(db.file_read_candidate(&generation).unwrap().state, "failed");
    }
}
#[test]
fn empty_replacement_removes_superseded_usage_and_headerless_file_has_a_durable_manifest() {
    use token_pulse_collector::replay::execute_rebuild;
    for initially_empty in [false, true] {
        let (_data, _logs, db, path) = setup();
        if initially_empty {
            fs::write(&path, []).unwrap();
            let (job, _) = queue_replacement(&db, &path);
            execute_rebuild(&db, &job, || false, || 4).unwrap();
            // Use a second, new physical identity with no metadata or session bindings.
            fs::remove_file(&path).unwrap();
            fs::write(&path, b"{\"type\":\"unknown\"}\n").unwrap();
        } else {
            fs::write(&path, []).unwrap();
        }
        let (job, generation) = queue_replacement(&db, &path);
        execute_rebuild(&db, &job, || false, || 5).unwrap();
        assert_eq!(old(&db, &path).3, "0");
        assert_eq!(
            db.file_read_candidate(&generation).unwrap().state,
            "published"
        );
        assert!(
            db.get_rebuild_manifest(&job)
                .unwrap()
                .files
                .iter()
                .any(|f| f.generation_id == generation)
        );
    }
}
#[test]
fn replacement_of_canonical_origin_keeps_verified_mirror_and_isolates_conflicting_new_copy() {
    let (_data, logs, db, path) = setup();
    fs::write(&path, anchored_log("old", None, &[2, 2, 2])).unwrap();
    let (job, _) = queue_replacement(&db, &path);
    token_pulse_collector::replay::execute_rebuild(&db, &job, || false, || 4).unwrap();
    let mirror = logs.path().join("sessions").join("mirror.jsonl");
    fs::write(&mirror, anchored_log("old", None, &[2, 2, 2])).unwrap();
    collect_file(&db, "source", &mirror, 5).unwrap();
    manual_rebuild(&db, "mirrors");
    assert_eq!(old(&db, &path).3, "6");
    let origin_path:String=db.snapshot(|tx,_|Ok(tx.query_row("SELECT f.canonical_path FROM active_usage_events e JOIN observations o ON o.observation_id=e.origin_observation_id JOIN file_generations g ON g.file_generation_id=o.file_generation_id JOIN source_files f ON f.file_id=g.file_id LIMIT 1",[],|r|r.get(0))?)).unwrap();
    let origin = Path::new(&origin_path);
    fs::write(origin, anchored_log("old", None, &[8, 8, 8])).unwrap();
    let (job, generation) = queue_replacement(&db, origin);
    token_pulse_collector::replay::execute_rebuild(&db, &job, || false, || 6).unwrap();
    assert_eq!(old(&db, &path).3, "6");
    db.snapshot(|tx,_| {
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id JOIN observations o ON o.observation_id=p.observation_id WHERE o.file_generation_id=?1 AND p.kind='pending'",[&generation],|r|r.get::<_,i64>(0))?,3);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM active_usage_events e JOIN observations o ON o.observation_id=e.origin_observation_id WHERE o.file_generation_id=?1",[&generation],|r|r.get::<_,i64>(0))?,0);Ok(())
    }).unwrap();
    collect_file(&db, "source", origin, 7).unwrap();
    assert_eq!(old(&db, &path).3, "6");
}
#[test]
fn replacement_child_rebuilds_new_parent_dependency_and_charges_only_its_continuation() {
    let (_data, logs, db, path) = setup();
    fs::write(&path, anchored_log("parent-a", None, &[2, 2])).unwrap();
    let (job, _) = queue_replacement(&db, &path);
    token_pulse_collector::replay::execute_rebuild(&db, &job, || false, || 4).unwrap();
    let parent_b = logs.path().join("sessions").join("parent-b.jsonl");
    let child = logs.path().join("sessions").join("child.jsonl");
    fs::write(&parent_b, anchored_log("parent-b", None, &[8, 8])).unwrap();
    collect_file(&db, "source", &parent_b, 5).unwrap();
    fs::write(&child, anchored_log("child", Some("parent-a"), &[2, 2, 3])).unwrap();
    collect_file(&db, "source", &child, 5).unwrap();
    manual_rebuild(&db, "family");
    assert_eq!(old(&db, &path).3, "23");
    fs::write(&child, anchored_log("child", Some("parent-b"), &[8, 8, 5])).unwrap();
    let (job, generation) = queue_replacement(&db, &child);
    token_pulse_collector::replay::execute_rebuild(&db, &job, || false, || 6).unwrap();
    assert_eq!(old(&db, &path).3, "25");
    db.snapshot(|tx,_| {
        let (parent,provider):(Option<String>,Option<String>)=tx.query_row("SELECT parent_key,parent_provider_id FROM sessions WHERE provider_session_id='child' AND active_ledger_id IS NOT NULL",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        assert_eq!(provider.as_deref(),Some("parent-b"));assert_eq!(tx.query_row("SELECT provider_session_id FROM sessions WHERE session_key=?1",[parent],|r|r.get::<_,String>(0))?,"parent-b");
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id JOIN observations o ON o.observation_id=p.observation_id WHERE o.file_generation_id=?1 AND p.kind='inherited'",[&generation],|r|r.get::<_,i64>(0))?,2);Ok(())
    }).unwrap();
    append(&child, &counted_call(7, 28));
    collect_file(&db, "source", &child, 7).unwrap();
    assert_eq!(old(&db, &path).3, "32");
}
#[test]
fn standalone_job_service_drains_owned_replacement_without_renderer_or_explicit_executor() {
    use std::{sync::Arc, time::Duration};
    let (_data, _logs, db, path) = setup();
    fs::write(&path, log("new", 130, 8)).unwrap();
    let (job, generation) = queue_replacement(&db, &path);
    let (sender, receiver) = std::sync::mpsc::channel();
    let service = token_pulse_collector::jobs::JobService::start_with_notify(
        db.clone(),
        Arc::new(move || {
            let _ = sender.send(());
        }),
    )
    .unwrap();
    receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    service.shutdown();
    assert_eq!(
        db.get_job(&job).unwrap().job.state,
        token_pulse_core::protocol::JobState::Succeeded
    );
    assert_eq!(
        db.file_read_candidate(&generation).unwrap().state,
        "published"
    );
    assert_eq!(old(&db, &path).3, "1040");
}
#[test]
fn completely_sessionless_source_replacement_publishes_without_manufacturing_an_identity() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    fs::create_dir(logs.path().join("sessions")).unwrap();
    let path = logs.path().join("sessions/unknown.jsonl");
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
    fs::write(&path, b"{\"type\":\"unknown\"}\n").unwrap();
    collect_file(&db, "source", &path, 1).unwrap();
    let before = old(&db, &path);
    assert_eq!(count(&db, "sessions"), 0);
    let bytes = b"{\"type\":\"different\"}\n";
    fs::write(&path, bytes).unwrap();
    let (job, generation) = queue_replacement(&db, &path);
    token_pulse_collector::replay::execute_rebuild(&db, &job, || false, || 4).unwrap();
    let manifest = db.get_rebuild_manifest(&job).unwrap();
    assert!(manifest.ledgers.is_empty());
    assert_eq!(manifest.files.len(), 1);
    assert_eq!(
        db.file_read_candidate(&generation).unwrap().state,
        "published"
    );
    assert_eq!(old(&db, &path).0, before.0 + 1);
    assert_eq!(count(&db, "sessions"), 0);
    assert_eq!(count(&db, "ledger_generations"), 0);
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
#[test]
fn append_after_sealing_keeps_frozen_prefix_then_collects_tail_and_readonly_publication_leaves_bytes_unchanged()
 {
    for after_sealing_append in [false, true] {
        let (_data, _logs, db, path) = setup();
        let bytes = log("new", 3, 8);
        fs::write(&path, &bytes).unwrap();
        let (job, generation) = queue_replacement(&db, &path);
        if after_sealing_append {
            append(&path, &call(8));
        }
        let original_permissions = fs::metadata(&path).unwrap().permissions();
        let mut readonly = original_permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        let result = token_pulse_collector::replay::execute_rebuild(&db, &job, || false, || 4);
        fs::set_permissions(&path, original_permissions).unwrap();
        result.unwrap();
        assert_eq!(
            db.file_read_candidate(&generation).unwrap().state,
            "published"
        );
        assert_eq!(old(&db, &path).3, "24");
        assert_eq!(old(&db, &path).4, bytes.len() as i64);
        let expected = if after_sealing_append {
            [bytes.as_slice(), call(8).as_slice()].concat()
        } else {
            bytes
        };
        assert_eq!(fs::read(&path).unwrap(), expected);
        collect_file(&db, "source", &path, 5).unwrap();
        assert_eq!(
            old(&db, &path).3,
            if after_sealing_append { "32" } else { "24" }
        );
    }
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

#[test]
fn readonly_replacement_to_owned_registration_reopens_across_batches_without_publishing_new_usage()
{
    use token_pulse_core::{jobs::JobProgress, protocol::JobState};
    use token_pulse_store::jobs::JobAdvance;
    for provider in ["old", "new"] {
        let (data, _logs, db, path) = setup();
        let before = old(&db, &path);
        let bytes = log(provider, 130, 8);
        fs::write(&path, &bytes).unwrap();
        let read = read_replacement_file(&db, "source", &path, 2).unwrap();
        assert!(read.read_complete);
        let job = db
            .enqueue_file_candidate_rebuild(read.generation_id.clone(), read.checkpoint_revision, 3)
            .unwrap();
        let stored = db.get_job(&job.job_id).unwrap();
        db.advance_job(
            job.job_id.clone(),
            JobAdvance {
                expected: JobState::Queued,
                next: JobState::Running,
                progress: JobProgress::default(),
                checkpoint: stored.checkpoint,
                error: None,
                at_ms: 4,
            },
        )
        .unwrap();
        let first = db
            .register_file_candidate_inputs(read.generation_id.clone(), job.job_id.clone(), -1, 5)
            .unwrap();
        assert_eq!(first.registered_rows, 128);
        assert!(!first.complete);
        assert_eq!(old(&db, &path), before);
        drop(db);
        let db = Database::open(data.path()).unwrap();
        let last = db
            .register_file_candidate_inputs(
                read.generation_id.clone(),
                job.job_id.clone(),
                first.after_offset,
                6,
            )
            .unwrap();
        assert_eq!(last.registered_rows, 3);
        assert!(last.complete);
        assert_eq!(old(&db, &path), before);
        db.snapshot(|tx,_| {
            let new_rows:i64=tx.query_row("SELECT COUNT(*) FROM observations WHERE file_generation_id=?1",[&read.generation_id],|r|r.get(0))?;
            assert_eq!(new_rows,131);
            let (session,created):(String,bool)=tx.query_row("SELECT session_key,created_at_ms IS NULL FROM file_rebuild_sessions WHERE generation_id=?1",[&read.generation_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
            assert!(created);
            if provider=="new" {assert!(tx.query_row("SELECT active_ledger_id IS NULL FROM sessions WHERE session_key=?1",[session],|r|r.get::<_,bool>(0))?);}
            Ok(())
        }).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let manifest = db.prepare_rebuild(job.job_id.clone(), 7).unwrap();
        assert_eq!(manifest.replacements.len(), 1);
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].generation_id, read.generation_id);
        db.fail_rebuild(job.job_id, ErrorCode::JobInterrupted, 8)
            .unwrap();
        assert_eq!(old(&db, &path), before);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
