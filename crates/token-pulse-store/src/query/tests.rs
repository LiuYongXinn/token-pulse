use super::*;
use crate::{
    SessionRegistration, SourceRecord,
    batch::tests::{fixture, setup},
};
use rusqlite::params;
use token_pulse_core::{
    jobs::{JobRequest, JobScope},
    numeric::EpochMs,
    protocol::{DateRange, JobKind},
    query::model_key,
};

pub(crate) fn filter() -> UsageFilter {
    UsageFilter {
        range: DateRange {
            start_ms: EpochMs::new(0).unwrap(),
            end_ms: EpochMs::new(5000).unwrap(),
            timezone: "UTC".into(),
        },
        sources: DimensionSelection::All {},
        models: DimensionSelection::All {},
        projects: DimensionSelection::All {},
        sessions: DimensionSelection::All {},
    }
}
pub(crate) fn ids(ids: &[&str], unknown: bool) -> DimensionSelection {
    DimensionSelection::Ids {
        ids: ids.iter().map(|s| s.to_string()).collect(),
        include_unknown: unknown,
    }
}

pub(crate) fn extra(
    db: &Database,
    id: &str,
    time: i64,
    total: i64,
    identity: (Option<&str>, Option<&str>),
    turn: Option<&str>,
    project: Option<&str>,
) {
    let id = id.to_owned();
    let model = identity.0.map(str::to_owned);
    let provider = identity.1.map(str::to_owned);
    let turn = turn.map(str::to_owned);
    let project = project.map(str::to_owned);
    db.write(move |conn| {
        let tx = conn.transaction()?;
        let offset: i64 = tx.query_row("SELECT MAX(byte_end) FROM observations", [], |r| r.get(0))?;
        let json = serde_json::json!({"kind":"usage", "effective_metadata":{"provider":provider,"model":model}}).to_string();
        tx.execute("INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,observed_at_ms,model,project_id,normalized_json,payload_fingerprint,format_version) VALUES(?1,'generation',?2,?3,'session','usage',?4,?5,?6,?7,?1,'test')",params![id,offset,offset+100,time,model,project,json])?;
        tx.execute("INSERT INTO usage_events(event_id,ledger_id,origin_observation_id,occurred_at_ms,episode_id,model,project_id,turn_id,source_total_tokens,total_tokens,calculation_method,quality_json) VALUES(?1,'ledger',?1,?2,'episode',?3,?4,?5,?6,?6,'fixture','[\"confirmed\"]')",params![id,time,model,project,turn,total])?;
        tx.execute("INSERT INTO event_provenance VALUES(?1,?1,'origin')",[id])?;
        tx.execute("UPDATE app_state SET data_revision=data_revision+1",[])?;
        tx.commit()?; Ok(())
    }).unwrap();
}

#[test]
fn partial_vectors_and_recognized_turns_have_independent_coverage() {
    let (_dir, db) = setup();
    let empty = db.usage_totals(&filter()).unwrap();
    assert_eq!(empty.total_tokens.as_str(), "0");
    assert!(empty.input_total.value.is_none());
    assert!(!empty.input_total.complete);
    assert!(empty.reliable_turn_count.is_none());
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute("UPDATE usage_events SET turn_id='turn-one'", [])?;
        Ok(())
    })
    .unwrap();
    let complete = db.usage_totals(&filter()).unwrap();
    assert!(
        complete.input_total.complete
            && complete.cached_input.complete
            && complete.noncached_input.complete
            && complete.output_total.complete
            && complete.reasoning_output.complete
    );
    assert!(complete.reliable_turns_complete);
    extra(&db, "unknown-seven", 2000, 7, (None, None), None, None);
    let t = db.usage_totals(&filter()).unwrap();
    assert_eq!(t.total_tokens.as_str(), "117");
    for (measure, expected) in [
        (&t.input_total, "100"),
        (&t.cached_input, "60"),
        (&t.noncached_input, "40"),
        (&t.output_total, "10"),
        (&t.reasoning_output, "2"),
    ] {
        assert_eq!(measure.value.as_ref().unwrap().as_str(), expected);
        assert_eq!(measure.covered_total_tokens.as_str(), "110");
        assert!(!measure.complete);
    }
    assert_eq!(t.reliable_turn_count.unwrap().as_str(), "1");
    assert!(!t.reliable_turns_complete);
    assert_eq!(t.usage_event_count.as_str(), "2");
    assert_eq!(t.session_count.as_str(), "1");
    let mut f = filter();
    f.range.start_ms = EpochMs::new(2000).unwrap();
    let t = db.usage_totals(&f).unwrap();
    assert_eq!(t.total_tokens.as_str(), "7");
    assert!(t.input_total.value.is_none());
    assert_eq!(t.input_total.covered_total_tokens.as_str(), "0");
}

#[test]
fn source_evidence_filters_never_multiply_mirrored_consumption() {
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
        conn.execute("INSERT INTO source_files(file_id,source_id,canonical_path,status) VALUES('mirror-file','mirror','synthetic-mirror.jsonl','known')",[])?;
        conn.execute("INSERT INTO file_generations SELECT 'mirror-generation','mirror-file',state,identity_json,observed_size,committed_offset,checkpoint_revision,anchor_json,reader_context_json,parser_version,created_at_ms FROM file_generations WHERE file_generation_id='generation'",[])?;
        for index in 1..=2 {
            conn.execute("INSERT INTO observations SELECT ?1,'mirror-generation',?2,?3,session_key,kind,observed_at_ms,stable_record_id,turn_id,stream_hint,model,project_id,normalized_json,payload_fingerprint,format_version FROM observations WHERE observation_id='observation'",params![format!("copy-{index}"),index*100,index*100+100])?;
            conn.execute("INSERT INTO event_provenance VALUES('event',?1,'mirror')",[format!("copy-{index}")])?;
        }
        Ok(())
    }).unwrap();
    for selected in [vec!["source"], vec!["mirror"], vec!["source", "mirror"]] {
        let mut f = filter();
        f.sources = ids(&selected, false);
        let t = db.usage_totals(&f).unwrap();
        assert_eq!(t.total_tokens.as_str(), "110");
        assert_eq!(t.usage_event_count.as_str(), "1");
    }
    for selected in [vec![], vec!["missing"], vec!["source') OR 1=1 --"]] {
        let mut f = filter();
        f.sources = ids(&selected, true);
        assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "0");
    }
}

#[test]
fn model_keys_separate_providers_and_unknowns_without_label_sql() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(
        &db,
        "openai",
        1100,
        20,
        (Some("same-model"), Some("openai")),
        None,
        None,
    );
    extra(
        &db,
        "other",
        1200,
        30,
        (Some("same-model"), Some("other")),
        None,
        None,
    );
    extra(
        &db,
        "unknown-provider",
        1300,
        40,
        (Some("same-model"), None),
        None,
        None,
    );
    extra(
        &db,
        "named-unknown",
        1400,
        50,
        (Some("未知模型"), None),
        None,
        None,
    );
    extra(
        &db,
        "sql-name",
        1500,
        60,
        (Some("x') OR 1=1 --"), Some("p")),
        None,
        None,
    );
    for (provider, model, expected) in [
        (Some("openai"), "same-model", "20"),
        (Some("other"), "same-model", "30"),
        (None, "same-model", "40"),
        (None, "未知模型", "50"),
        (Some("p"), "x') OR 1=1 --", "60"),
    ] {
        let key = model_key(provider, Some(model)).unwrap();
        let mut f = filter();
        f.models = ids(&[&key], false);
        assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), expected);
    }
    let mut f = filter();
    f.models = ids(&[], true);
    assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "110");
    let key = model_key(None, Some("same-model")).unwrap();
    f.models = ids(&[&key], true);
    assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "150");
    db.snapshot(|tx, _| {
        let groups = grouped(
            tx,
            &filter(),
            GroupDimension::Models,
            GroupSort::TotalDesc,
            200,
        )?;
        assert_eq!(groups.len(), 6);
        assert!(groups[0].key.is_none());
        assert_eq!(groups[0].totals.total_tokens.as_str(), "110");
        Ok(())
    })
    .unwrap();
}

#[test]
fn unknown_model_is_one_dimension_even_when_provider_is_known() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(&db, "unknown-a", 1100, 7, (None, Some("a")), None, None);
    extra(&db, "unknown-b", 1200, 9, (None, Some("b")), None, None);
    db.snapshot(|tx, _| {
        let groups = grouped(
            tx,
            &filter(),
            GroupDimension::Models,
            GroupSort::TotalDesc,
            200,
        )?;
        assert_eq!(groups.len(), 1);
        assert!(groups[0].key.is_none());
        assert_eq!(groups[0].totals.total_tokens.as_str(), "126");
        Ok(())
    })
    .unwrap();
}

#[test]
fn cache_evidence_versions_include_provenance_and_preserve_real_snapshots() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.snapshot(|tx,_| {
        assert_eq!(tx.query_row("SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",[],|r|r.get::<_,i64>(0))?,2);
        db.write(|conn| {
            conn.execute("INSERT INTO event_provenance VALUES('event','observation','origin') ON CONFLICT DO NOTHING",[])?;
            assert_eq!(conn.query_row("SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",[],|r|r.get::<_,i64>(0))?,2);
            conn.execute("UPDATE event_provenance SET relation='mirror' WHERE event_id='event'",[])?;
            assert_eq!(conn.query_row("SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",[],|r|r.get::<_,i64>(0))?,3);
            conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.effective_metadata.provider','different') WHERE observation_id='observation'",[])?;
            assert_eq!(conn.query_row("SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",[],|r|r.get::<_,i64>(0))?,4);
            conn.execute("UPDATE usage_events SET turn_id='recognized' WHERE event_id='event'",[])?;
            assert_eq!(conn.query_row("SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",[],|r|r.get::<_,i64>(0))?,5);
            Ok(())
        })?;
        assert_eq!(tx.query_row("SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",[],|r|r.get::<_,i64>(0))?,2); Ok(())
    }).unwrap();
    db.write(|conn| {
        conn.execute("DELETE FROM event_provenance WHERE event_id='event'", [])?;
        assert_eq!(
            conn.query_row(
                "SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            6
        );
        conn.execute("DELETE FROM usage_events WHERE event_id='event'", [])?;
        assert_eq!(
            conn.query_row(
                "SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            7
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn rollup_schema_rejects_unpublished_ready_sets_and_invalid_cohort_references() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        assert!(conn.execute("INSERT INTO usage_rollup_sets VALUES('bad','ledger',2,1,'p','a','ready',1,NULL)",[]).is_err());
        assert!(conn.execute("INSERT INTO usage_rollup_sets VALUES('bad','ledger',2,1,'p','a','building',1,1)",[]).is_err());
        conn.execute("INSERT INTO usage_rollup_sets VALUES('candidate','ledger',2,1,'p','a','building',1,NULL)",[])?;
        let sums: String = conn.query_row("SELECT sum_usage_vector(input_tokens_total,cached_input_tokens,output_tokens_total,reasoning_output_tokens,total_tokens) FROM usage_events",[],|r|r.get(0))?;
        assert!(conn.execute("INSERT INTO utc_hour_usage_rollups VALUES('candidate',1,'cohort',NULL,NULL,NULL,'[\"source\"]',?1,1,0)",[&sums]).is_err());
        assert!(conn.execute("INSERT INTO utc_hour_usage_rollups VALUES('candidate',0,'cohort',NULL,NULL,NULL,'{}',?1,1,0)",[&sums]).is_err());
        assert!(conn.execute("INSERT INTO utc_hour_usage_rollups VALUES('candidate',0,'cohort',NULL,NULL,NULL,'[\"source\"]',?1,1,2)",[&sums]).is_err());
        conn.execute("INSERT INTO utc_hour_usage_rollups VALUES('candidate',0,'cohort',NULL,NULL,NULL,'[\"source\"]',?1,1,0)",[&sums])?;
        assert!(conn.execute("INSERT INTO utc_hour_rollup_turns VALUES('candidate',3600000,'cohort','turn')",[]).is_err());
        assert!(conn.execute("INSERT INTO utc_hour_rollup_turns VALUES('candidate',0,'cohort','')",[]).is_err());
        conn.execute("INSERT INTO utc_hour_rollup_turns VALUES('candidate',0,'cohort','turn')",[])?;
        conn.execute("DELETE FROM usage_rollup_sets WHERE set_id='candidate'",[])?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM utc_hour_usage_rollups",[],|r|r.get::<_,i64>(0))?,0);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM utc_hour_rollup_turns",[],|r|r.get::<_,i64>(0))?,0);
        assert_eq!(conn.query_row("SELECT revision FROM ledger_usage_versions WHERE ledger_id='ledger'",[],|r|r.get::<_,i64>(0))?,2);
        Ok(())
    }).unwrap();
}

#[test]
fn exact_large_group_sort_and_half_open_time_range() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(
        &db,
        "huge1",
        2000,
        i64::MAX,
        (Some("huge"), None),
        Some("a"),
        None,
    );
    extra(
        &db,
        "huge2",
        3000,
        i64::MAX,
        (Some("huge"), None),
        Some("a"),
        None,
    );
    extra(
        &db,
        "smaller",
        4000,
        9_007_199_254_740_993,
        (Some("smaller"), None),
        Some("b"),
        None,
    );
    db.snapshot(|tx, _| {
        let groups = grouped(
            tx,
            &filter(),
            GroupDimension::Models,
            GroupSort::TotalDesc,
            2,
        )?;
        assert_eq!(groups[0].display_name, "huge");
        assert_eq!(
            groups[0].totals.total_tokens.as_str(),
            "18446744073709551614"
        );
        assert_eq!(
            groups[0]
                .totals
                .reliable_turn_count
                .as_ref()
                .unwrap()
                .as_str(),
            "1"
        );
        assert!(groups[0].totals.reliable_turns_complete);
        assert_eq!(groups[1].totals.total_tokens.as_str(), "9007199254740993");
        assert!(
            grouped(
                tx,
                &filter(),
                GroupDimension::Models,
                GroupSort::TotalDesc,
                201
            )
            .is_err()
        );
        Ok(())
    })
    .unwrap();
    let mut f = filter();
    f.range.start_ms = EpochMs::new(2000).unwrap();
    f.range.end_ms = EpochMs::new(3000).unwrap();
    assert_eq!(
        db.usage_totals(&f).unwrap().total_tokens.as_str(),
        i64::MAX.to_string()
    );
}

#[test]
fn aliases_and_multiple_reads_resolve_within_the_same_real_snapshot() {
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
            "INSERT INTO session_aliases VALUES('alias','session','proof')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let mut f = filter();
    f.sessions = ids(&["alias"], false);
    assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "110");
    db.snapshot(|tx, revision| {
        assert_eq!(totals(tx, &f)?.total_tokens.as_str(), "110");
        let writer = db.clone();
        std::thread::spawn(move || extra(&writer, "later", 2000, 7, (None, None), None, None))
            .join()
            .unwrap();
        assert_eq!(totals(tx, &f)?.total_tokens.as_str(), "110");
        let group = grouped(tx, &f, GroupDimension::Models, GroupSort::TotalDesc, 10)?;
        assert_eq!(group[0].totals.total_tokens.as_str(), "110");
        assert_eq!(
            tx.query_row("SELECT data_revision FROM app_state", [], |r| r
                .get::<_, i64>(0))?,
            revision.data
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "117");
}

#[test]
fn project_ids_distinguish_unknown_and_keep_user_aliases_in_the_snapshot() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute(
            "INSERT INTO projects VALUES('named-unknown','synthetic-one','未知项目',NULL,1)",
            [],
        )?;
        conn.execute(
            "INSERT INTO projects VALUES('aliased','synthetic-two','original','用户别名',1)",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    extra(
        &db,
        "p1",
        2000,
        20,
        (None, None),
        None,
        Some("named-unknown"),
    );
    extra(&db, "p2", 3000, 30, (None, None), None, Some("aliased"));
    let mut f = filter();
    f.projects = ids(&["named-unknown"], false);
    assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "20");
    f.projects = ids(&[], true);
    assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "110");
    f.projects = ids(&["aliased"], true);
    assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "140");
    db.snapshot(|tx, _| {
        let groups = grouped(
            tx,
            &filter(),
            GroupDimension::Projects,
            GroupSort::NameAsc,
            200,
        )?;
        assert_eq!(groups.len(), 3);
        assert_eq!(
            groups
                .iter()
                .find(|r| r.key.as_deref() == Some("aliased"))
                .unwrap()
                .display_name,
            "用户别名"
        );
        assert_eq!(
            groups
                .iter()
                .filter(|r| r.display_name == "未知项目")
                .count(),
            2
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn series_binds_calendar_bounds_and_uses_indexed_event_ranges() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(
        &db,
        "hour-edge",
        3_600_000,
        7,
        (Some("edge"), None),
        None,
        None,
    );
    extra(
        &db,
        "end-excluded",
        10_800_000,
        19,
        (None, None),
        None,
        None,
    );
    let mut f = filter();
    f.range.end_ms = EpochMs::new(10_800_000).unwrap();
    db.snapshot(|tx, _| {
        let s = series(tx, &f, Grain::Hour)?;
        assert_eq!(
            s.iter()
                .map(|b| b.totals.total_tokens.as_str())
                .collect::<Vec<_>>(),
            vec!["110", "7", "0"]
        );
        assert_eq!(s[1].bucket.start_ms.value(), 3_600_000);
        assert!(s[0].totals.input_total.complete);
        assert!(s[1].totals.input_total.value.is_none());
        assert!(s[2].totals.input_total.value.is_none() && !s[2].totals.input_total.complete);
        assert_eq!(totals(tx, &f)?.total_tokens.as_str(), "117");
        let _function = bucket_function(tx, &calendar::buckets(&f.range, Grain::Hour)?)?;
        let (sql, values) = series_sql(&f)?;
        let mut statement = tx.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
        let plan = statement
            .query_map(params_from_iter(values), |r| r.get::<_, String>(3))?
            .collect::<Result<Vec<_>, _>>()?;
        assert!(
            plan.iter()
                .any(|p| p.contains("SEARCH e USING INDEX events_")
                    && p.contains("occurred_at_ms>?")
                    && p.contains("occurred_at_ms<?")),
            "{plan:?}"
        );
        assert!(!plan.iter().any(|p| p.starts_with("SCAN e")), "{plan:?}");
        Ok(())
    })
    .unwrap();
    let key = model_key(None, Some("edge")).unwrap();
    f.models = ids(&[&key], false);
    db.snapshot(|tx, _| {
        assert_eq!(
            series(tx, &f, Grain::Hour)?
                .iter()
                .map(|b| b.totals.total_tokens.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec!["0", "7", "0"]
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn series_keeps_repeated_dst_hours_and_snapshot_results_separate() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let ms = |s| chrono_test_ms(s);
    let first = ms("2024-11-03T01:15:00-04:00");
    let second = ms("2024-11-03T01:15:00-05:00");
    extra(&db, "first-one", first, 17, (None, None), None, None);
    extra(&db, "second-one", second, 23, (None, None), None, None);
    let mut f = filter();
    f.range = DateRange {
        start_ms: EpochMs::new(ms("2024-11-03T00:00:00-04:00")).unwrap(),
        end_ms: EpochMs::new(ms("2024-11-03T03:00:00-05:00")).unwrap(),
        timezone: "America/New_York".into(),
    };
    db.snapshot(|tx, _| {
        let writer = db.clone();
        std::thread::spawn(move || {
            extra(&writer, "late-first", first, 5, (None, None), None, None)
        })
        .join()
        .unwrap();
        let s = series(tx, &f, Grain::Hour)?;
        assert_eq!(
            s.iter()
                .map(|b| b.totals.total_tokens.as_str())
                .collect::<Vec<_>>(),
            vec!["0", "17", "23", "0"]
        );
        assert_eq!(s[1].bucket.display_label, s[2].bucket.display_label);
        assert_ne!(s[1].bucket.utc_offset, s[2].bucket.utc_offset);
        assert_ne!(s[1].bucket.start_ms, s[2].bucket.start_ms);
        assert_eq!(totals(tx, &f)?.total_tokens.as_str(), "40");
        Ok(())
    })
    .unwrap();
    assert_eq!(db.usage_totals(&f).unwrap().total_tokens.as_str(), "45");
}

fn chrono_test_ms(value: &str) -> i64 {
    // Independent fixture timestamps, fixed RFC3339 conversions from the
    // already verified calendar cases. No production bucketing computes them.
    match value {
        "2024-11-03T00:00:00-04:00" => 1_730_606_400_000,
        "2024-11-03T01:15:00-04:00" => 1_730_610_900_000,
        "2024-11-03T01:15:00-05:00" => 1_730_614_500_000,
        "2024-11-03T03:00:00-05:00" => 1_730_620_800_000,
        _ => panic!("unknown fixture time"),
    }
}

#[test]
fn repeated_series_on_one_connection_does_not_reuse_prior_timezone_or_range() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let mut f = filter();
    f.range.end_ms = EpochMs::new(7_200_000).unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(series(tx, &f, Grain::Hour)?.len(), 2);
        assert!(tx.prepare("SELECT current_usage_bucket(1000)").is_err());
        f.range.timezone = "Asia/Kathmandu".into();
        let s = series(tx, &f, Grain::Hour)?;
        assert_eq!(s.len(), 3);
        assert_eq!(
            s.iter()
                .map(|b| b.totals.total_tokens.as_str())
                .collect::<Vec<_>>(),
            vec!["110", "0", "0"]
        );
        f.range.end_ms = EpochMs::new(86_400_000).unwrap();
        f.range.timezone = "UTC".into();
        assert_eq!(series(tx, &f, Grain::Day)?.len(), 1);
        assert!(tx.prepare("SELECT current_usage_bucket(1000)").is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
#[ignore = "explicit 300k-event query benchmark; does not read user sources"]
fn benchmark_300k_event_totals_groups_and_calendar_series() {
    use std::time::Instant;
    let (_dir, db) = setup();
    db.write(|conn| {
        let tx = conn.transaction()?;
        for i in 0..100 {
            let session = format!("bench-session-{i}"); let ledger = format!("bench-ledger-{i}");
            tx.execute("INSERT INTO sessions(session_key,provider,identity_status) VALUES(?1,'codex','fixture')",[&session])?;
            tx.execute("INSERT INTO ledger_generations VALUES(?1,?2,'active','fixture','fixture',0,0,0,'{}')",params![ledger,session])?;
            tx.execute("UPDATE sessions SET active_ledger_id=?1 WHERE session_key=?2",params![ledger,session])?;
        }
        tx.execute_batch("WITH RECURSIVE n(v) AS (VALUES(0) UNION ALL SELECT v+1 FROM n WHERE v<299999)
          INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,observed_at_ms,model,normalized_json,payload_fingerprint,format_version)
          SELECT 'bench-'||v,'generation',v*100,v*100+100,'bench-session-'||(v%100),'usage',v*60000,'model-'||(v%10),json_object('kind','usage','effective_metadata',json_object('provider','perf','model','model-'||(v%10))),'fingerprint-'||v,'fixture' FROM n;
          INSERT INTO usage_events(event_id,ledger_id,origin_observation_id,occurred_at_ms,episode_id,model,input_tokens_total,cached_input_tokens,output_tokens_total,reasoning_output_tokens,source_total_tokens,total_tokens,calculation_method,quality_json)
          SELECT observation_id,'bench-ledger-'||((byte_offset/100)%100),observation_id,observed_at_ms,'episode',model,1,0,0,0,1,1,'fixture','[\"confirmed\"]' FROM observations;
          INSERT INTO event_provenance SELECT event_id,origin_observation_id,'origin' FROM usage_events;
          UPDATE app_state SET data_revision=data_revision+1;")?;
        tx.commit()?; Ok(())
    }).unwrap();
    let mut f = filter();
    f.range.end_ms = EpochMs::new(300000 * 60000).unwrap();
    let wal = std::fs::metadata(format!("{}-wal", db.path().display()))
        .unwrap()
        .len();
    let mut total_times = Vec::new();
    let mut group_times = Vec::new();
    let mut series_times = Vec::new();
    for _ in 0..6 {
        db.snapshot(|tx, _| {
            let start = Instant::now();
            let t = totals(tx, &f)?;
            total_times.push(start.elapsed().as_micros());
            assert_eq!(t.total_tokens.as_str(), "300000");
            assert_eq!(t.session_count.as_str(), "100");
            assert!(t.input_total.complete);
            let start = Instant::now();
            let g = grouped(tx, &f, GroupDimension::Models, GroupSort::TotalDesc, 200)?;
            group_times.push(start.elapsed().as_micros());
            assert_eq!(g.len(), 10);
            assert!(g.iter().all(|r| r.totals.total_tokens.as_str() == "30000"));
            let start = Instant::now();
            let s = series(tx, &f, Grain::Day)?;
            series_times.push(start.elapsed().as_micros());
            assert_eq!(s.len(), 209);
            assert_eq!(s.last().unwrap().totals.total_tokens.as_str(), "480");
            assert!(
                s[..208]
                    .iter()
                    .all(|b| b.totals.total_tokens.as_str() == "1440")
            );
            Ok(())
        })
        .unwrap();
    }
    for (name, values) in [
        ("totals", total_times),
        ("models", group_times),
        ("daily_series", series_times),
    ] {
        let first = values[0];
        let mut hot = values[1..].to_vec();
        hot.sort_unstable();
        println!(
            "BENCH {name} events=300000 sessions=100 first_reader_us={first} hot_p50_us={} hot_p95_nearest_rank_us={} hot_max_us={} samples=5 wal_bytes={wal}",
            hot[2], hot[4], hot[4]
        );
    }
}
