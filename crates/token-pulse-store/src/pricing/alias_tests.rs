//! Synthetic mappings, prices and independent costs; no market or user data.
use super::tests::{draft, estimate};
use super::*;
use crate::batch::tests::{fixture, setup};
use token_pulse_core::pricing::{ModelAliasDraft, ModelAliasMutation, PriceOutcome, PricingEvent};
use token_pulse_core::{domain::UsageVector, protocol::PriceBasis};

fn alias(name: &str, target: &str) -> ModelAliasDraft {
    ModelAliasDraft {
        provider: "fixture-provider".into(),
        alias: name.into(),
        canonical_model: target.into(),
    }
}
fn unpriced(catalog: PriceCatalog) -> bool {
    matches!(
        catalog.estimate(
            &PricingEvent {
                provider: Some("fixture-provider"),
                model: Some("fixture-model"),
                source_ids: &[],
                occurred_at_ms: EpochMs::new(1000).unwrap(),
                usage: UsageVector {
                    input_total: Some(100),
                    cached_input: Some(60),
                    output_total: Some(10),
                    reasoning_output: Some(2),
                    reported_total: Some(110),
                },
            },
            &PriceBasis::EventTime {}
        ),
        PriceOutcome::Unpriced {
            reason: token_pulse_core::pricing::UnpricedCode::MissingRule
        }
    )
}
#[test]
fn alias_publication_replacement_and_retirement_preserve_old_estimates_and_consumption() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    for (revision, model, input) in [(0, "canonical-one", 10), (1, "canonical-two", 20)] {
        let mut rule = draft();
        rule.model_exact = model.into();
        rule.input_rate_atoms = DecimalInt::from_nonnegative(input).unwrap();
        db.mutate_price_rule(PriceRuleMutation::Create { draft: rule }, revision, 1)
            .unwrap();
    }
    let published = db
        .mutate_model_alias_snapshot(
            ModelAliasMutation::Create {
                draft: alias("fixture-model", "canonical-one"),
            },
            2,
            2,
        )
        .unwrap();
    assert_eq!(published.price_revision.as_str(), "3");
    let id = published.aliases[0].alias_id.clone();
    db.snapshot(|tx, revision| {
        assert_eq!(revision.data, 1);
        assert_eq!(estimate(catalog_at(tx, revision.price)?), "900"); // 40*10 + 60*5 + 10*20
        let next = db
            .mutate_model_alias_snapshot(
                ModelAliasMutation::Replace {
                    alias_id: id.clone(),
                    draft: alias("fixture-model", "canonical-two"),
                },
                3,
                3,
            )
            .unwrap();
        assert_eq!(next.price_revision.as_str(), "4");
        assert_ne!(next.aliases[0].alias_id, id);
        assert_eq!(estimate(catalog_at(tx, revision.price)?), "900");
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, revision| {
        assert_eq!(revision.data, 1);
        assert_eq!(estimate(catalog_at(tx, 3)?), "900");
        assert_eq!(estimate(catalog_at(tx, 4)?), "1300"); // 40*20 + 60*5 + 10*20
        let retired: i64 = tx.query_row(
            "SELECT retired_revision FROM model_aliases WHERE alias_id=?1",
            [id],
            |row| row.get(0),
        )?;
        assert_eq!(retired, 4);
        Ok(())
    })
    .unwrap();
    let id = db.price_rules().unwrap().aliases[0].alias_id.clone();
    let retired = db
        .mutate_model_alias_snapshot(ModelAliasMutation::Retire { alias_id: id }, 4, 4)
        .unwrap();
    assert_eq!(retired.price_revision.as_str(), "5");
    assert!(retired.aliases.is_empty());
    assert_eq!(
        db.price_rules_at(Some(3)).unwrap().aliases[0].canonical_model,
        "canonical-one"
    );
    db.snapshot(|tx, rev| {
        assert_eq!(rev.data, 1);
        assert!(unpriced(catalog_at(tx, rev.price)?));
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
}
#[test]
fn duplicate_chains_cycles_self_mapping_and_stale_edits_are_rejected_without_publication() {
    let (_dir, db) = setup();
    let current = db
        .mutate_model_alias_snapshot(
            ModelAliasMutation::Create {
                draft: alias("a", "canonical"),
            },
            0,
            1,
        )
        .unwrap();
    for draft in [
        alias("a", "different"),
        alias("incoming", "a"),
        alias("canonical", "other"),
        alias("canonical", "a"),
    ] {
        assert_eq!(
            db.mutate_model_alias_snapshot(ModelAliasMutation::Create { draft }, 1, 2)
                .unwrap_err()
                .code,
            ErrorCode::PriceRuleConflict
        );
        assert_eq!(db.price_rules().unwrap().price_revision.as_str(), "1");
    }
    assert_eq!(
        db.mutate_model_alias_snapshot(
            ModelAliasMutation::Create {
                draft: alias("self", "self")
            },
            1,
            2
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.mutate_model_alias_snapshot(
            ModelAliasMutation::Retire {
                alias_id: current.aliases[0].alias_id.clone()
            },
            0,
            2
        )
        .unwrap_err()
        .code,
        ErrorCode::RevisionConflict
    );
    for id in ["offline-alias", "alias-custom-missing"] {
        assert_eq!(
            db.mutate_model_alias_snapshot(
                ModelAliasMutation::Retire {
                    alias_id: id.into()
                },
                1,
                2
            )
            .unwrap_err()
            .code,
            ErrorCode::InvalidQuery
        );
    }
    let mut other_provider = alias("a", "canonical");
    other_provider.provider = "separate-provider".into();
    let next = db
        .mutate_model_alias_snapshot(
            ModelAliasMutation::Create {
                draft: other_provider,
            },
            1,
            2,
        )
        .unwrap();
    assert_eq!(next.aliases.len(), 2);
    assert_eq!(next.price_revision.as_str(), "2");
}
#[test]
fn alias_insert_and_precommit_failures_roll_back_retirement_and_revision() {
    let (_dir, db) = setup();
    let current = db
        .mutate_model_alias_snapshot(
            ModelAliasMutation::Create {
                draft: alias("a", "canonical"),
            },
            0,
            1,
        )
        .unwrap();
    let id = current.aliases[0].alias_id.clone();
    let fault_id = id.clone();
    let error = db
        .write(move |conn| {
            let tx = conn.transaction()?;
            apply_alias(
                &tx,
                ModelAliasMutation::Retire { alias_id: fault_id },
                1,
                EpochMs::new(2)?,
            )?;
            Err::<(), _>(ErrorCode::DbWriteFailed.into())
        })
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::DbWriteFailed);
    db.write(|conn| { conn.execute_batch("CREATE TRIGGER reject_alias BEFORE INSERT ON model_aliases BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?; Ok(()) }).unwrap();
    assert_eq!(
        db.mutate_model_alias_snapshot(
            ModelAliasMutation::Replace {
                alias_id: id.clone(),
                draft: alias("a", "other")
            },
            1,
            2
        )
        .unwrap_err()
        .code,
        ErrorCode::DbWriteFailed
    );
    let after = db.price_rules().unwrap();
    assert_eq!(after.price_revision.as_str(), "1");
    assert_eq!(after.aliases[0].alias_id, id);
    assert!(after.aliases[0].retired_revision.is_none());
}
#[test]
fn rule_and_alias_publishers_share_one_compare_and_swap_winner() {
    use std::sync::{Arc, Barrier};
    let (_dir, db) = setup();
    let gate = Arc::new(Barrier::new(3));
    let alias_db = db.clone();
    let alias_gate = gate.clone();
    let one = std::thread::spawn(move || {
        alias_gate.wait();
        alias_db.mutate_model_alias_snapshot(
            ModelAliasMutation::Create {
                draft: alias("a", "canonical"),
            },
            0,
            1,
        )
    });
    let rule_db = db.clone();
    let rule_gate = gate.clone();
    let two = std::thread::spawn(move || {
        rule_gate.wait();
        rule_db.mutate_price_rule_snapshot(PriceRuleMutation::Create { draft: draft() }, 0, 1)
    });
    gate.wait();
    let results = [one.join().unwrap(), two.join().unwrap()];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .find_map(|result| result.as_ref().err())
            .unwrap()
            .code,
        ErrorCode::RevisionConflict
    );
    let after = db.price_rules().unwrap();
    assert_eq!(after.price_revision.as_str(), "1");
    assert_eq!(after.rules.len() + after.aliases.len(), 1);
}
