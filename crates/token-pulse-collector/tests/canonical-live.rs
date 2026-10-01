use std::{
    fs,
    io::Write,
    path::Path,
    sync::{Arc, Barrier},
};
use token_pulse_collector::{collect_file, replay::execute_rebuild};
use token_pulse_core::{
    jobs::{JobRequest, JobScope},
    protocol::JobKind,
};
use token_pulse_store::{Database, ErrorCode, SourceRecord};
fn usage(last: i64, total: i64) -> String {
    let vector = |n| serde_json::json!({"input_tokens":n,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0,"total_tokens":n});
    serde_json::json!({"timestamp":"1970-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":vector(last),"total_token_usage":vector(total)}}}).to_string()+"\n"
}
fn source(db: &Database, root: &Path, id: &str) -> std::path::PathBuf {
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
    let path = root.join("sessions/source.jsonl");
    let header=serde_json::json!({"timestamp":"1970-01-01T00:00:00Z","type":"session_meta","payload":{"id":"shared"}}).to_string()+"\n";
    fs::write(&path, header + &usage(110, 110)).unwrap();
    collect_file(db, id, &path, 5000).unwrap();
    path
}
fn rebuild(db: &Database, id: &str) {
    db.create_job(
        id.into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::All {},
            request_key: id.into(),
        },
        6000,
    )
    .unwrap();
    execute_rebuild(db, id, || false, || 6001).unwrap();
}
fn total(db: &Database) -> String {
    db.snapshot(|tx, _| {
        Ok(tx.query_row(
            "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
            [],
            |r| r.get(0),
        )?)
    })
    .unwrap()
}
fn append(path: &Path, bytes: &str) {
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes.as_bytes())
        .unwrap();
}
#[test]
fn two_live_sources_race_and_catch_up_across_pages_without_double_consumption() {
    for _ in 0..4 {
        let data = tempfile::tempdir().unwrap();
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        let db = Database::open(data.path()).unwrap();
        let a = source(&db, one.path(), "one");
        let b = source(&db, two.path(), "two");
        rebuild(&db, "initial");
        let mut bytes = String::new();
        for n in 1..=600 {
            bytes += &usage(1, 110 + n);
        }
        append(&a, &bytes);
        append(&b, &bytes);
        let barrier = Arc::new(Barrier::new(2));
        let mut workers = vec![];
        for (source, path) in [("one", a.clone()), ("two", b.clone())] {
            let db = db.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                let mut conflicts = 0;
                loop {
                    match collect_file(&db, source, &path, 7000) {
                        Ok(receipt) => {
                            if !receipt.has_more {
                                break;
                            }
                        }
                        Err(error) if error.code == ErrorCode::CheckpointConflict => {
                            conflicts += 1;
                            assert!(conflicts < 30);
                        }
                        Err(error) => panic!("unexpected {error}"),
                    }
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(total(&db), "710");
        db.snapshot(|tx,_|{
            assert_eq!(tx.query_row("SELECT COUNT(*) FROM active_usage_events",[],|r|r.get::<_,i64>(0))?,601);
            assert_eq!(tx.query_row("SELECT COUNT(*) FROM canonical_usage_sequence c JOIN sessions s ON s.active_ledger_id=c.ledger_id",[],|r|r.get::<_,i64>(0))?,601);
            assert_eq!(tx.query_row("SELECT COUNT(*) FROM event_provenance p JOIN active_usage_events e ON e.event_id=p.event_id",[],|r|r.get::<_,i64>(0))?,1202);
            assert_eq!(tx.query_row("SELECT COUNT(*) FROM file_usage_cursors c JOIN sessions s ON s.active_ledger_id=c.ledger_id WHERE c.next_ordinal=601 AND c.state='aligned'",[],|r|r.get::<_,i64>(0))?,2);Ok(())
        }).unwrap();
        append(&a, &usage(0, 710));
        append(&b, &usage(0, 710));
        collect_file(&db, "two", &b, 8000).unwrap();
        collect_file(&db, "one", &a, 8000).unwrap();
        assert_eq!(total(&db), "710");
        rebuild(&db, "again");
        assert_eq!(total(&db), "710");
        db.snapshot(|tx,_|{assert_eq!(tx.query_row("SELECT COUNT(*) FROM active_usage_events",[],|r|r.get::<_,i64>(0))?,601);assert_eq!(tx.query_row("SELECT COUNT(*) FROM canonical_usage_sequence c JOIN sessions s ON s.active_ledger_id=c.ledger_id",[],|r|r.get::<_,i64>(0))?,602);Ok(())}).unwrap();
    }
}
#[test]
fn divergent_live_copy_preserves_baselines_marks_rebuild_and_keeps_following_records_pending() {
    let data = tempfile::tempdir().unwrap();
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    let a = source(&db, one.path(), "one");
    let b = source(&db, two.path(), "two");
    rebuild(&db, "initial");
    append(&a, &usage(25, 135));
    collect_file(&db, "one", &a, 7000).unwrap();
    assert_eq!(total(&db), "135");
    append(&b, &usage(13, 123));
    collect_file(&db, "two", &b, 7000).unwrap();
    assert_eq!(total(&db), "135");
    append(&b, &usage(9, 132));
    collect_file(&db, "two", &b, 8000).unwrap();
    assert_eq!(total(&db), "135");
    append(&a, &usage(7, 142));
    collect_file(&db, "one", &a, 8000).unwrap();
    assert_eq!(total(&db), "142");
    db.snapshot(|tx,_|{assert_eq!(tx.query_row("SELECT COUNT(*) FROM file_usage_cursors c JOIN sessions s ON s.active_ledger_id=c.ledger_id WHERE c.state='rebuild_required' AND c.next_ordinal=1",[],|r|r.get::<_,i64>(0))?,1);assert_eq!(tx.query_row("SELECT COUNT(*) FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id WHERE p.kind='pending'",[],|r|r.get::<_,i64>(0))?,2);Ok(())}).unwrap();
    rebuild(&db, "conflict");
    assert_eq!(total(&db), "142");
}
