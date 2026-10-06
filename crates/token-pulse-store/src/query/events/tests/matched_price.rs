//! Public metadata follows fixed snapshots and exposes no internal request identity.
use super::*;
use token_pulse_core::{
    domain::NormalizedObservation,
    pricing::{ModelAliasDraft, ModelAliasMutation, PriceMatchBasis, UnpricedCode, offline::*},
    privacy::{DisplayPolicyStamp, PrivacyState, PrivateResponse},
};

fn known_batch(model: &str, write: Option<i64>, provider: &str) -> crate::batch::WriteBatch {
    let mut batch = fixture();
    batch.events[0].model = Some(model.into());
    batch.events[0].usage.cache_write_input = write;
    batch.streams[0].baseline.cache_write_input = write;
    let NormalizedObservation::Usage(observed) = &mut batch.observations[0].record else {
        panic!()
    };
    observed.effective_metadata.model = Some(model.into());
    observed.effective_metadata.provider = Some(provider.into());
    observed.last.as_mut().unwrap().cache_write_input = write;
    observed.cumulative.as_mut().unwrap().cache_write_input = write;
    batch
}

#[test]
fn alias_source_rule_match_keeps_original_price_lease_after_retirement_and_cache_build() {
    let (_dir, db) = setup();
    install(&db);
    db.mutate_model_alias_snapshot(
        ModelAliasMutation::Create {
            draft: ModelAliasDraft {
                provider: "P".into(),
                alias: "synthetic-alias".into(),
                canonical_model: "M".into(),
            },
        },
        1,
        2,
    )
    .unwrap();
    let rule = PriceRuleDraft {
        provider: "P".into(),
        model_exact: "M".into(),
        source_id: Some("source".into()),
        currency: "USD".into(),
        effective_from_ms: EpochMs::new(0).unwrap(),
        effective_to_ms: None,
        priority: 1,
        input_rate_atoms: DecimalInt::parse("2000000000").unwrap(),
        cached_rate_atoms: Some(DecimalInt::parse("2000000000").unwrap()),
        cache_write_rate_atoms: None,
        output_rate_atoms: DecimalInt::parse("2000000000").unwrap(),
        origin_reference: Some("synthetic only".into()),
    };
    db.mutate_price_rule(
        PriceRuleMutation::Create {
            draft: rule.clone(),
        },
        2,
        3,
    )
    .unwrap();
    db.commit(known_batch("synthetic-alias", None, "P"))
        .unwrap();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 4)
        .unwrap();
    let current = fetch(&db, &request_for_test());
    let matched = current.events[0].matched_price.as_ref().unwrap();
    assert_eq!(matched.model_exact, "M");
    assert_eq!(matched.introduced_revision.as_str(), "3");
    assert!(matches!(
        matched.basis,
        PriceMatchBasis::CustomRule {
            source_specific: true
        }
    ));
    assert_eq!(current.events[0].model.as_deref(), Some("synthetic-alias"));
    extra(&db, "newer", 2000, 0, (None, None), None, None);
    let mut req = request(UsageEventSort::TimeDesc, 1);
    let first = fetch(&db, &req);
    req.cursor = first.next_cursor;
    db.mutate_price_rule(
        PriceRuleMutation::Retire {
            rule_id: matched.rule_id.clone(),
        },
        3,
        5,
    )
    .unwrap();
    let old = fetch(&db, &req);
    assert_eq!(old.meta.price_revision.as_str(), "3");
    assert_eq!(
        serde_json::to_value(old.events[0].matched_price.as_ref().unwrap()).unwrap(),
        serde_json::to_value(matched).unwrap()
    );
    let fresh = fetch(&db, &request_for_test());
    let matched = fresh.events[1].matched_price.as_ref().unwrap();
    assert_eq!(fresh.meta.price_revision.as_str(), "4");
    assert_eq!(matched.introduced_revision.as_str(), "1");
    assert!(matches!(
        matched.basis,
        PriceMatchBasis::CustomRule {
            source_specific: false
        }
    ));
    // No rule, request, provider or path is fabricated when the model is unknown.
    assert!(fresh.events[0].matched_price.is_none());
    assert!(
        !serde_json::to_string(matched)
            .unwrap()
            .contains("source_id")
    );
}

#[test]
fn selected_custom_rule_survives_insufficient_usage_and_late_privacy_hides_metadata() {
    let (_dir, db) = setup();
    install(&db);
    db.commit(known_batch("M", Some(20), "P")).unwrap();
    for cached in [false, true] {
        if cached {
            db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 2000)
                .unwrap();
        }
        let page = fetch(&db, &request_for_test());
        assert!(matches!(
            page.events[0].price,
            PriceOutcome::Unpriced {
                reason: UnpricedCode::InsufficientUsage
            }
        ));
        assert!(matches!(
            page.events[0].matched_price.as_ref().unwrap().basis,
            PriceMatchBasis::CustomRule {
                source_specific: false
            }
        ));
        let policy = PrivacyState::new(DisplayPolicyStamp {
            settings_revision: DecimalInt::parse("1").unwrap(),
            privacy: false,
        });
        let response = PrivateResponse::new("matched-proof".into(), page.clone(), policy.clone());
        policy
            .publish(DisplayPolicyStamp {
                settings_revision: DecimalInt::parse("2").unwrap(),
                privacy: true,
            })
            .unwrap();
        let encoded = serde_json::to_value(&response).unwrap();
        assert!(encoded["data"]["events"][0]["matched_price"].is_null());
        assert_eq!(encoded["data"]["summary"]["total_tokens"], "110");
        assert!(page.events[0].matched_price.is_some());
    }
}

fn flat_catalog(id: &str, at: i64, input: &str) -> OfflinePriceCatalog {
    OfflinePriceCatalog {
        format_version: 1,
        catalog_id: id.into(),
        verified_at_ms: EpochMs::new(at).unwrap(),
        provider: "openai".into(),
        currency: "USD".into(),
        short_context_max_input: 272000,
        reference_basis: OfflineReferenceBasis::GlobalApiReference,
        entries: vec![OfflinePriceEntry {
            model_exact: "synthetic-flat".into(),
            tier: OfflinePriceTier::Standard,
            context: OfflineContextBand::All,
            input_per_million: input.into(),
            cached_per_million: Some("2".into()),
            cache_write_per_million: None,
            output_per_million: "3".into(),
            reference: "https://developers.openai.com/api/docs/pricing".into(),
        }],
    }
}
#[test]
fn historical_flat_reference_is_labelled_as_standard_assumption_without_an_actual_mode() {
    let (_dir, db) = setup();
    db.install_offline_price_catalog(flat_catalog("openai-text-old-match", 500, "1"), 500)
        .unwrap();
    db.commit(known_batch("synthetic-flat", None, "openai"))
        .unwrap();
    db.install_offline_price_catalog(flat_catalog("openai-text-new-match", 1500, "9"), 1500)
        .unwrap();
    for cached in [false, true] {
        if cached {
            db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 2000)
                .unwrap();
        }
        let page = fetch(&db, &request_for_test());
        let matched = page.events[0].matched_price.as_ref().unwrap();
        assert!(
            matches!(&matched.basis,PriceMatchBasis::OfflineStandardReference {catalog_id,..} if catalog_id=="openai-text-old-match")
        );
        let encoded = serde_json::to_string(matched).unwrap();
        assert!(!encoded.contains("actual_tier"));
        let PriceOutcome::Priced { cost_atoms, .. } = &page.events[0].price else {
            panic!("flat reference unexpectedly unpriced")
        };
        assert_eq!(cost_atoms.as_str(), "190000000000");
    }
}
