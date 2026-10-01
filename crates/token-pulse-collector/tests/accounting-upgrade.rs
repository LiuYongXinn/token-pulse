use std::{
    fs,
    time::{Duration, Instant},
};
use token_pulse_collector::{collect_file, jobs::JobService, replay::execute_rebuild};
use token_pulse_core::{domain::ACCOUNTING_VERSION, protocol::JobState};
use token_pulse_store::{Database, ErrorCode, SourceRecord};

fn fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    Database,
    std::path::PathBuf,
    Vec<u8>,
) {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    fs::create_dir_all(logs.path().join("sessions")).unwrap();
    let db = Database::open(data.path()).unwrap();
    db.add_source(SourceRecord {
        source_id: "local".into(),
        root_path: logs.path().to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    let path = logs.path().join("sessions/synthetic.jsonl");
    let bytes = include_bytes!("../../../fixtures/codex-rollout-v1.jsonl").to_vec();
    fs::write(&path, &bytes).unwrap();
    collect_file(&db, "local", &path, 5000).unwrap();
    let owned_fixture_path = db.path().to_owned();
    drop(db);
    // Seed an earlier engine database only while the app is closed. This
    // test connection never accesses source logs or a production directory.
    let fixture_conn = token_pulse_store::rusqlite::Connection::open(owned_fixture_path).unwrap();
    fixture_conn.execute_batch("UPDATE ledger_generations SET accounting_version='previous-engine'; UPDATE stream_states SET baseline_json='{}';").unwrap();
    drop(fixture_conn);
    let db = Database::open(data.path()).unwrap();
    (data, logs, db, path, bytes)
}
fn facts(db: &Database) -> (String, String, i64) {
    db.snapshot(|tx,_|Ok(tx.query_row("SELECT s.active_ledger_id,(SELECT sum_token_decimal(total_tokens) FROM active_usage_events),(SELECT committed_offset FROM file_generations WHERE state='current') FROM sessions s LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?)).unwrap()
}

#[test]
fn worker_upgrades_old_ledgers_without_loading_old_baselines_and_then_collects_append() {
    let (_data, _logs, db, path, mut bytes) = fixture();
    let old = facts(&db);
    let v = |input, cached, output, reason, total| serde_json::json!({"input_tokens":input,"cached_input_tokens":cached,"output_tokens":output,"reasoning_output_tokens":reason,"total_tokens":total});
    bytes.extend_from_slice((serde_json::json!({"timestamp":"1970-01-01T00:00:10Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":v(10,0,0,0,10),"total_token_usage":v(110,40,20,5,130)}}}).to_string()+"\n").as_bytes());
    fs::write(&path, &bytes).unwrap();
    assert_eq!(
        collect_file(&db, "local", &path, 6000).err().unwrap().code,
        ErrorCode::CandidateObsolete
    );
    assert_eq!(facts(&db), old);
    let jobs = JobService::start(db.clone()).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !db
        .list_jobs(10)
        .unwrap()
        .iter()
        .any(|job| job.state == JobState::Succeeded)
    {
        assert!(
            Instant::now() < until,
            "upgrade did not complete: {:?}",
            db.list_jobs(10)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    jobs.shutdown();
    let upgraded = facts(&db);
    assert_ne!(upgraded.0, old.0);
    assert_eq!(upgraded.1, "120");
    assert_eq!(upgraded.2, old.2);
    assert!(db.enqueue_accounting_upgrade(7000).unwrap().is_none());
    collect_file(&db, "local", &path, 8000).unwrap();
    assert_eq!(facts(&db).1, "130");
    assert_eq!(facts(&db).2, bytes.len() as i64);
    db.snapshot(|tx,_| {
        assert_eq!(tx.query_row("SELECT l.accounting_version FROM sessions s JOIN ledger_generations l ON l.ledger_id=s.active_ledger_id LIMIT 1",[],|r|r.get::<_,String>(0))?,ACCOUNTING_VERSION);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM rebuild_audits",[],|r|r.get::<_,i64>(0))?,1); Ok(())
    }).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn failed_prefix_verification_keeps_old_pointer_facts_and_checkpoint() {
    let (_data, _logs, db, path, mut bytes) = fixture();
    let old = facts(&db);
    let job = db.enqueue_accounting_upgrade(6000).unwrap().unwrap();
    bytes[0] = b' ';
    fs::write(&path, &bytes).unwrap();
    assert!(execute_rebuild(&db, &job.job_id, || false, || 7000).is_err());
    assert_eq!(db.get_job(&job.job_id).unwrap().job.state, JobState::Failed);
    assert_eq!(facts(&db), old);
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
