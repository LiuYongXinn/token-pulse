//! Synthetic rates only: these are exact math fixtures, not a market catalog.
use token_pulse_core::{
    domain::UsageVector,
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
    pricing::*,
    protocol::PriceBasis,
};

fn n(value: i128) -> DecimalInt {
    DecimalInt::from_nonnegative(value).unwrap()
}
fn t(value: i64) -> EpochMs {
    EpochMs::new(value).unwrap()
}
fn rule(id: &str) -> PriceRule {
    PriceRule {
        rule_id: id.into(),
        introduced_revision: n(1),
        retired_revision: None,
        provider: "fixture-provider".into(),
        model_exact: "fixture-model".into(),
        source_id: None,
        currency: "USD".into(),
        effective_from_ms: t(0),
        effective_to_ms: None,
        priority: 0,
        input_rate_atoms: rate_atoms("1.75").unwrap(),
        cached_rate_atoms: Some(rate_atoms("0.175").unwrap()),
        output_rate_atoms: rate_atoms("14").unwrap(),
        origin: PriceOrigin::Custom,
        origin_reference: Some("synthetic fixture".into()),
        created_at_ms: t(0),
    }
}
fn usage() -> UsageVector {
    UsageVector {
        input_total: Some(100),
        cached_input: Some(60),
        output_total: Some(10),
        reasoning_output: Some(2),
        reported_total: Some(110),
    }
}
fn event(usage: UsageVector) -> PricingEvent<'static> {
    PricingEvent {
        provider: Some("fixture-provider"),
        model: Some("fixture-model"),
        source_ids: &[],
        occurred_at_ms: t(1000),
        usage,
    }
}
fn catalog(rules: Vec<PriceRule>) -> PriceCatalog {
    PriceCatalog::new(rules, vec![], n(1)).unwrap()
}
fn atoms(outcome: PriceOutcome) -> (String, String, String) {
    match outcome {
        PriceOutcome::Priced {
            rule_id,
            cost_atoms,
            estimated_cost,
            ..
        } => (
            rule_id,
            cost_atoms.as_str().into(),
            estimated_cost.as_str().into(),
        ),
        PriceOutcome::Unpriced { reason } => panic!("unexpected {reason:?}"),
    }
}
fn reason(outcome: PriceOutcome) -> UnpricedCode {
    match outcome {
        PriceOutcome::Unpriced { reason } => reason,
        _ => panic!("unexpected pricing"),
    }
}

#[test]
fn rate_conversion_is_exact_to_nine_places_and_rejects_bad_or_overflowing_inputs() {
    for (value, expected, normalized) in [
        ("0", "0", "0"),
        ("1.750000000", "1750000000", "1.75"),
        ("0.000000001", "1", "0.000000001"),
        ("12.123456789", "12123456789", "12.123456789"),
        ("1000000.000000000", "1000000000000000", "1000000"),
    ] {
        let a = rate_atoms(value).unwrap();
        assert_eq!(a.as_str(), expected);
        assert_eq!(rate_per_million(&a), normalized);
    }
    for value in [
        "",
        "-1",
        "+1",
        "1e3",
        "1.",
        ".5",
        "1.1234567890",
        "1.2.3",
        "NaN",
        " 1",
        "01",
        "1. 0",
    ] {
        assert!(rate_atoms(value).is_err(), "{value}");
    }
    assert_eq!(
        rate_atoms("170141183460469231731687303715884105727").unwrap_err(),
        ErrorCode::InvalidQuery
    );
    let big = DecimalInt::parse("170141183460469231731687303715884105727").unwrap();
    assert!(rate_atoms(&rate_per_million(&big)).is_err());
}

#[test]
fn business_rate_limit_is_enforced_for_all_rates_and_drafts_without_rounding() {
    for value in ["1000000.000000001", "1000001", "9999999.123456789"] {
        assert_eq!(rate_atoms(value).unwrap_err(), ErrorCode::InvalidQuery);
    }
    for field in 0..3 {
        let mut r = rule("bounded");
        let excessive = n(MAX_RATE_ATOMS + 1);
        match field {
            0 => r.input_rate_atoms = excessive,
            1 => r.cached_rate_atoms = Some(excessive),
            _ => r.output_rate_atoms = excessive,
        }
        assert_eq!(r.validate().unwrap_err(), ErrorCode::InvalidQuery);
        assert!(PriceCatalog::new(vec![r], vec![], n(1)).is_err());
    }
}

#[test]
fn inclusive_cache_and_reasoning_are_not_charged_twice_and_smallest_atom_is_preserved() {
    let c = catalog(vec![rule("r")]);
    let (id, a, money) = atoms(c.estimate(&event(usage()), &PriceBasis::EventTime {}));
    assert_eq!(id, "r");
    assert_eq!(a, "220500000000");
    assert_eq!(money, "0.000220500000000");
    let mut missing_reason = usage();
    missing_reason.reasoning_output = None;
    assert_eq!(
        atoms(c.estimate(&event(missing_reason), &PriceBasis::EventTime {})).1,
        a
    );
    let mut tiny = rule("tiny");
    tiny.input_rate_atoms = rate_atoms("0.000000001").unwrap();
    tiny.cached_rate_atoms = None;
    tiny.output_rate_atoms = n(0);
    let c = catalog(vec![tiny]);
    let u = UsageVector {
        input_total: Some(1),
        cached_input: Some(0),
        output_total: Some(0),
        reasoning_output: None,
        reported_total: Some(1),
    };
    let (_, a, m) = atoms(c.estimate(&event(u), &PriceBasis::EventTime {}));
    assert_eq!(a, "1");
    assert_eq!(m, "0.000000000000001");
}

#[test]
fn unknown_cache_only_prices_when_split_is_unnecessary_and_known_zero_needs_no_cache_rate() {
    let mut r = rule("r");
    let mut u = usage();
    u.cached_input = None;
    assert_eq!(
        reason(catalog(vec![r.clone()]).estimate(&event(u), &PriceBasis::EventTime {})),
        UnpricedCode::InsufficientUsage
    );
    r.cached_rate_atoms = Some(r.input_rate_atoms.clone());
    assert_eq!(
        atoms(catalog(vec![r.clone()]).estimate(&event(u), &PriceBasis::EventTime {})).1,
        "315000000000"
    );
    r.cached_rate_atoms = None;
    u.cached_input = Some(0);
    assert_eq!(
        atoms(catalog(vec![r.clone()]).estimate(&event(u), &PriceBasis::EventTime {})).1,
        "315000000000"
    );
    u.input_total = Some(0);
    u.cached_input = None;
    u.reported_total = Some(10);
    assert_eq!(
        atoms(catalog(vec![r.clone()]).estimate(&event(u), &PriceBasis::EventTime {})).1,
        "140000000000"
    );
    u.input_total = None;
    assert_eq!(
        reason(catalog(vec![r]).estimate(&event(u), &PriceBasis::EventTime {})),
        UnpricedCode::InsufficientUsage
    );
}

#[test]
fn revisions_event_time_and_explicit_valuation_time_use_half_open_rule_intervals() {
    let mut first = rule("first");
    first.effective_to_ms = Some(t(1000));
    first.retired_revision = Some(n(2));
    let mut replacement = first.clone();
    replacement.rule_id = "replacement".into();
    replacement.introduced_revision = n(2);
    replacement.retired_revision = None;
    let mut after = rule("after");
    after.effective_from_ms = t(1000);
    after.input_rate_atoms = n(0);
    after.cached_rate_atoms = Some(n(0));
    after.output_rate_atoms = n(0);
    let old = PriceCatalog::new(
        vec![first.clone(), replacement.clone(), after.clone()],
        vec![],
        n(1),
    )
    .unwrap();
    assert_eq!(
        atoms(old.estimate(&event(usage()), &PriceBasis::EventTime {})).0,
        "after"
    );
    assert_eq!(
        atoms(old.estimate(
            &event(usage()),
            &PriceBasis::SpecifiedTime {
                specified_at_ms: t(999)
            }
        ))
        .0,
        "first"
    );
    let current = PriceCatalog::new(vec![first, replacement, after], vec![], n(2)).unwrap();
    assert_eq!(
        atoms(current.estimate(
            &event(usage()),
            &PriceBasis::SpecifiedTime {
                specified_at_ms: t(999)
            }
        ))
        .0,
        "replacement"
    );
    assert_eq!(
        reason(current.estimate(
            &event(usage()),
            &PriceBasis::SpecifiedTime {
                specified_at_ms: t(-1)
            }
        )),
        UnpricedCode::MissingRule
    );
}

#[test]
fn source_custom_global_custom_and_offline_ranks_are_stable_and_mirror_conflicts_are_unpriced() {
    let mut offline = rule("offline");
    offline.origin = PriceOrigin::Offline;
    offline.priority = 10000;
    let global = rule("global");
    let mut source = rule("source");
    source.source_id = Some("source".into());
    assert_eq!(
        atoms(catalog(vec![offline.clone()]).estimate(&event(usage()), &PriceBasis::EventTime {}))
            .0,
        "offline"
    );
    assert_eq!(
        atoms(
            catalog(vec![offline.clone(), global.clone(), source.clone()])
                .estimate(&event(usage()), &PriceBasis::EventTime {})
        )
        .0,
        "global"
    );
    let actual_sources = vec!["source".into(), "mirror".into()];
    let mut e = event(usage());
    e.source_ids = &actual_sources;
    assert_eq!(
        atoms(
            catalog(vec![offline.clone(), source.clone(), global.clone()])
                .estimate(&e, &PriceBasis::EventTime {})
        )
        .0,
        "source"
    );
    let mut mirror = source.clone();
    mirror.rule_id = "mirror".into();
    mirror.source_id = Some("mirror".into());
    let ambiguous = catalog(vec![source.clone(), mirror.clone(), global]);
    assert_eq!(
        reason(ambiguous.estimate(&e, &PriceBasis::EventTime {})),
        UnpricedCode::AmbiguousRule
    );
    source.priority = 1;
    assert_eq!(
        atoms(catalog(vec![mirror, offline, source]).estimate(&e, &PriceBasis::EventTime {})).0,
        "source"
    );
}

#[test]
fn aliases_are_explicit_versioned_exact_provider_matches_and_ambiguous_mapping_is_rejected() {
    let a = ModelAlias {
        alias_id: "a".into(),
        provider: "fixture-provider".into(),
        alias: "fixture-alias".into(),
        canonical_model: "fixture-model".into(),
        introduced_revision: n(1),
        retired_revision: Some(n(2)),
    };
    let c = PriceCatalog::new(vec![rule("r")], vec![a.clone()], n(1)).unwrap();
    let mut e = event(usage());
    e.model = Some("fixture-alias");
    assert_eq!(atoms(c.estimate(&e, &PriceBasis::EventTime {})).0, "r");
    let later = PriceCatalog::new(vec![rule("r")], vec![a.clone()], n(2)).unwrap();
    assert_eq!(
        reason(later.estimate(&e, &PriceBasis::EventTime {})),
        UnpricedCode::MissingRule
    );
    let mut conflict = a.clone();
    conflict.alias_id = "conflict".into();
    conflict.canonical_model = "other".into();
    assert_eq!(
        reason(
            PriceCatalog::new(vec![rule("r")], vec![conflict, a], n(1))
                .unwrap()
                .estimate(&e, &PriceBasis::EventTime {})
        ),
        UnpricedCode::AmbiguousRule
    );
    e.model = Some("fixture-model-2026");
    assert_eq!(
        reason(c.estimate(&e, &PriceBasis::EventTime {})),
        UnpricedCode::MissingRule
    );
    e.model = None;
    assert_eq!(
        reason(c.estimate(&e, &PriceBasis::EventTime {})),
        UnpricedCode::UnknownModel
    );
    e.model = Some("fixture-model");
    e.provider = Some("other-provider");
    assert_eq!(
        reason(c.estimate(&e, &PriceBasis::EventTime {})),
        UnpricedCode::MissingRule
    );
    e.provider = None;
    assert_eq!(
        reason(c.estimate(&e, &PriceBasis::EventTime {})),
        UnpricedCode::UnknownModel
    );
}

#[test]
fn large_amounts_invalid_usage_and_overflow_never_become_rounded_or_zero_estimates() {
    let mut r = rule("r");
    r.input_rate_atoms = n(1);
    r.cached_rate_atoms = None;
    r.output_rate_atoms = n(0);
    let u = UsageVector {
        input_total: Some(9_007_199_254_740_993),
        cached_input: Some(0),
        output_total: Some(0),
        reasoning_output: None,
        reported_total: None,
    };
    assert_eq!(
        atoms(catalog(vec![r.clone()]).estimate(&event(u), &PriceBasis::EventTime {})).2,
        "9.007199254740993"
    );
    let mut invalid = u;
    invalid.input_total = Some(i64::MAX);
    invalid.output_total = Some(1);
    assert_eq!(
        reason(catalog(vec![r.clone()]).estimate(&event(invalid), &PriceBasis::EventTime {})),
        UnpricedCode::Overflow
    );
    invalid.input_total = Some(1);
    invalid.output_total = Some(0);
    invalid.cached_input = Some(2);
    assert_eq!(
        reason(catalog(vec![r.clone()]).estimate(&event(invalid), &PriceBasis::EventTime {})),
        UnpricedCode::InsufficientUsage
    );
    invalid = UsageVector::default();
    assert_eq!(
        reason(catalog(vec![r]).estimate(&event(invalid), &PriceBasis::EventTime {})),
        UnpricedCode::InsufficientUsage
    );
}

#[test]
fn synthetic_complete_vectors_match_independent_integer_formula_for_all_inclusion_cases() {
    let mut r = rule("r");
    r.input_rate_atoms = n(2_000_000_000);
    r.cached_rate_atoms = Some(n(3_000_000_000));
    r.output_rate_atoms = n(5_000_000_000);
    let c = catalog(vec![r]);
    for input in 0..=20 {
        for cache in 0..=input {
            for output in 0..=10 {
                for reasoning in 0..=output {
                    let u = UsageVector {
                        input_total: Some(input),
                        cached_input: Some(cache),
                        output_total: Some(output),
                        reasoning_output: Some(reasoning),
                        reported_total: Some(input + output),
                    };
                    let expected = (i128::from(input - cache) * 2
                        + i128::from(cache) * 3
                        + i128::from(output) * 5)
                        * 1_000_000_000;
                    assert_eq!(
                        atoms(c.estimate(&event(u), &PriceBasis::EventTime {})).1,
                        expected.to_string()
                    );
                }
            }
        }
    }
}

#[test]
fn catalog_validation_rejects_uncontrolled_strings_revisions_currency_and_duplicate_ids() {
    let valid = rule("r");
    for change in 0..8 {
        let mut r = valid.clone();
        match change {
            0 => r.provider = "\n".into(),
            1 => r.currency = "usd".into(),
            2 => r.currency = "U$D".into(),
            3 => r.effective_to_ms = Some(t(0)),
            4 => r.retired_revision = Some(n(1)),
            5 => r.priority = -1,
            6 => {
                r.origin = PriceOrigin::Offline;
                r.source_id = Some("s".into());
            }
            _ => r.origin_reference = Some("line\nline".into()),
        };
        assert!(PriceCatalog::new(vec![r], vec![], n(1)).is_err());
    }
    assert!(PriceCatalog::new(vec![valid.clone(), valid], vec![], n(1)).is_err());
}
