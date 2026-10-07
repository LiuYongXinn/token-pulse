use super::*;
use crate::batch::tests::{fixture, setup};
use crate::query::tests::filter;
#[test]
fn simultaneous_same_version_queries_share_one_computation_and_release_flight_keys() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let db = db.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                db.snapshot(|tx, revision| {
                    barrier.wait();
                    db.scope_summary(tx, revision, &filter(), &PriceBasis::EventTime {})
                })
                .unwrap()
            })
        })
        .collect();
    let summaries: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert!(Arc::ptr_eq(&summaries[0], &summaries[1]));
    assert_eq!(db.summary_cache_stats(), (1, 1));
    assert!(db.summary_cache().flights.lock().unwrap().is_empty());
}
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
