use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::tests::{filter, ids},
};
use rusqlite::params;
use token_pulse_core::domain::UsageVector;

#[test]
fn pending_date_indexes_avoid_visiting_unrelated_records_and_preserve_unknown_time() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let (_directory, db) = setup();
    db.commit(fixture()).unwrap();
    pending(&db, "within", Some(2000), "pending", None);
    pending(&db, "unknown", None, "unattributed", None);
    db.write(|conn| {
        conn.execute_batch("WITH RECURSIVE rows(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM rows WHERE n<2000) INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,observed_at_ms,normalized_json,payload_fingerprint,format_version) SELECT 'old-'||n,'generation',1000+n*100,1100+n*100,'session','usage',9000,'{}','old-'||n,'fixture' FROM rows; INSERT INTO pending_usage SELECT observation_id,'ledger',observation_id,'pending','fixture',NULL,'{}' FROM observations WHERE observation_id LIKE 'old-%';")?;
        Ok(())
    }).unwrap();
    db.snapshot(|tx, _| {
        let steps = Arc::new(AtomicUsize::new(0));
        let counted = steps.clone();
        tx.progress_handler(
            1,
            Some(move || {
                counted.fetch_add(1, Ordering::Relaxed);
                false
            }),
        )?;
        let counts = pending_counts(tx, &filter())?;
        tx.progress_handler(0, None::<fn() -> bool>)?;
        assert_eq!(counts.0, 1);
        assert_eq!(counts.1, 1);
        assert!(counts.2.is_none()); // Unknown amount must stay unknown.
        assert_eq!(counts.3, 1);
        assert!(
            steps.load(Ordering::Relaxed) < 1000,
            "unrelated rows must not be visited"
        );
        Ok(())
    })
    .unwrap();
}

pub(super) fn pending(
    db: &Database,
    id: &str,
    time: Option<i64>,
    kind: &str,
    vector: Option<UsageVector>,
) {
    let id = id.to_owned();
    let kind = kind.to_owned();
    db.write(move|conn| {
        let tx=conn.transaction()?;
        let offset:i64=tx.query_row("SELECT COALESCE(MAX(byte_end),0) FROM observations",[],|r|r.get(0))?;
        tx.execute("INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,observed_at_ms,normalized_json,payload_fingerprint,format_version) VALUES(?1,'generation',?2,?3,'session','usage',?4,'{}',?1,'fixture')",params![id,offset,offset+100,time])?;
        tx.execute("INSERT INTO pending_usage VALUES(?1,'ledger',?1,?2,'fixture',?3,'{}')",params![id,kind,vector.map(|v|serde_json::to_string(&v)).transpose()?])?;
        tx.commit()?; Ok(())
    }).unwrap();
}
pub(super) fn vector(total: i64) -> UsageVector {
    UsageVector {
        reported_total: Some(total),
        ..Default::default()
    }
}

#[test]
fn absent_manifest_keeps_coverage_unknown_even_for_known_breakdown_and_empty_filter() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert!(matches!(c.state, CoverageState::Unknown));
    assert!(!c.breakdown_complete); // Legacy cache writes remain unknown.
    assert_eq!(c.pending_file_count.as_str(), "0");
    assert_eq!(c.pending_observation_count.as_str(), "0");
    assert!(c.unattributed_total_tokens.is_none());
    db.write(|conn| {
        conn.execute(
            "UPDATE sources SET readability='readable',last_success_at_ms=1234",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert!(matches!(c.state, CoverageState::Unknown));
    assert_eq!(c.source_issues[0].code, "scan_evidence_missing");
    assert_eq!(c.source_issues[0].last_success_ms.unwrap().value(), 1234);
    let mut f = filter();
    f.sources = ids(&[], true);
    let c = db.usage_coverage(&f).unwrap();
    assert!(matches!(c.state, CoverageState::Unknown));
    assert!(c.source_issues.is_empty());
    assert!(!c.breakdown_complete);
    assert_eq!(c.pending_file_count.as_str(), "0");
}

#[test]
fn pending_time_unknown_is_included_but_duplicate_inherited_and_other_dates_are_not_gaps() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    pending(&db, "within", Some(2000), "pending", Some(vector(7)));
    pending(&db, "unknown-time", None, "pending", None);
    pending(&db, "at-end", Some(5000), "pending", Some(vector(19)));
    pending(&db, "before", Some(-1), "pending", Some(vector(23)));
    pending(&db, "duplicate", Some(2000), "duplicate", None);
    pending(&db, "inherited", Some(2000), "inherited", None);
    let c = db.usage_coverage(&filter()).unwrap();
    assert!(matches!(c.state, CoverageState::Partial));
    assert_eq!(c.pending_observation_count.as_str(), "2");
    assert_eq!(c.unattributed_observation_count.as_str(), "0");
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "110"
    );
    let mut f = filter();
    f.models = ids(&[], false);
    assert_eq!(
        db.usage_coverage(&f)
            .unwrap()
            .pending_observation_count
            .as_str(),
        "0"
    );
    f.models = ids(&[], true);
    assert_eq!(
        db.usage_coverage(&f)
            .unwrap()
            .pending_observation_count
            .as_str(),
        "2"
    );
    f.sources = ids(&["source') OR 1=1 --"], true);
    assert_eq!(
        db.usage_coverage(&f)
            .unwrap()
            .pending_observation_count
            .as_str(),
        "0"
    );
    f.sources = DimensionSelection::All {};
    f.sessions = ids(&[], true);
    assert_eq!(
        db.usage_coverage(&f)
            .unwrap()
            .pending_observation_count
            .as_str(),
        "0"
    );
    db.write(|conn| {conn.execute("INSERT INTO ledger_generations VALUES('candidate','session','candidate','fixture','fixture',0,0,0,'{}')",[])?;conn.execute("UPDATE pending_usage SET ledger_id='candidate' WHERE pending_id='within'",[])?;Ok(())}).unwrap();
    assert_eq!(
        db.usage_coverage(&filter())
            .unwrap()
            .pending_observation_count
            .as_str(),
        "1"
    );
}

#[test]
fn unattributed_amount_is_checked_i128_and_null_if_any_vector_is_unknown_or_invalid() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    pending(
        &db,
        "huge-a",
        Some(2000),
        "unattributed",
        Some(vector(i64::MAX)),
    );
    pending(
        &db,
        "huge-b",
        Some(3000),
        "unattributed",
        Some(vector(i64::MAX)),
    );
    let c = db.usage_coverage(&filter()).unwrap();
    assert_eq!(
        c.unattributed_total_tokens.unwrap().as_str(),
        "18446744073709551614"
    );
    assert_eq!(c.unattributed_observation_count.as_str(), "2");
    pending(&db, "unknown", None, "unattributed", None);
    assert!(
        db.usage_coverage(&filter())
            .unwrap()
            .unattributed_total_tokens
            .is_none()
    );
    db.write(|conn| {
        conn.execute("DELETE FROM pending_usage WHERE pending_id='unknown'", [])?;
        Ok(())
    })
    .unwrap();
    pending(
        &db,
        "invalid",
        None,
        "unattributed",
        Some(UsageVector {
            input_total: Some(4),
            cached_input: Some(5),
            reported_total: Some(4),
            ..Default::default()
        }),
    );
    assert!(
        db.usage_coverage(&filter())
            .unwrap()
            .unattributed_total_tokens
            .is_none()
    );
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "110"
    );
}

#[test]
fn whole_source_file_and_format_gaps_survive_date_and_model_filters_and_old_snapshot() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let mut f = filter();
    f.models = ids(&["does-not-match"], false);
    db.snapshot(|tx,_| {
        let t=super::super::totals(tx,&f)?;
        db.write(|conn| {
            conn.execute("UPDATE file_generations SET observed_size=200",[])?;
            conn.execute("UPDATE sources SET readability='unreadable'",[])?;
            conn.execute("INSERT INTO diagnostics(diagnostic_id,source_id,code,severity,metadata_json,dedup_key,occurrences,first_seen_at_ms,last_seen_at_ms) VALUES('format','source','UNSUPPORTED_FORMAT','warning','{\"parser_version\":\"fixture-v2\"}','format',3,8000,8000)",[])?; Ok(())
        }).unwrap();
        let c=coverage(tx,&f,&t)?; assert!(matches!(c.state,CoverageState::Unknown)); assert_eq!(c.pending_file_count.as_str(),"0"); assert!(c.format_issues.is_empty()); Ok(())
    }).unwrap();
    let c = db.usage_coverage(&f).unwrap();
    assert!(matches!(c.state, CoverageState::Partial));
    assert_eq!(c.pending_file_count.as_str(), "1");
    assert_eq!(c.format_issues[0].format, "fixture-v2");
    assert_eq!(c.format_issues[0].count.as_str(), "3");
    assert_eq!(c.source_issues[0].code, "source_unreadable");
    f.sources = ids(&[], true);
    let c = db.usage_coverage(&f).unwrap();
    assert!(c.format_issues.is_empty() && c.source_issues.is_empty());
    assert_eq!(c.pending_file_count.as_str(), "0");
    db.write(|conn| {
        conn.execute("UPDATE diagnostics SET resolved_at_ms=9000", [])?;
        Ok(())
    })
    .unwrap();
    assert!(
        db.usage_coverage(&filter())
            .unwrap()
            .format_issues
            .is_empty()
    );
}

#[test]
fn periodic_scan_confirmation_is_separate_from_unread_or_new_files() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute("UPDATE sources SET readability='readable'", [])?;
        Ok(())
    })
    .unwrap();
    let scan = || {
        db.begin_source_scan("source".into(), "synthetic".into(), 1)
            .unwrap()
    };
    let h = scan();
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
    assert!(matches!(
        db.usage_coverage(&filter()).unwrap().state,
        CoverageState::Complete
    ));
    let old_total = db.usage_totals(&filter()).unwrap().total_tokens;
    let next = scan();
    db.record_source_scan_files(next.clone(), vec![("synthetic.jsonl".into(), 100)])
        .unwrap();
    db.finish_source_scan(next.clone(), None, 4).unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert_eq!(c.pending_file_count.as_str(), "0");
    assert_eq!(c.verifying_file_count.unwrap().as_str(), "1");
    assert!(matches!(c.state, CoverageState::Unknown));
    assert_eq!(c.source_issues[0].code, "source_scan_verifying");
    assert_eq!(db.usage_totals(&filter()).unwrap().total_tokens, old_total);
    db.confirm_source_scan_file(next, "synthetic.jsonl".into(), "generation".into(), 1, 5)
        .unwrap();
    assert!(matches!(
        db.usage_coverage(&filter()).unwrap().state,
        CoverageState::Complete
    ));

    // An append found by enumeration is unread even before observed_size is updated.
    let appended = scan();
    db.record_source_scan_files(appended.clone(), vec![("synthetic.jsonl".into(), 120)])
        .unwrap();
    db.finish_source_scan(appended, None, 6).unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert_eq!(c.pending_file_count.as_str(), "1");
    assert_eq!(c.verifying_file_count.unwrap().as_str(), "0");
    assert!(matches!(c.state, CoverageState::Partial));

    let new_file = scan();
    db.record_source_scan_files(
        new_file.clone(),
        vec![("synthetic.jsonl".into(), 100), ("unknown.jsonl".into(), 5)],
    )
    .unwrap();
    db.finish_source_scan(new_file, None, 7).unwrap();
    let c = db.usage_coverage(&filter()).unwrap();
    assert_eq!(c.pending_file_count.as_str(), "1");
    assert_eq!(c.verifying_file_count.unwrap().as_str(), "1");
    assert!(matches!(c.state, CoverageState::Partial));
}
