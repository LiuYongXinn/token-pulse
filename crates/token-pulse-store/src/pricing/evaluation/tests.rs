//! Synthetic mode evidence only; no real logs, mode override or account reads.
use super::*;
use crate::batch::tests::{fixture, setup};
use token_pulse_core::{
    domain::{
        NormalizedObservation, PhysicalPosition, RequestUsageEvidence, UsageObservation,
        UsageVector,
    },
    pricing::{UnpricedCode, offline::*},
};

fn catalog(id: &str, at: i64, factor: i64) -> OfflinePriceCatalog {
    let mut catalog = OfflinePriceCatalog {
        format_version: 1,
        catalog_id: id.into(),
        verified_at_ms: EpochMs::new(at).unwrap(),
        provider: "openai".into(),
        currency: "USD".into(),
        short_context_max_input: 272000,
        reference_basis: OfflineReferenceBasis::GlobalApiReference,
        entries: vec![],
    };
    for (tier, mode) in [
        (OfflinePriceTier::Standard, 1),
        (OfflinePriceTier::Fast, 2),
        (OfflinePriceTier::Batch, 3),
        (OfflinePriceTier::Flex, 4),
        (OfflinePriceTier::Ultrafast, 5),
    ] {
        for (context, rates) in [
            (OfflineContextBand::Short, [1, 2, 3, 5]),
            (OfflineContextBand::Long, [7, 11, 13, 17]),
        ] {
            let rates = rates.map(|rate| (rate * mode * factor).to_string());
            catalog.entries.push(OfflinePriceEntry {
                model_exact: "synthetic-conditional".into(),
                tier,
                context,
                input_per_million: rates[0].clone(),
                cached_per_million: Some(rates[1].clone()),
                cache_write_per_million: Some(rates[2].clone()),
                output_per_million: rates[3].clone(),
                reference: "https://developers.openai.com/api/docs/pricing".into(),
            });
        }
    }
    catalog.validate().unwrap();
    catalog
}
fn observation(input: i64, write: Option<i64>) -> UsageObservation {
    let NormalizedObservation::Usage(mut observed) = fixture().observations.remove(0).record else {
        panic!()
    };
    let usage = UsageVector {
        input_total: Some(input),
        cached_input: Some(60),
        cache_write_input: write,
        output_total: Some(10),
        reasoning_output: Some(2),
        reported_total: Some(input + 10),
    };
    observed.physical_position.byte_offset = 10;
    observed.last = Some(usage);
    observed.cumulative = Some(usage);
    observed.effective_metadata.provider = Some("openai".into());
    observed.effective_metadata.turn_id = Some("synthetic-turn".into());
    observed.request_usage = Some(Box::new(RequestUsageEvidence {
        response_id: "synthetic-response".into(),
        turn_id: "synthetic-turn".into(),
        usage,
        thread_usage: usage,
        physical_position: PhysicalPosition {
            file_generation_id: "generation".into(),
            byte_offset: 0,
            byte_end: 10,
        },
    }));
    observed
}
fn event(observed: &UsageObservation, at: i64) -> PricingEvent<'static> {
    PricingEvent {
        provider: Some("openai"),
        model: Some("synthetic-conditional"),
        source_ids: &[],
        occurred_at_ms: EpochMs::new(at).unwrap(),
        usage: observed.last.unwrap(),
    }
}
fn proof(observed: &UsageObservation, tier: Option<OfflinePriceTier>) -> RequestPriceEvidence<'_> {
    RequestPriceEvidence {
        response: observed.request_usage.as_ref().unwrap(),
        context: observed.into(),
        actual_tier: tier,
    }
}
fn atoms(outcome: &PriceOutcome) -> i128 {
    let PriceOutcome::Priced { cost_atoms, .. } = outcome else {
        panic!("{outcome:?}")
    };
    cost_atoms.value()
}
fn selected(evaluation: &PriceEvaluation) -> &SelectedRequestReference {
    let Some(SelectedPrice::Request(reference)) = &evaluation.selection else {
        panic!("missing request selection")
    };
    reference
}

#[test]
fn synthetic_modes_and_bands_keep_exact_verified_rule_identity_and_four_category_amount() {
    let (_dir, db) = setup();
    db.install_offline_price_catalog(catalog("openai-text-selection-fixture", 1000, 1), 1000)
        .unwrap();
    for (tier, factor) in [
        (OfflinePriceTier::Standard, 1),
        (OfflinePriceTier::Fast, 2),
        (OfflinePriceTier::Batch, 3),
        (OfflinePriceTier::Flex, 4),
        (OfflinePriceTier::Ultrafast, 5),
    ] {
        for (input, context, rates) in [
            (272000, OfflineContextBand::Short, [1, 2, 3, 5]),
            (272001, OfflineContextBand::Long, [7, 11, 13, 17]),
        ] {
            let observed = observation(input, Some(20));
            db.snapshot(|tx, r| {
                let engine = catalog_at(tx, r.price)?;
                let evaluation = evaluate(
                    tx,
                    &engine,
                    &event(&observed, 2000),
                    &PriceBasis::EventTime {},
                    Some(proof(&observed, Some(tier))),
                )?;
                let reference = selected(&evaluation);
                assert_eq!(reference.catalog_id, "openai-text-selection-fixture");
                assert_eq!(reference.tier, tier);
                assert_eq!(reference.context, context);
                assert_eq!(reference.rule.introduced_revision.as_str(), "1");
                // Independent category oracle: input excludes reads + writes; reasoning
                // stays inside the whole output, and the whole request shares one band.
                let expected = i128::from(
                    ((input - 80) * rates[0] + 60 * rates[1] + 20 * rates[2] + 10 * rates[3])
                        * factor,
                ) * 1_000_000_000;
                assert_eq!(atoms(&evaluation.outcome), expected);
                let matched = evaluation.matched_price(&evaluation.outcome).unwrap();
                assert!(matches!(&matched.basis,
                    token_pulse_core::pricing::PriceMatchBasis::OfflineRequestReference {
                        catalog_id, actual_tier, context: band, ..
                    } if catalog_id == "openai-text-selection-fixture" && *actual_tier == tier && *band == context));
                let encoded = serde_json::to_string(&matched)?;
                assert!(!encoded.contains("synthetic-response"));
                assert!(!encoded.contains("physical_position"));
                assert!(evaluation.matched_price(&PriceOutcome::Redacted {}).is_none());
                Ok(())
            })
            .unwrap();
        }
    }
}

#[test]
fn missing_modes_and_unknown_quantities_have_distinct_selection_results() {
    let (_dir, db) = setup();
    db.install_offline_price_catalog(catalog("openai-text-selection-fixture", 1000, 1), 1000)
        .unwrap();
    db.snapshot(|tx, r| {
        let engine = catalog_at(tx, r.price)?;
        let observed = observation(100, None);
        let unknown = evaluate(
            tx,
            &engine,
            &event(&observed, 2000),
            &PriceBasis::EventTime {},
            Some(proof(&observed, None)),
        )?;
        assert_eq!(atoms(&unknown.outcome), 210_000_000_000);
        assert!(matches!(
            unknown.matched_price(&unknown.outcome).unwrap().basis,
            token_pulse_core::pricing::PriceMatchBasis::OfflineAssumedReference {
                context_assumed: false,
                cache_write_assumed_zero: true,
                ..
            }
        ));
        let known = evaluate(
            tx,
            &engine,
            &event(&observed, 2000),
            &PriceBasis::EventTime {},
            Some(proof(&observed, Some(OfflinePriceTier::Standard))),
        )?;
        assert_eq!(selected(&known).tier, OfflinePriceTier::Standard);
        assert!(matches!(
            known.outcome,
            PriceOutcome::Unpriced {
                reason: UnpricedCode::InsufficientUsage
            }
        ));
        let incomplete = PricingEvent {
            usage: UsageVector {
                reported_total: None,
                ..observed.last.unwrap()
            },
            ..event(&observed, 2000)
        };
        let incomplete = evaluate(
            tx,
            &engine,
            &incomplete,
            &PriceBasis::EventTime {},
            Some(proof(&observed, Some(OfflinePriceTier::Standard))),
        )?;
        assert!(incomplete.selection.is_none());
        assert!(matches!(
            incomplete.outcome,
            PriceOutcome::Unpriced {
                reason: UnpricedCode::IncompletePricingConditions
            }
        ));
        Ok(())
    })
    .unwrap();
}

#[test]
fn old_snapshot_and_valuation_time_verify_the_original_publication_not_latest_alias_revision() {
    let (_dir, db) = setup();
    db.install_offline_price_catalog(catalog("openai-text-old-fixture", 1000, 1), 1000)
        .unwrap();
    let observed = observation(100, Some(20));
    db.snapshot(|old, r| {
        db.install_offline_price_catalog(catalog("openai-text-new-fixture", 2000, 10), 2000)?;
        let engine = catalog_at(old, r.price)?;
        let evaluation = evaluate(
            old,
            &engine,
            &event(&observed, 3000),
            &PriceBasis::EventTime {},
            Some(proof(&observed, Some(OfflinePriceTier::Fast))),
        )?;
        assert_eq!(selected(&evaluation).catalog_id, "openai-text-old-fixture");
        assert_eq!(atoms(&evaluation.outcome), 500_000_000_000);
        Ok(())
    })
    .unwrap();
    db.mutate_model_alias_snapshot(
        ModelAliasMutation::Create {
            draft: token_pulse_core::pricing::ModelAliasDraft {
                provider: "openai".into(),
                alias: "synthetic-alias".into(),
                canonical_model: "synthetic-conditional".into(),
            },
        },
        2,
        3000,
    )
    .unwrap();
    db.snapshot(|tx, r| {
        let engine = catalog_at(tx, r.price)?;
        for (basis, id, amount) in [
            (
                PriceBasis::EventTime {},
                "openai-text-new-fixture",
                5_000_000_000_000,
            ),
            (
                PriceBasis::SpecifiedTime {
                    specified_at_ms: EpochMs::new(1500)?,
                },
                "openai-text-old-fixture",
                500_000_000_000,
            ),
        ] {
            let event = PricingEvent {
                model: Some("synthetic-alias"),
                ..event(&observed, 3000)
            };
            let evaluation = evaluate(
                tx,
                &engine,
                &event,
                &basis,
                Some(proof(&observed, Some(OfflinePriceTier::Fast))),
            )?;
            assert_eq!(selected(&evaluation).catalog_id, id);
            assert_eq!(atoms(&evaluation.outcome), amount);
            assert_eq!(
                selected(&evaluation).rule.introduced_revision.as_str(),
                if id.contains("old") { "1" } else { "2" }
            );
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn stored_parent_and_conditional_index_must_match_the_fixed_selected_quote() {
    for fault in 0..5 {
        let (_dir, db) = setup();
        db.install_offline_price_catalog(catalog("openai-text-selection-fixture", 1000, 1), 1000)
            .unwrap();
        let observed = observation(100, Some(20));
        db.snapshot(|old,r| {
            let engine=catalog_at(old,r.price)?;
            // Retained fixed snapshot remains valid while a synthetic writer damages
            // one row in a later snapshot. Never modify a real user's database.
            db.write(move|conn| {
                let id="offline/openai-text-selection-fixture/synthetic-conditional/fast/short";
                match fault {
                    0=>{conn.execute("UPDATE price_rules SET input_rate_atoms='9' WHERE rule_id=?1",[id])?;},
                    1=>{conn.execute("UPDATE price_rules SET request_conditional=0 WHERE rule_id=?1",[id])?;},
                    2=>{conn.execute("UPDATE conditional_price_rules SET context_band='all' WHERE rule_id=?1",[id])?;},
                    3=>{conn.execute("DELETE FROM conditional_price_rules WHERE rule_id=?1",[id])?;},
                    _=>{conn.execute("UPDATE conditional_price_rules SET model_exact='other' WHERE rule_id=?1",[id])?;},
                } Ok(())
            })?;
            assert_eq!(atoms(&evaluate(old,&engine,&event(&observed,2000),&PriceBasis::EventTime {},Some(proof(&observed,Some(OfflinePriceTier::Fast))))?.outcome),500_000_000_000);
            db.snapshot(|tx,_| {
                let err=match evaluate(tx,&engine,&event(&observed,2000),&PriceBasis::EventTime {},Some(proof(&observed,Some(OfflinePriceTier::Fast)))) {Ok(_)=>panic!("inconsistent row accepted"),Err(err)=>err};
                assert_eq!(err.code,ErrorCode::DbCorrupt);Ok(())
            })?;
            Ok(())
        }).unwrap();
    }
}

#[test]
fn production_query_and_background_do_not_turn_request_or_configuration_into_actual_mode() {
    let (_dir, db) = setup();
    db.install_offline_price_catalog(catalog("openai-text-selection-fixture", 1000, 1), 1000)
        .unwrap();
    let mut batch = fixture();
    let mut observed = observation(100, Some(20));
    observed.event_time_ms = Some(2000);
    observed.effective_metadata.model = Some("synthetic-conditional".into());
    batch.observations[0].record = NormalizedObservation::Usage(observed.clone());
    batch.events[0].usage = observed.last.unwrap();
    batch.events[0].model = Some("synthetic-conditional".into());
    batch.events[0].occurred_at_ms = 2000;
    batch.events[0].turn_id = Some("synthetic-turn".into());
    batch.streams[0].baseline = observed.last.unwrap();
    db.commit(batch).unwrap();
    db.write(|conn| {conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.effective_metadata.service_tier','fast') WHERE observation_id='observation'",[])?;Ok(())}).unwrap();
    let basis = PriceBasis::EventTime {};
    let mut filter = crate::query::tests::filter();
    filter.range.end_ms = EpochMs::new(3000).unwrap();
    let before = db.usage_totals(&filter).unwrap().total_tokens;
    for cached in [false, true] {
        if cached {
            db.build_event_valuation("ledger", &basis, 4000).unwrap();
        }
        let summary = db.pricing_summary(&filter, &basis).unwrap();
        assert_eq!(
            summary.currencies[0]
                .estimated_cost
                .as_ref()
                .unwrap()
                .as_str(),
            "0.000250000000000"
        );
        assert!(summary.reasons.is_empty());
        assert_eq!(summary.priced_total_tokens, before);
    }
}
