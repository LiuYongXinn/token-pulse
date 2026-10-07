use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::{
        series,
        tests::{filter, ids},
    },
};
use token_pulse_core::calendar::Grain;

fn ready(db: &Database) {
    db.write(|conn| {
        conn.execute("UPDATE sources SET readability='readable'", [])?;
        Ok(())
    })
    .unwrap();
    let h = db
        .begin_source_scan("source".into(), "synthetic".into(), 1)
        .unwrap();
    db.record_source_scan_files(h.clone(), vec![("synthetic.jsonl".into(), 100)])
        .unwrap();
    db.confirm_source_scan_file(
        h.clone(),
        "synthetic.jsonl".into(),
        "generation".into(),
        1,
        2,
    )
    .unwrap();
    db.finish_source_scan(h, None, 3).unwrap();
}
#[test]
fn full_proof_can_complete_without_confusing_breakdown_or_empty_source_selection() {
    let (_d, db) = setup();
    db.commit(fixture()).unwrap();
    ready(&db);
    let c = db.usage_coverage(&filter()).unwrap();
    assert!(matches!(c.state, CoverageState::Complete));
    assert!(c.source_issues.is_empty());
    assert!(!c.breakdown_complete); // Legacy cache writes remain unknown.
    assert!(c.unattributed_total_tokens.is_none());
    let mut f = filter();
    f.models = ids(&[], false);
    let c = db.usage_coverage(&f).unwrap();
    assert!(matches!(c.state, CoverageState::Complete));
    assert!(!c.breakdown_complete);
    f.sources = ids(&[], false);
    assert!(matches!(
        db.usage_coverage(&f).unwrap().state,
        CoverageState::Unknown
    ));
}
#[test]
fn discovered_unregistered_and_known_unconfirmed_files_are_separate_work() {
    let (_d, db) = setup();
    db.commit(fixture()).unwrap();
    ready(&db);
    let h = db
        .begin_source_scan("source".into(), "synthetic".into(), 4)
        .unwrap();
    db.record_source_scan_files(
        h.clone(),
        vec![("synthetic.jsonl".into(), 100), ("new.jsonl".into(), 12)],
    )
    .unwrap();
    db.finish_source_scan(h.clone(), None, 5).unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert_eq!(c.pending_file_count.as_str(), "1");
    assert_eq!(c.verifying_file_count.unwrap().as_str(), "1");
    assert!(matches!(c.state, CoverageState::Partial));
    db.confirm_source_scan_file(h, "synthetic.jsonl".into(), "generation".into(), 1, 6)
        .unwrap();
    let mut f = filter();
    f.models = ids(&[], false);
    assert_eq!(
        db.usage_coverage(&f).unwrap().pending_file_count.as_str(),
        "1"
    );
}
#[test]
fn cached_ready_requires_current_checkpoint_and_upper_bound_in_actual_snapshot() {
    let (_d, db) = setup();
    db.commit(fixture()).unwrap();
    ready(&db);
    db.snapshot(|tx,_| {
        assert!(matches!(coverage(tx,&filter(),&super::super::empty_totals())?.state,CoverageState::Complete));
        db.write(|conn| {conn.execute("UPDATE file_generations SET checkpoint_revision=checkpoint_revision+1,observed_size=110",[])?;Ok(())})?;
        assert!(matches!(coverage(tx,&filter(),&super::super::empty_totals())?.state,CoverageState::Complete));Ok(())
    }).unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert!(matches!(c.state, CoverageState::Partial));
    assert_eq!(c.pending_file_count.as_str(), "1");
    assert_eq!(c.source_issues[0].code, "source_scan_pending");
}
#[test]
fn interrupted_or_membership_changed_evidence_is_unknown_until_new_scan() {
    let (_d, db) = setup();
    db.commit(fixture()).unwrap();
    ready(&db);
    db.interrupt_source_scans().unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert!(matches!(c.state, CoverageState::Unknown));
    assert_eq!(c.source_issues[0].code, "source_scan_interrupted");
    ready(&db);
    db.invalidate_source_scan_file("source".into(), "unknown.jsonl".into())
        .unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert!(matches!(c.state, CoverageState::Unknown));
    assert_eq!(c.source_issues[0].code, "source_scan_changed");
}
#[test]
fn readability_pause_and_root_change_override_old_ready_evidence() {
    let (_d, db) = setup();
    db.commit(fixture()).unwrap();
    ready(&db);
    for (sql, expected) in [
        (
            "UPDATE sources SET readability='awaiting_directory'",
            CoverageState::Unknown,
        ),
        (
            "UPDATE sources SET readability='partially_readable'",
            CoverageState::Partial,
        ),
        (
            "UPDATE sources SET readability='readable',enabled=0",
            CoverageState::Partial,
        ),
        (
            "UPDATE sources SET enabled=1,root_path='changed'",
            CoverageState::Unknown,
        ),
    ] {
        db.write(move |conn| {
            conn.execute(sql, [])?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            serde_json::to_value(db.usage_coverage(&filter()).unwrap().state).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
}
#[test]
fn enumeration_error_and_missing_old_file_remain_partial_with_consumption_retained() {
    let (_d, db) = setup();
    db.commit(fixture()).unwrap();
    ready(&db);
    let h = db
        .begin_source_scan("source".into(), "synthetic".into(), 4)
        .unwrap();
    db.finish_source_scan(h, Some(ErrorCode::SourceUnreadable), 5)
        .unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert!(matches!(c.state, CoverageState::Partial));
    assert_eq!(c.source_issues[0].code, "source_scan_incomplete");
    let h = db
        .begin_source_scan("source".into(), "synthetic".into(), 6)
        .unwrap();
    db.finish_source_scan(h, None, 7).unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert!(matches!(c.state, CoverageState::Partial));
    assert_eq!(c.pending_file_count.as_str(), "1");
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "110"
    );
}
#[test]
fn dated_pending_gaps_do_not_prevent_other_verified_buckets_from_completing() {
    let (_d, db) = setup();
    db.commit(fixture()).unwrap();
    ready(&db);
    let mut f = filter();
    f.range.end_ms = EpochMs::new(10_800_000).unwrap();
    super::tests::pending(&db, "dated", Some(3_600_000), "pending", None);
    db.snapshot(|tx, _| {
        let bins = series(tx, &f, Grain::Hour)?;
        let c = series_coverage(tx, &f, &bins)?;
        assert!(matches!(c[0].state, CoverageState::Complete));
        assert!(matches!(c[1].state, CoverageState::Partial));
        assert!(matches!(c[2].state, CoverageState::Complete));
        Ok(())
    })
    .unwrap();
}
