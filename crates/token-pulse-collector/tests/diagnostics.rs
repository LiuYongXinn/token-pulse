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
