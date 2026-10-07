use super::*;
use crate::batch::tests::{fixture, setup};
use token_pulse_core::{
    domain::UsageVector,
    pricing::{PriceOutcome, PriceRuleDraft, PricingEvent, UnpricedCode},
    protocol::PriceBasis,
};

fn quote(
    catalog: PriceCatalog,
    model: &str,
    at: EpochMs,
    specified: Option<EpochMs>,
) -> PriceOutcome {
    catalog.estimate(
        &PricingEvent {
            provider: Some("openai"),
            model: Some(model),
            source_ids: &[],
            occurred_at_ms: at,
            usage: UsageVector {
                input_total: Some(100),
                cache_write_input: None,
                cached_input: Some(60),
                output_total: Some(10),
                reasoning_output: Some(2),
                reported_total: Some(110),
            },
        },
        &specified
            .map(|specified_at_ms| PriceBasis::SpecifiedTime { specified_at_ms })
            .unwrap_or(PriceBasis::EventTime {}),
    )
}
fn atoms(outcome: PriceOutcome) -> String {
    match outcome {
        PriceOutcome::Priced { cost_atoms, .. } => cost_atoms.as_str().into(),
        other => panic!("{other:?}"),
    }
}
#[test]
fn install_reopen_and_same_content_are_idempotent_without_changing_consumption() {
    let (directory, db) = setup();
    db.commit(fixture()).unwrap();
    assert!(db.offline_price_catalog_at(None).unwrap().catalog.is_none());
    let catalog = OfflinePriceCatalog::bundled().unwrap();
    let at = catalog.verified_at_ms;
    assert_eq!(
        db.install_offline_price_catalog(catalog.clone(), at.value())
            .unwrap(),
        1
    );
    assert_eq!(
        db.install_offline_price_catalog(catalog.clone(), at.value() + 10)
            .unwrap(),
        1
    );
    assert_eq!(db.price_rules().unwrap().rules.len(), 37);
    assert_eq!(db.offline_price_catalog_at(Some(0)).unwrap().catalog, None);
    assert_eq!(
        db.offline_price_catalog_at(None).unwrap().catalog,
        Some(catalog.clone())
    );
    db.snapshot(|tx, rev| {
        assert_eq!(rev.data, 1);
        // Independently calculated: (40*1.75 + 60*0.175 + 10*14)/1M USD.
        assert_eq!(
            atoms(quote(catalog_at(tx, 1)?, "gpt-5.3-codex", at, None)),
            "220500000000"
        );
        assert!(matches!(
            quote(
                catalog_at(tx, 1)?,
                "gpt-5.3-codex",
                EpochMs::new(at.value() - 1)?,
                None
            ),
            PriceOutcome::Unpriced {
                reason: UnpricedCode::MissingRule
            }
        ));
        assert_eq!(
            atoms(quote(
                catalog_at(tx, 1)?,
                "gpt-5.3-codex",
                EpochMs::new(1000)?,
                Some(at)
            )),
            "220500000000"
        );
        assert_eq!(
            atoms(quote(catalog_at(tx, 1)?, "gpt-6.1-sol", at, None)),
            "186000000000"
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.usage_totals(&crate::query::tests::filter())
            .unwrap()
            .total_tokens
            .as_str(),
        "110"
    );
    drop(db);
    let reopened = Database::open(directory.path()).unwrap();
    assert_eq!(
        reopened.offline_price_catalog_at(None).unwrap().catalog,
        Some(catalog)
    );
}
#[test]
fn update_publishes_closed_history_and_preserves_captured_revisions_and_custom_rules() {
    let (_directory, db) = setup();
    let catalog = OfflinePriceCatalog::bundled().unwrap();
    let at = catalog.verified_at_ms;
    db.install_offline_price_catalog(catalog.clone(), at.value())
        .unwrap();
    let mut newer = catalog;
    newer.catalog_id = "openai-text-next-fixture".into();
    newer.verified_at_ms = EpochMs::new(at.value() + 1000).unwrap();
    for entry in newer.entries.iter_mut().filter(|e| {
        e.model_exact == "gpt-5.3-codex"
            && e.tier == token_pulse_core::pricing::offline::OfflinePriceTier::Standard
    }) {
        entry.input_per_million = "2.00".into();
    }
    db.snapshot(|tx, rev| {
        assert_eq!(rev.price, 1);
        assert_eq!(
            db.install_offline_price_catalog(newer.clone(), at.value() + 1000)?,
            2
        );
        assert_eq!(
            atoms(quote(
                catalog_at(tx, rev.price)?,
                "gpt-5.3-codex",
                newer.verified_at_ms,
                None
            )),
            "220500000000"
        );
        assert_eq!(
            snapshot_at(tx, rev.price)?.catalog.unwrap().verified_at_ms,
            at
        );
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(
            atoms(quote(catalog_at(tx, 2)?, "gpt-5.3-codex", at, None)),
            "220500000000"
        );
        assert_eq!(
            atoms(quote(
                catalog_at(tx, 2)?,
                "gpt-5.3-codex",
                newer.verified_at_ms,
                None
            )),
            "230500000000"
        );
        assert_eq!(
            atoms(quote(
                catalog_at(tx, 1)?,
                "gpt-5.3-codex",
                newer.verified_at_ms,
                None
            )),
            "220500000000"
        );
        Ok(())
    })
    .unwrap();
    let mut custom = super::super::tests::draft();
    custom.provider = "openai".into();
    custom.model_exact = "gpt-5.3-codex".into();
    // Synthetic user override takes precedence regardless of offline publication.
    db.mutate_price_rule(
        PriceRuleMutation::Create { draft: custom },
        2,
        at.value() + 1000,
    )
    .unwrap();
    assert_eq!(
        db.install_offline_price_catalog(newer, at.value() + 2000)
            .unwrap(),
        3
    );
    db.snapshot(|tx, rev| {
        assert_eq!(
            atoms(quote(catalog_at(tx, rev.price)?, "gpt-5.3-codex", at, None)),
            "900"
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn conflicting_identity_downgrade_and_failed_publication_preserve_old_revision() {
    let (_directory, db) = setup();
    let catalog = OfflinePriceCatalog::bundled().unwrap();
    let at = catalog.verified_at_ms.value();
    db.install_offline_price_catalog(catalog.clone(), at)
        .unwrap();
    let mut changed = catalog.clone();
    changed.entries[0].input_per_million = "999".into();
    assert_eq!(
        db.install_offline_price_catalog(changed, at)
            .unwrap_err()
            .code,
        ErrorCode::PriceRuleConflict
    );
    let mut downgrade = catalog.clone();
    downgrade.catalog_id = "openai-text-downgrade-fixture".into();
    downgrade.verified_at_ms = EpochMs::new(at - 1).unwrap();
    assert_eq!(
        db.install_offline_price_catalog(downgrade, at)
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    db.write(|conn| { conn.execute_batch("CREATE TRIGGER offline_publication_failure BEFORE UPDATE OF price_revision ON app_state BEGIN SELECT RAISE(ABORT,'fixture'); END;")?; Ok(()) }).unwrap();
    let mut newer = catalog.clone();
    newer.catalog_id = "openai-text-failed-fixture".into();
    newer.verified_at_ms = EpochMs::new(at + 1).unwrap();
    assert!(db.install_offline_price_catalog(newer, at + 1).is_err());
    assert_eq!(db.price_rules().unwrap().price_revision.as_str(), "1");
    assert_eq!(db.price_rules().unwrap().rules.len(), 37);
    assert_eq!(
        db.offline_price_catalog_at(None).unwrap().catalog,
        Some(catalog)
    );
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM offline_price_catalogs", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM price_rules WHERE retired_revision IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn offline_rows_cannot_be_edited_or_retired_through_user_mutations() {
    let (_directory, db) = setup();
    let catalog = OfflinePriceCatalog::bundled().unwrap();
    let at = catalog.verified_at_ms.value();
    db.install_offline_price_catalog(catalog, at).unwrap();
    let id = db.price_rules().unwrap().rules[0].rule_id.clone();
    let draft: PriceRuleDraft = super::super::tests::draft();
    for mutation in [
        PriceRuleMutation::Replace {
            rule_id: id.clone(),
            draft,
        },
        PriceRuleMutation::Retire { rule_id: id },
    ] {
        assert_eq!(
            db.mutate_price_rule(mutation, 1, at).unwrap_err().code,
            ErrorCode::InvalidQuery
        );
    }
    assert_eq!(db.price_rules().unwrap().price_revision.as_str(), "1");
}
