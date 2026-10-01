use super::*;
use crate::batch::tests::{fixture, setup};
use token_pulse_core::{
    pricing::{PriceRuleDraft, PriceRuleMutation},
    protocol::ScopeStart,
    settings::TimezoneMutation,
};
fn at(value: i64) -> EpochMs {
    EpochMs::new(value).unwrap()
}
fn fixed(key: &str, start: i64) -> MiniScope {
    MiniScope::Session {
        session_key: key.into(),
        start: ScopeStart::Fixed {
            start_ms: at(start),
        },
    }
}
fn set(db: &Database, scope: MiniScope, revision: &str) -> StoreResult<(MiniScopeSnapshot, bool)> {
    db.mutate_mini_scope(
        MiniScopeMutation {
            mini_scope: scope,
            expected_settings_revision: DecimalInt::parse(revision).unwrap(),
        },
        at(10_000),
    )
}
fn initialize(db: &Database) {
    db.mutate_display_timezone(
        TimezoneMutation::Initialize {
            system_timezone: "UTC".into(),
        },
        at(100),
    )
    .unwrap();
}
fn rule(rate: &str) -> PriceRuleDraft {
    PriceRuleDraft {
        provider: "test-provider".into(),
        model_exact: "test-model".into(),
        source_id: None,
        currency: "USD".into(),
        effective_from_ms: at(0),
        effective_to_ms: None,
        priority: 0,
        input_rate_atoms: DecimalInt::parse(rate).unwrap(),
        cached_rate_atoms: Some(DecimalInt::parse(rate).unwrap()),
        output_rate_atoms: DecimalInt::parse(rate).unwrap(),
        origin_reference: Some("synthetic mini fixture".into()),
    }
}
#[test]
fn explicit_stats_navigation_preserves_exact_scope_and_changes_no_stored_main_or_mini_settings() {
    let (_, db) = setup();
    db.commit(fixture()).unwrap();
    initialize(&db);
    crate::query::tests::extra(&db, "later", 2000, 7, (None, None), None, None);
    let before = db.mini_scope().unwrap().settings_revision;
    let (saved, _) = set(&db, fixed("session", 1501), before.as_str()).unwrap();
    let usage = db.mini_usage(at(2000), "navigation").unwrap();
    assert_eq!(
        token_pulse_core::mini::open_stats_request(
            &usage,
            &token_pulse_core::mini::MiniStatsOpenRequest {
                expected_settings_revision: before
            },
            "stale-open".into()
        )
        .unwrap_err(),
        ErrorCode::RevisionConflict
    );
    let intent =
        token_pulse_core::mini::stats_request(&usage, "explicit-navigation".into()).unwrap();
    assert_eq!(intent.calendar.range.start_ms.value(), 1501);
    assert_eq!(intent.calendar.range.end_ms.value(), 2001);
    assert_eq!(intent.calendar.range.timezone, "UTC");
    assert_eq!(intent.calendar.local_today, "1970-01-01");
    assert_eq!(intent.calendar.heatmap_range.end_ms.value(), 86_400_000);
    let mut filter = crate::query::tests::filter();
    filter.range = intent.calendar.range;
    filter.sessions = token_pulse_core::protocol::DimensionSelection::Ids {
        ids: vec!["session".into()],
        include_unknown: false,
    };
    assert_eq!(db.usage_totals(&filter).unwrap().total_tokens.as_str(), "7");
    let scope = db.mini_scope().unwrap();
    assert_eq!(
        scope.settings_revision.as_str(),
        saved.settings_revision.as_str()
    );
    assert_eq!(scope.mini_scope, fixed("session", 1501));
    assert_eq!(
        db.display_settings().unwrap().settings_revision.as_str(),
        saved.settings_revision.as_str()
    );
    assert!(token_pulse_core::mini::stats_request(&usage, "bad\nrequest".into()).is_err());
}
#[test]
fn mini_is_independent_of_main_filters_and_uses_captured_cutoff_and_persistent_fixed_start() {
    let (dir, db) = setup();
    db.commit(fixture()).unwrap();
    assert_eq!(
        db.mini_usage(at(1000), "mini").unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    initialize(&db);
    crate::query::tests::extra(&db, "later", 2000, 7, (None, None), None, None);
    crate::query::tests::extra(&db, "future", 3000, 9, (None, None), None, None);
    let first = db.mini_usage(at(1000), "mini").unwrap();
    assert_eq!(first.range.start_ms.value(), 0);
    assert_eq!(first.range.end_ms.value(), 1001);
    assert_eq!(first.usage.total_tokens.as_str(), "110");
    let mut unrelated = crate::query::tests::filter();
    unrelated.range.start_ms = at(2999);
    assert_eq!(
        db.usage_totals(&unrelated).unwrap().total_tokens.as_str(),
        "9"
    );
    assert_eq!(
        db.mini_usage(at(2000), "mini")
            .unwrap()
            .usage
            .total_tokens
            .as_str(),
        "117"
    );
    let revision = db.mini_scope().unwrap().settings_revision;
    let (saved, changed) = set(&db, fixed("session", 1500), revision.as_str()).unwrap();
    assert!(changed);
    let fixed_result = db.mini_usage(at(2000), "mini-fixed").unwrap();
    assert_eq!(fixed_result.usage.total_tokens.as_str(), "7");
    assert_eq!(
        fixed_result.scope_display_name.as_deref(),
        Some("provider-session")
    );
    assert_eq!(fixed_result.range.start_ms.value(), 1500);
    assert_eq!(fixed_result.range.end_ms.value(), 2001);
    assert_eq!(fixed_result.pricing.unpriced_total_tokens.as_str(), "7");
    assert!(fixed_result.usage.input_total.value.is_none());
    assert!(
        !set(
            &db,
            fixed("session", 1500),
            saved.settings_revision.as_str()
        )
        .unwrap()
        .1
    );
    drop(db);
    let reopened = Database::open(dir.path()).unwrap();
    assert_eq!(
        reopened.mini_scope().unwrap().mini_scope,
        fixed("session", 1500)
    );
    assert_eq!(
        reopened
            .mini_usage(at(3000), "mini-reopened")
            .unwrap()
            .usage
            .total_tokens
            .as_str(),
        "16"
    );
}
#[test]
fn scope_counts_and_exact_prices_remain_in_the_real_old_read_transaction_after_writes() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    initialize(&db);
    db.write(|c| { c.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.effective_metadata.provider','test-provider')", [])?; c.execute("UPDATE usage_events SET model='test-model'", [])?; Ok(()) }).unwrap();
    db.mutate_price_rule_snapshot(
        PriceRuleMutation::Create {
            draft: rule("1000000000"),
        },
        0,
        100,
    )
    .unwrap();
    let before = db.mini_usage(at(2000), "mini-before").unwrap();
    assert_eq!(
        before.pricing.currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap()
            .as_str(),
        "0.000110000000000"
    );
    db.snapshot(|tx, revision| {
        let old = usage(tx, revision, at(2000), "mini-old")?;
        let r = db.mini_scope()?.settings_revision;
        set(&db, fixed("session", 1500), r.as_str())?;
        let catalog = db.price_rules_at(None)?;
        db.mutate_price_rule_snapshot(
            PriceRuleMutation::Replace {
                rule_id: catalog.rules[0].rule_id.clone(),
                draft: rule("2000000000"),
            },
            1,
            200,
        )?;
        let again = usage(tx, revision, at(2000), "mini-old")?;
        assert_eq!(
            serde_json::to_value(&old).unwrap(),
            serde_json::to_value(&again).unwrap()
        );
        Ok(())
    })
    .unwrap();
    let current = db.mini_usage(at(2000), "mini-new").unwrap();
    assert_eq!(current.usage.total_tokens.as_str(), "0");
    assert_eq!(current.meta.price_revision.as_str(), "2");
    assert_eq!(current.range.start_ms.value(), 1500);
}
#[test]
fn invalid_future_missing_and_stale_scope_cannot_overwrite_and_failed_writer_rolls_back() {
    let (_dir, db) = setup();
    initialize(&db);
    let r = db.mini_scope().unwrap().settings_revision;
    for candidate in [fixed("missing", 0), fixed("session", 10_001), fixed("", 0)] {
        assert_eq!(
            set(&db, candidate, r.as_str()).unwrap_err().code,
            ErrorCode::InvalidQuery
        );
    }
    assert_eq!(
        set(&db, fixed("session", 0), "0").unwrap_err().code,
        ErrorCode::RevisionConflict
    );
    db.write(|c| { c.execute_batch("CREATE TRIGGER fail_mini_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")?; Ok(()) }).unwrap();
    assert_eq!(
        set(&db, fixed("session", 0), r.as_str()).unwrap_err().code,
        ErrorCode::DbWriteFailed
    );
    assert_eq!(
        db.mini_scope().unwrap().mini_scope,
        MiniScope::TodayAllSources {}
    );
    assert_eq!(db.mini_scope().unwrap().settings_revision, r);
    assert_eq!(
        db.display_settings()
            .unwrap()
            .preferences
            .display_timezone
            .as_deref(),
        Some("UTC")
    );
}

#[test]
fn mini_old_usage_obeys_latest_privacy_without_modifying_original_scope_or_exact_numbers() {
    use token_pulse_core::privacy::{DisplayPolicyStamp, PrivacyState, PrivateResponse};
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    initialize(&db);
    let r = db.mini_scope().unwrap().settings_revision;
    let (saved, _) = set(&db, fixed("session", 0), r.as_str()).unwrap();
    let data = db.mini_usage(at(2000), "private-mini").unwrap();
    let policy = PrivacyState::new(DisplayPolicyStamp {
        settings_revision: saved.settings_revision.clone(),
        privacy: false,
    });
    let reply = PrivateResponse::new("private-mini".into(), data.clone(), policy.clone());
    policy
        .commit_update(|| {
            let (s, _) = db.mutate_display_privacy(
                token_pulse_core::settings::DisplayPrivacyMutation {
                    privacy: true,
                    expected_settings_revision: saved.settings_revision,
                },
                at(2000),
            )?;
            Ok::<_, crate::StoreError>((
                (),
                DisplayPolicyStamp {
                    settings_revision: s.settings_revision,
                    privacy: s.preferences.privacy,
                },
            ))
        })
        .unwrap();
    let value = serde_json::to_value(reply).unwrap();
    assert!(!value.to_string().contains("provider-session"));
    assert_eq!(value["data"]["usage"]["total_tokens"], "110");
    assert_eq!(value["data"]["range"]["start_ms"], 0);
    assert_eq!(value["data"]["mini_scope"]["session_key"], "session");
    assert_eq!(value["data"]["pricing"]["redacted"], true);
    assert_eq!(data.scope_display_name.as_deref(), Some("provider-session"));
    assert_eq!(
        db.mini_usage(at(2000), "raw-mini")
            .unwrap()
            .scope_display_name
            .as_deref(),
        Some("provider-session")
    );
}
