use super::*;
use crate::{
    SessionRegistration,
    batch::tests::{fixture, setup},
    query::tests::{extra, filter, ids},
};
use rusqlite::params;
use token_pulse_core::{
    numeric::EpochMs,
    pricing::{PriceRuleDraft, PriceRuleMutation},
    protocol::{CoverageState, DateRange},
};
fn request() -> DashboardRequest {
    let mut filter = filter();
    filter.range.end_ms = EpochMs::new(86_400_000).unwrap();
    DashboardRequest {
        filter,
        price_basis: PriceBasis::EventTime {},
        grain: Grain::Hour,
        heatmap_range: DateRange {
            start_ms: EpochMs::new(0).unwrap(),
            end_ms: EpochMs::new(172_800_000).unwrap(),
            timezone: "UTC".into(),
        },
    }
}
fn read(db: &Database, request: &DashboardRequest) -> DashboardBundle {
    db.dashboard_bundle(
        request,
        EpochMs::new(10_000).unwrap(),
        "synthetic-dashboard",
    )
    .unwrap()
}
fn draft(rate: i128) -> PriceRuleDraft {
    PriceRuleDraft {
        provider: "synthetic-provider".into(),
        model_exact: "synthetic-model".into(),
        source_id: None,
        currency: "USD".into(),
        effective_from_ms: EpochMs::new(0).unwrap(),
        effective_to_ms: None,
        priority: 0,
        input_rate_atoms: DecimalInt::from_nonnegative(rate).unwrap(),
        cache_write_rate_atoms: None,
        cached_rate_atoms: Some(DecimalInt::from_nonnegative(rate).unwrap()),
        output_rate_atoms: DecimalInt::from_nonnegative(rate).unwrap(),
        origin_reference: Some("synthetic fixture".into()),
    }
}

#[test]
fn date_and_grain_switches_reuse_heatmap_totals_without_changing_bundle_semantics() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(&db, "second-day", 86_401_000, 7, (None, None), None, None);
    let mut request = request();
    for index in 0..3 {
        if index == 1 {
            request.filter.range.end_ms = request.heatmap_range.end_ms;
            request.grain = Grain::Day;
        } else if index == 2 {
            request.filter.range.start_ms = EpochMs::new(86_400_000).unwrap();
            request.grain = Grain::Hour;
        }
        let cached = read(&db, &request);
        let expected = db
            .snapshot(|tx, revision| {
                bundle(
                    tx,
                    revision,
                    &request,
                    EpochMs::new(10_000).unwrap(),
                    "synthetic-dashboard",
                )
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(cached).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
    db.write(|conn| {
        conn.execute("UPDATE sources SET enabled=0", [])?;
        Ok(())
    })
    .unwrap();
    let cached = read(&db, &request);
    assert!(cached.heatmap.iter().all(|bucket| {
        bucket
            .coverage
            .source_issues
            .iter()
            .any(|issue| issue.code == "source_paused")
    }));
}

#[test]
fn summary_series_heatmap_and_recent_sessions_share_dimensions_but_independent_date_ranges() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(&db, "second-day", 86_401_000, 7, (None, None), None, None);
    let r = request();
    let b = read(&db, &r);
    assert_eq!(b.summary.total_tokens.as_str(), "110");
    assert_eq!(b.series.len(), 24);
    assert_eq!(b.heatmap.len(), 2);
    assert_eq!(b.series[0].totals.total_tokens.as_str(), "110");
    assert_eq!(b.series[1].totals.total_tokens.as_str(), "0");
    assert!(b.series[1].totals.input_total.value.is_none());
    assert_eq!(b.heatmap[0].totals.total_tokens.as_str(), "110");
    assert_eq!(b.heatmap[1].totals.total_tokens.as_str(), "7");
    assert_eq!(b.recent_sessions.len(), 1);
    let s = &b.recent_sessions[0];
    assert_eq!(s.session_key, "session");
    assert_eq!(s.display_name, "provider-session");
    assert_eq!(s.latest_at_ms.value(), 1000);
    assert_eq!(s.summary.total_tokens.as_str(), "110");
    assert!(s.latest_model.is_none() && s.latest_project_name.is_none());
    assert_eq!(s.pricing.unpriced_total_tokens.as_str(), "110");
    assert!(s.pricing.currencies.is_empty());
    assert_eq!(b.pricing.unpriced_total_tokens.as_str(), "110");
    assert!(matches!(b.coverage.state, CoverageState::Unknown));
    assert!(!b.coverage.breakdown_complete); // Legacy cache writes remain unknown.
    assert_eq!(b.meta.data_revision.as_str(), "2");
    assert_eq!(b.meta.price_revision.as_str(), "0");
    assert_eq!(b.meta.display_timezone, "UTC");
    assert_eq!(
        b.meta.parser_versions,
        [token_pulse_core::domain::PARSER_VERSION]
    );
    assert_eq!(
        b.meta.accounting_versions,
        [token_pulse_core::domain::ACCOUNTING_VERSION]
    );
    let mut none = r.clone();
    none.filter.sources = ids(&["missing"], false);
    let b = read(&db, &none);
    assert_eq!(b.summary.total_tokens.as_str(), "0");
    assert!(b.meta.parser_versions.is_empty() && b.recent_sessions.is_empty());
    assert!(
        b.heatmap
            .iter()
            .all(|b| b.totals.total_tokens.as_str() == "0")
    );
}

#[test]
fn old_sqlite_bundle_keeps_its_rules_facts_and_metadata_after_concurrent_commits() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn|{conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.effective_metadata.provider','synthetic-provider')",[])?;conn.execute("UPDATE usage_events SET model='synthetic-model'",[])?;Ok(())}).unwrap();
    db.mutate_price_rule(PriceRuleMutation::Create { draft: draft(1) }, 0, 1)
        .unwrap();
    let r = request();
    db.snapshot(|tx, revision| {
        let before = bundle(tx, revision, &r, EpochMs::new(10_000).unwrap(), "pinned")?;
        assert_eq!(
            before.pricing.currencies[0]
                .estimated_cost
                .as_ref()
                .unwrap()
                .as_str(),
            "0.000000000000110"
        );
        let old = crate::pricing::rules_at(tx, revision.price)?
            .rules
            .remove(0);
        db.mutate_price_rule(
            PriceRuleMutation::Replace {
                rule_id: old.rule_id,
                draft: draft(2),
            },
            1,
            2,
        )?;
        extra(&db, "concurrent", 2000, 7, (None, None), None, None);
        assert_eq!(
            serde_json::to_value(bundle(
                tx,
                revision,
                &r,
                EpochMs::new(10_000).unwrap(),
                "pinned"
            )?)
            .unwrap(),
            serde_json::to_value(before).unwrap()
        );
        Ok(())
    })
    .unwrap();
    let current = read(&db, &r);
    assert_eq!(current.summary.total_tokens.as_str(), "117");
    assert_eq!(current.meta.data_revision.as_str(), "2");
    assert_eq!(current.meta.price_revision.as_str(), "2");
    assert_eq!(
        current.pricing.currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap()
            .as_str(),
        "0.000000000000220"
    );
    assert_eq!(current.pricing.unpriced_total_tokens.as_str(), "7");
    assert_eq!(
        current.recent_sessions[0].summary.total_tokens.as_str(),
        "117"
    );
    assert_eq!(current.recent_sessions[0].latest_at_ms.value(), 2000);
}

#[test]
fn recent_session_limit_and_same_time_order_do_not_sort_large_amounts_as_floats() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    for i in 0..12 {
        db.ensure_session(SessionRegistration {
            session_key: format!("s-{i:02}"),
            provider_session_id: None,
            parent_key: None,
            parent_provider_id: None,
            created_at_ms: None,
            ledger_id: format!("l-{i:02}"),
            registered_at_ms: 1,
        })
        .unwrap();
    }
    db.write(|conn|{
        for i in 0..12 {
            let id=format!("recent-{i:02}");let session=format!("s-{i:02}");let ledger=format!("l-{i:02}");let offset=100+i*100;let total=9_007_199_254_740_993_i64+i;
            conn.execute("INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,observed_at_ms,normalized_json,payload_fingerprint,format_version) VALUES(?1,'generation',?2,?3,?4,'usage',4000,'{}',?1,'synthetic')",params![id,offset,offset+100,session])?;
            conn.execute("INSERT INTO usage_events(event_id,ledger_id,origin_observation_id,occurred_at_ms,episode_id,source_total_tokens,total_tokens,calculation_method,quality_json) VALUES(?1,?2,?1,4000,'synthetic',?3,?3,'synthetic','[\"confirmed\"]')",params![id,ledger,total])?;
            conn.execute("INSERT INTO event_provenance VALUES(?1,?1,'origin')",[id])?;
        }conn.execute("UPDATE app_state SET data_revision=data_revision+1",[])?;Ok(())
    }).unwrap();
    let b = read(&db, &request());
    assert_eq!(b.summary.total_tokens.as_str(), "108086391056892092");
    assert_eq!(b.recent_sessions.len(), 10);
    for (i, s) in b.recent_sessions.iter().enumerate() {
        assert_eq!(s.session_key, format!("s-{i:02}"));
        assert_eq!(s.latest_at_ms.value(), 4000);
        assert_eq!(
            s.summary.total_tokens.as_str(),
            (9_007_199_254_740_993_i64 + i as i64).to_string()
        );
        assert_eq!(s.pricing.unpriced_total_tokens, s.summary.total_tokens);
    }
}

#[test]
fn latest_project_and_model_come_from_the_stably_selected_event_not_final_metadata() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute(
            "INSERT INTO projects VALUES('a','synthetic-a','Project A','Alias A',1)",
            [],
        )?;
        conn.execute(
            "INSERT INTO projects VALUES('b','synthetic-b','Project B',NULL,1)",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    extra(
        &db,
        "aa",
        2000,
        1,
        (Some("first-model"), None),
        None,
        Some("a"),
    );
    extra(
        &db,
        "zz",
        2000,
        2,
        (Some("later-model"), None),
        None,
        Some("b"),
    );
    let b = read(&db, &request());
    let s = &b.recent_sessions[0];
    assert_eq!(s.latest_model.as_deref(), Some("first-model"));
    assert_eq!(s.latest_project_id.as_deref(), Some("a"));
    assert_eq!(s.latest_project_name.as_deref(), Some("Alias A"));
    assert_eq!(s.summary.total_tokens.as_str(), "113");
    let mut r = request();
    r.filter.projects = ids(&["b"], false);
    let b = read(&db, &r);
    assert_eq!(
        b.recent_sessions[0].latest_model.as_deref(),
        Some("later-model")
    );
    assert_eq!(b.recent_sessions[0].summary.total_tokens.as_str(), "2");
}

#[test]
fn empty_bundle_keeps_nulls_and_invalid_requests_do_not_downgrade_or_truncate() {
    let (_dir, db) = setup();
    let r = request();
    let b = read(&db, &r);
    assert_eq!(b.summary.total_tokens.as_str(), "0");
    assert!(b.summary.input_total.value.is_none());
    assert!(b.pricing.currencies.is_empty() && b.recent_sessions.is_empty());
    assert!(
        b.series
            .iter()
            .all(|s| s.totals.total_tokens.as_str() == "0" && !s.coverage.breakdown_complete)
    );
    let mut invalid = r.clone();
    invalid.heatmap_range.timezone = "Asia/Shanghai".into();
    assert_eq!(
        db.dashboard_bundle(&invalid, EpochMs::new(0).unwrap(), "valid")
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    invalid = r.clone();
    invalid.filter.range.end_ms = EpochMs::new(2001 * 3_600_000).unwrap();
    assert_eq!(
        db.dashboard_bundle(&invalid, EpochMs::new(0).unwrap(), "valid")
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.dashboard_bundle(&r, EpochMs::new(0).unwrap(), "")
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
}
