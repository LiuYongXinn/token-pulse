use serde_json::{Value, json};
use std::{fs, io::Write, path::Path};
use token_pulse_collector::{collect_file, replay::execute_rebuild};
use token_pulse_core::{
    jobs::{JobRequest, JobScope},
    protocol::JobKind,
};
use token_pulse_store::{Database, SourceRecord};

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
fn vector(input: i64, cache: Option<i64>, output: i64, total: i64) -> Value {
    let mut v = json!({"input_tokens":input,"output_tokens":output,"reasoning_output_tokens":0,"total_tokens":total});
    if let Some(cache) = cache {
        v["cached_input_tokens"] = json!(cache);
    }
    v
}
fn usage(last: Value, cumulative: Value) -> String {
    format!(
        "{}\n",
        json!({"timestamp":"1970-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":last,"total_token_usage":cumulative}}})
    )
}
fn header() -> String {
    format!(
        "{}\n",
        json!({"type":"session_meta","payload":{"id":"thread"}})
    )
}
fn facts(db: &Database) -> (String, i64) {
    db.snapshot(|tx,_| Ok((
        tx.query_row("SELECT COALESCE(sum_token_decimal(total_tokens),'0') FROM active_usage_events",[],|r|r.get(0))?,
        tx.query_row("SELECT COUNT(*) FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id WHERE p.kind='pending'",[],|r|r.get(0))?,
    ))).unwrap()
}
fn rebuild(db: &Database, id: &str, reread: bool) {
    let request = JobRequest {
        kind: JobKind::Rebuild,
        scope: JobScope::All {},
        request_key: id.into(),
    };
    if reread {
        db.create_source_reread_job(id.into(), request, 5000)
            .unwrap();
    } else {
        db.create_job(id.into(), request, 5000).unwrap();
    }
    execute_rebuild(db, id, || false, || 6000).unwrap();
}

#[test]
fn unhinted_refresh_and_bad_last_continue_across_restart_and_both_replay_paths() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let path = logs.path().join("sessions/resilient.jsonl");
    let first = vector(100, Some(60), 10, 110);
    fs::write(&path, header() + &usage(first.clone(), first.clone())).unwrap();
    collect_file(&db, "local", &path, 1000).unwrap();
    drop(db);
    let records = [
        usage(first.clone(), first.clone()),
        usage(vector(0, Some(0), 0, 27), first),
        usage(vector(20, None, 5, 25), vector(120, None, 15, 135)),
        usage(vector(10, Some(5), 5, 999), vector(130, Some(75), 20, 150)),
        usage(vector(5, Some(2), 2, 7), vector(135, Some(77), 22, 157)),
    ];
    for record in records {
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(record.as_bytes())
            .unwrap();
        let db = Database::open(data.path()).unwrap();
        collect_file(&db, "local", &path, 2000).unwrap();
        assert_eq!(facts(&db).1, 0);
    }
    let db = Database::open(data.path()).unwrap();
    assert_eq!(facts(&db), ("157".into(), 0));
    let original = fs::read(&path).unwrap();
    rebuild(&db, "stored", false);
    assert_eq!(facts(&db), ("157".into(), 0));
    rebuild(&db, "reread", true);
    assert_eq!(facts(&db), ("157".into(), 0));
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn unreadable_intervening_record_cannot_be_charged_as_current_delta_on_restart_or_replay() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let path = logs.path().join("sessions/gap.jsonl");
    let first = vector(100, Some(60), 10, 110);
    fs::write(&path, header() + &usage(first.clone(), first)).unwrap();
    collect_file(&db, "local", &path, 1000).unwrap();
    drop(db);
    let tail = "{broken}\n".to_owned()
        + &usage(vector(20, Some(10), 5, 999), vector(120, Some(70), 15, 135))
        + &usage(vector(10, Some(5), 5, 15), vector(130, Some(75), 20, 150));
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(tail.as_bytes())
        .unwrap();
    let db = Database::open(data.path()).unwrap();
    collect_file(&db, "local", &path, 2000).unwrap();
    assert_eq!(facts(&db), ("125".into(), 1));
    rebuild(&db, "stored-gap", false);
    assert_eq!(facts(&db), ("125".into(), 1));
    rebuild(&db, "reread-gap", true);
    assert_eq!(facts(&db), ("125".into(), 1));
}

#[test]
fn v2_pending_refreshes_upgrade_atomically_without_new_consumption() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let path = logs.path().join("sessions/upgrade.jsonl");
    let first = vector(100, Some(60), 10, 110);
    let bytes = header()
        + &usage(first.clone(), first.clone())
        + &usage(first.clone(), first.clone())
        + &usage(vector(0, Some(0), 0, 27), first);
    fs::write(&path, &bytes).unwrap();
    collect_file(&db, "local", &path, 1000).unwrap();
    let owned_path = db.path().to_owned();
    drop(db);
    let conn = token_pulse_store::rusqlite::Connection::open(owned_path).unwrap();
    conn.execute_batch("UPDATE ledger_generations SET accounting_version='accounting-v2'; UPDATE stream_states SET baseline_json='{}'; UPDATE pending_usage SET kind='pending',reason_code='ambiguous_usage' WHERE kind='duplicate';").unwrap();
    drop(conn);
    let db = Database::open(data.path()).unwrap();
    assert_eq!(facts(&db), ("110".into(), 2));
    let job = db.enqueue_accounting_upgrade(2000).unwrap().unwrap();
    db.snapshot(|tx,_| {
        execute_rebuild(&db,&job.job_id,||false,||3000)?;
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id WHERE p.kind='pending'",[],|r|r.get::<_,i64>(0))?,2);
        Ok(())
    }).unwrap();
    assert_eq!(facts(&db), ("110".into(), 0));
    assert!(db.enqueue_accounting_upgrade(4000).unwrap().is_none());
    assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
}
