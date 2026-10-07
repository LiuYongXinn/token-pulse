use super::*;
use crate::batch::tests::{fixture, setup};
use crate::query::tests::filter;
#[test]
fn full_vector_reuses_aggregates_and_old_real_snapshot_keeps_old_names_coverage_and_cost() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.snapshot(|tx, revision| {
        let before = db.scope_summary(tx, revision, &filter(), &PriceBasis::EventTime {})?;
        let uncached = compute(tx, revision, &filter(), &PriceBasis::EventTime {})?;
        assert_eq!(
            serde_json::to_value(before.as_ref())?,
            serde_json::to_value(uncached)?
        );
        let again = db.scope_summary(tx, revision, &filter(), &PriceBasis::EventTime {})?;
        assert!(Arc::ptr_eq(&before, &again));
        db.write(|conn| {
            conn.execute("UPDATE sources SET enabled=0", [])?;
            Ok(())
        })?;
        let old = db.scope_summary(tx, revision, &filter(), &PriceBasis::EventTime {})?;
        assert!(Arc::ptr_eq(&before, &old));
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, revision| {
        let current = db.scope_summary(tx, revision, &filter(), &PriceBasis::EventTime {})?;
        assert_eq!(current.totals.total_tokens.as_str(), "110");
        assert_eq!(
            serde_json::to_value(current.as_ref())?,
            serde_json::to_value(compute(tx, revision, &filter(), &PriceBasis::EventTime {})?)?
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(db.summary_cache_stats(), (2, 2));
}
