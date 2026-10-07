use super::*;
use crate::batch::tests::{fixture, setup};
use crate::query::tests::{extra, filter, ids};

fn day_filter() -> UsageFilter {
    let mut filter = filter();
    filter.range.end_ms = token_pulse_core::numeric::EpochMs::new(86_400_000).unwrap();
    filter
}

#[test]
fn exact_series_reuses_normalized_scope_and_keeps_old_read_versions() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.snapshot(|tx, _| {
        let mut filter = day_filter();
        filter.sources = ids(&["source", "source"], false);
        let before = db.series_cache().get(tx, &filter, Grain::Day)?;
        assert_eq!(
            serde_json::to_value(before.as_ref())?,
            serde_json::to_value(crate::query::series(tx, &filter, Grain::Day)?)?
        );
        filter.sources = ids(&["source"], false);
        let normalized = db.series_cache().get(tx, &filter, Grain::Day)?;
        assert!(Arc::ptr_eq(&before, &normalized));
        extra(&db, "added", 2000, 7, (None, None), None, None);
        assert!(Arc::ptr_eq(
            &before,
            &db.series_cache().get(tx, &filter, Grain::Day)?
        ));
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, _| {
        let current = db.series_cache().get(tx, &day_filter(), Grain::Day)?;
        assert_eq!(current[0].totals.total_tokens.as_str(), "117");
        let mut other = day_filter();
        other.models = ids(&["missing"], false);
        assert_eq!(
            db.series_cache().get(tx, &other, Grain::Day)?[0]
                .totals
                .total_tokens
                .as_str(),
            "0"
        );
        assert_eq!(
            db.series_cache().get(tx, &day_filter(), Grain::Hour)?.len(),
            24
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn overlapping_reads_share_computation_and_capacity_is_bounded() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let db = db.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                db.snapshot(|tx, _| {
                    barrier.wait();
                    db.series_cache().get(tx, &day_filter(), Grain::Day)
                })
                .unwrap()
            })
        })
        .collect();
    let values: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert!(Arc::ptr_eq(&values[0], &values[1]));
    db.snapshot(|tx, _| {
        for index in 0..30 {
            let mut filter = day_filter();
            filter.models = ids(&[&format!("model-{index}")], false);
            db.series_cache().get(tx, &filter, Grain::Day)?;
        }
        Ok(())
    })
    .unwrap();
    let values = db.series_cache().values.lock().unwrap();
    assert_eq!(values.len(), LIMIT);
    assert!(values.iter().map(|entry| entry.2).sum::<usize>() <= BUDGET);
    assert!(
        db.series_cache()
            .flights
            .lock()
            .unwrap()
            .values()
            .all(|lock| lock.strong_count() == 0)
    );
}
