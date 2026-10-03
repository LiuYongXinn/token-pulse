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
fn configured_engine_requires_conditions_and_respects_historical_basis() {
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
    for proof in [
        None,
        Some(evidence(&o, None)),
        Some(evidence(&o, Some(OfflinePriceTier::Fast))),
    ] {
        assert!(matches!(
            engine.estimate_with_request(&original, &PriceBasis::EventTime {}, proof),
            PriceOutcome::Unpriced {
                reason: UnpricedCode::IncompletePricingConditions
            }
        ));
    }
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

#[test]
fn confirmed_modes_replace_only_attached_flat_references_and_never_fall_back() {
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
            PriceOutcome::Unpriced {
                reason: UnpricedCode::IncompletePricingConditions
            }
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
        PriceOutcome::Unpriced {
            reason: UnpricedCode::IncompletePricingConditions
        }
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
