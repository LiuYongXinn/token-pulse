use super::*;
use crate::batch::tests::{fixture, setup};
fn populated() -> (tempfile::TempDir, Database) {
    let (d, db) = setup();
    db.commit(fixture()).unwrap();
    (d, db)
}
fn begin(db: &Database) -> ScanHandle {
    db.begin_source_scan("source".into(), "synthetic".into(), 1)
        .unwrap()
}
fn state(db: &Database) -> String {
    db.snapshot(|tx, _| {
        Ok(tx.query_row(
            "SELECT state FROM source_scan_state WHERE source_id='source'",
            [],
            |r| r.get(0),
        )?)
    })
    .unwrap()
}
fn record(db: &Database, h: &ScanHandle) {
    db.record_source_scan_files(h.clone(), vec![("synthetic.jsonl".into(), 100)])
        .unwrap();
}
fn confirm(db: &Database, h: &ScanHandle) {
    assert!(
        db.confirm_source_scan_file(
            h.clone(),
            "synthetic.jsonl".into(),
            "generation".into(),
            1,
            2
        )
        .unwrap()
    );
}

#[test]
fn discovery_and_all_bounded_reads_are_separate_and_do_not_change_facts() {
    let (_d, db) = populated();
    let before=db.snapshot(|tx,r|Ok((r.data,r.price,r.settings,tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?))).unwrap();
    let h = begin(&db);
    record(&db, &h);
    confirm(&db, &h);
    assert_eq!(state(&db), "scanning");
    db.finish_source_scan(h, None, 3).unwrap();
    assert_eq!(state(&db), "ready");
    db.snapshot(|tx,r| {assert_eq!((r.data,r.price,r.settings,tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?),before);Ok(())}).unwrap();
}
#[test]
fn complete_discovery_with_unregistered_or_partial_file_is_not_ready() {
    let (_d, db) = populated();
    let h = begin(&db);
    db.record_source_scan_files(h.clone(), vec![("unknown.jsonl".into(), 5)])
        .unwrap();
    db.finish_source_scan(h, None, 2).unwrap();
    assert_eq!(state(&db), "incomplete");
    let h = begin(&db);
    record(&db, &h);
    db.write(|conn| {
        conn.execute(
            "UPDATE file_generations SET observed_size=110 WHERE file_generation_id='generation'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    confirm(&db, &h);
    db.finish_source_scan(h, None, 3).unwrap();
    assert_eq!(state(&db), "incomplete");
}
#[test]
fn old_run_or_checkpoint_cannot_confirm_current_evidence() {
    let (_d, db) = populated();
    let old = begin(&db);
    record(&db, &old);
    let new = begin(&db);
    record(&db, &new);
    assert_eq!(
        db.confirm_source_scan_file(old, "synthetic.jsonl".into(), "generation".into(), 1, 3)
            .unwrap_err()
            .code,
        ErrorCode::CheckpointConflict
    );
    assert_eq!(
        db.confirm_source_scan_file(
            new.clone(),
            "synthetic.jsonl".into(),
            "generation".into(),
            0,
            3
        )
        .unwrap_err()
        .code,
        ErrorCode::CheckpointConflict
    );
    db.finish_source_scan(new, None, 3).unwrap();
    assert_eq!(state(&db), "incomplete");
}
#[test]
fn paused_or_changed_root_cannot_publish_a_scan() {
    let (_d, db) = populated();
    let h = begin(&db);
    record(&db, &h);
    confirm(&db, &h);
    db.write(|conn| {
        conn.execute("UPDATE sources SET enabled=0 WHERE source_id='source'", [])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.finish_source_scan(h.clone(), None, 3).unwrap_err().code,
        ErrorCode::PermissionDenied
    );
    db.write(|conn| {
        conn.execute(
            "UPDATE sources SET enabled=1,root_path='other' WHERE source_id='source'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.finish_source_scan(h, None, 3).unwrap_err().code,
        ErrorCode::PermissionDenied
    );
    assert_eq!(state(&db), "scanning");
}
#[test]
fn known_hint_can_be_reconfirmed_but_unknown_membership_requires_new_enumeration() {
    let (_d, db) = populated();
    let h = begin(&db);
    record(&db, &h);
    confirm(&db, &h);
    db.finish_source_scan(h.clone(), None, 3).unwrap();
    assert!(
        !db.invalidate_source_scan_file("source".into(), "synthetic.jsonl".into())
            .unwrap()
    );
    assert_eq!(state(&db), "incomplete");
    confirm(&db, &h);
    assert_eq!(state(&db), "ready");
    assert!(
        db.invalidate_source_scan_file("source".into(), "new.jsonl".into())
            .unwrap()
    );
    confirm(&db, &h);
    assert_eq!(state(&db), "incomplete");
}
#[test]
fn traversal_issue_and_interruption_survive_reopen() {
    let (d, db) = populated();
    let h = begin(&db);
    record(&db, &h);
    confirm(&db, &h);
    db.finish_source_scan(h, Some(ErrorCode::SourceUnreadable), 3)
        .unwrap();
    assert_eq!(state(&db), "incomplete");
    db.interrupt_source_scans().unwrap();
    drop(db);
    let db = Database::open(d.path()).unwrap();
    assert_eq!(state(&db), "interrupted");
    let h = begin(&db);
    record(&db, &h);
    confirm(&db, &h);
    db.finish_source_scan(h, None, 4).unwrap();
    assert_eq!(state(&db), "ready");
}
#[test]
fn new_run_is_atomic_and_old_sqlite_snapshot_keeps_its_previous_proof() {
    let (_d, db) = populated();
    let h = begin(&db);
    record(&db, &h);
    confirm(&db, &h);
    db.finish_source_scan(h, None, 3).unwrap();
    db.snapshot(|tx, _| {
        let prior: String =
            tx.query_row("SELECT state FROM source_scan_state", [], |r| r.get(0))?;
        begin(&db);
        assert_eq!(
            tx.query_row("SELECT state FROM source_scan_state", [], |r| r
                .get::<_, String>(0))?,
            prior
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM source_scan_files", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(state(&db), "scanning");
    db.write(|conn| {conn.execute_batch("CREATE TRIGGER reject_scan BEFORE UPDATE OF scan_revision ON source_scan_state BEGIN SELECT RAISE(ABORT,'fixture'); END;")?;Ok(())}).unwrap();
    let h = db
        .snapshot(|tx, _| {
            Ok(ScanHandle {
                source_id: "source".into(),
                revision: tx.query_row("SELECT scan_revision FROM source_scan_state", [], |r| {
                    r.get(0)
                })?,
            })
        })
        .unwrap();
    record(&db, &h);
    assert_eq!(
        db.begin_source_scan("source".into(), "synthetic".into(), 5)
            .unwrap_err()
            .code,
        ErrorCode::DbWriteFailed
    );
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM source_scan_files", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn empty_enumeration_marks_missing_old_file_without_erasing_consumption() {
    let (_d, db) = populated();
    let h = begin(&db);
    db.finish_source_scan(h, None, 2).unwrap();
    assert_eq!(state(&db), "ready");
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT status FROM source_files WHERE file_id='file'",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "missing"
        );
        assert_eq!(
            tx.query_row(
                "SELECT SUM(total_tokens) FROM active_usage_events",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            110
        );
        Ok(())
    })
    .unwrap();
}
