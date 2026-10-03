//! Explicit source rereads against isolated real files; no real user data or benchmarks.
use std::{fs, io::Write, path::Path};
use token_pulse_collector::{collect_file, replay::execute_rebuild};
use token_pulse_core::{
    jobs::{JobRequest, JobScope},
    protocol::{JobKind, JobState},
    reader::{ReaderCheckpoint, ReaderLimits, read_batch},
};
use token_pulse_store::{Database, ErrorCode, SourceRecord, batch::*};

fn source(db: &Database, root: &Path, id: &str) {
    fs::create_dir_all(root.join("sessions")).unwrap();
    db.add_source(SourceRecord {
        source_id: id.into(),
        root_path: root.to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
}
fn request(id: &str, scope: JobScope) -> JobRequest {
    JobRequest {
        kind: JobKind::Rebuild,
        scope,
        request_key: id.into(),
    }
}
fn facts(db: &Database) -> (i64, i64, i64, String) {
    db.snapshot(|tx, r| {
        Ok((
            r.data,
            r.price,
            r.settings,
            tx.query_row(
                "SELECT COALESCE(sum_token_decimal(total_tokens),'0') FROM active_usage_events",
                [],
                |r| r.get(0),
            )?,
        ))
    })
    .unwrap()
}
fn header() -> Vec<u8> {
    b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread\",\"timestamp\":\"2026-10-03T00:00:00Z\"}}\n{\"type\":\"turn_context\",\"payload\":{\"model\":\"synthetic\",\"turn_id\":\"turn\"}}\n".to_vec()
}
fn usage(
    input: i64,
    read: i64,
    write: i64,
    output: i64,
    cumulative: Option<serde_json::Value>,
) -> Vec<u8> {
    let vector = serde_json::json!({"input_tokens":input,"cached_input_tokens":read,"cache_write_tokens":write,"output_tokens":output,"reasoning_output_tokens":0,"total_tokens":input+output});
    let mut bytes = serde_json::to_vec(&serde_json::json!({"timestamp":"2026-10-03T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":vector,"total_token_usage":cumulative.unwrap_or(vector)}}})).unwrap();
    bytes.push(b'\n');
    bytes
}
fn legacy_rejected_checkpoint(db: &Database, path: &Path) {
    // Model the old whitelist rejecting a newly supported write field, via the normal Writer.
    let cp = db
        .file_checkpoint("local", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    let batch = read_batch(
        path,
        &cp.file_generation_id,
        &ReaderCheckpoint {
            file_identity: cp.file_identity,
            observed_size: Some(cp.observed_size as u64),
            committed_offset: cp.committed_offset as u64,
            anchors: cp.anchors,
            oversized_line: None,
            modified_at: None,
        },
        &ReaderLimits::default(),
    )
    .unwrap();
    let mut context = cp.context;
    context.independent_head_available = false;
    db.commit(WriteBatch {
        file_generation_id: cp.file_generation_id,
        expected_offset: cp.committed_offset,
        expected_checkpoint_revision: cp.checkpoint_revision,
        next_offset: batch.next_offset as i64,
        observed_size: batch.observed_size as i64,
        anchors: batch.anchors,
        reader_context: context,
        ledgers: vec![],
        observations: vec![],
        events: vec![],
        streams: vec![],
        provenance: vec![],
        pending: vec![],
        contexts: vec![],
        canonical: vec![],
        diagnostics: vec![],
    })
    .unwrap();
}

#[test]
fn explicit_read_recovers_previously_rejected_writes_and_repeats_without_double_counting() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path(), "local");
    let path = logs.path().join("sessions/legacy.jsonl");
    fs::write(&path, header()).unwrap();
    collect_file(&db, "local", &path, 2).unwrap();
    let first = usage(100, 60, 20, 10, None);
    let cumulative = serde_json::json!({"input_tokens":120,"cached_input_tokens":65,"cache_write_tokens":22,"output_tokens":15,"reasoning_output_tokens":0,"total_tokens":135});
    let second = usage(20, 5, 2, 5, Some(cumulative));
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&[first, second].concat())
        .unwrap();
    legacy_rejected_checkpoint(&db, &path);
    let original = fs::read(&path).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let before = facts(&db);
    assert_eq!(before.3, "0");
    let saved = db
        .file_checkpoint("local", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    // Ordinary ledger rebuild intentionally cannot recover absent normalized observations.
    db.create_job("saved".into(), request("saved", JobScope::All {}), 3)
        .unwrap();
    execute_rebuild(&db, "saved", || false, || 4).unwrap();
    assert_eq!(facts(&db).3, "0");
    for id in ["read-one", "read-two"] {
        let before = facts(&db);
        db.create_source_reread_job(id.into(), request(id, JobScope::All {}), 5)
            .unwrap();
        execute_rebuild(&db, id, || false, || 6).unwrap();
        let after = facts(&db);
        assert_eq!(after.3, "135");
        assert_eq!((after.1, after.2), (before.1, before.2));
        assert_eq!(db.get_job(id).unwrap().job.state, JobState::Succeeded);
        assert!(db.get_job(id).unwrap().checkpoint.reread_sources);
        db.snapshot(|tx, _| {
            assert_eq!(
                tx.query_row(
                    "SELECT SUM(cache_write_input_tokens) FROM active_usage_events",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                22
            );
            assert_eq!(
                tx.query_row("SELECT COUNT(*) FROM active_usage_events", [], |r| r
                    .get::<_, i64>(0))?,
                2
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
    }
    let current = db
        .file_checkpoint("local", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    assert_ne!(current.file_generation_id, saved.file_generation_id);
    assert_eq!(current.committed_offset, original.len() as i64);
    collect_file(&db, "local", &path, 7).unwrap();
    assert_eq!(facts(&db).3, "135");
    drop(db);
    let db = Database::open(data.path()).unwrap();
    assert_eq!(facts(&db).3, "135");
    assert_eq!(
        db.create_source_reread_job("retry".into(), request("read-two", JobScope::All {}), 8)
            .unwrap()
            .job_id,
        "read-two"
    );
    assert_eq!(
        db.create_job(
            "wrong-mode".into(),
            request("read-two", JobScope::All {}),
            8
        )
        .err()
        .unwrap()
        .code,
        ErrorCode::RequestKeyConflict
    );
}

#[test]
fn cancellation_during_bounded_read_releases_owned_candidate_and_preserves_old_results() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path(), "local");
    let path = logs.path().join("sessions/cancel.jsonl");
    let mut original = header();
    for _ in 0..1000 {
        original
            .extend(b"{\"type\":\"response_item\",\"payload\":{\"text\":\"SYNTHETIC_DISCARD\"}}\n");
    }
    original.extend(usage(100, 60, 20, 10, None));
    fs::write(&path, &original).unwrap();
    while collect_file(&db, "local", &path, 2).unwrap().has_more {}
    let before = facts(&db);
    let cp = db
        .file_checkpoint("local", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    db.create_source_reread_job("cancel".into(), request("cancel", JobScope::All {}), 3)
        .unwrap();
    let result = execute_rebuild(
        &db,
        "cancel",
        || {
            if let Some(candidate) = db.active_file_read_candidate(&cp.file_id).unwrap() {
                if candidate.state == "reading" {
                    assert!(db.file_has_frozen_rebuild(&cp.file_id).unwrap());
                    assert_eq!(
                        collect_file(&db, "local", &path, 4).err().unwrap().code,
                        ErrorCode::CheckpointConflict
                    );
                    assert_eq!(
                        token_pulse_collector::replacement::read_replacement_file(
                            &db, "local", &path, 4
                        )
                        .err()
                        .unwrap()
                        .code,
                        ErrorCode::RevisionConflict
                    );
                    db.cancel_job("cancel".into(), 4).unwrap();
                    return true;
                }
            }
            false
        },
        || 5,
    );
    assert_eq!(result.err().unwrap().code, ErrorCode::JobCancelled);
    assert_eq!(facts(&db), before);
    assert!(!db.file_has_frozen_rebuild(&cp.file_id).unwrap());
    assert!(
        db.active_file_read_candidate(&cp.file_id)
            .unwrap()
            .is_none()
    );
    let after = db
        .file_checkpoint("local", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            after.file_generation_id,
            after.committed_offset,
            after.checkpoint_revision
        ),
        (
            cp.file_generation_id,
            cp.committed_offset,
            cp.checkpoint_revision
        )
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    db.create_source_reread_job("retry".into(), request("retry", JobScope::All {}), 6)
        .unwrap();
    execute_rebuild(&db, "retry", || false, || 7).unwrap();
    assert_eq!(facts(&db).3, "110");
}

#[test]
fn partial_tail_or_missing_source_fails_without_publishing_and_does_not_read_unselected_source() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path(), "local");
    source(&db, other.path(), "other");
    let path = logs.path().join("sessions/tail.jsonl");
    let other_path = other.path().join("sessions/other.jsonl");
    let original = [header(), usage(100, 60, 20, 10, None)].concat();
    fs::write(&path, &original).unwrap();
    collect_file(&db, "local", &path, 2).unwrap();
    fs::write(
        &other_path,
        b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"other\"}}\n",
    )
    .unwrap();
    collect_file(&db, "other", &other_path, 2).unwrap();
    fs::remove_file(&other_path).unwrap();
    let scope = JobScope::Sources {
        source_ids: vec!["local".into()],
    };
    db.create_source_reread_job("scoped".into(), request("scoped", scope.clone()), 3)
        .unwrap();
    execute_rebuild(&db, "scoped", || false, || 4).unwrap();
    assert_eq!(facts(&db).3, "110");
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{partial")
        .unwrap();
    let before = facts(&db);
    let cp = db
        .file_checkpoint("local", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    db.create_source_reread_job("tail".into(), request("tail", scope.clone()), 5)
        .unwrap();
    assert_eq!(
        execute_rebuild(&db, "tail", || false, || 6)
            .err()
            .unwrap()
            .code,
        ErrorCode::CheckpointConflict
    );
    assert_eq!(facts(&db), before);
    assert!(
        db.active_file_read_candidate(&cp.file_id)
            .unwrap()
            .is_none()
    );
    fs::remove_file(&path).unwrap();
    db.create_source_reread_job("missing".into(), request("missing", scope), 7)
        .unwrap();
    assert_eq!(
        execute_rebuild(&db, "missing", || false, || 8)
            .err()
            .unwrap()
            .code,
        ErrorCode::SourceUnreadable
    );
    assert_eq!(facts(&db), before);
}

#[test]
fn source_change_after_read_keeps_old_ledger_and_paused_sources_use_saved_history() {
    use std::cell::Cell;
    use token_pulse_store::source_management::SourceMutation;
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path(), "local");
    let path = logs.path().join("sessions/change.jsonl");
    let original = [header(), usage(100, 60, 20, 10, None)].concat();
    fs::write(&path, &original).unwrap();
    collect_file(&db, "local", &path, 2).unwrap();
    let before = facts(&db);
    let cp = db
        .file_checkpoint("local", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    db.create_source_reread_job("changed".into(), request("changed", JobScope::All {}), 3)
        .unwrap();
    let changed = Cell::new(false);
    let result = execute_rebuild(
        &db,
        "changed",
        || {
            if !changed.get()
                && db
                    .active_file_read_candidate(&cp.file_id)
                    .unwrap()
                    .is_some_and(|candidate| candidate.state == "claimed")
            {
                fs::write(&path, b"{}\n").unwrap();
                changed.set(true);
            }
            false
        },
        || 4,
    );
    assert!(changed.get() && result.is_err());
    assert_eq!(facts(&db), before);
    assert!(
        db.active_file_read_candidate(&cp.file_id)
            .unwrap()
            .is_none()
    );
    let revision = facts(&db).2;
    db.mutate_sources(SourceMutation::Pause("local".into()), revision, 5)
        .unwrap();
    fs::remove_file(&path).unwrap();
    db.create_source_reread_job("paused".into(), request("paused", JobScope::All {}), 6)
        .unwrap();
    execute_rebuild(&db, "paused", || false, || 7).unwrap();
    assert_eq!(facts(&db).3, "110");
    assert!(db.rebuild_file_candidates("paused").unwrap().is_empty());
}
