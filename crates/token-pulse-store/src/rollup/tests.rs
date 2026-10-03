use super::*;
use crate::batch::tests::{fixture, setup};

#[test]
fn ready_cache_preserves_null_coverage_source_sets_and_exact_turn_membership() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute("UPDATE usage_events SET turn_id='turn-one'", [])?;
        Ok(())
    })
    .unwrap();
    let result = db.build_hourly_rollup("ledger", 10).unwrap();
    assert!(!result.already_ready);
    assert_eq!(result.cohort_count, 1);
    assert_eq!(result.evidence_revision, 3);
    db.snapshot(|tx,r| {
        assert_eq!(r.data,1);
        let (sources,sums,event_count,known):(String,String,i64,i64)=tx.query_row("SELECT source_ids_json,token_sums_json,usage_event_count,known_turn_event_count FROM utc_hour_usage_rollups WHERE set_id=?1",[&result.set_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        assert_eq!(sources,"[\"source\"]");assert_eq!(event_count,1);assert_eq!(known,1);
        let sums:crate::aggregate::TokenSums=serde_json::from_str(&sums)?;
        assert_eq!(sums.total.as_str(),"110");
        assert_eq!(sums.measures[2].value.as_ref().unwrap().as_str(),"40");
        assert!(sums.measures[..5].iter().all(|m|m.complete && m.covered_total_tokens.as_str()=="110"));
        assert_eq!(tx.query_row("SELECT turn_id FROM utc_hour_rollup_turns WHERE set_id=?1",[&result.set_id],|r|r.get::<_,String>(0))?,"turn-one");
        assert_eq!(tx.query_row("SELECT state FROM usage_rollup_sets WHERE set_id=?1",[&result.set_id],|r|r.get::<_,String>(0))?,"ready");Ok(())
    }).unwrap();
    let second = db.build_hourly_rollup("ledger", 11).unwrap();
    assert!(second.already_ready);
    assert_eq!(second.set_id, result.set_id);
}

fn prepared(db: &Database) -> Candidate {
    match db
        .snapshot(|tx, _| prepare(tx, "ledger", &AtomicBool::new(false)))
        .unwrap()
    {
        Prepared::Candidate(c) => c,
        Prepared::Ready(_) => panic!("unexpected cache"),
    }
}
#[test]
fn changed_evidence_and_tampered_candidates_never_become_ready() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let candidate = prepared(&db);
    let input = candidate.input.clone();
    let opened = input.clone();
    db.write(move |conn| {
        let tx = conn.transaction()?;
        begin(&tx, &opened, 10)?;
        tx.commit()?;
        Ok(())
    })
    .unwrap();
    let staged = input.clone();
    let mut row = candidate.rows.into_iter().next().unwrap();
    row.sums_json = "{\"total\":\"999\",\"measures\":[]}".into();
    db.write(move |conn| {
        let tx = conn.transaction()?;
        stage_row(&tx, &staged, &row)?;
        tx.commit()?;
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, _| {
        assert_ne!(persisted_digest(tx, &input)?, candidate.digest);
        Ok(())
    })
    .unwrap();
    let rejected = input.clone();
    let expected = candidate.digest;
    assert_eq!(
        db.write(move |conn| {
            let tx = conn.transaction()?;
            publish(&tx, &rejected, &expected, 11)?;
            tx.commit()?;
            Ok(())
        })
        .unwrap_err()
        .code,
        ErrorCode::DbCorrupt
    );
    db.write(|conn| {
        conn.execute("UPDATE event_provenance SET relation='mirror'", [])?;
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(
            fresh(tx, &input, true).unwrap_err().code,
            ErrorCode::CandidateObsolete
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(db.interrupt_rollup_builds().unwrap(), 1);
    let new = db.build_hourly_rollup("ledger", 20).unwrap();
    assert_ne!(new.set_id, input.set_id);
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT state FROM usage_rollup_sets WHERE set_id=?1",
                [&input.set_id],
                |r| r.get::<_, String>(0)
            )?,
            "obsolete"
        );
        assert_eq!(
            tx.query_row("SELECT total_tokens FROM active_usage_events", [], |r| r
                .get::<_, i64>(0))?,
            110
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn interrupted_builds_do_not_touch_facts_and_same_input_can_retry_after_restart() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    assert_eq!(
        db.build_hourly_rollup_interruptible("ledger", 10, &AtomicBool::new(true))
            .unwrap_err()
            .code,
        ErrorCode::JobInterrupted
    );
    let input = prepared(&db).input;
    let opened = input.clone();
    db.write(move |conn| {
        let tx = conn.transaction()?;
        begin(&tx, &opened, 10)?;
        tx.commit()?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.build_hourly_rollup("ledger", 11).unwrap_err().code,
        ErrorCode::RevisionConflict
    );
    assert_eq!(db.interrupt_rollup_builds().unwrap(), 1);
    let result = db.build_hourly_rollup("ledger", 12).unwrap();
    assert_eq!(result.set_id, input.set_id);
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 1);
        assert_eq!(
            tx.query_row(
                "SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            2
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn cache_batches_span_500_cohorts_without_changing_facts_or_precision() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        let tx=conn.transaction()?;
        tx.execute_batch("WITH RECURSIVE n(v) AS (VALUES(1) UNION ALL SELECT v+1 FROM n WHERE v<601) INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,observed_at_ms,model,normalized_json,payload_fingerprint,format_version) SELECT 'obs-'||v,'generation',v*100,v*100+100,'session','usage',v*3600000,'model-'||v,json_object('kind','usage','effective_metadata',json_object('model','model-'||v)),'fp-'||v,'fixture' FROM n;
            INSERT INTO usage_events(event_id,ledger_id,origin_observation_id,occurred_at_ms,episode_id,model,source_total_tokens,total_tokens,calculation_method,quality_json) SELECT observation_id,'ledger',observation_id,observed_at_ms,'episode',model,9223372036854775807,9223372036854775807,'fixture','[\"confirmed\"]' FROM observations WHERE observation_id<>'observation';
            INSERT INTO event_provenance SELECT event_id,origin_observation_id,'origin' FROM usage_events WHERE event_id<>'event';")?;
        tx.commit()?;Ok(())
    }).unwrap();
    let result = db.build_hourly_rollup("ledger", 10).unwrap();
    assert_eq!(result.cohort_count, 602);
    db.snapshot(|tx,_| {
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM utc_hour_usage_rollups WHERE set_id=?1",[&result.set_id],|r|r.get::<_,i64>(0))?,602);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM usage_events",[],|r|r.get::<_,i64>(0))?,602);
        let sum:String=tx.query_row("SELECT sum_token_decimal(total_tokens) FROM usage_events",[],|r|r.get(0))?;
        assert_eq!(sum,(i128::from(i64::MAX)*601+110).to_string());
        let partial:String=tx.query_row("SELECT token_sums_json FROM utc_hour_usage_rollups WHERE set_id=?1 AND hour_start_ms=3600000",[&result.set_id],|r|r.get(0))?;
        let partial:crate::aggregate::TokenSums=serde_json::from_str(&partial)?;
        assert!(partial.measures.iter().all(|m|m.value.is_none() && !m.complete));Ok(())
    }).unwrap();
}

#[test]
fn negative_epoch_hours_use_euclidean_boundaries_and_empty_cache_is_explicit() {
    let (_dir, db) = setup();
    let empty = db.build_hourly_rollup("ledger", 1).unwrap();
    assert_eq!(empty.cohort_count, 0);
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute("UPDATE usage_events SET occurred_at_ms=-1", [])?;
        Ok(())
    })
    .unwrap();
    let result = db.build_hourly_rollup("ledger", 10).unwrap();
    assert_ne!(empty.set_id, result.set_id);
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT hour_start_ms FROM utc_hour_usage_rollups WHERE set_id=?1",
                [&result.set_id],
                |r| r.get::<_, i64>(0)
            )?,
            -3600000
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn publication_rolls_back_on_failure_and_old_snapshots_keep_candidate_invisible() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let candidate = prepared(&db);
    let input = candidate.input.clone();
    let staged = input.clone();
    let rows = candidate.rows;
    db.write(move |conn| {
        let tx = conn.transaction()?;
        begin(&tx, &staged, 10)?;
        for row in rows {
            stage_row(&tx, &staged, &row)?;
        }
        tx.commit()?;
        Ok(())
    })
    .unwrap();
    let rejected = input.clone();
    let expected = candidate.digest.clone();
    assert_eq!(
        db.write(move |conn| {
            let tx = conn.transaction()?;
            publish(&tx, &rejected, &expected, 11)?;
            Err::<(), _>(ErrorCode::DbWriteFailed.into())
        })
        .unwrap_err()
        .code,
        ErrorCode::DbWriteFailed
    );
    db.snapshot(|tx, _| {
        assert!(ready_set(tx, "ledger")?.is_none());
        let published = input.clone();
        let expected = candidate.digest.clone();
        db.write(move |conn| {
            let tx = conn.transaction()?;
            publish(&tx, &published, &expected, 12)?;
            tx.commit()?;
            Ok(())
        })?;
        assert!(ready_set(tx, "ledger")?.is_none());
        assert_eq!(
            tx.query_row(
                "SELECT state FROM usage_rollup_sets WHERE set_id=?1",
                [&input.set_id],
                |r| r.get::<_, String>(0)
            )?,
            "building"
        );
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(
            ready_set(tx, "ledger")?.as_deref(),
            Some(input.set_id.as_str())
        );
        Ok(())
    })
    .unwrap();
    db.write(|conn| {
        conn.execute("UPDATE event_provenance SET relation='mirror'", [])?;
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, _| {
        assert!(ready_set(tx, "ledger")?.is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
fn mirrored_source_cohorts_keep_one_event_with_complete_shared_provenance() {
    use crate::SourceRecord;
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.add_source(SourceRecord {
        source_id: "mirror".into(),
        root_path: "synthetic-mirror".into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    db.write(|conn| {
        let tx=conn.transaction()?;
        tx.execute("INSERT INTO source_files(file_id,source_id,canonical_path,status) VALUES('mirror-file','mirror','mirror.jsonl','known')",[])?;
        tx.execute("INSERT INTO file_generations SELECT 'mirror-generation','mirror-file',state,identity_json,observed_size,committed_offset,checkpoint_revision,anchor_json,reader_context_json,parser_version,created_at_ms FROM file_generations WHERE file_generation_id='generation'",[])?;
        tx.execute("INSERT INTO observations SELECT 'copy','mirror-generation',byte_offset,byte_end,session_key,kind,observed_at_ms,stable_record_id,turn_id,stream_hint,model,project_id,normalized_json,payload_fingerprint,format_version FROM observations WHERE observation_id='observation'",[])?;
        tx.execute("INSERT INTO event_provenance VALUES('event','copy','mirror')",[])?;
        tx.commit()?;Ok(())
    }).unwrap();
    let result = db.build_hourly_rollup("ledger", 10).unwrap();
    db.snapshot(|tx,_| {
        let (sources,sums,count):(String,String,i64)=tx.query_row("SELECT source_ids_json,token_sums_json,usage_event_count FROM utc_hour_usage_rollups WHERE set_id=?1",[&result.set_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        assert_eq!(sources,"[\"mirror\",\"source\"]");assert_eq!(count,1);
        assert_eq!(serde_json::from_str::<crate::aggregate::TokenSums>(&sums)?.total.as_str(),"110");Ok(())
    }).unwrap();
}
