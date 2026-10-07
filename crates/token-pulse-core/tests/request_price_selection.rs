//! Independent synthetic rate oracle; no real account, mode inference or online requests.
use token_pulse_core::{
    domain::*,
    numeric::{DecimalInt, EpochMs},
    pricing::{offline::*, *},
    protocol::PriceBasis,
};
fn n(value: i128) -> DecimalInt {
    DecimalInt::from_nonnegative(value).unwrap()
}
fn time(value: i64) -> EpochMs {
    EpochMs::new(value).unwrap()
}
fn observation(input: i64, read: Option<i64>, write: Option<i64>) -> UsageObservation {
    let usage = UsageVector {
        input_total: Some(input),
        cached_input: read,
        cache_write_input: write,
        output_total: Some(17),
        reasoning_output: Some(9),
        reported_total: Some(input + 17),
    };
    let cumulative = UsageVector {
        input_total: Some(input.max(500000)),
        output_total: Some(40),
        reported_total: Some(input.max(500000) + 40),
        ..usage
    };
    UsageObservation {
        physical_position: PhysicalPosition {
            file_generation_id: "file".into(),
            byte_offset: 10,
            byte_end: 20,
        },
        session_key: "session".into(),
        event_time_ms: Some(2000),
        request_identity: None,
        request_usage: Some(Box::new(RequestUsageEvidence {
            response_id: "response".into(),
            turn_id: "turn".into(),
            usage,
            thread_usage: cumulative,
            physical_position: PhysicalPosition {
                file_generation_id: "file".into(),
                byte_offset: 0,
                byte_end: 10,
            },
        })),
        stream_hint: None,
        last: Some(usage),
        cumulative: Some(cumulative),
        effective_metadata: EffectiveMetadata {
            turn_id: Some("turn".into()),
            ..Default::default()
        },
        explicit_episode_start: false,
        model_context_window: Some(1000000),
    }
}
fn event(usage: UsageVector) -> PricingEvent<'static> {
    PricingEvent {
        provider: Some("openai"),
        model: Some("synthetic-conditional"),
        source_ids: &[],
        occurred_at_ms: time(2000),
        usage,
    }
}
fn evidence(o: &UsageObservation, tier: Option<OfflinePriceTier>) -> RequestPriceEvidence<'_> {
    RequestPriceEvidence {
        response: o.request_usage.as_ref().unwrap(),
        context: o.into(),
        actual_tier: tier,
    }
}
fn catalog() -> OfflinePriceCatalog {
    let mut result = OfflinePriceCatalog {
        format_version: 1,
        catalog_id: "openai-text-synthetic".into(),
        verified_at_ms: time(1000),
        provider: "openai".into(),
        currency: "USD".into(),
        short_context_max_input: 272000,
        reference_basis: OfflineReferenceBasis::GlobalApiReference,
        entries: Vec::new(),
    };
    for (mode, factor) in [
        (OfflinePriceTier::Standard, 1),
        (OfflinePriceTier::Fast, 2),
        (OfflinePriceTier::Batch, 3),
        (OfflinePriceTier::Flex, 4),
        (OfflinePriceTier::Ultrafast, 5),
    ] {
        for (band, rates) in [
            (OfflineContextBand::Short, [1, 2, 3, 5]),
            (OfflineContextBand::Long, [7, 11, 13, 17]),
        ] {
            let values = rates.map(|r| (r * factor).to_string());
            result.entries.push(OfflinePriceEntry {
                model_exact: "synthetic-conditional".into(),
                tier: mode,
                context: band,
                input_per_million: values[0].clone(),
                cached_per_million: Some(values[1].clone()),
                cache_write_per_million: Some(values[2].clone()),
                output_per_million: values[3].clone(),
                reference: "https://developers.openai.com/api/docs/pricing".into(),
            });
        }
    }
    result.validate().unwrap();
    result
}
fn estimate(rule: PriceRule, event: &PricingEvent<'_>, basis: &PriceBasis) -> PriceOutcome {
    PriceCatalog::new(vec![rule], vec![], n(4))
        .unwrap()
        .estimate(event, basis)
}
fn atoms(outcome: &PriceOutcome) -> i128 {
    let PriceOutcome::Priced { cost_atoms, .. } = outcome else {
        panic!("expected estimate, got {outcome:?}")
    };
    cost_atoms.value()
}

#[test]
fn default_reference_discloses_assumptions_and_preserves_unknown_usage() {
    let engine = PriceCatalog::new(vec![], vec![], n(4))
        .unwrap()
        .with_offline_reference(&catalog())
        .unwrap();
    let o = observation(500000, Some(100), None);
    let event = event(o.last.unwrap());
    let evaluated = engine.evaluate_with_request(&event, &PriceBasis::EventTime {}, None);
    // Short reference: (499900*1 + 100*2 + 17*5) * 1e9 atoms.
    assert_eq!(atoms(&evaluated.outcome), 500_185_000_000_000);
    assert_eq!(event.usage.cache_write_input, None);
    let matched = evaluated.matched_price(&evaluated.outcome).unwrap();
    assert!(matches!(
        matched.basis,
        PriceMatchBasis::OfflineAssumedReference {
            context: OfflineContextBand::Short,
            context_assumed: true,
            cache_write_assumed_zero: true,
            ..
        }
    ));
    assert!(
        !serde_json::to_string(&matched)
            .unwrap()
            .contains("actual_tier")
    );

    let bound =
        engine.evaluate_with_request(&event, &PriceBasis::EventTime {}, Some(evidence(&o, None)));
    // Long reference: (499900*7 + 100*11 + 17*17) * 1e9 atoms.
    assert_eq!(atoms(&bound.outcome), 3_500_689_000_000_000);
    assert!(matches!(
        bound.matched_price(&bound.outcome).unwrap().basis,
        PriceMatchBasis::OfflineAssumedReference {
            context: OfflineContextBand::Long,
            context_assumed: false,
            cache_write_assumed_zero: true,
            ..
        }
    ));
}

#[test]
fn assumed_standard_respects_context_boundary_without_using_cumulative_consumption() {
    let engine = PriceCatalog::new(vec![], vec![], n(4))
        .unwrap()
        .with_offline_reference(&catalog())
        .unwrap();
    for (input, band, expected) in [
        (272000, OfflineContextBand::Short, 272_235_000_000_000),
        (272001, OfflineContextBand::Long, 1_904_846_000_000_000),
    ] {
        let o = observation(input, Some(100), Some(25));
        let e = event(o.last.unwrap());
        let result =
            engine.evaluate_with_request(&e, &PriceBasis::EventTime {}, Some(evidence(&o, None)));
        assert_eq!(atoms(&result.outcome), expected);
        assert!(
            matches!(result.matched_price(&result.outcome).unwrap().basis,
            PriceMatchBasis::OfflineAssumedReference { context, context_assumed: false, cache_write_assumed_zero: false, .. } if context == band)
        );
    }
    let o = observation(272001, Some(100), Some(25));
    let e = event(UsageVector {
        input_total: Some(50),
        cached_input: Some(10),
        cache_write_input: Some(5),
        reported_total: Some(67),
        ..o.last.unwrap()
    });
    let result =
        engine.evaluate_with_request(&e, &PriceBasis::EventTime {}, Some(evidence(&o, None)));
    assert_eq!(atoms(&result.outcome), 155_000_000_000);
    assert!(matches!(
        result.matched_price(&result.outcome).unwrap().basis,
        PriceMatchBasis::OfflineAssumedReference {
            context: OfflineContextBand::Short,
            context_assumed: true,
            ..
        }
    ));
}

#[test]
fn configured_engine_prices_each_full_request_and_accumulates_mixed_bands() {
    let engine = PriceCatalog::new(vec![], vec![], n(4))
        .unwrap()
        .with_offline_reference(&catalog())
        .unwrap();
    let short = observation(200000, Some(100), Some(25));
    let long = observation(272001, Some(100), Some(25));
    let short_event = event(short.last.unwrap());
    let long_event = event(long.last.unwrap());
    let short_result = engine.estimate_with_request(
        &short_event,
        &PriceBasis::EventTime {},
        Some(evidence(&short, Some(OfflinePriceTier::Standard))),
    );
    let long_result = engine.estimate_with_request(
        &long_event,
        &PriceBasis::EventTime {},
        Some(evidence(&long, Some(OfflinePriceTier::Fast))),
    );
    // Independent disjoint input/output oracle, including the whole long output.
    assert_eq!(atoms(&short_result), 200_235_000_000_000);
    assert_eq!(atoms(&long_result), 3_809_692_000_000_000);
    let PriceOutcome::Priced { rule_id, .. } = &long_result else {
        unreachable!()
    };
    assert_eq!(
        rule_id,
        "offline/openai-text-synthetic/synthetic-conditional/fast/long"
    );
    let mut sum = PricingAccumulator::new(PriceBasis::EventTime {});
    sum.push(200017, short_result).unwrap();
    sum.push(272018, long_result).unwrap();
    let summary = sum.summary(false).unwrap();
    assert_eq!(summary.priced_total_tokens.as_str(), "472035");
    assert_eq!(
        summary.currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap()
            .as_str(),
        "4.009927000000000"
    );
}

#[test]
fn configured_engine_keeps_aliases_explicit_rule_precedence_and_source_ambiguity() {
    let reference = catalog();
    let o = observation(200000, Some(100), Some(25));
    let alias = ModelAlias {
        alias_id: "alias".into(),
        provider: "openai".into(),
        alias: "snapshot".into(),
        canonical_model: "synthetic-conditional".into(),
        introduced_revision: n(1),
        retired_revision: None,
    };
    let alias_event = PricingEvent {
        model: Some("snapshot"),
        ..event(o.last.unwrap())
    };
    let engine = PriceCatalog::new(vec![], vec![alias.clone()], n(4))
        .unwrap()
        .with_offline_reference(&reference)
        .unwrap();
    let quote = engine.estimate_with_request(
        &alias_event,
        &PriceBasis::EventTime {},
        Some(evidence(&o, Some(OfflinePriceTier::Standard))),
    );
    assert_eq!(atoms(&quote), 200_235_000_000_000);
    let mut fixed = reference
        .select_request_reference(
            &event(o.last.unwrap()),
            Some(evidence(&o, Some(OfflinePriceTier::Standard))),
            n(1),
            time(1000),
        )
        .unwrap()
        .rule;
    fixed.rule_id = "fixed-estimate".into();
    fixed.origin = PriceOrigin::Custom;
    fixed.input_rate_atoms = n(0);
    fixed.cached_rate_atoms = Some(n(0));
    fixed.cache_write_rate_atoms = Some(n(0));
    fixed.output_rate_atoms = rate_atoms("2").unwrap();
    let mut source = fixed.clone();
    source.rule_id = "source-estimate".into();
    source.source_id = Some("left".into());
    source.currency = "EUR".into();
    source.output_rate_atoms = rate_atoms("3").unwrap();
    let engine = PriceCatalog::new(
        vec![fixed.clone(), source.clone()],
        vec![alias.clone()],
        n(4),
    )
    .unwrap()
    .with_offline_reference(&reference)
    .unwrap();
    let source_ids = vec!["left".into()];
    let from_source = PricingEvent {
        source_ids: &source_ids,
        ..alias_event
    };
    // Explicit estimates do not require or infer a real processing mode.
    assert_eq!(
        atoms(&engine.estimate_with_request(&alias_event, &PriceBasis::EventTime {}, None)),
        34_000_000_000
    );
    let result = engine.estimate_with_request(
        &from_source,
        &PriceBasis::EventTime {},
        Some(evidence(&o, Some(OfflinePriceTier::Ultrafast))),
    );
    assert_eq!(atoms(&result), 51_000_000_000);
    let PriceOutcome::Priced {
        currency, rule_id, ..
    } = result
    else {
        unreachable!()
    };
    assert_eq!(
        (currency.as_str(), rule_id.as_str()),
        ("EUR", "source-estimate")
    );
    let mut other = source.clone();
    other.rule_id = "other-source".into();
    other.source_id = Some("right".into());
    let engine = PriceCatalog::new(vec![fixed, source, other], vec![alias], n(4))
        .unwrap()
        .with_offline_reference(&reference)
        .unwrap();
    let source_ids = vec!["left".into(), "right".into()];
    let mirrored = PricingEvent {
        source_ids: &source_ids,
        ..alias_event
    };
    assert!(matches!(
        engine.estimate_with_request(
            &mirrored,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        ),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::AmbiguousRule
        }
    ));
}

#[test]
fn configured_engine_estimates_unknown_mode_but_requires_confirmed_mode_quotes() {
    let mut reference = catalog();
    reference.entries.retain(|entry| {
        !(entry.tier == OfflinePriceTier::Fast && entry.context == OfflineContextBand::Long)
    });
    let engine = PriceCatalog::new(vec![], vec![], n(4))
        .unwrap()
        .with_offline_reference(&reference)
        .unwrap();
    let o = observation(272001, Some(100), Some(25));
    let original = event(o.last.unwrap());
    assert_eq!(
        atoms(&engine.estimate_with_request(&original, &PriceBasis::EventTime {}, None)),
        272_236_000_000_000
    );
    assert_eq!(
        atoms(&engine.estimate_with_request(
            &original,
            &PriceBasis::EventTime {},
            Some(evidence(&o, None))
        )),
        1_904_846_000_000_000
    );
    assert!(matches!(
        engine.estimate_with_request(
            &original,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        ),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::IncompletePricingConditions
        }
    ));
    let partial = PricingEvent {
        usage: UsageVector {
            reported_total: None,
            ..original.usage
        },
        ..original
    };
    assert!(matches!(
        engine.estimate_with_request(
            &partial,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Standard)))
        ),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::IncompletePricingConditions
        }
    ));
    let old = PricingEvent {
        occurred_at_ms: time(500),
        ..original
    };
    assert!(matches!(
        engine.estimate_with_request(
            &old,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Standard)))
        ),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::MissingRule
        }
    ));
    assert_eq!(
        atoms(&engine.estimate_with_request(
            &old,
            &PriceBasis::SpecifiedTime {
                specified_at_ms: time(2000)
            },
            Some(evidence(&o, Some(OfflinePriceTier::Standard)))
        )),
        1_904_846_000_000_000
    );
    assert_eq!(original.usage.validated_total().unwrap(), Some(272018));
}

fn flat_catalog() -> OfflinePriceCatalog {
    let mut reference = catalog();
    reference.entries = [(OfflinePriceTier::Standard, 1), (OfflinePriceTier::Fast, 4)]
        .into_iter()
        .map(|(tier, factor)| OfflinePriceEntry {
            model_exact: "synthetic-flat".into(),
            tier,
            context: OfflineContextBand::All,
            input_per_million: factor.to_string(),
            cached_per_million: Some((factor * 2).to_string()),
            cache_write_per_million: None,
            output_per_million: (factor * 5).to_string(),
            reference: "https://developers.openai.com/api/docs/pricing".into(),
        })
        .collect();
    reference
}
#[test]
fn confirmed_modes_replace_only_attached_flat_references_and_never_fall_back() {
    let reference = flat_catalog();
    let rules = reference.flat_standard_rules(n(4), time(1000)).unwrap();
    let engine = PriceCatalog::new(rules.clone(), vec![], n(4))
        .unwrap()
        .with_offline_reference(&reference)
        .unwrap();
    let o = observation(1000, Some(100), Some(0));
    let event = PricingEvent {
        model: Some("synthetic-flat"),
        ..event(o.last.unwrap())
    };
    assert_eq!(
        atoms(&engine.estimate(&event, &PriceBasis::EventTime {})),
        1_185_000_000_000
    );
    assert_eq!(
        atoms(&engine.estimate_with_request(
            &event,
            &PriceBasis::EventTime {},
            Some(evidence(&o, None))
        )),
        1_185_000_000_000
    );
    assert_eq!(
        atoms(&engine.estimate_with_request(
            &event,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        )),
        4_740_000_000_000
    );
    assert!(matches!(
        engine.estimate_with_request(
            &event,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Batch)))
        ),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::IncompletePricingConditions
        }
    ));
    let mut invalid = o.clone();
    invalid.effective_metadata.turn_id = Some("other".into());
    assert!(matches!(
        engine.estimate_with_request(
            &event,
            &PriceBasis::EventTime {},
            Some(evidence(&invalid, Some(OfflinePriceTier::Fast)))
        ),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::IncompletePricingConditions
        }
    ));
    let mut explicit = rules[0].clone();
    explicit.rule_id = "independent-offline-reference".into();
    explicit.priority = 2;
    explicit.output_rate_atoms = rate_atoms("10").unwrap();
    let engine = PriceCatalog::new(vec![explicit], vec![], n(4))
        .unwrap()
        .with_offline_reference(&reference)
        .unwrap();
    assert_eq!(
        atoms(&engine.estimate_with_request(
            &event,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        )),
        1_270_000_000_000
    );
}

fn publication(
    catalog: OfflinePriceCatalog,
    revision: i128,
    installed: i64,
) -> OfflineReferencePublication {
    OfflineReferencePublication {
        catalog,
        introduced_revision: n(revision),
        installed_at_ms: time(installed),
    }
}
fn next_catalog(original: &OfflinePriceCatalog) -> OfflinePriceCatalog {
    let mut next = original.clone();
    next.catalog_id = "openai-text-next-synthetic".into();
    next.verified_at_ms = time(3000);
    for entry in &mut next.entries {
        entry.input_per_million =
            (entry.input_per_million.parse::<i128>().unwrap() * 10).to_string();
        entry.cached_per_million = entry
            .cached_per_million
            .as_ref()
            .map(|rate| (rate.parse::<i128>().unwrap() * 10).to_string());
        entry.cache_write_per_million = entry
            .cache_write_per_million
            .as_ref()
            .map(|rate| (rate.parse::<i128>().unwrap() * 10).to_string());
        entry.output_per_million =
            (entry.output_per_million.parse::<i128>().unwrap() * 10).to_string();
    }
    next
}

#[test]
fn conditional_catalog_history_uses_estimation_time_and_keeps_captured_revisions() {
    let old = catalog();
    let next = next_catalog(&old);
    let history = vec![
        publication(old.clone(), 1, 1001),
        publication(next.clone(), 3, 3001),
    ];
    let engine = PriceCatalog::new(vec![], vec![], n(4))
        .unwrap()
        .with_offline_history(history)
        .unwrap();
    let o = observation(200000, Some(100), Some(25));
    let original = event(o.last.unwrap());
    for (occurred, specified, expected, id) in [
        (
            2000,
            None,
            400_470_000_000_000,
            "offline/openai-text-synthetic/synthetic-conditional/fast/short",
        ),
        (
            3000,
            None,
            4_004_700_000_000_000,
            "offline/openai-text-next-synthetic/synthetic-conditional/fast/short",
        ),
        (
            2000,
            Some(3000),
            4_004_700_000_000_000,
            "offline/openai-text-next-synthetic/synthetic-conditional/fast/short",
        ),
        (
            3000,
            Some(2999),
            400_470_000_000_000,
            "offline/openai-text-synthetic/synthetic-conditional/fast/short",
        ),
    ] {
        let event = PricingEvent {
            occurred_at_ms: time(occurred),
            ..original
        };
        let basis = specified
            .map(|at| PriceBasis::SpecifiedTime {
                specified_at_ms: time(at),
            })
            .unwrap_or(PriceBasis::EventTime {});
        let outcome = engine.estimate_with_request(
            &event,
            &basis,
            Some(evidence(&o, Some(OfflinePriceTier::Fast))),
        );
        assert_eq!(atoms(&outcome), expected);
        assert!(matches!(outcome,PriceOutcome::Priced {rule_id,..} if rule_id==id));
    }
    assert!(matches!(
        engine.estimate(&original, &PriceBasis::EventTime {}),
        PriceOutcome::Priced { .. }
    ));
    assert!(matches!(
        engine.estimate_with_request(
            &original,
            &PriceBasis::SpecifiedTime {
                specified_at_ms: time(999)
            },
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        ),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::MissingRule
        }
    ));
    let captured = PriceCatalog::new(vec![], vec![], n(2))
        .unwrap()
        .with_offline_history(vec![publication(old, 1, 1001)])
        .unwrap();
    let later = PricingEvent {
        occurred_at_ms: time(4000),
        ..original
    };
    assert_eq!(
        atoms(&captured.estimate_with_request(
            &later,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        )),
        400_470_000_000_000
    );
    let mut removed = next_catalog(&catalog());
    for entry in &mut removed.entries {
        entry.model_exact = "synthetic-other-model".into();
    }
    let no_fallback = PriceCatalog::new(vec![], vec![], n(4))
        .unwrap()
        .with_offline_history(vec![
            publication(catalog(), 1, 1001),
            publication(removed, 3, 3001),
        ])
        .unwrap();
    assert!(matches!(
        no_fallback.estimate_with_request(
            &later,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        ),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::MissingRule
        }
    ));
}

#[test]
fn historical_flat_reference_does_not_mask_a_confirmed_mode_or_restore_removed_quotes() {
    let old = flat_catalog();
    let mut next = next_catalog(&old);
    next.entries
        .retain(|entry| entry.tier != OfflinePriceTier::Fast);
    let mut closed = old.flat_standard_rules(n(1), time(1001)).unwrap().remove(0);
    // Golden digest of the old persisted JSON tuple [3, original rule ID].
    closed.rule_id =
        "offline-history/a74c3452edcae61e22ed72eb864cf78c5a18de5d636725f8184aea2458b84672".into();
    closed.introduced_revision = n(3);
    closed.effective_to_ms = Some(time(3000));
    closed.created_at_ms = time(3001);
    let mut rules = next.flat_standard_rules(n(3), time(3001)).unwrap();
    rules.push(closed);
    let engine = PriceCatalog::new(rules, vec![], n(4))
        .unwrap()
        .with_offline_history(vec![publication(old, 1, 1001), publication(next, 3, 3001)])
        .unwrap();
    let o = observation(1000, Some(100), Some(0));
    let event = PricingEvent {
        model: Some("synthetic-flat"),
        ..event(o.last.unwrap())
    };
    assert_eq!(
        atoms(&engine.estimate(&event, &PriceBasis::EventTime {})),
        1_185_000_000_000
    );
    assert_eq!(
        atoms(&engine.estimate_with_request(
            &event,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        )),
        4_740_000_000_000
    );
    let later = PricingEvent {
        occurred_at_ms: time(3000),
        ..event
    };
    assert_eq!(
        atoms(&engine.estimate(&later, &PriceBasis::EventTime {})),
        11_850_000_000_000
    );
    assert!(matches!(
        engine.estimate_with_request(
            &later,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        ),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::IncompletePricingConditions
        }
    ));
    assert_eq!(
        atoms(&engine.estimate_with_request(
            &later,
            &PriceBasis::SpecifiedTime {
                specified_at_ms: time(2000)
            },
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        )),
        4_740_000_000_000
    );
    let mut invalid = o.clone();
    invalid.effective_metadata.turn_id = None;
    assert!(matches!(
        engine.estimate_with_request(
            &event,
            &PriceBasis::EventTime {},
            Some(evidence(&invalid, Some(OfflinePriceTier::Fast)))
        ),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::IncompletePricingConditions
        }
    ));
}

#[test]
fn publication_history_rejects_invalid_order_and_uses_the_latest_equal_instant() {
    let old = catalog();
    let next = next_catalog(&old);
    for change in 0..7 {
        let mut history = vec![
            publication(old.clone(), 1, 1001),
            publication(next.clone(), 3, 3001),
        ];
        match change {
            0 => history.reverse(),
            1 => history[1].catalog.catalog_id = old.catalog_id.clone(),
            2 => history[1].introduced_revision = n(5),
            3 => history[1].catalog.verified_at_ms = time(999),
            4 => history[0].installed_at_ms = time(999),
            5 => history[1].introduced_revision = n(1),
            _ => history[1].catalog.format_version = 2,
        }
        assert!(
            PriceCatalog::new(vec![], vec![], n(4))
                .unwrap()
                .with_offline_history(history)
                .is_err()
        );
    }
    let mut equal = next;
    equal.verified_at_ms = old.verified_at_ms;
    let engine = PriceCatalog::new(vec![], vec![], n(4))
        .unwrap()
        .with_offline_history(vec![publication(old, 1, 1001), publication(equal, 3, 1002)])
        .unwrap();
    let o = observation(200000, Some(100), Some(25));
    let event = event(o.last.unwrap());
    assert_eq!(
        atoms(&engine.estimate_with_request(
            &event,
            &PriceBasis::EventTime {},
            Some(evidence(&o, Some(OfflinePriceTier::Fast)))
        )),
        4_004_700_000_000_000
    );
    assert!(matches!(
        PriceCatalog::new(vec![], vec![], n(4))
            .unwrap()
            .with_offline_history(vec![])
            .unwrap()
            .estimate(&event, &PriceBasis::EventTime {}),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::MissingRule
        }
    ));
}
#[test]
fn each_request_selects_all_four_rates_and_whole_output_with_exact_input_boundaries() {
    let catalog = catalog();
    for (tier, factor) in [
        (OfflinePriceTier::Standard, 1),
        (OfflinePriceTier::Fast, 2),
        (OfflinePriceTier::Batch, 3),
        (OfflinePriceTier::Flex, 4),
        (OfflinePriceTier::Ultrafast, 5),
    ] {
        for input in [0, 272000, 272001, 9007199254740993] {
            let read = if input == 0 { 0 } else { 100 };
            let write = if input == 0 { 0 } else { 25 };
            let o = observation(input, Some(read), Some(write));
            let event = event(o.last.unwrap());
            let selected = catalog
                .select_request_reference(&event, Some(evidence(&o, Some(tier))), n(4), time(2000))
                .unwrap();
            let (band, rates) = if input <= 272000 {
                (OfflineContextBand::Short, [1, 2, 3, 5])
            } else {
                (OfflineContextBand::Long, [7, 11, 13, 17])
            };
            assert_eq!(selected.context, band);
            assert_eq!(selected.tier, tier);
            assert_eq!(selected.basis, OfflineReferenceBasis::GlobalApiReference);
            let expected = ((i128::from(input - read - write) * rates[0])
                + i128::from(read) * rates[1]
                + i128::from(write) * rates[2]
                + 17 * rates[3])
                * factor
                * 1_000_000_000;
            let outcome = estimate(selected.rule, &event, &PriceBasis::EventTime {});
            assert_eq!(atoms(&outcome), expected);
            assert_eq!(event.usage.validated_total().unwrap(), Some(input + 17));
        }
    }
}
#[test]
fn unknown_modes_missing_input_and_partial_consumption_never_choose_a_reference() {
    let catalog = catalog();
    let o = observation(272001, Some(100), Some(25));
    let event = event(o.last.unwrap());
    assert!(matches!(
        catalog.select_request_reference(&event, None, n(4), time(2000)),
        Err(RequestPriceSelectionError::MissingInputEvidence)
    ));
    assert!(matches!(
        catalog.select_request_reference(&event, Some(evidence(&o, None)), n(4), time(2000)),
        Err(RequestPriceSelectionError::MissingActualTier)
    ));
    for usage in [
        UsageVector {
            input_total: Some(20),
            cached_input: Some(0),
            cache_write_input: Some(0),
            output_total: Some(5),
            reasoning_output: Some(0),
            reported_total: Some(25),
        },
        UsageVector {
            reported_total: None,
            ..event.usage
        },
        UsageVector {
            cached_input: None,
            ..event.usage
        },
    ] {
        let partial = PricingEvent { usage, ..event };
        assert!(matches!(
            catalog.select_request_reference(
                &partial,
                Some(evidence(&o, Some(OfflinePriceTier::Standard))),
                n(4),
                time(2000)
            ),
            Err(RequestPriceSelectionError::DifferentConsumption)
        ));
    }
    let mut invalid = o.clone();
    invalid.physical_position.file_generation_id = "different".into();
    assert!(matches!(
        catalog.select_request_reference(
            &event,
            Some(evidence(&invalid, Some(OfflinePriceTier::Standard))),
            n(4),
            time(2000)
        ),
        Err(RequestPriceSelectionError::MissingInputEvidence)
    ));
}
#[test]
fn unsupported_combinations_never_fall_back_to_a_different_mode_or_band() {
    let mut catalog = catalog();
    let o = observation(272001, Some(100), Some(25));
    let event = event(o.last.unwrap());
    catalog
        .entries
        .retain(|e| !(e.tier == OfflinePriceTier::Fast && e.context == OfflineContextBand::Long));
    assert!(matches!(
        catalog.select_request_reference(
            &event,
            Some(evidence(&o, Some(OfflinePriceTier::Fast))),
            n(4),
            time(2000)
        ),
        Err(RequestPriceSelectionError::MissingQuote)
    ));
    let other = PricingEvent {
        provider: Some("synthetic-other"),
        ..event
    };
    assert!(matches!(
        catalog.select_request_reference(
            &other,
            Some(evidence(&o, Some(OfflinePriceTier::Standard))),
            n(4),
            time(2000)
        ),
        Err(RequestPriceSelectionError::UnsupportedProvider)
    ));
    let unknown = PricingEvent {
        model: None,
        ..event
    };
    assert!(matches!(
        catalog.select_request_reference(
            &unknown,
            Some(evidence(&o, Some(OfflinePriceTier::Standard))),
            n(4),
            time(2000)
        ),
        Err(RequestPriceSelectionError::UnknownModel)
    ));
    let missing = PricingEvent {
        model: Some("synthetic-absent"),
        ..event
    };
    assert!(matches!(
        catalog.select_request_reference(
            &missing,
            Some(evidence(&o, Some(OfflinePriceTier::Standard))),
            n(4),
            time(2000)
        ),
        Err(RequestPriceSelectionError::MissingQuote)
    ));
    catalog.entries.push(catalog.entries[0].clone());
    assert!(matches!(
        catalog.select_request_reference(
            &event,
            Some(evidence(&o, Some(OfflinePriceTier::Standard))),
            n(4),
            time(2000)
        ),
        Err(RequestPriceSelectionError::InvalidCatalog)
    ));
}
#[test]
fn mixed_requests_sum_their_own_bands_and_keep_catalog_and_time_identity() {
    let catalog = catalog();
    let mut sums = PricingAccumulator::new(PriceBasis::EventTime {});
    // Two individually short requests exceed the threshold when summed. The third is long.
    let mut expected = 0;
    for input in [200000, 200000, 272001] {
        let o = observation(input, Some(0), Some(0));
        let event = event(o.last.unwrap());
        let selected = catalog
            .select_request_reference(
                &event,
                Some(evidence(&o, Some(OfflinePriceTier::Standard))),
                n(4),
                time(2000),
            )
            .unwrap();
        let rates = if input <= 272000 { [1, 5] } else { [7, 17] };
        expected += (i128::from(input) * rates[0] + 17 * rates[1]) * 1_000_000_000;
        sums.push(
            input + 17,
            estimate(selected.rule, &event, &PriceBasis::EventTime {}),
        )
        .unwrap();
    }
    assert_eq!(
        sums.summary(false).unwrap().currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap(),
        &token_pulse_core::numeric::DecimalMoney::from_atoms(expected).unwrap()
    );
    let o = observation(272001, Some(0), Some(0));
    let old_event = PricingEvent {
        occurred_at_ms: time(999),
        ..event(o.last.unwrap())
    };
    let selected = catalog
        .select_request_reference(
            &old_event,
            Some(evidence(&o, Some(OfflinePriceTier::Standard))),
            n(4),
            time(2000),
        )
        .unwrap();
    let old_id = selected.rule.rule_id.clone();
    assert!(matches!(
        estimate(selected.rule.clone(), &old_event, &PriceBasis::EventTime {}),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::MissingRule
        }
    ));
    assert_eq!(
        atoms(&estimate(
            selected.rule,
            &old_event,
            &PriceBasis::SpecifiedTime {
                specified_at_ms: time(1000)
            }
        )),
        (272001 * 7 + 17 * 17) * 1_000_000_000
    );
    let mut later = catalog.clone();
    later.catalog_id = "openai-text-synthetic-later".into();
    later
        .entries
        .iter_mut()
        .for_each(|e| e.input_per_million = "99".into());
    let new = later
        .select_request_reference(
            &old_event,
            Some(evidence(&o, Some(OfflinePriceTier::Standard))),
            n(4),
            time(2000),
        )
        .unwrap();
    assert_ne!(new.rule.rule_id, old_id);
}
#[test]
fn conditional_reference_still_requires_write_quantities_and_preserves_flat_compatibility() {
    let catalog = OfflinePriceCatalog::bundled().unwrap();
    let o = observation(272001, Some(100), None);
    let event = PricingEvent {
        model: Some("gpt-6.1-sol"),
        occurred_at_ms: catalog.verified_at_ms,
        ..event(o.last.unwrap())
    };
    let selected = catalog
        .select_request_reference(
            &event,
            Some(evidence(&o, Some(OfflinePriceTier::Standard))),
            n(4),
            catalog.verified_at_ms,
        )
        .unwrap();
    assert_eq!(selected.rule.input_rate_atoms.value(), 4_000_000_000);
    assert_eq!(selected.rule.output_rate_atoms.value(), 15_000_000_000);
    assert!(matches!(
        estimate(selected.rule, &event, &PriceBasis::EventTime {}),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::InsufficientUsage
        }
    ));
    assert_eq!(
        catalog
            .flat_standard_rules(n(4), catalog.verified_at_ms)
            .unwrap()
            .len(),
        37
    );
}

#[test]
fn catalog_facts_explain_unpriced_conditions_without_overriding_custom_rules_or_aliases() {
    let reference = catalog();
    let o = observation(272001, Some(100), Some(25));
    let event = event(o.last.unwrap());
    let alias = ModelAlias {
        alias_id: "alias".into(),
        provider: "openai".into(),
        alias: "synthetic-alias".into(),
        canonical_model: "synthetic-conditional".into(),
        introduced_revision: n(4),
        retired_revision: None,
    };
    let prices = PriceCatalog::new(vec![], vec![alias.clone()], n(4))
        .unwrap()
        .with_offline_reference(&reference)
        .unwrap();
    for model in ["synthetic-conditional", "synthetic-alias"] {
        let known = PricingEvent {
            model: Some(model),
            ..event
        };
        assert!(matches!(
            prices.estimate(&known, &PriceBasis::EventTime {}),
            PriceOutcome::Priced { .. }
        ));
    }
    let earlier = PricingEvent {
        occurred_at_ms: time(999),
        ..event
    };
    assert!(matches!(
        prices.estimate(&earlier, &PriceBasis::EventTime {}),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::MissingRule
        }
    ));
    assert!(matches!(
        prices.estimate(
            &earlier,
            &PriceBasis::SpecifiedTime {
                specified_at_ms: time(1000)
            }
        ),
        PriceOutcome::Priced { .. }
    ));
    let absent = PricingEvent {
        model: Some("synthetic-absent"),
        ..event
    };
    assert!(matches!(
        prices.estimate(&absent, &PriceBasis::EventTime {}),
        PriceOutcome::Unpriced {
            reason: UnpricedCode::MissingRule
        }
    ));
    let mut custom = reference
        .select_request_reference(
            &event,
            Some(evidence(&o, Some(OfflinePriceTier::Standard))),
            n(4),
            time(2000),
        )
        .unwrap()
        .rule;
    custom.rule_id = "custom-reference-estimate".into();
    custom.origin = PriceOrigin::Custom;
    let prices = PriceCatalog::new(vec![custom], vec![alias], n(4))
        .unwrap()
        .with_offline_reference(&reference)
        .unwrap();
    let aliased = PricingEvent {
        model: Some("synthetic-alias"),
        ..event
    };
    assert!(matches!(
        prices.estimate(&aliased, &PriceBasis::EventTime {}),
        PriceOutcome::Priced { .. }
    ));
}
