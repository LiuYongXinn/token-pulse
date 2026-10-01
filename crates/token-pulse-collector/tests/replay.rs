use std::{fs, path::Path};
use token_pulse_collector::{collect_file, replay::execute_rebuild};
use token_pulse_core::{
    jobs::{JobRequest, JobScope},
    protocol::{JobKind, JobState},
};
use token_pulse_store::{Database, ErrorCode, SourceRecord};

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
fn job(db: &Database, id: &str) {
    db.create_job(
        id.into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::All {},
            request_key: id.into(),
        },
        1,
    )
    .unwrap();
}
fn header(id: &str, parent: Option<&str>) -> String {
    serde_json::json!({"timestamp":"1970-01-01T00:00:00Z","type":"session_meta","payload":{"id":id,"forked_from_id":parent}}).to_string()+"\n"
}
fn usage(second: u32, last: [i64; 5], cumulative: [i64; 5]) -> String {
    let vector = |v: [i64; 5]| serde_json::json!({"input_tokens":v[0],"cached_input_tokens":v[1],"output_tokens":v[2],"reasoning_output_tokens":v[3],"total_tokens":v[4]});
    serde_json::json!({"timestamp":format!("1970-01-01T00:00:{second:02}Z"),"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":vector(last),"total_token_usage":vector(cumulative)}}}).to_string()+"\n"
}
#[test]
fn rebuild_replays_real_observations_and_source_absence_does_not_erase_history() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let path = logs.path().join("sessions/source.jsonl");
    let original = include_bytes!("../../../fixtures/codex-rollout-v1.jsonl");
    fs::write(&path, original).unwrap();
    collect_file(&db, "local", &path, 5000).unwrap();
    assert_eq!(total(&db), "120");
    job(&db, "first");
    execute_rebuild(&db, "first", || false, || 6000).unwrap();
    assert_eq!(total(&db), "120");
    assert_eq!(fs::read(&path).unwrap(), original);
    db.snapshot(|tx, _| {
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM active_usage_events", [], |r| r.get(0))?;
        assert_eq!(count, 1);
        Ok(())
    })
    .unwrap();
    fs::remove_file(&path).unwrap();
    job(&db, "offline");
    execute_rebuild(&db, "offline", || false, || 7000).unwrap();
    assert_eq!(total(&db), "120");
    assert_eq!(
        db.get_job("offline").unwrap().job.state,
        JobState::Succeeded
    );
}
#[test]
fn child_arriving_before_parent_converges_to_parent_135_and_child_only_13() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let a = usage(1, [100, 60, 10, 2, 110], [100, 60, 10, 2, 110]);
    let b = usage(2, [20, 5, 5, 1, 25], [120, 65, 15, 3, 135]);
    let c = usage(3, [10, 2, 3, 1, 13], [130, 67, 18, 4, 148]);
    let child = logs.path().join("sessions/child.jsonl");
    fs::write(&child, header("child", Some("parent")) + &a + &b + &c).unwrap();
    collect_file(&db, "local", &child, 5000).unwrap();
    assert_eq!(total(&db), "0");
    job(&db, "missing-parent");
    execute_rebuild(&db, "missing-parent", || false, || 6000).unwrap();
    assert_eq!(total(&db), "0");
    let parent = logs.path().join("sessions/parent.jsonl");
    fs::write(&parent, header("parent", None) + &a + &b).unwrap();
    collect_file(&db, "local", &parent, 7000).unwrap();
    assert_eq!(total(&db), "135");
    job(&db, "parent-late");
    execute_rebuild(&db, "parent-late", || false, || 8000).unwrap();
    assert_eq!(total(&db), "148");
    db.snapshot(|tx,_| {
        let mut s=tx.prepare("SELECT se.provider_session_id,e.total_tokens,e.input_tokens_total,e.cached_input_tokens,e.output_tokens_total,e.reasoning_output_tokens FROM active_usage_events e JOIN sessions se ON se.session_key=e.session_key ORDER BY se.provider_session_id,e.occurred_at_ms")?;
        let rows=s.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?)))?.collect::<Result<Vec<_>,_>>()?;
        assert_eq!(rows,vec![("child".into(),13,10,2,3,1),("parent".into(),110,100,60,10,2),("parent".into(),25,20,5,5,1)]);
        let inherited:i64=tx.query_row("SELECT COUNT(*) FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id WHERE p.kind='inherited'",[],|r|r.get(0))?;assert_eq!(inherited,2);Ok(())
    }).unwrap();
    job(&db, "again");
    execute_rebuild(&db, "again", || false, || 9000).unwrap();
    assert_eq!(total(&db), "148");
}
#[test]
fn cancelled_and_physically_obsolete_replays_leave_old_active_events() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let path = logs.path().join("sessions/source.jsonl");
    fs::write(
        &path,
        header("session", None) + &usage(1, [100, 60, 10, 2, 110], [100, 60, 10, 2, 110]),
    )
    .unwrap();
    collect_file(&db, "local", &path, 5000).unwrap();
    assert_eq!(total(&db), "110");
    job(&db, "cancel");
    let calls = AtomicUsize::new(0);
    assert_eq!(
        execute_rebuild(
            &db,
            "cancel",
            || calls.fetch_add(1, Ordering::Relaxed) > 4,
            || 6000
        )
        .unwrap_err()
        .code,
        ErrorCode::JobCancelled
    );
    assert_eq!(total(&db), "110");
    assert_eq!(db.get_job("cancel").unwrap().job.state, JobState::Cancelled);
    // The database checkpoint has not changed, but the physical prefix has been replaced.
    fs::write(&path, b"{}\n").unwrap();
    job(&db, "obsolete");
    assert_eq!(
        execute_rebuild(&db, "obsolete", || false, || 7000)
            .unwrap_err()
            .code,
        ErrorCode::CandidateObsolete
    );
    assert_eq!(total(&db), "110");
    assert_eq!(db.get_job("obsolete").unwrap().job.state, JobState::Failed);
}
#[test]
fn repeated_calls_and_zero_usage_survive_multiple_replay_pages() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let path = logs.path().join("sessions/many.jsonl");
    let mut bytes = header("many", None);
    for n in 1..=600 {
        bytes += &usage(1, [1, 0, 1, 0, 2], [n, 0, n, 0, n * 2]);
    }
    bytes += &usage(2, [0, 0, 0, 0, 0], [600, 0, 600, 0, 1200]);
    fs::write(&path, &bytes).unwrap();
    while collect_file(&db, "local", &path, 5000).unwrap().has_more {}
    assert_eq!(total(&db), "1200");
    job(&db, "many");
    execute_rebuild(&db, "many", || false, || 6000).unwrap();
    assert_eq!(total(&db), "1200");
    db.snapshot(|tx, _| {
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM active_usage_events", [], |r| r.get(0))?;
        assert_eq!(count, 600);
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
}

#[test]
fn parent_cycles_cannot_turn_pending_usage_into_inheritance_or_consumption() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let entry = usage(1, [100, 60, 10, 2, 110], [100, 60, 10, 2, 110]);
    for (child, parent) in [("a", "b"), ("b", "a")] {
        let path = logs.path().join(format!("sessions/{child}.jsonl"));
        fs::write(&path, header(child, Some(parent)) + &entry).unwrap();
        collect_file(&db, "local", &path, 5000).unwrap();
    }
    job(&db, "cycle");
    execute_rebuild(&db, "cycle", || false, || 6000).unwrap();
    assert_eq!(total(&db), "0");
    db.snapshot(|tx,_|{let inherited:i64=tx.query_row("SELECT COUNT(*) FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id WHERE p.kind='inherited'",[],|r|r.get(0))?;assert_eq!(inherited,0);Ok(())}).unwrap();
}
