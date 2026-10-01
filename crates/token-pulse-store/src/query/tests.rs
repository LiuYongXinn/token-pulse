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

fn filter() -> UsageFilter {
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
fn ids(ids: &[&str], unknown: bool) -> DimensionSelection {
    DimensionSelection::Ids {
        ids: ids.iter().map(|s| s.to_string()).collect(),
        include_unknown: unknown,
    }
}

fn extra(
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
