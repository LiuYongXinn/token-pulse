use std::fs;
use token_pulse_collector::collect_file;
use token_pulse_store::{Database, ErrorCode, SourceRecord};

#[test]
fn historical_import_restarts_at_500_record_boundary_and_preserves_identical_calls() {
    let logs = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    fs::create_dir(logs.path().join("sessions")).unwrap();
    let path = logs.path().join("sessions/long.jsonl");
    let mut bytes =
        b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"synthetic-long\"}}\n".to_vec();
    let call=b"{\"timestamp\":\"1970-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"total_tokens\":1}}}}\n";
    for _ in 0..1001 {
        bytes.extend(call);
    }
    fs::write(&path, &bytes).unwrap();
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
    let first = collect_file(&db, "source", &path, 10).unwrap();
    assert!(first.has_more);
    db.snapshot(|tx, _| {
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))?;
        assert_eq!(count, 500);
        Ok(())
    })
    .unwrap();
    drop(db);
    let db = Database::open(data.path()).unwrap();
    while collect_file(&db, "source", &path, 20).unwrap().has_more {}
    db.snapshot(|tx, _| {
        let (count, total): (i64, String) = tx.query_row(
            "SELECT COUNT(*),sum_token_decimal(total_tokens) FROM active_usage_events",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        assert_eq!(count, 1001);
        assert_eq!(total, "1001");
        let offset: i64 =
            tx.query_row("SELECT committed_offset FROM file_generations", [], |r| {
                r.get(0)
            })?;
        assert_eq!(offset, bytes.len() as i64);
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn collection_rejects_paths_outside_rollout_trees_and_auth_before_open() {
    let logs = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
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
    for path in [
        logs.path().join("auth.json"),
        logs.path().join("unrelated.jsonl"),
        data.path().join("sessions/outside.jsonl"),
    ] {
        assert_eq!(
            collect_file(&db, "source", &path, 1).err().unwrap().code,
            ErrorCode::PermissionDenied
        );
    }
}

#[test]
fn large_effective_metadata_reduces_batch_budget_instead_of_stalling() {
    let logs = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    fs::create_dir(logs.path().join("sessions")).unwrap();
    let path = logs.path().join("sessions/metadata.jsonl");
    let header = serde_json::json!({"type":"session_meta","payload":{"id":"metadata","cwd":format!("E:\\synthetic\\{}","a".repeat(32_000))}});
    let mut bytes = serde_json::to_vec(&header).unwrap();
    bytes.push(b'\n');
    let turn = serde_json::json!({"type":"turn_context","payload":{"model":"m".repeat(32_000)}});
    bytes.extend(serde_json::to_vec(&turn).unwrap());
    bytes.push(b'\n');
    let call=b"{\"timestamp\":\"1970-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"total_tokens\":1}}}}\n";
    for _ in 0..600 {
        bytes.extend(call);
    }
    fs::write(&path, &bytes).unwrap();
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
    assert!(collect_file(&db, "source", &path, 1).unwrap().has_more);
    db.snapshot(|tx, _| {
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))?;
        assert!(count < 500);
        Ok(())
    })
    .unwrap();
    while collect_file(&db, "source", &path, 1).unwrap().has_more {}
    db.snapshot(|tx, _| {
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM usage_events", [], |r| r.get(0))?;
        assert_eq!(count, 600);
        Ok(())
    })
    .unwrap();
}
