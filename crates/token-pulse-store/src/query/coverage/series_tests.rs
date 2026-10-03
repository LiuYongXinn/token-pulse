use super::tests::{pending, vector};
use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::{series, tests::filter},
};
use token_pulse_core::calendar::Grain;

#[test]
fn each_bucket_has_its_own_gaps_and_unknown_times_apply_to_every_bucket() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let mut f = filter();
    f.range.end_ms = EpochMs::new(10_800_000).unwrap();
    pending(&db, "one-hour", Some(3_600_000), "pending", None);
    pending(&db, "at-end", Some(10_800_000), "pending", None);
    db.snapshot(|tx, _| {
        let bins = series(tx, &f, Grain::Hour)?;
        let c = series_coverage(tx, &f, &bins)?;
        assert_eq!(
            c.iter()
                .map(|c| c.pending_observation_count.as_str())
                .collect::<Vec<_>>(),
            ["0", "1", "0"]
        );
        assert!(matches!(c[0].state, CoverageState::Unknown));
        assert!(matches!(c[1].state, CoverageState::Partial));
        assert!(matches!(c[2].state, CoverageState::Unknown));
        assert!(!c[0].breakdown_complete); // Legacy cache writes remain unknown.
        assert!(!c[1].breakdown_complete);
        Ok(())
    })
    .unwrap();
    pending(&db, "unknown-time", None, "pending", None);
    pending(
        &db,
        "known-amount",
        Some(7_200_000),
        "unattributed",
        Some(vector(11)),
    );
    pending(
        &db,
        "unknown-time-amount",
        None,
        "unattributed",
        Some(vector(5)),
    );
    db.snapshot(|tx, _| {
        let bins = series(tx, &f, Grain::Hour)?;
        let before = series_coverage(tx, &f, &bins)?;
        assert_eq!(
            before
                .iter()
                .map(|c| c.pending_observation_count.as_str())
                .collect::<Vec<_>>(),
            ["1", "2", "1"]
        );
        assert_eq!(
            before
                .iter()
                .map(|c| c.unattributed_observation_count.as_str())
                .collect::<Vec<_>>(),
            ["1", "1", "2"]
        );
        assert_eq!(
            before
                .iter()
                .map(|c| c.unattributed_total_tokens.as_ref().unwrap().as_str())
                .collect::<Vec<_>>(),
            ["5", "5", "16"]
        );
        assert!(
            before
                .iter()
                .all(|c| matches!(c.state, CoverageState::Partial))
        );
        pending(&db, "null-vector", None, "unattributed", None);
        assert_eq!(
            serde_json::to_value(series_coverage(tx, &f, &bins)?).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, _| {
        let bins = series(tx, &f, Grain::Hour)?;
        let c = series_coverage(tx, &f, &bins)?;
        assert!(c.iter().all(|c| c.unattributed_total_tokens.is_none()));
        assert_eq!(c[2].unattributed_observation_count.as_str(), "3");
        Ok(())
    })
    .unwrap();
}

#[test]
fn common_source_gaps_survive_empty_bins_and_invalid_bucket_sequences_are_rejected() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let mut f = filter();
    f.range.end_ms = EpochMs::new(10_800_000).unwrap();
    db.write(|conn| {
        conn.execute("UPDATE sources SET readability='unreadable'", [])?;
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, _| {
        let mut bins = series(tx, &f, Grain::Hour)?;
        let c = series_coverage(tx, &f, &bins)?;
        assert!(c.iter().all(|c| matches!(c.state, CoverageState::Partial)
            && c.source_issues[0].code == "source_unreadable"));
        assert_eq!(c[2].pending_observation_count.as_str(), "0");
        assert_eq!(c[2].unattributed_observation_count.as_str(), "0");
        assert!(c[2].unattributed_total_tokens.is_none());
        assert_eq!(
            series_coverage(tx, &f, &[]).unwrap_err().code,
            ErrorCode::InvalidQuery
        );
        bins[1].bucket.start_ms = EpochMs::new(3_600_001).unwrap();
        assert_eq!(
            series_coverage(tx, &f, &bins).unwrap_err().code,
            ErrorCode::InvalidQuery
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn repeated_local_dst_hours_keep_distinct_utc_gap_boundaries() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let mut f = filter();
    f.range.start_ms = EpochMs::new(1_793_509_200_000).unwrap();
    f.range.end_ms = EpochMs::new(1_793_516_400_000).unwrap();
    f.range.timezone = "America/New_York".into();
    pending(
        &db,
        "first-local-hour",
        Some(1_793_512_799_999),
        "pending",
        None,
    );
    pending(
        &db,
        "second-local-hour",
        Some(1_793_512_800_000),
        "unattributed",
        Some(vector(7)),
    );
    db.snapshot(|tx, _| {
        let bins = series(tx, &f, Grain::Hour)?;
        assert_eq!(bins.len(), 2);
        assert_eq!(bins[0].bucket.utc_offset, "-04:00");
        assert_eq!(bins[1].bucket.utc_offset, "-05:00");
        let c = series_coverage(tx, &f, &bins)?;
        assert_eq!(c[0].pending_observation_count.as_str(), "1");
        assert_eq!(c[0].unattributed_observation_count.as_str(), "0");
        assert_eq!(c[1].pending_observation_count.as_str(), "0");
        assert_eq!(
            c[1].unattributed_total_tokens.as_ref().unwrap().as_str(),
            "7"
        );
        Ok(())
    })
    .unwrap();
}
