use token_pulse_store::{Database, SourceRecord};

#[test]
fn registration_and_database_transactions_never_write_source_directory() {
    let root = tempfile::tempdir().unwrap();
    let logs = root.path().join("codex-source");
    std::fs::create_dir(&logs).unwrap();
    let source_log = logs.join("rollout.jsonl");
    let original = b"synthetic original source bytes\n";
    std::fs::write(&source_log, original).unwrap();
    let data = root.path().join("application-data");
    let db = Database::open(&data).unwrap();
    db.add_source(SourceRecord {
        source_id: "source".into(),
        root_path: logs.to_string_lossy().into_owned(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    drop(db);
    assert_eq!(std::fs::read(source_log).unwrap(), original);
    assert_eq!(std::fs::read_dir(logs).unwrap().count(), 1);
    assert!(data.join("token-pulse.db").is_file());
}
