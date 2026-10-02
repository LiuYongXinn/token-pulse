use super::*;
use crate::batch::tests::{fixture, setup};
use token_pulse_core::{
    domain::UsageVector,
    pricing::{PriceOutcome, PriceRuleDraft, PricingEvent},
    protocol::PriceBasis,
};

fn n(value: i128) -> DecimalInt {
    DecimalInt::from_nonnegative(value).unwrap()
}
pub(super) fn draft() -> PriceRuleDraft {
    // Synthetic atoms only. Production contains no seed from this fixture.
    PriceRuleDraft {
        provider: "fixture-provider".into(),
        model_exact: "fixture-model".into(),
        source_id: None,
        currency: "USD".into(),
        effective_from_ms: EpochMs::new(0).unwrap(),
        effective_to_ms: None,
        priority: 0,
        input_rate_atoms: n(10),
        cached_rate_atoms: Some(n(5)),
        output_rate_atoms: n(20),
        origin_reference: Some("synthetic fixture".into()),
    }
}

#[test]
fn rate_limit_rejects_create_and_replace_without_retiring_or_advancing_revision() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let limit = token_pulse_core::pricing::MAX_RATE_ATOMS;
    for field in 0..3 {
        let mut excessive = draft();
        match field {
            0 => excessive.input_rate_atoms = n(limit + 1),
            1 => excessive.cached_rate_atoms = Some(n(limit + 1)),
            _ => excessive.output_rate_atoms = n(limit + 1),
        }
        assert_eq!(
            db.mutate_price_rule(PriceRuleMutation::Create { draft: excessive }, 0, 1)
                .unwrap_err()
                .code,
            ErrorCode::InvalidQuery
        );
        assert_eq!(db.price_rules().unwrap().price_revision.as_str(), "0");
    }
    let mut valid = draft();
    valid.input_rate_atoms = n(limit);
    db.mutate_price_rule(
        PriceRuleMutation::Create {
            draft: valid.clone(),
        },
        0,
        1,
    )
    .unwrap();
    let original = db.price_rules().unwrap().rules.remove(0);
    valid.input_rate_atoms = n(limit + 1);
    assert_eq!(
        db.mutate_price_rule(
            PriceRuleMutation::Replace {
                rule_id: original.rule_id.clone(),
                draft: valid
            },
            1,
            2
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    let after = db.price_rules().unwrap();
    assert_eq!(after.price_revision.as_str(), "1");
    assert_eq!(after.rules[0].rule_id, original.rule_id);
    assert!(after.rules[0].retired_revision.is_none());
    assert_eq!(
        db.usage_totals(&crate::query::tests::filter())
            .unwrap()
            .total_tokens
            .as_str(),
        "110"
    );
    db.write(move |conn| {
        conn.execute(
            "UPDATE price_rules SET input_rate_atoms=?1",
            [(limit + 1).to_string()],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(db.price_rules().unwrap_err().code, ErrorCode::DbCorrupt);
}
pub(super) fn estimate(catalog: PriceCatalog) -> String {
    let e = PricingEvent {
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
    };
    match catalog.estimate(&e, &PriceBasis::EventTime {}) {
        PriceOutcome::Priced { cost_atoms, .. } => cost_atoms.as_str().into(),
        _ => panic!("expected price"),
    }
}

#[test]
fn replacement_is_immutable_and_new_old_snapshots_keep_matching_price_revisions() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    assert!(db.price_rules().unwrap().rules.is_empty());
    assert_eq!(
        db.mutate_price_rule(PriceRuleMutation::Create { draft: draft() }, 0, 1)
            .unwrap(),
        1
    );
    let old = db.price_rules().unwrap().rules.remove(0);
    db.snapshot(|tx, revision| {
        assert_eq!(revision.data, 1);
        assert_eq!(revision.price, 1);
        assert_eq!(estimate(catalog_at(tx, revision.price)?), "900");
        let mut replacement = draft();
        replacement.input_rate_atoms = n(1000);
        db.mutate_price_rule(
            PriceRuleMutation::Replace {
                rule_id: old.rule_id.clone(),
                draft: replacement,
            },
            1,
            2,
        )
        .unwrap();
        assert_eq!(estimate(catalog_at(tx, revision.price)?), "900");
        assert!(
            rules_at(tx, revision.price)?.rules[0]
                .retired_revision
                .is_none()
        );
        Ok(())
    })
    .unwrap();
    let current = db.price_rules().unwrap();
    assert_eq!(current.price_revision.as_str(), "2");
    assert_ne!(old.rule_id, current.rules[0].rule_id);
    db.snapshot(|tx, revision| {
        assert_eq!(revision.data, 1);
        assert_eq!(estimate(catalog_at(tx, 1)?), "900");
        assert_eq!(estimate(catalog_at(tx, 2)?), "40500");
        assert_eq!(
            tx.query_row(
                "SELECT input_rate_atoms,retired_revision FROM price_rules WHERE rule_id=?1",
                [old.rule_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            )?,
            ("10".into(), 2)
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn overlapping_same_scope_priority_is_rejected_but_adjacent_and_higher_scope_are_allowed() {
    let (_dir, db) = setup();
    let mut first = draft();
    first.effective_to_ms = Some(EpochMs::new(1000).unwrap());
    db.mutate_price_rule(
        PriceRuleMutation::Create {
            draft: first.clone(),
        },
        0,
        1,
    )
    .unwrap();
    let mut adjacent = draft();
    adjacent.effective_from_ms = EpochMs::new(1000).unwrap();
    db.mutate_price_rule(PriceRuleMutation::Create { draft: adjacent }, 1, 2)
        .unwrap();
    let mut overlap = draft();
    overlap.effective_from_ms = EpochMs::new(999).unwrap();
    assert_eq!(
        db.mutate_price_rule(
            PriceRuleMutation::Create {
                draft: overlap.clone()
            },
            2,
            3
        )
        .unwrap_err()
        .code,
        ErrorCode::PriceRuleConflict
    );
    assert_eq!(db.price_rules().unwrap().price_revision.as_str(), "2");
    assert_eq!(db.price_rules().unwrap().rules.len(), 2);
    overlap.priority = 1;
    db.mutate_price_rule(PriceRuleMutation::Create { draft: overlap }, 2, 3)
        .unwrap();
    let mut source = draft();
    source.source_id = Some("source".into());
    db.mutate_price_rule(
        PriceRuleMutation::Create {
            draft: source.clone(),
        },
        3,
        4,
    )
    .unwrap();
    assert_eq!(
        db.mutate_price_rule(PriceRuleMutation::Create { draft: source }, 4, 5)
            .unwrap_err()
            .code,
        ErrorCode::PriceRuleConflict
    );
    assert_eq!(db.price_rules().unwrap().price_revision.as_str(), "4");
}

#[test]
fn replacement_errors_and_pre_commit_failure_rollback_retirement_revision_and_rules() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.mutate_price_rule(PriceRuleMutation::Create { draft: draft() }, 0, 1)
        .unwrap();
    let id = db.price_rules().unwrap().rules[0].rule_id.clone();
    let mut invalid = draft();
    invalid.source_id = Some("missing-source".into());
    assert_eq!(
        db.mutate_price_rule(
            PriceRuleMutation::Replace {
                rule_id: id.clone(),
                draft: invalid
            },
            1,
            2
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    let fault_id = id.clone();
    assert_eq!(
        db.write(move |conn| {
            let tx = conn.transaction()?;
            apply(
                &tx,
                PriceRuleMutation::Retire { rule_id: fault_id },
                1,
                EpochMs::new(2)?,
            )?;
            Err::<(), _>(ErrorCode::DbWriteFailed.into())
        })
        .unwrap_err()
        .code,
        ErrorCode::DbWriteFailed
    );
    db.write(|conn| {conn.execute_batch("CREATE TRIGGER reject_price BEFORE INSERT ON price_rules BEGIN SELECT RAISE(ABORT,'fixture'); END;")?;Ok(())}).unwrap();
    assert_eq!(
        db.mutate_price_rule(
            PriceRuleMutation::Replace {
                rule_id: id,
                draft: draft()
            },
            1,
            2
        )
        .unwrap_err()
        .code,
        ErrorCode::DbWriteFailed
    );
    let current = db.price_rules().unwrap();
    assert_eq!(current.price_revision.as_str(), "1");
    assert_eq!(current.rules.len(), 1);
    assert!(current.rules[0].retired_revision.is_none());
    assert_eq!(
        db.usage_totals(&crate::query::tests::filter())
            .unwrap()
            .total_tokens
            .as_str(),
        "110"
    );
}

#[test]
fn retirement_stale_writes_and_concurrent_editing_have_one_cas_winner() {
    use std::sync::{Arc, Barrier};
    let (_dir, db) = setup();
    let barrier = Arc::new(Barrier::new(3));
    let mut threads = Vec::new();
    for i in 0..2 {
        let db = db.clone();
        let gate = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let mut d = draft();
            d.model_exact = format!("model-{i}");
            gate.wait();
            db.mutate_price_rule(PriceRuleMutation::Create { draft: d }, 0, 1)
        }));
    }
    barrier.wait();
    let results = threads
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .next()
            .unwrap()
            .code,
        ErrorCode::RevisionConflict
    );
    let current = db.price_rules().unwrap();
    assert_eq!(current.price_revision.as_str(), "1");
    assert_eq!(current.rules.len(), 1);
    let id = current.rules[0].rule_id.clone();
    assert_eq!(
        db.mutate_price_rule(
            PriceRuleMutation::Retire {
                rule_id: id.clone()
            },
            1,
            2
        )
        .unwrap(),
        2
    );
    assert!(db.price_rules().unwrap().rules.is_empty());
    assert_eq!(
        db.mutate_price_rule(PriceRuleMutation::Retire { rule_id: id }, 2, 3)
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.mutate_price_rule(PriceRuleMutation::Create { draft: draft() }, 0, 4)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
}

#[test]
fn revision_overflow_and_corrupt_currency_preserve_existing_configuration() {
    let (_dir, db) = setup();
    db.mutate_price_rule(PriceRuleMutation::Create { draft: draft() }, 0, 1)
        .unwrap();
    db.write(|conn| {
        conn.execute("UPDATE price_rules SET currency='U$D'", [])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(db.price_rules().unwrap_err().code, ErrorCode::DbCorrupt);
    db.write(|conn| {
        conn.execute("UPDATE price_rules SET currency='USD'", [])?;
        conn.execute("UPDATE app_state SET price_revision=?1", [i64::MAX])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.mutate_price_rule(PriceRuleMutation::Create { draft: draft() }, i64::MAX, 2)
            .unwrap_err()
            .code,
        ErrorCode::NumericOverflow
    );
    assert_eq!(
        db.price_rules().unwrap().price_revision.as_str(),
        "9223372036854775807"
    );
}

#[test]
fn aliases_loaded_at_the_requested_revision_do_not_take_the_current_mapping() {
    let (_dir, db) = setup();
    db.mutate_price_rule(PriceRuleMutation::Create { draft: draft() }, 0, 1)
        .unwrap();
    db.write(|conn| {
        conn.execute("INSERT INTO model_aliases VALUES('old','fixture-provider','short','fixture-model',1,2)",[])?;
        conn.execute("INSERT INTO model_aliases VALUES('new','fixture-provider','short','different-model',2,NULL)",[])?;
        conn.execute("UPDATE app_state SET price_revision=2",[])?;Ok(())
    }).unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(rules_at(tx, 1)?.aliases[0].canonical_model, "fixture-model");
        assert_eq!(
            rules_at(tx, 2)?.aliases[0].canonical_model,
            "different-model"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn active_rule_limit_is_explicit_and_replacement_does_not_exceed_the_bound() {
    let (_dir, db) = setup();
    db.write(|conn| {
        conn.execute_batch("BEGIN;
          WITH RECURSIVE n(v) AS(VALUES(0) UNION ALL SELECT v+1 FROM n WHERE v<4095)
          INSERT INTO price_rules(rule_id,introduced_revision,provider,model_exact,currency,effective_from_ms,priority,input_rate_atoms,output_rate_atoms,origin,created_at_ms)
          SELECT 'rule-'||v,1,'fixture-provider','model-'||v,'USD',0,0,'0','0','custom',0 FROM n;
          UPDATE app_state SET price_revision=1; COMMIT;")?;Ok(())
    }).unwrap();
    assert_eq!(db.price_rules().unwrap().rules.len(), 4096);
    assert_eq!(
        db.mutate_price_rule(PriceRuleMutation::Create { draft: draft() }, 1, 1)
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(db.price_rules().unwrap().price_revision.as_str(), "1");
    assert_eq!(
        db.mutate_price_rule(
            PriceRuleMutation::Replace {
                rule_id: "rule-0".into(),
                draft: draft()
            },
            1,
            2
        )
        .unwrap(),
        2
    );
    assert_eq!(db.price_rules().unwrap().rules.len(), 4096);
}

#[test]
fn mutation_returns_its_own_transaction_snapshot_and_historical_requests_are_bounded() {
    let (_dir, db) = setup();
    let response = db
        .mutate_price_rule_snapshot(PriceRuleMutation::Create { draft: draft() }, 0, 1)
        .unwrap();
    assert_eq!(response.price_revision.as_str(), "1");
    assert_eq!(response.rules[0].input_rate_atoms.as_str(), "10");
    let id = response.rules[0].rule_id.clone();
    let mut next = draft();
    next.input_rate_atoms = n(1000);
    db.mutate_price_rule_snapshot(
        PriceRuleMutation::Replace {
            rule_id: id,
            draft: next,
        },
        1,
        2,
    )
    .unwrap();
    assert_eq!(response.price_revision.as_str(), "1");
    assert_eq!(response.rules[0].input_rate_atoms.as_str(), "10");
    assert!(response.rules[0].retired_revision.is_none());
    assert_eq!(
        db.price_rules_at(None).unwrap().price_revision.as_str(),
        "2"
    );
    assert_eq!(
        db.price_rules_at(Some(1)).unwrap().rules[0]
            .input_rate_atoms
            .as_str(),
        "10"
    );
    assert!(db.price_rules_at(Some(0)).unwrap().rules.is_empty());
    assert_eq!(
        db.price_rules_at(Some(3)).unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.price_rules_at(Some(-1)).unwrap_err().code,
        ErrorCode::InvalidQuery
    );
}
