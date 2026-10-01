use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::{
        grouped, raw_grouped, raw_totals,
        tests::{extra, filter, ids},
        totals,
    },
};
use token_pulse_core::{numeric::EpochMs, query::model_key};

fn fixture_ms(s: &str) -> i64 {
    // Independently converted with Windows DateTimeOffset, never with the
    // application's calendar implementation or another production helper.
    match s {
        "2024-01-01T00:00:00+05:45" => 1_704_046_500_000,
        "2024-01-01T00:30:00+05:45" => 1_704_048_300_000,
        "2024-01-01T01:30:00+05:45" => 1_704_051_900_000,
        "2024-01-01T04:00:00+05:45" => 1_704_060_900_000,
        "2024-01-01T12:00:00+05:45" => 1_704_089_700_000,
        "2024-01-01T13:00:00+05:45" => 1_704_093_300_000,
        "2024-02-01T12:00:00+05:45" => 1_706_768_100_000,
        "2024-02-02T00:00:00+05:45" => 1_706_811_300_000,
        "2024-04-07T00:00:00+11:00" => 1_712_408_400_000,
        "2024-04-07T01:45:00+10:30" => 1_712_416_500_000,
        "2024-04-07T01:45:00+11:00" => 1_712_414_700_000,
        "2024-04-07T04:00:00+10:30" => 1_712_424_600_000,
        "2024-11-03T00:00:00-04:00" => 1_730_606_400_000,
        "2024-11-03T01:15:00-04:00" => 1_730_610_900_000,
        "2024-11-03T01:15:00-05:00" => 1_730_614_500_000,
        "2024-11-03T03:00:00-05:00" => 1_730_620_800_000,
        _ => panic!("unexpected fixture timestamp"),
    }
}

fn assert_same(tx: &Transaction<'_>, f: &UsageFilter) -> StoreResult<()> {
    let cached = try_totals(tx, f)?.expect("cache selected");
    assert_eq!(
        serde_json::to_value(&cached)?,
        serde_json::to_value(raw_totals(tx, f)?)?
    );
    for dimension in [GroupDimension::Models, GroupDimension::Projects] {
        for sort in [GroupSort::TotalDesc, GroupSort::NameAsc] {
            assert_eq!(
                serde_json::to_value(
                    try_grouped(tx, f, dimension, sort, 200)?.expect("cache selected")
                )?,
                serde_json::to_value(raw_grouped(tx, f, dimension, sort, 200)?)?
            );
        }
    }
    Ok(())
}

fn assert_series_same(
    tx: &Transaction<'_>,
    f: &UsageFilter,
    grain: token_pulse_core::calendar::Grain,
) -> StoreResult<Vec<super::super::BucketTotals>> {
    let actual = crate::query::series(tx, f, grain)?;
    let expected = crate::query::raw_series(tx, f, grain)?;
    let json = |s: &[crate::query::BucketTotals]| {
        serde_json::to_value(s.iter().map(|b| (&b.bucket, &b.totals)).collect::<Vec<_>>())
    };
    assert_eq!(json(&actual)?, json(&expected)?);
    assert!(tx.prepare("SELECT current_usage_bucket(0)").is_err());
    assert!(tx.prepare("SELECT current_usage_cache_bucket(0)").is_err());
    Ok(actual)
}

#[test]
fn series_cache_preserves_dst_repeated_hours_fractional_offsets_and_exact_edges() {
    use token_pulse_core::calendar::Grain;
    // Explicit RFC3339 fixture instants; the production bucket code does not
    // generate event timestamps or expected consumption.
    let ms = fixture_ms;
    for (zone, start, end, a, b, expected_bins) in [
        (
            "America/New_York",
            "2024-11-03T00:00:00-04:00",
            "2024-11-03T03:00:00-05:00",
            "2024-11-03T01:15:00-04:00",
            "2024-11-03T01:15:00-05:00",
            Some(vec!["0", "17", "23", "0"]),
        ),
        (
            "Australia/Lord_Howe",
            "2024-04-07T00:00:00+11:00",
            "2024-04-07T04:00:00+10:30",
            "2024-04-07T01:45:00+11:00",
            "2024-04-07T01:45:00+10:30",
            None,
        ),
        (
            "Asia/Kathmandu",
            "2024-01-01T00:00:00+05:45",
            "2024-01-01T04:00:00+05:45",
            "2024-01-01T00:30:00+05:45",
            "2024-01-01T01:30:00+05:45",
            Some(vec!["17", "23", "0", "0"]),
        ),
    ] {
        let (_dir, db) = setup();
        db.commit(fixture()).unwrap();
        extra(&db, "a", ms(a), 17, (None, None), Some("common"), None);
        extra(&db, "b", ms(b), 23, (None, None), Some("common"), None);
        extra(
            &db,
            "at-end",
            ms(end),
            31,
            (None, None),
            Some("excluded"),
            None,
        );
        db.build_hourly_rollup("ledger", 10).unwrap();
        let mut f = filter();
        f.range.start_ms = EpochMs::new(ms(start)).unwrap();
        f.range.end_ms = EpochMs::new(ms(end)).unwrap();
        f.range.timezone = zone.into();
        db.snapshot(|tx,_| {
            let s=assert_series_same(tx,&f,Grain::Hour)?;
            if let Some(expected)=expected_bins { assert_eq!(s.iter().map(|b|b.totals.total_tokens.as_str()).collect::<Vec<_>>(),expected); }
            assert_eq!(s.iter().map(|b|b.totals.total_tokens.value()).sum::<i128>(),40);
            assert_eq!(totals(tx,&f)?.reliable_turn_count.unwrap().as_str(),"1");
            let bins=token_pulse_core::calendar::buckets(&f.range,Grain::Hour)?;
            let _guard=crate::query::bucket_function(tx,&bins)?;
            let usable:i64=tx.query_row("SELECT COUNT(*) FROM utc_hour_usage_rollups WHERE current_usage_cache_bucket(hour_start_ms) IS NOT NULL",[],|r|r.get(0))?;
            if zone=="Asia/Kathmandu" { assert_eq!(usable,0); }
            if zone=="America/New_York" { assert_eq!(usable,2); }
            Ok(())
        }).unwrap();
    }
}

#[test]
fn series_cache_merges_calendar_days_months_sources_and_old_snapshot() {
    use token_pulse_core::calendar::Grain;
    let ms = fixture_ms;
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(
        &db,
        "edge",
        ms("2024-01-01T00:00:00+05:45"),
        7,
        (None, None),
        Some("same"),
        None,
    );
    extra(
        &db,
        "inside",
        ms("2024-01-01T12:00:00+05:45"),
        17,
        (Some("m"), Some("p")),
        Some("same"),
        None,
    );
    extra(
        &db,
        "february",
        ms("2024-02-01T12:00:00+05:45"),
        23,
        (None, None),
        Some("other"),
        None,
    );
    let mut f = filter();
    f.range.start_ms = EpochMs::new(ms("2024-01-01T00:00:00+05:45")).unwrap();
    f.range.end_ms = EpochMs::new(ms("2024-02-02T00:00:00+05:45")).unwrap();
    f.range.timezone = "Asia/Kathmandu".into();
    db.build_hourly_rollup("ledger", 10).unwrap();
    db.snapshot(|tx, _| {
        let writer = db.clone();
        let late = ms("2024-01-01T13:00:00+05:45");
        std::thread::spawn(move || extra(&writer, "late", late, 5, (None, None), None, None))
            .join()
            .unwrap();
        let s = assert_series_same(tx, &f, Grain::Month)?;
        assert_eq!(
            s.iter()
                .map(|b| b.totals.total_tokens.as_str())
                .collect::<Vec<_>>(),
            vec!["24", "23"]
        );
        let day = assert_series_same(tx, &f, Grain::Day)?;
        assert_eq!(day.len(), 32);
        assert_eq!(day[0].totals.total_tokens.as_str(), "24");
        assert_eq!(day[31].totals.total_tokens.as_str(), "23");
        f.sources = ids(&["source"], false);
        assert_series_same(tx, &f, Grain::Day)?;
        f.models = ids(&[], true);
        let s = assert_series_same(tx, &f, Grain::Month)?;
        assert_eq!(s[0].totals.total_tokens.as_str(), "7");
        Ok(())
    })
    .unwrap();
    f.models = DimensionSelection::All {};
    db.snapshot(|tx, _| {
        let s = assert_series_same(tx, &f, Grain::Month)?;
        assert_eq!(s[0].totals.total_tokens.as_str(), "29");
        Ok(())
    })
    .unwrap();
}

#[test]
fn grouped_cache_keeps_exact_sort_provider_identity_alias_and_project_labels() {
    use crate::SessionRegistration;
    use token_pulse_core::{
        jobs::{JobRequest, JobScope},
        protocol::JobKind,
    };
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.ensure_session(SessionRegistration {
        session_key: "alias".into(),
        provider_session_id: Some("alias-provider".into()),
        parent_key: None,
        parent_provider_id: None,
        created_at_ms: None,
        ledger_id: "alias-ledger".into(),
        registered_at_ms: 1,
    })
    .unwrap();
    db.create_job(
        "proof".into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::All {},
            request_key: "proof".into(),
        },
        1,
    )
    .unwrap();
    db.write(|conn| {
        conn.execute(
            "INSERT INTO projects VALUES('p','synthetic','原名','未知项目',1)",
            [],
        )?;
        conn.execute(
            "INSERT INTO session_aliases VALUES('alias','session','proof')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    extra(
        &db,
        "huge-a",
        3_600_000,
        i64::MAX,
        (Some("same"), Some("a")),
        Some("common"),
        Some("p"),
    );
    extra(
        &db,
        "huge-b",
        7_200_000,
        i64::MAX,
        (Some("same"), Some("a")),
        Some("common"),
        Some("p"),
    );
    extra(
        &db,
        "second",
        7_200_500,
        9_007_199_254_740_993,
        (Some("same"), Some("b")),
        Some("common"),
        None,
    );
    extra(
        &db,
        "unknown-provider",
        10_800_000,
        7,
        (None, Some("known-provider")),
        None,
        None,
    );
    db.build_hourly_rollup("ledger", 10).unwrap();
    let mut f = filter();
    f.range.end_ms = EpochMs::new(14_400_000).unwrap();
    f.sessions = ids(&["alias"], false);
    db.snapshot(|tx, _| {
        assert_same(tx, &f)?;
        let g = grouped(tx, &f, GroupDimension::Models, GroupSort::TotalDesc, 2)?;
        assert_eq!(g[0].display_name, "same · a");
        assert_eq!(g[0].totals.total_tokens.as_str(), "18446744073709551614");
        assert_eq!(
            g[0].totals.reliable_turn_count.as_ref().unwrap().as_str(),
            "1"
        );
        assert_eq!(g[1].display_name, "same · b");
        assert_eq!(g[1].totals.total_tokens.as_str(), "9007199254740993");
        let projects = grouped(tx, &f, GroupDimension::Projects, GroupSort::NameAsc, 200)?;
        assert_eq!(projects.len(), 2);
        assert!(projects.iter().all(|g| g.display_name == "未知项目"));
        assert!(projects[0].key.is_none());
        assert_eq!(projects[1].key.as_deref(), Some("p"));
        f.projects = ids(&["p"], true);
        assert_same(tx, &f)?;
        f.projects = ids(&[], false);
        assert_same(tx, &f)?;
        assert!(grouped(tx, &f, GroupDimension::Models, GroupSort::NameAsc, 200)?.is_empty());
        Ok(())
    })
    .unwrap();
}

#[test]
fn empty_ready_cache_and_negative_partial_range_do_not_create_groups_or_zeros() {
    let (_dir, db) = setup();
    db.build_hourly_rollup("ledger", 10).unwrap();
    let mut f = filter();
    f.range.start_ms = EpochMs::new(-7_200_500).unwrap();
    f.range.end_ms = EpochMs::new(7_200_500).unwrap();
    db.snapshot(|tx, _| {
        assert_same(tx, &f)?;
        let t = totals(tx, &f)?;
        assert_eq!(t.total_tokens.as_str(), "0");
        assert_eq!(t.session_count.as_str(), "0");
        assert!(t.input_total.value.is_none());
        Ok(())
    })
    .unwrap();
    db.commit(fixture()).unwrap();
    extra(
        &db,
        "negative",
        -7_200_000,
        7,
        (None, Some("p")),
        Some("common"),
        None,
    );
    extra(
        &db,
        "negative-partial",
        -7_200_499,
        11,
        (None, None),
        Some("common"),
        None,
    );
    db.build_hourly_rollup("ledger", 20).unwrap();
    db.snapshot(|tx, _| {
        assert_same(tx, &f)?;
        assert_eq!(totals(tx, &f)?.total_tokens.as_str(), "128");
        Ok(())
    })
    .unwrap();
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
