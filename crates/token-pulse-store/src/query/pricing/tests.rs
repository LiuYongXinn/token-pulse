//! Synthetic rates and application-owned fixtures only; no real Codex files.
use super::*;
use crate::{
    SourceRecord,
    batch::tests::{fixture, setup},
    query::tests::{extra, filter, ids},
};
use rusqlite::params;
use token_pulse_core::{
    jobs::{JobRequest, JobScope},
    numeric::DecimalInt,
    pricing::{PriceRuleDraft, PriceRuleMutation},
    protocol::JobKind,
    query::model_key,
};

fn draft(
    model: &str,
    currency: &str,
    input: i128,
    cache: Option<i128>,
    output: i128,
) -> PriceRuleDraft {
    PriceRuleDraft {
        provider: "synthetic-provider".into(),
        model_exact: model.into(),
        source_id: None,
        currency: currency.into(),
        effective_from_ms: EpochMs::new(0).unwrap(),
        effective_to_ms: None,
        priority: 0,
        input_rate_atoms: DecimalInt::from_nonnegative(input).unwrap(),
        cached_rate_atoms: cache.map(|v| DecimalInt::from_nonnegative(v).unwrap()),
        output_rate_atoms: DecimalInt::from_nonnegative(output).unwrap(),
        origin_reference: Some("synthetic fixture only".into()),
    }
}
fn install(db: &Database, draft: PriceRuleDraft, expected: i64) {
    db.mutate_price_rule(PriceRuleMutation::Create { draft }, expected, 1)
        .unwrap();
}
fn known_origin(db: &Database) {
    db.write(|conn| {conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.effective_metadata.provider','synthetic-provider') WHERE observation_id='observation'",[])?;conn.execute("UPDATE usage_events SET model='synthetic-model' WHERE event_id='event'",[])?; Ok(())}).unwrap();
}
fn reason<'a>(s: &'a PricingSummary, code: &str) -> &'a token_pulse_core::protocol::UnpricedReason {
    s.reasons.iter().find(|r| r.code == code).unwrap()
}

#[test]
fn partial_pricing_totals_are_exact_split_by_currency_and_filtered_like_consumption() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    known_origin(&db);
    extra(&db, "unknown", 2000, 7, (None, None), None, None);
    extra(
        &db,
        "no-rule",
        2100,
        11,
        (Some("missing"), Some("synthetic-provider")),
        None,
        None,
    );
    extra(
        &db,
        "incomplete",
        2200,
        13,
        (Some("synthetic-model"), Some("synthetic-provider")),
        None,
        None,
    );
    extra(
        &db,
        "eur",
        2300,
        17,
        (Some("eur-model"), Some("synthetic-provider")),
        None,
        None,
    );
    db.write(|conn| { conn.execute("UPDATE usage_events SET input_tokens_total=17,cached_input_tokens=0,output_tokens_total=0 WHERE event_id='eur'",[])?; Ok(()) }).unwrap();
    install(&db, draft("synthetic-model", "USD", 10, Some(5), 20), 0);
    install(&db, draft("eur-model", "EUR", 0, None, 0), 1);
    let s = db
        .pricing_summary(&filter(), &PriceBasis::EventTime {})
        .unwrap();
    assert_eq!(s.priced_total_tokens.as_str(), "127");
    assert_eq!(s.unpriced_total_tokens.as_str(), "31");
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "158"
    );
    assert_eq!(s.currencies.len(), 2);
    assert_eq!(s.currencies[0].currency, "EUR");
    assert_eq!(
        s.currencies[0].estimated_cost.as_ref().unwrap().as_str(),
        "0.000000000000000"
    );
    assert_eq!(
        s.currencies[1].estimated_cost.as_ref().unwrap().as_str(),
        "0.000000000000900"
    );
    for (code, tokens) in [
        ("unknown_model", "7"),
        ("missing_rule", "11"),
        ("insufficient_usage", "13"),
    ] {
        assert_eq!(reason(&s, code).total_tokens.as_str(), tokens);
        assert_eq!(reason(&s, code).event_count.as_str(), "1");
    }
    let mut f = filter();
    f.models = ids(
        &[&model_key(Some("synthetic-provider"), Some("synthetic-model")).unwrap()],
        false,
    );
    let s = db.pricing_summary(&f, &PriceBasis::EventTime {}).unwrap();
    assert_eq!(s.priced_total_tokens.as_str(), "110");
    assert_eq!(s.unpriced_total_tokens.as_str(), "13");
    f.range.end_ms = EpochMs::new(2200).unwrap();
    let s = db.pricing_summary(&f, &PriceBasis::EventTime {}).unwrap();
    assert_eq!(s.unpriced_total_tokens.as_str(), "0");
    f.sources = ids(&["missing-source"], false);
    let s = db.pricing_summary(&f, &PriceBasis::EventTime {}).unwrap();
    assert!(s.currencies.is_empty());
    assert!(s.reasons.is_empty());
}

#[test]
fn mirror_filters_keep_all_actual_sources_for_rule_rank_and_ambiguity() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    known_origin(&db);
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
        } Ok(())
    }).unwrap();
    install(&db, draft("synthetic-model", "USD", 10, Some(5), 20), 0);
    let mut specific = draft("synthetic-model", "EUR", 20, Some(10), 40);
    specific.source_id = Some("mirror".into());
    install(&db, specific.clone(), 1);
    for selected in [vec!["source"], vec!["mirror"], vec!["source", "mirror"]] {
        let mut f = filter();
        f.sources = ids(&selected, false);
        let s = db.pricing_summary(&f, &PriceBasis::EventTime {}).unwrap();
        assert_eq!(s.priced_total_tokens.as_str(), "110");
        assert_eq!(s.currencies.len(), 1);
        assert_eq!(s.currencies[0].currency, "EUR");
        assert_eq!(
            s.currencies[0].estimated_cost.as_ref().unwrap().as_str(),
            "0.000000000001800"
        );
    }
    specific.source_id = Some("source".into());
    install(&db, specific, 2);
    for selected in [vec!["source"], vec!["mirror"], vec!["source", "mirror"]] {
        let mut f = filter();
        f.sources = ids(&selected, false);
        let s = db.pricing_summary(&f, &PriceBasis::EventTime {}).unwrap();
        assert!(s.currencies.is_empty());
        assert_eq!(reason(&s, "ambiguous_rule").total_tokens.as_str(), "110");
        assert_eq!(reason(&s, "ambiguous_rule").event_count.as_str(), "1");
    }
}

#[test]
fn old_real_read_snapshot_keeps_rules_aliases_and_events_after_concurrent_publication() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    known_origin(&db);
    install(&db, draft("synthetic-model", "USD", 10, Some(5), 20), 0);
    db.ensure_session(crate::SessionRegistration {
        session_key: "alias".into(),
        provider_session_id: Some("synthetic-alias".into()),
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
            request_key: "synthetic-proof".into(),
        },
        1,
    )
    .unwrap();
    db.write(|conn|{
        conn.execute("UPDATE usage_events SET model='model-alias' WHERE event_id='event'",[])?;
        conn.execute("INSERT INTO model_aliases VALUES('mapping-1','synthetic-provider','model-alias','synthetic-model',1,NULL)",[])?;
        Ok(())
    }).unwrap();
    db.snapshot(|tx, revision| {
        let before = summary(tx, &filter(), &PriceBasis::EventTime {}, revision.price)?;
        let old = crate::pricing::rules_at(tx, revision.price)?
            .rules
            .remove(0);
        db.mutate_price_rule(
            PriceRuleMutation::Replace {
                rule_id: old.rule_id,
                draft: draft("replacement-model", "USD", 20, Some(10), 40),
            },
            1,
            2,
        )?;
        extra(&db, "new", 2000, 7, (None, None), None, None);
        db.write(|conn| {
            conn.execute(
                "INSERT INTO session_aliases VALUES('alias','session','proof')",
                [],
            )?;
            conn.execute("UPDATE model_aliases SET retired_revision=2 WHERE alias_id='mapping-1'",[])?;
            conn.execute("INSERT INTO model_aliases VALUES('mapping-2','synthetic-provider','model-alias','replacement-model',2,NULL)",[])?;
            conn.execute("UPDATE app_state SET data_revision=data_revision+1",[])?;
            Ok(())
        })?;
        assert_eq!(
            serde_json::to_value(summary(
                tx,
                &filter(),
                &PriceBasis::EventTime {},
                revision.price
            )?)
            .unwrap(),
            serde_json::to_value(before).unwrap()
        );
        let mut f = filter();
        f.sessions = ids(&["alias"], false);
        assert!(
            summary(tx, &f, &PriceBasis::EventTime {}, revision.price)?
                .currencies
                .is_empty()
        );
        Ok(())
    })
    .unwrap();
    let mut f = filter();
    f.sessions = ids(&["alias"], false);
    let s = db.pricing_summary(&f, &PriceBasis::EventTime {}).unwrap();
    assert_eq!(
        s.currencies[0].estimated_cost.as_ref().unwrap().as_str(),
        "0.000000000001800"
    );
    assert_eq!(s.unpriced_total_tokens.as_str(), "7");
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "117"
    );
}

#[test]
fn specified_time_has_explicit_half_open_rules_and_wholly_unpriced_is_not_zero() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    known_origin(&db);
    let empty = db
        .pricing_summary(&filter(), &PriceBasis::EventTime {})
        .unwrap();
    assert!(empty.currencies.is_empty());
    assert_eq!(reason(&empty, "missing_rule").total_tokens.as_str(), "110");
    let mut early = draft("synthetic-model", "USD", 10, Some(5), 20);
    early.effective_to_ms = Some(EpochMs::new(1000).unwrap());
    install(&db, early, 0);
    let mut late = draft("synthetic-model", "USD", 0, Some(0), 0);
    late.effective_from_ms = EpochMs::new(1000).unwrap();
    install(&db, late, 1);
    for (time, expected) in [(999, "0.000000000000900"), (1000, "0.000000000000000")] {
        let s = db
            .pricing_summary(
                &filter(),
                &PriceBasis::SpecifiedTime {
                    specified_at_ms: EpochMs::new(time).unwrap(),
                },
            )
            .unwrap();
        assert_eq!(
            s.currencies[0].estimated_cost.as_ref().unwrap().as_str(),
            expected
        );
    }
    let s = db
        .pricing_summary(
            &filter(),
            &PriceBasis::SpecifiedTime {
                specified_at_ms: EpochMs::new(-1).unwrap(),
            },
        )
        .unwrap();
    assert!(s.currencies.is_empty());
    assert_eq!(s.unpriced_total_tokens.as_str(), "110");
}

#[test]
fn sqlite_price_reads_keep_large_integers_and_reject_unbounded_source_evidence() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    known_origin(&db);
    install(&db, draft("synthetic-model", "USD", 1, None, 0), 0);
    db.write(|conn|{
        conn.execute("UPDATE usage_events SET input_tokens_total=9007199254740993,cached_input_tokens=0,output_tokens_total=0,reasoning_output_tokens=0,total_tokens=9007199254740993,source_total_tokens=9007199254740993",[])?;
        Ok(())
    }).unwrap();
    let s = db
        .pricing_summary(&filter(), &PriceBasis::EventTime {})
        .unwrap();
    assert_eq!(s.priced_total_tokens.as_str(), "9007199254740993");
    assert_eq!(
        s.currencies[0].estimated_cost.as_ref().unwrap().as_str(),
        "9.007199254740993"
    );
    for i in 1..=32 {
        db.add_source(SourceRecord {
            source_id: format!("s-{i}"),
            root_path: format!("synthetic-{i}"),
            directory_identity: None,
            kind: "local".into(),
            enabled: true,
            created_at_ms: 1,
        })
        .unwrap();
    }
    db.write(|conn|{
        for i in 1..=32 {
            conn.execute("INSERT INTO source_files(file_id,source_id,canonical_path,status) VALUES(?1,?2,?3,'known')",params![format!("f-{i}"),format!("s-{i}"),format!("synthetic-{i}.jsonl")])?;
            conn.execute("INSERT INTO file_generations SELECT ?1,?2,state,identity_json,observed_size,committed_offset,checkpoint_revision,anchor_json,reader_context_json,parser_version,created_at_ms FROM file_generations WHERE file_generation_id='generation'",params![format!("g-{i}"),format!("f-{i}")])?;
            conn.execute("INSERT INTO observations SELECT ?1,?2,byte_offset,byte_end,session_key,kind,observed_at_ms,stable_record_id,turn_id,stream_hint,model,project_id,normalized_json,payload_fingerprint,format_version FROM observations WHERE observation_id='observation'",params![format!("o-{i}"),format!("g-{i}")])?;
            conn.execute("INSERT INTO event_provenance VALUES('event',?1,'mirror')",[format!("o-{i}")])?;
        } Ok(())
    }).unwrap();
    // Production source configuration permits at most 32; this fixture writes
    // directly through the test-only helper to verify no truncation at 33.
    assert_eq!(
        db.pricing_summary(&filter(), &PriceBasis::EventTime {})
            .unwrap_err()
            .code,
        ErrorCode::DbCorrupt
    );
}
