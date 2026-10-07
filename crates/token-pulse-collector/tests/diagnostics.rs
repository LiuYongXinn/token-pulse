//! Actual read-only rollout adaptation feeding the public, bounded diagnostic query.
use std::fs;
use token_pulse_collector::collect_file;
use token_pulse_core::{
    diagnostics::{DiagnosticKind, DiagnosticsRequest},
    error::ErrorCode,
};
use token_pulse_store::{Database, SourceRecord};

#[test]
fn real_rollout_format_error_has_exact_position_and_no_raw_content() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    fs::create_dir(logs.path().join("sessions")).unwrap();
    let path = logs.path().join("sessions").join("log.jsonl");
    let head = b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"synthetic\"}}\n";
    let raw = b"{\"type\":\"unknown_record\",\"body\":\"do not persist this conversation body\"}\n";
    let bytes = [head.as_slice(), raw.as_slice()].concat();
    fs::write(&path, &bytes).unwrap();
    let db = Database::open(data.path()).unwrap();
    db.add_source(SourceRecord {
        source_id: "fixture".into(),
        root_path: logs.path().to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    collect_file(&db, "fixture", &path, 2).unwrap();
    let value = db
        .diagnostics(&DiagnosticsRequest {
            source_id: Some("fixture".into()),
        })
        .unwrap();
    assert_eq!(value.issues.len(), 1);
    assert!(!value.has_more);
    let issue = &value.issues[0];
    assert_eq!(issue.kind, DiagnosticKind::LogRecord);
    assert_eq!(issue.code, Some(ErrorCode::UnsupportedFormat));
    assert_eq!(issue.path.as_deref(), path.to_str());
    assert_eq!(
        issue.byte_offset.as_ref().unwrap().value(),
        head.len() as i128
    );
    assert!(
        !serde_json::to_string(&value)
            .unwrap()
            .contains("conversation body")
    );
    db.snapshot(|tx,_| {
        let raw_saved:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM diagnostics WHERE metadata_json LIKE '%conversation body%') OR EXISTS(SELECT 1 FROM observations WHERE normalized_json LIKE '%conversation body%')",[],|r|r.get(0))?;
        assert!(!raw_saved); Ok(())
    }).unwrap();
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn auxiliary_diagnostics_are_rechecked_in_bounded_pages_and_unknown_records_remain() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    fs::create_dir(logs.path().join("sessions")).unwrap();
    let path = logs.path().join("sessions/auxiliary.jsonl");
    let mut bytes = "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread\"}}\n".to_owned();
    let mut offsets = vec![];
    for _ in 0..140 {
        offsets.push(bytes.len() as i64);
        bytes.push_str("{\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_settings_applied\",\"thread_settings\":{\"model\":\"synthetic\"}}}\n");
    }
    let usage = serde_json::json!({"input_tokens":100,"cached_input_tokens":60,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":110});
    bytes+=&(serde_json::json!({"timestamp":"1970-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":usage,"total_token_usage":usage}}}).to_string()+"\n");
    bytes += "{\"type\":\"unknown_usage_record\",\"body\":\"private conversation body\"}\n";
    fs::write(&path, &bytes).unwrap();
    let db = Database::open(data.path()).unwrap();
    db.add_source(SourceRecord {
        source_id: "fixture".into(),
        root_path: logs.path().to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    collect_file(&db, "fixture", &path, 1000).unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "110"
        );
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM pending_usage WHERE kind='pending'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        Ok(())
    })
    .unwrap();
    let generation = db
        .file_checkpoint("fixture", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap()
        .file_generation_id;
    let owned_path = db.path().to_owned();
    drop(db);
    let conn = token_pulse_store::rusqlite::Connection::open(owned_path).unwrap();
    for offset in offsets {
        let id = format!("old-{offset}");
        conn.execute("INSERT INTO diagnostics(diagnostic_id,source_id,file_generation_id,byte_offset,code,severity,metadata_json,dedup_key,first_seen_at_ms,last_seen_at_ms) VALUES(?1,'fixture',?2,?3,'UNSUPPORTED_FORMAT','warning','{\"parser_version\":\"codex-rollout-v1\"}',?1,1,1)",token_pulse_store::rusqlite::params![id,generation,offset]).unwrap();
    }
    drop(conn);
    let db = Database::open(data.path()).unwrap();
    let cp = db
        .file_checkpoint("fixture", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    let (positions, more) = db
        .auxiliary_diagnostic_positions(&generation, cp.committed_offset)
        .unwrap();
    assert_eq!(positions.len(), 64);
    assert!(more);
    assert_eq!(
        db.publish_auxiliary_diagnostic_recheck(
            generation.clone(),
            cp.checkpoint_revision + 1,
            vec![(positions[0].diagnostic_id.clone(), true)],
            2000
        )
        .err()
        .unwrap()
        .code,
        ErrorCode::CheckpointConflict
    );
    let mut batches = 0;
    loop {
        batches += 1;
        let receipt = collect_file(&db, "fixture", &path, 3000).unwrap();
        if !receipt.has_more {
            break;
        }
        assert!(batches < 4);
    }
    assert_eq!(batches, 3);
    db.snapshot(|tx,_| {
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM diagnostics WHERE resolved_at_ms IS NULL",[],|r|r.get::<_,i64>(0))?,1);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM diagnostics WHERE resolved_at_ms IS NOT NULL",[],|r|r.get::<_,i64>(0))?,140);
        assert_eq!(tx.query_row("SELECT sum_token_decimal(total_tokens) FROM active_usage_events",[],|r|r.get::<_,String>(0))?,"110");
        assert!(!tx.query_row("SELECT EXISTS(SELECT 1 FROM diagnostics WHERE metadata_json LIKE '%private conversation body%')",[],|r|r.get::<_,bool>(0))?);
        Ok(())
    }).unwrap();
    collect_file(&db, "fixture", &path, 4000).unwrap();
    assert_eq!(
        db.file_checkpoint("fixture", path.to_str().unwrap(), None)
            .unwrap()
            .unwrap()
            .checkpoint_revision,
        cp.checkpoint_revision
    );
    assert_eq!(fs::read_to_string(path).unwrap(), bytes);
}
