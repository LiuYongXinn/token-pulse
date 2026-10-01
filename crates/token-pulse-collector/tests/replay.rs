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
fn turn(model: &str) -> String {
    serde_json::json!({"timestamp":"1970-01-01T00:00:00Z","type":"turn_context","payload":{"model":model}}).to_string()+"\n"
}
#[test]
fn mirror_publication_keeps_one_identity_primary_metadata_and_all_source_evidence() {
    let data = tempfile::tempdir().unwrap();
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, one.path());
    fs::create_dir_all(two.path().join("sessions")).unwrap();
    db.add_source(SourceRecord {
        source_id: "custom".into(),
        root_path: two.path().to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    let a = usage(1, [100, 60, 10, 2, 110], [100, 60, 10, 2, 110]);
    let b = usage(2, [20, 5, 5, 1, 25], [120, 65, 15, 3, 135]);
    let c = usage(3, [10, 2, 3, 1, 13], [130, 67, 18, 4, 148]);
    let primary = one.path().join("sessions/primary.jsonl");
    let mirror = two.path().join("sessions/mirror.jsonl");
    let primary_bytes = header("shared", None) + &turn("primary-model") + &a + &b;
    let mirror_bytes = header("shared", None) + &turn("mirror-model") + &a + &b + &c;
    fs::write(&primary, &primary_bytes).unwrap();
    collect_file(&db, "local", &primary, 5000).unwrap();
    let stable: String = db
        .snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT session_key FROM active_usage_events LIMIT 1",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    fs::write(&mirror, &mirror_bytes).unwrap();
    collect_file(&db, "custom", &mirror, 5001).unwrap();
    assert_eq!(total(&db), "135");
    job(&db, "mirror");
    execute_rebuild(&db, "mirror", || false, || 6000).unwrap();
    assert_eq!(total(&db), "148");
    db.snapshot(|tx,_| {
        let mut s=tx.prepare("SELECT total_tokens,model,session_key FROM active_usage_events ORDER BY occurred_at_ms")?;
        let rows=s.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<Result<Vec<_>,_>>()?;
        assert_eq!(rows,vec![(110,"primary-model".into(),stable.clone()),(25,"primary-model".into(),stable.clone()),(13,"mirror-model".into(),stable.clone())]);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM session_aliases",[],|r|r.get::<_,i64>(0))?,1);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM event_provenance p JOIN active_usage_events e ON e.event_id=p.event_id",[],|r|r.get::<_,i64>(0))?,5);
        for (source,expected) in [("local","135"),("custom","148")] {
            let sum:String=tx.query_row("SELECT sum_token_decimal(e.total_tokens) FROM active_usage_events e WHERE EXISTS(SELECT 1 FROM event_provenance p JOIN observations o ON o.observation_id=p.observation_id JOIN file_generations g ON g.file_generation_id=o.file_generation_id JOIN source_files f ON f.file_id=g.file_id WHERE p.event_id=e.event_id AND f.source_id=?1)",[source],|r|r.get(0))?;assert_eq!(sum,expected);
        }
        assert_eq!(tx.query_row("SELECT COUNT(DISTINCT session_key) FROM observations WHERE kind='usage'",[],|r|r.get::<_,i64>(0))?,1);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM file_usage_cursors c JOIN sessions s ON s.active_ledger_id=c.ledger_id WHERE c.state='aligned'",[],|r|r.get::<_,i64>(0))?,2);
        Ok(())
    }).unwrap();
    assert_eq!(fs::read_to_string(&primary).unwrap(), primary_bytes);
    assert_eq!(fs::read_to_string(&mirror).unwrap(), mirror_bytes);
    job(&db, "again-mirror");
    execute_rebuild(&db, "again-mirror", || false, || 7000).unwrap();
    assert_eq!(total(&db), "148");
    use std::io::Write;
    let d = usage(4, [5, 1, 2, 0, 7], [135, 68, 20, 4, 155]);
    // Keep the shorter source's order intact when it catches up to the longer source.
    fs::OpenOptions::new()
        .append(true)
        .open(&primary)
        .unwrap()
        .write_all((c.clone() + &d).as_bytes())
        .unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&mirror)
        .unwrap()
        .write_all(d.as_bytes())
        .unwrap();
    collect_file(&db, "local", &primary, 8000).unwrap();
    collect_file(&db, "custom", &mirror, 8001).unwrap();
    assert_eq!(total(&db), "155"); // Both sources align the same continuation and consume it only once.
    job(&db, "append-mirror");
    execute_rebuild(&db, "append-mirror", || false, || 9000).unwrap();
    assert_eq!(total(&db), "155");
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(DISTINCT session_key) FROM active_usage_events",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        assert_eq!(
            tx.query_row(
                "SELECT canonical_session_key FROM session_aliases",
                [],
                |r| r.get::<_, String>(0)
            )?,
            stable
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn mirrored_parent_is_one_lineage_candidate_and_conflicting_tails_remain_pending() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let a = usage(1, [100, 60, 10, 2, 110], [100, 60, 10, 2, 110]);
    let b = usage(2, [20, 5, 5, 1, 25], [120, 65, 15, 3, 135]);
    let c = usage(3, [10, 2, 3, 1, 13], [130, 67, 18, 4, 148]);
    for (name, bytes) in [
        ("parent", header("parent", None) + &a + &b),
        ("mirror", header("parent", None) + &a + &b),
        ("child", header("child", Some("parent")) + &a + &b + &c),
    ] {
        let path = logs.path().join(format!("sessions/{name}.jsonl"));
        fs::write(&path, bytes).unwrap();
        collect_file(&db, "local", &path, 5000).unwrap();
    }
    job(&db, "family-mirror");
    execute_rebuild(&db, "family-mirror", || false, || 6000).unwrap();
    assert_eq!(total(&db), "148");
    db.snapshot(|tx,_|{assert_eq!(tx.query_row("SELECT COUNT(*) FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id WHERE p.kind='inherited'",[],|r|r.get::<_,i64>(0))?,2);Ok(())}).unwrap();
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    for (name, bytes) in [
        ("primary", header("same", None) + &a),
        ("one", header("same", None) + &a + &b),
        (
            "two",
            header("same", None) + &a + &usage(2, [10, 2, 3, 1, 13], [110, 62, 13, 3, 123]),
        ),
    ] {
        let path = logs.path().join(format!("sessions/{name}.jsonl"));
        fs::write(&path, bytes).unwrap();
        collect_file(&db, "local", &path, 5000).unwrap();
    }
    job(&db, "conflict");
    execute_rebuild(&db, "conflict", || false, || 6000).unwrap();
    assert_eq!(total(&db), "110");
    db.snapshot(|tx,_|{assert_eq!(tx.query_row("SELECT COUNT(*) FROM session_aliases",[],|r|r.get::<_,i64>(0))?,0);assert_eq!(tx.query_row("SELECT COUNT(*) FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id WHERE p.reason_code='sequence_identity_conflict'",[],|r|r.get::<_,i64>(0))?,4);Ok(())}).unwrap();
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
