use serde_json::json;
use std::{fs, io::Write, path::Path};
use token_pulse_collector::collect_file;
use token_pulse_core::{
    numeric::EpochMs,
    protocol::{DateRange, DimensionSelection, PriceBasis, UsageFilter},
    query::{TurnsPage, TurnsQuery, TurnsRequest},
};
use token_pulse_store::{Database, SourceRecord};

fn source(db: &Database, root: &Path) {
    fs::create_dir_all(root.join("sessions")).unwrap();
    db.add_source(SourceRecord {
        source_id: "source".into(),
        root_path: root.to_str().unwrap().into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
}
fn usage(turn: &str) -> String {
    let vector = json!({"input_tokens":100,"output_tokens":10,"total_tokens":110});
    format!(
        "{}\n{}\n",
        json!({"type":"turn_context","payload":{"turn_id":turn,"model":"synthetic"}}),
        json!({"type":"event_msg","timestamp":"2026-10-10T12:00:00Z","payload":{"type":"token_count","info":{"last_token_usage":vector}}})
    )
}
fn completion(turn: &str, duration: i64) -> String {
    format!(
        "{}\n",
        json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":turn,"duration_ms":duration,"time_to_first_token_ms":1583,"last_agent_message":"must never be stored"}})
    )
}
fn drain(db: &Database, path: &Path) {
    for _ in 0..100 {
        if !collect_file(db, "source", path, 2000).unwrap().has_more {
            return;
        }
    }
    panic!("bounded timing scan failed to finish");
}
fn turns(db: &Database, path: &Path) -> TurnsPage {
    let checkpoint = db
        .file_checkpoint("source", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    let session = checkpoint.context.session_key.unwrap();
    let all = DimensionSelection::All {};
    db.query_turns(
        "test",
        &TurnsRequest {
            query: TurnsQuery {
                session_key: session,
                filter: UsageFilter {
                    range: DateRange {
                        start_ms: EpochMs::new(0).unwrap(),
                        end_ms: EpochMs::new(2000000000000).unwrap(),
                        timezone: "UTC".into(),
                    },
                    models: all.clone(),
                    projects: all.clone(),
                    sessions: all.clone(),
                    sources: all,
                },
                price_basis: PriceBasis::EventTime {},
                page_size: 20,
            },
            cursor: None,
        },
        EpochMs::new(2000).unwrap(),
    )
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
fn historic_backfill_is_bounded_resumable_and_does_not_reaccount_usage() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let path = logs.path().join("sessions/history.jsonl");
    let mut raw = format!(
        "{}\n",
        json!({"type":"session_meta","payload":{"id":"thread"}})
    );
    for _ in 0..750 {
        raw.push_str("{\"type\":\"response_item\",\"payload\":{\"type\":\"message\"}}\n");
    }
    raw.push_str(&usage("turn"));
    raw.push_str(&completion("turn", 370887));
    fs::write(&path, &raw).unwrap();
    drain(&db, &path);
    let original = db
        .file_checkpoint("source", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    let before = db.usage_revision().unwrap();
    assert_eq!(
        turns(&db, &path).turns[0]
            .duration_ms
            .as_ref()
            .unwrap()
            .as_str(),
        "370887"
    );
    // Simulate a source fully imported by the old build, which ignored completion metadata.
    drop(db);
    let conn =
        token_pulse_store::rusqlite::Connection::open(data.path().join("token-pulse.db")).unwrap();
    conn.execute_batch("DELETE FROM turn_timing_records; DELETE FROM turn_timing_scans;")
        .unwrap();
    drop(conn);
    let db = Database::open(data.path()).unwrap();
    assert!(turns(&db, &path).turns[0].duration_ms.is_none());
    let first = collect_file(&db, "source", &path, 2000).unwrap();
    assert!(first.has_more);
    assert!(!first.commit.usage_changed);
    drop(db);
    let db = Database::open(data.path()).unwrap();
    drain(&db, &path);
    let page = turns(&db, &path);
    assert_eq!(page.summary.total_tokens.as_str(), "110");
    assert_eq!(
        page.turns[0].duration_ms.as_ref().unwrap().as_str(),
        "370887"
    );
    assert_eq!(
        page.turns[0]
            .time_to_first_token_ms
            .as_ref()
            .unwrap()
            .as_str(),
        "1583"
    );
    let after = db.usage_revision().unwrap();
    assert_eq!(before.data_revision, after.data_revision);
    assert_eq!(before.price_revision, after.price_revision);
    assert!(after.usage_view_revision.value() > before.usage_view_revision.value());
    let current = db
        .file_checkpoint("source", path.to_str().unwrap(), None)
        .unwrap()
        .unwrap();
    assert_eq!(original.committed_offset, current.committed_offset);
    assert_eq!(original.checkpoint_revision, current.checkpoint_revision);
    assert_eq!(fs::read_to_string(&path).unwrap(), raw);
    assert!(
        !collect_file(&db, "source", &path, 2000)
            .unwrap()
            .commit
            .usage_changed
    );
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM turn_timing_records", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert!(
            !tx.query_row("SELECT context_json FROM turn_timing_scans", [], |r| r
                .get::<_, String>(
                0
            ))?
            .contains("must never be stored")
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn late_completion_waits_for_newline_and_conflicts_or_unrelated_threads_stay_unknown() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    source(&db, logs.path());
    let path = logs.path().join("sessions/live.jsonl");
    fs::write(
        &path,
        format!(
            "{}\n{}",
            json!({"type":"session_meta","payload":{"id":"thread"}}),
            usage("same-turn")
        ),
    )
    .unwrap();
    drain(&db, &path);
    assert!(turns(&db, &path).turns[0].duration_ms.is_none());
    let other = logs.path().join("sessions/other.jsonl");
    fs::write(
        &other,
        format!(
            "{}\n{}{}",
            json!({"type":"session_meta","payload":{"id":"other-thread"}}),
            usage("same-turn"),
            completion("same-turn", 10000)
        ),
    )
    .unwrap();
    drain(&db, &other);
    assert!(turns(&db, &path).turns[0].duration_ms.is_none());
    assert_eq!(
        turns(&db, &other).turns[0]
            .duration_ms
            .as_ref()
            .unwrap()
            .as_str(),
        "10000"
    );
    append(&path, completion("same-turn", 37534).trim_end());
    drain(&db, &path);
    assert!(turns(&db, &path).turns[0].duration_ms.is_none());
    append(&path, "\n");
    drain(&db, &path);
    assert_eq!(
        turns(&db, &path).turns[0]
            .duration_ms
            .as_ref()
            .unwrap()
            .as_str(),
        "37534"
    );
    append(&path, &completion("same-turn", 37534));
    drain(&db, &path);
    assert_eq!(
        turns(&db, &path).turns[0]
            .duration_ms
            .as_ref()
            .unwrap()
            .as_str(),
        "37534"
    );
    append(&path, &completion("same-turn", 40000));
    drain(&db, &path);
    assert!(turns(&db, &path).turns[0].duration_ms.is_none());
    assert!(turns(&db, &path).turns[0].time_to_first_token_ms.is_none());
}
