use std::{
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use token_pulse_collector::{
    collect_file,
    jobs::JobService,
    replay::execute_rebuild,
    service::{CollectorOptions, CollectorService},
};
use token_pulse_core::protocol::JobState;
use token_pulse_store::{Database, ErrorCode, SourceRecord};
fn add(db: &Database, root: &Path, id: &str) {
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
fn header(id: &str, parent: Option<&str>) -> String {
    serde_json::json!({"timestamp":"1970-01-01T00:00:00Z","type":"session_meta","payload":{"id":id,"forked_from_id":parent}}).to_string()+"\n"
}
fn usage(second: u32, last: i64, total: i64) -> String {
    let v = |n| serde_json::json!({"input_tokens":n,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0,"total_tokens":n});
    serde_json::json!({"timestamp":format!("1970-01-01T00:00:{second:02}Z"),"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":v(last),"total_token_usage":v(total)}}}).to_string()+"\n"
}
fn total(db: &Database) -> Option<String> {
    db.snapshot(|tx, _| {
        Ok(tx.query_row(
            "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
            [],
            |r| r.get(0),
        )?)
    })
    .unwrap()
}
fn wait_for(db: &Database, collector: &CollectorService, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "background proof timed out: status={:?}, total={:?}, jobs={:?}",
            collector.status(),
            total(db),
            db.list_jobs(10)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn services(db: &Database, manifest: Duration) -> (Arc<CollectorService>, JobService) {
    let collector = Arc::new(
        CollectorService::start(
            db.clone(),
            CollectorOptions {
                watcher: false,
                active_poll: Duration::from_millis(50),
                manifest_poll: manifest,
            },
        )
        .unwrap(),
    );
    let c = collector.clone();
    let jobs = JobService::start_with_notify(db.clone(), Arc::new(move || c.reconcile())).unwrap();
    (collector, jobs)
}
#[test]
fn startup_mirrors_converge_via_background_jobs_without_manual_rebuild_or_source_writes() {
    let data = tempfile::tempdir().unwrap();
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    add(&db, one.path(), "one");
    add(&db, two.path(), "two");
    let bytes = header("same", None) + &usage(1, 110, 110) + &usage(2, 25, 135);
    let longer = bytes.clone() + &usage(3, 13, 148);
    let a = one.path().join("sessions/a.jsonl");
    let b = two.path().join("sessions/b.jsonl");
    fs::write(&a, &bytes).unwrap();
    fs::write(&b, &longer).unwrap();
    let (collector, jobs) = services(&db, Duration::from_secs(30));
    wait_for(&db, &collector, || {
        total(&db).as_deref() == Some("148")
            && db
                .list_jobs(10)
                .unwrap()
                .iter()
                .any(|j| j.state == JobState::Succeeded)
    });
    collector.reconcile();
    wait_for(&db, &collector, || collector.status().queue_length == 0);
    assert!(db.enqueue_proof_rebuild(7000).unwrap().is_none());
    jobs.shutdown();
    collector.shutdown();
    assert_eq!(db.list_jobs(10).unwrap().len(), 1);
    assert_eq!(fs::read_to_string(&a).unwrap(), bytes);
    assert_eq!(fs::read_to_string(&b).unwrap(), longer);
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM active_usage_events", [], |r| r
                .get::<_, i64>(0))?,
            3
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM session_aliases", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn parent_arriving_later_automatically_resolves_child_inheritance() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    add(&db, logs.path(), "local");
    let prefix = usage(1, 110, 110) + &usage(2, 25, 135);
    let child = logs.path().join("sessions/child.jsonl");
    fs::write(
        &child,
        header("child", Some("parent")) + &prefix + &usage(3, 13, 148),
    )
    .unwrap();
    let (collector, jobs) = services(&db, Duration::from_millis(300));
    wait_for(&db, &collector, || collector.status().commits > 0);
    assert_eq!(total(&db), None);
    assert!(db.enqueue_proof_rebuild(5000).unwrap().is_none());
    fs::write(
        logs.path().join("sessions/parent.jsonl"),
        header("parent", None) + &prefix,
    )
    .unwrap();
    wait_for(&db, &collector, || total(&db).as_deref() == Some("148"));
    jobs.shutdown();
    collector.shutdown();
    db.snapshot(|tx,_|{assert_eq!(tx.query_row("SELECT sum_token_decimal(e.total_tokens) FROM active_usage_events e JOIN sessions s ON s.session_key=e.session_key WHERE s.provider_session_id='child'",[],|r|r.get::<_,String>(0))?,"13");Ok(())}).unwrap();
}
#[test]
fn unchanged_unproven_inputs_do_not_repeat_jobs_when_only_checkpoints_or_ledgers_change() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    add(&db, logs.path(), "local");
    let bytes = include_bytes!("../../../fixtures/codex-rollout-v1.jsonl");
    for name in ["one", "two"] {
        let p = logs.path().join(format!("sessions/{name}.jsonl"));
        fs::write(&p, bytes).unwrap();
        collect_file(&db, "local", &p, 5000).unwrap();
    }
    let job = db.enqueue_proof_rebuild(6000).unwrap().unwrap();
    execute_rebuild(&db, &job.job_id, || false, || 6001).unwrap();
    assert_eq!(total(&db).as_deref(), Some("120"));
    for _ in 0..4 {
        for name in ["one", "two"] {
            let p = logs.path().join(format!("sessions/{name}.jsonl"));
            collect_file(&db, "local", &p, 7000).unwrap();
        }
        assert!(db.enqueue_proof_rebuild(7001).unwrap().is_none());
    }
    assert_eq!(db.list_jobs(10).unwrap().len(), 1);
}
#[test]
fn proof_requests_honor_cancellation_new_evidence_and_bounded_obsolete_retries() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    add(&db, logs.path(), "local");
    let bytes = header("same", None) + &usage(1, 110, 110);
    let a = logs.path().join("sessions/a.jsonl");
    let b = logs.path().join("sessions/b.jsonl");
    for path in [&a, &b] {
        fs::write(path, &bytes).unwrap();
        collect_file(&db, "local", path, 5000).unwrap();
    }
    let initial = db.enqueue_proof_rebuild(6000).unwrap().unwrap();
    assert!(db.enqueue_proof_rebuild(6000).unwrap().is_none());
    db.cancel_job(initial.job_id.clone(), 6001).unwrap();
    assert!(db.enqueue_proof_rebuild(6002).unwrap().is_none());
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(&b)
        .unwrap()
        .write_all(usage(2, 25, 135).as_bytes())
        .unwrap();
    collect_file(&db, "local", &b, 7000).unwrap();
    for retry in 0..3 {
        let job = db.enqueue_proof_rebuild(7001).unwrap().unwrap();
        assert!(job.job_id.ends_with(&format!("-{retry}")));
        db.fail_rebuild(job.job_id, ErrorCode::CandidateObsolete, 7002)
            .unwrap();
    }
    assert!(db.enqueue_proof_rebuild(7003).unwrap().is_none());
    assert_eq!(db.list_jobs(10).unwrap().len(), 4);
}
