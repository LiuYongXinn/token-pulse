use super::tests::{fixture, setup};
use super::*;
use crate::query::{self, tests::filter};
use token_pulse_core::{
    calendar::Grain,
    numeric::EpochMs,
    protocol::PriceBasis,
    query::{UsageEventSort, UsageEventsQuery, UsageEventsRequest},
};

fn batch(writes: Option<i64>) -> WriteBatch {
    let mut b = fixture();
    b.events[0].usage.cache_write_input = writes;
    b.streams[0].baseline.cache_write_input = writes;
    if let NormalizedObservation::Usage(u) = &mut b.observations[0].record {
        u.last.as_mut().unwrap().cache_write_input = writes;
        u.cumulative.as_mut().unwrap().cache_write_input = writes;
    }
    b
}

#[test]
fn cache_write_facts_baselines_and_checkpoint_commit_together_and_survive_reopen() {
    for writes in [None, Some(0), Some(20)] {
        let (dir, db) = setup();
        let failed = batch(writes);
        assert_eq!(
            db.write(move |conn| commit_batch(conn, failed, |stage| {
                if stage == CommitStage::BeforeCommit {
                    Err(ErrorCode::DiskFull.into())
                } else {
                    Ok(())
                }
            }))
            .unwrap_err()
            .code,
            ErrorCode::DiskFull
        );
        db.snapshot(|tx, _| {
            for table in ["usage_events", "observations", "stream_states"] {
                assert_eq!(
                    tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))?,
                    0
                );
            }
            assert_eq!(
                tx.query_row("SELECT committed_offset FROM file_generations", [], |r| r
                    .get::<_, i64>(
                    0
                ))?,
                0
            );
            Ok(())
        })
        .unwrap();
        db.commit(batch(writes)).unwrap();
        drop(db);
        let reopened = crate::Database::open(dir.path()).unwrap();
        reopened
            .snapshot(|tx, _| {
                assert_eq!(
                    tx.query_row(
                        "SELECT cache_write_input_tokens FROM usage_events",
                        [],
                        |r| r.get::<_, Option<i64>>(0)
                    )?,
                    writes
                );
                let baseline: String =
                    tx.query_row("SELECT baseline_json FROM stream_states", [], |r| r.get(0))?;
                assert_eq!(
                    serde_json::from_str::<UsageVector>(&baseline)?.cache_write_input,
                    writes
                );
                assert_eq!(
                    tx.query_row("SELECT committed_offset FROM file_generations", [], |r| r
                        .get::<_, i64>(
                        0
                    ))?,
                    100
                );
                Ok(())
            })
            .unwrap();
        let totals = reopened.usage_totals(&filter()).unwrap();
        assert_eq!(totals.total_tokens.as_str(), "110");
        assert_eq!(
            totals.cache_write_input.value.as_ref().map(|n| n.as_str()),
            writes.map(|n| n.to_string()).as_deref()
        );
        assert_eq!(totals.cache_write_input.complete, writes.is_some());
    }
}

#[test]
fn cache_write_live_snapshot_rollup_and_event_evidence_share_exact_included_counts() {
    let (_dir, db) = setup();
    db.commit(batch(Some(20))).unwrap();
    db.build_hourly_rollup("ledger", 10).unwrap();
    db.snapshot(|tx, _| {
        let old = query::totals(tx, &filter())?;
        assert_eq!(old.cache_write_input.value.unwrap().as_str(), "20");
        let mut next = fixture();
        next.expected_offset = 100;
        next.expected_checkpoint_revision = 1;
        next.next_offset = 200;
        next.observed_size = 200;
        let usage = UsageVector {
            input_total: Some(50),
            cached_input: Some(10),
            cache_write_input: Some(5),
            output_total: Some(5),
            reasoning_output: Some(1),
            reported_total: Some(55),
        };
        let cumulative = UsageVector {
            input_total: Some(150),
            cached_input: Some(70),
            cache_write_input: Some(25),
            output_total: Some(15),
            reasoning_output: Some(3),
            reported_total: Some(165),
        };
        next.observations[0].observation_id = "second-observation".into();
        if let NormalizedObservation::Usage(u) = &mut next.observations[0].record {
            u.physical_position.byte_offset = 100;
            u.physical_position.byte_end = 200;
            u.event_time_ms = Some(2000);
            u.explicit_episode_start = false;
            u.last = Some(usage);
            u.cumulative = Some(cumulative);
        }
        next.events[0].event_id = "second-event".into();
        next.events[0].origin_observation_id = "second-observation".into();
        next.events[0].occurred_at_ms = 2000;
        next.events[0].usage = usage;
        next.streams[0].observation_id = "second-observation".into();
        next.streams[0].baseline = cumulative;
        next.streams[0].expected_state_revision = Some(1);
        db.commit(next)?;
        assert_eq!(query::totals(tx, &filter())?.total_tokens.as_str(), "110");
        assert_eq!(
            query::totals(tx, &filter())?
                .cache_write_input
                .value
                .unwrap()
                .as_str(),
            "20"
        );
        Ok(())
    })
    .unwrap();
    let raw = db.usage_totals(&filter()).unwrap();
    assert_eq!(raw.total_tokens.as_str(), "165");
    assert_eq!(raw.cache_write_input.value.unwrap().as_str(), "25");
    assert!(db.usage_coverage(&filter()).unwrap().breakdown_complete);
    db.build_hourly_rollup("ledger", 20).unwrap();
    db.snapshot(|tx, _| {
        let cached = query::totals(tx, &filter())?;
        assert_eq!(cached.total_tokens.as_str(), "165");
        assert_eq!(cached.cache_write_input.value.unwrap().as_str(), "25");
        let hours = query::series(tx, &filter(), Grain::Hour)?;
        assert_eq!(
            hours[0]
                .totals
                .cache_write_input
                .value
                .as_ref()
                .unwrap()
                .as_str(),
            "25"
        );
        Ok(())
    })
    .unwrap();
    let events = db
        .query_usage_events(
            "main",
            &UsageEventsRequest {
                query: UsageEventsQuery {
                    filter: filter(),
                    price_basis: PriceBasis::EventTime {},
                    sort: UsageEventSort::TimeDesc,
                    page_size: 50,
                },
                cursor: None,
            },
            EpochMs::new(3000).unwrap(),
        )
        .unwrap();
    assert_eq!(
        events.events[0]
            .usage
            .cache_write_input
            .as_ref()
            .unwrap()
            .value(),
        5
    );
    assert_eq!(
        events.events[0]
            .raw_last
            .as_ref()
            .unwrap()
            .cache_write_input
            .as_ref()
            .unwrap()
            .value(),
        5
    );
    assert_eq!(
        events.events[0]
            .raw_cumulative
            .as_ref()
            .unwrap()
            .cache_write_input
            .as_ref()
            .unwrap()
            .value(),
        25
    );
}

#[test]
fn old_schema_upgrade_keeps_unknown_writes_and_sql_rejects_overlapping_input_categories() {
    let (dir, db) = setup();
    db.commit(fixture()).unwrap();
    // Own isolated prior schema, without the additive column; no real database is used.
    db.write(|conn| { conn.execute_batch("ALTER TABLE usage_events DROP COLUMN cache_write_input_tokens; DELETE FROM schema_migrations WHERE version=11; UPDATE app_state SET schema_version=10 WHERE singleton=1; PRAGMA user_version=10;")?; Ok(()) }).unwrap();
    drop(db);
    let db = crate::Database::open(dir.path()).unwrap();
    let totals = db.usage_totals(&filter()).unwrap();
    assert_eq!(totals.total_tokens.as_str(), "110");
    assert!(totals.cache_write_input.value.is_none());
    for invalid in [-1, 41, 101] {
        assert!(
            db.write(move |conn| {
                conn.execute(
                    "UPDATE usage_events SET cache_write_input_tokens=?1",
                    [invalid],
                )?;
                Ok(())
            })
            .is_err()
        );
    }
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT committed_offset FROM file_generations", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            100
        );
        assert_eq!(
            tx.query_row(
                "SELECT cache_write_input_tokens FROM usage_events",
                [],
                |r| r.get::<_, Option<i64>>(0)
            )?,
            None
        );
        Ok(())
    })
    .unwrap();
}
