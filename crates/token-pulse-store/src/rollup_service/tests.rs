use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::tests::{extra, filter},
};
use std::time::Instant;

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(Instant::now() < deadline, "cache worker deadline exceeded");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn worker_builds_new_evidence_once_and_shutdown_is_idempotent() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    use token_pulse_core::{
        jobs::{JobRequest, JobScope},
        protocol::JobKind,
    };
    db.create_job(
        "priority".into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::All {},
            request_key: "priority".into(),
        },
        1,
    )
    .unwrap();
    assert!(db.next_rollup_ledger().unwrap().is_none());
    db.cancel_job("priority".into(), 2).unwrap();
    assert_eq!(db.next_rollup_ledger().unwrap().as_deref(), Some("ledger"));
    let service =
        RollupService::start_with_interval(db.clone(), Duration::from_millis(50)).unwrap();
    wait_until(|| service.status().completed_builds >= 1);
    assert!(db.next_rollup_ledger().unwrap().is_none());
    let mut f = filter();
    f.range.end_ms = token_pulse_core::numeric::EpochMs::new(7_200_000).unwrap();
    assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "110");
    extra(&db, "new", 2000, 7, (None, None), None, None);
    service.wake();
    wait_until(|| service.status().completed_builds >= 2);
    wait_until(|| {
        db.snapshot(|tx, _| {
            Ok(
                tx.query_row("SELECT COUNT(*) FROM usage_rollup_sets", [], |r| {
                    r.get::<_, i64>(0)
                })?,
            )
        })
        .unwrap()
            == 1
    });
    assert_eq!(service.status().completed_builds, 2);
    assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "117");
    assert!(service.status().last_error.is_none());
    service.shutdown();
    service.shutdown();
    assert!(service.status().building_ledger.is_none());
    assert!(RollupService::start_with_interval(db.clone(), Duration::ZERO).is_err());
}

#[test]
fn pruning_is_bounded_keeps_facts_and_old_real_snapshot_and_retains_failure_marker() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute_batch("BEGIN;
          UPDATE usage_events SET turn_id='original';
          WITH RECURSIVE n(v) AS(VALUES(1) UNION ALL SELECT v+1 FROM n WHERE v<602)
          INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,observed_at_ms,normalized_json,payload_fingerprint,format_version)
          SELECT 'gc-'||v,'generation',v*100,v*100+100,'session','usage',v*3600000,'{}','gc-'||v,'fixture' FROM n;
          INSERT INTO usage_events(event_id,ledger_id,origin_observation_id,occurred_at_ms,episode_id,turn_id,total_tokens,calculation_method,quality_json)
          SELECT observation_id,'ledger',observation_id,observed_at_ms,'episode',observation_id,1,'fixture','[]' FROM observations WHERE observation_id LIKE 'gc-%';
          COMMIT;")?; Ok(())
    }).unwrap();
    let result = db.build_hourly_rollup("ledger", 10).unwrap();
    assert_eq!(result.cohort_count, 603);
    db.snapshot(|tx, revision| {
        db.write(|conn| {
            conn.execute("UPDATE event_provenance SET relation='mirror'", [])?;
            Ok(())
        })
        .unwrap();
        let mut reclaimed = Vec::new();
        for _ in 0..5 {
            reclaimed.push(db.prune_rollups_step()?);
        }
        assert_eq!(reclaimed, vec![500, 103, 500, 103, 1]);
        assert_eq!(
            crate::rollup::ready_set(tx, "ledger")?.as_deref(),
            Some(result.set_id.as_str())
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM utc_hour_usage_rollups", [], |r| r
                .get::<_, i64>(0))?,
            603
        );
        assert_eq!(
            tx.query_row("SELECT data_revision FROM app_state", [], |r| r
                .get::<_, i64>(0))?,
            revision.data
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(db.next_rollup_ledger().unwrap().as_deref(), Some("ledger"));
    let new = db.build_hourly_rollup("ledger", 20).unwrap();
    db.write(move |conn| {
        conn.execute(
            "UPDATE usage_rollup_sets SET state='failed' WHERE set_id=?1",
            [new.set_id],
        )?;
        Ok(())
    })
    .unwrap();
    for _ in 0..5 {
        assert!(db.prune_rollups_step().unwrap() <= 500);
    }
    assert!(db.next_rollup_ledger().unwrap().is_none());
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*),SUM(total_tokens) FROM active_usage_events",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            )?,
            (603, 712)
        );
        assert_eq!(
            tx.query_row("SELECT state FROM usage_rollup_sets", [], |r| r
                .get::<_, String>(0))?,
            "failed"
        );
        Ok(())
    })
    .unwrap();
    assert!(!db.build_hourly_rollup("ledger", 30).unwrap().already_ready);
}

#[test]
fn oversized_projection_failure_is_persistent_until_evidence_changes() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute_batch("BEGIN;
          WITH RECURSIVE n(v) AS(VALUES(1) UNION ALL SELECT v+1 FROM n WHERE v<32769)
          INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,observed_at_ms,normalized_json,payload_fingerprint,format_version)
          SELECT 'limit-'||v,'generation',v*100,v*100+100,'session','usage',v*3600000,'{}','limit-'||v,'fixture' FROM n;
          INSERT INTO usage_events(event_id,ledger_id,origin_observation_id,occurred_at_ms,episode_id,total_tokens,calculation_method,quality_json)
          SELECT observation_id,'ledger',observation_id,observed_at_ms,'episode',1,'fixture','[]' FROM observations WHERE observation_id LIKE 'limit-%';
          COMMIT;")?; Ok(())
    }).unwrap();
    assert_eq!(
        db.build_hourly_rollup("ledger", 10).unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    assert!(db.next_rollup_ledger().unwrap().is_none());
    db.write(|conn| {
        conn.execute(
            "UPDATE usage_events SET occurred_at_ms=1000 WHERE event_id LIKE 'limit-%'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(db.next_rollup_ledger().unwrap().as_deref(), Some("ledger"));
    assert_eq!(
        db.build_hourly_rollup("ledger", 20).unwrap().cohort_count,
        2
    );
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "32879"
    );
}
