use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::{
        raw_totals,
        tests::{extra, filter, ids},
        totals,
    },
};
use token_pulse_core::{numeric::EpochMs, query::model_key};

fn assert_same(tx: &Transaction<'_>, f: &UsageFilter) -> StoreResult<()> {
    let cached = try_totals(tx, f)?.expect("cache selected");
    assert_eq!(
        serde_json::to_value(&cached)?,
        serde_json::to_value(raw_totals(tx, f)?)?
    );
    Ok(())
}
#[test]
fn full_hours_and_partial_edges_preserve_turn_identity_and_unknown_breakdown() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute("UPDATE usage_events SET turn_id='common'", [])?;
        Ok(())
    })
    .unwrap();
    extra(
        &db,
        "middle",
        3_600_000,
        7,
        (Some("middle"), None),
        Some("common"),
        None,
    );
    extra(&db, "last", 7_200_000, 19, (None, None), Some("late"), None);
    extra(
        &db,
        "excluded",
        7_200_500,
        31,
        (None, None),
        Some("excluded"),
        None,
    );
    db.build_hourly_rollup("ledger", 10).unwrap();
    let mut f = filter();
    f.range.start_ms = EpochMs::new(500).unwrap();
    f.range.end_ms = EpochMs::new(7_200_500).unwrap();
    db.snapshot(|tx, _| {
        assert_same(tx, &f)?;
        let t = totals(tx, &f)?;
        assert_eq!(t.total_tokens.as_str(), "136");
        assert_eq!(t.usage_event_count.as_str(), "3");
        assert_eq!(t.session_count.as_str(), "1");
        assert_eq!(t.reliable_turn_count.unwrap().as_str(), "2");
        assert!(t.reliable_turns_complete);
        assert_eq!(t.input_total.value.unwrap().as_str(), "100");
        assert_eq!(t.input_total.covered_total_tokens.as_str(), "110");
        assert!(!t.input_total.complete);
        Ok(())
    })
    .unwrap();
    f.range.start_ms = EpochMs::new(0).unwrap();
    f.range.end_ms = EpochMs::new(10_800_000).unwrap();
    db.snapshot(|tx, _| {
        assert_same(tx, &f)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn cache_filters_bind_models_sources_and_unknowns_with_the_same_fact_semantics() {
    use crate::SourceRecord;
    use rusqlite::params;
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(
        &db,
        "named",
        2000,
        7,
        (Some("未知模型"), Some("provider")),
        None,
        None,
    );
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
        conn.execute("INSERT INTO source_files(file_id,source_id,canonical_path,status) VALUES('mirror-file','mirror','mirror.jsonl','known')",[])?;
        conn.execute("INSERT INTO file_generations SELECT 'mirror-generation','mirror-file',state,identity_json,observed_size,committed_offset,checkpoint_revision,anchor_json,reader_context_json,parser_version,created_at_ms FROM file_generations WHERE file_generation_id='generation'",[])?;
        for i in 0..2 {
            conn.execute("INSERT INTO observations SELECT ?1,'mirror-generation',?2,?3,session_key,kind,observed_at_ms,stable_record_id,turn_id,stream_hint,model,project_id,normalized_json,payload_fingerprint,format_version FROM observations WHERE observation_id='observation'",params![format!("copy-{i}"),i*100,i*100+100])?;
            conn.execute("INSERT INTO event_provenance VALUES('event',?1,'mirror')",[format!("copy-{i}")])?;
        }Ok(())
    }).unwrap();
    db.build_hourly_rollup("ledger", 10).unwrap();
    let mut f = filter();
    f.range.end_ms = EpochMs::new(7_200_000).unwrap();
    db.snapshot(|tx, _| {
        for (selected, expected) in [
            (vec!["source"], "117"),
            (vec!["mirror"], "110"),
            (vec!["source", "mirror"], "117"),
            (vec![], "0"),
            (vec!["source') OR 1=1 --"], "0"),
        ] {
            f.sources = ids(&selected, true);
            assert_same(tx, &f)?;
            assert_eq!(totals(tx, &f)?.total_tokens.as_str(), expected);
        }
        f.sources = DimensionSelection::All {};
        let key = model_key(Some("provider"), Some("未知模型")).unwrap();
        for (selected, unknown, expected) in [
            (vec![key.as_str()], false, "7"),
            (vec![], true, "110"),
            (vec![key.as_str()], true, "117"),
        ] {
            f.models = ids(&selected, unknown);
            assert_same(tx, &f)?;
            assert_eq!(totals(tx, &f)?.total_tokens.as_str(), expected);
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn stale_cache_falls_back_and_old_read_snapshot_keeps_its_matching_revision() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.build_hourly_rollup("ledger", 10).unwrap();
    let mut f = filter();
    f.range.end_ms = EpochMs::new(7_200_000).unwrap();
    db.snapshot(|tx, _| {
        assert_same(tx, &f)?;
        let writer = db.clone();
        std::thread::spawn(move || extra(&writer, "new", 2000, 7, (None, None), None, None))
            .join()
            .unwrap();
        assert_same(tx, &f)?;
        assert_eq!(totals(tx, &f)?.total_tokens.as_str(), "110");
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, _| {
        assert!(try_totals(tx, &f)?.is_none());
        assert_eq!(totals(tx, &f)?.total_tokens.as_str(), "117");
        Ok(())
    })
    .unwrap();
    db.build_hourly_rollup("ledger", 20).unwrap();
    db.snapshot(|tx, _| {
        assert_same(tx, &f)?;
        assert_eq!(totals(tx, &f)?.total_tokens.as_str(), "117");
        Ok(())
    })
    .unwrap();
}

#[test]
fn cached_and_uncached_ledgers_count_sessions_and_equal_turn_ids_separately() {
    use crate::{FileRegistration, SessionRegistration};
    use token_pulse_core::domain::{NormalizedObservation, ReaderContext, UsageVector};
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute("UPDATE usage_events SET turn_id='common'", [])?;
        Ok(())
    })
    .unwrap();
    db.build_hourly_rollup("ledger", 10).unwrap();
    db.ensure_session(SessionRegistration {
        session_key: "second".into(),
        provider_session_id: Some("second-provider".into()),
        parent_key: None,
        parent_provider_id: None,
        created_at_ms: None,
        ledger_id: "second-ledger".into(),
        registered_at_ms: 1,
    })
    .unwrap();
    db.register_file(FileRegistration {
        file_id: "second-file".into(),
        source_id: "source".into(),
        canonical_path: "second.jsonl".into(),
        file_identity: None,
        file_generation_id: "second-generation".into(),
        observed_size: 100,
        created_at_ms: 1,
        reader_context: ReaderContext::default(),
    })
    .unwrap();
    let mut b = fixture();
    b.file_generation_id = "second-generation".into();
    b.ledgers[0].session_key = "second".into();
    b.ledgers[0].ledger_id = "second-ledger".into();
    b.observations[0].observation_id = "second-observation".into();
    b.observations[0].session_key = Some("second".into());
    let usage = UsageVector {
        input_total: Some(6),
        cached_input: Some(0),
        output_total: Some(1),
        reasoning_output: Some(0),
        reported_total: Some(7),
    };
    if let NormalizedObservation::Usage(u) = &mut b.observations[0].record {
        u.session_key = "second".into();
        u.physical_position.file_generation_id = "second-generation".into();
        u.last = Some(usage);
        u.cumulative = Some(usage);
        u.effective_metadata.turn_id = Some("common".into());
    }
    b.events[0].event_id = "second-event".into();
    b.events[0].ledger_id = "second-ledger".into();
    b.events[0].origin_observation_id = "second-observation".into();
    b.events[0].usage = usage;
    b.events[0].turn_id = Some("common".into());
    b.streams[0].ledger_id = "second-ledger".into();
    b.streams[0].observation_id = "second-observation".into();
    b.streams[0].baseline = usage;
    db.commit(b).unwrap();
    let mut f = filter();
    f.range.end_ms = EpochMs::new(7_200_000).unwrap();
    db.snapshot(|tx, _| {
        assert_same(tx, &f)?;
        let t = totals(tx, &f)?;
        assert_eq!(t.total_tokens.as_str(), "117");
        assert_eq!(t.session_count.as_str(), "2");
        assert_eq!(t.reliable_turn_count.unwrap().as_str(), "2");
        assert!(t.reliable_turns_complete);
        assert_eq!(t.input_total.value.unwrap().as_str(), "106");
        assert!(t.input_total.complete);
        f.sessions = ids(&["second"], false);
        assert_same(tx, &f)?;
        assert_eq!(totals(tx, &f)?.total_tokens.as_str(), "7");
        Ok(())
    })
    .unwrap();
    db.build_hourly_rollup("second-ledger", 10).unwrap();
    f.sessions = DimensionSelection::All {};
    db.snapshot(|tx, _| {
        assert_same(tx, &f)?;
        assert_eq!(totals(tx, &f)?.total_tokens.as_str(), "117");
        Ok(())
    })
    .unwrap();
}
