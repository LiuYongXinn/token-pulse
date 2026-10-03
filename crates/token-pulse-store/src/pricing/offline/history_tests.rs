//! SQLite price history with internal synthetic mode evidence, never real mode capture.
use super::*;
use crate::batch::tests::{fixture, setup};
use token_pulse_core::{
    domain::{NormalizedObservation, RequestUsageEvidence, UsageObservation, UsageVector},
    pricing::{
        ModelAliasDraft, ModelAliasMutation, PriceOutcome, PricingEvent, UnpricedCode, offline::*,
    },
    protocol::PriceBasis,
};

fn usage_fixture(model: &str, at: i64, write: i64) -> (crate::batch::WriteBatch, UsageObservation) {
    let mut batch = fixture();
    let NormalizedObservation::Usage(observation) = &mut batch.observations[0].record else {
        panic!()
    };
    let usage = UsageVector {
        cache_write_input: Some(write),
        ..observation.last.unwrap()
    };
    observation.physical_position.byte_offset = 10;
    observation.event_time_ms = Some(at);
    observation.last = Some(usage);
    observation.cumulative = Some(usage);
    observation.effective_metadata.provider = Some("openai".into());
    observation.effective_metadata.model = Some(model.into());
    observation.effective_metadata.turn_id = Some("synthetic-turn".into());
    observation.request_usage = Some(Box::new(RequestUsageEvidence {
        response_id: "synthetic-response".into(),
        turn_id: "synthetic-turn".into(),
        usage,
        thread_usage: usage,
        physical_position: token_pulse_core::domain::PhysicalPosition {
            file_generation_id: "generation".into(),
            byte_offset: 0,
            byte_end: 10,
        },
    }));
    let observation = observation.clone();
    batch.events[0].usage = usage;
    batch.events[0].model = Some(model.into());
    batch.events[0].turn_id = Some("synthetic-turn".into());
    batch.events[0].occurred_at_ms = at;
    batch.streams[0].baseline = usage;
    (batch, observation)
}
fn estimate(
    catalog: &PriceCatalog,
    observation: &UsageObservation,
    model: &str,
    occurred: i64,
    specified: Option<i64>,
    tier: Option<OfflinePriceTier>,
) -> PriceOutcome {
    catalog.estimate_with_request(
        &PricingEvent {
            provider: Some("openai"),
            model: Some(model),
            source_ids: &[],
            occurred_at_ms: EpochMs::new(occurred).unwrap(),
            usage: observation.last.unwrap(),
        },
        &specified
            .map(|at| PriceBasis::SpecifiedTime {
                specified_at_ms: EpochMs::new(at).unwrap(),
            })
            .unwrap_or(PriceBasis::EventTime {}),
        Some(RequestPriceEvidence {
            response: observation.request_usage.as_ref().unwrap(),
            context: observation.into(),
            actual_tier: tier,
        }),
    )
}
fn atoms(outcome: PriceOutcome) -> String {
    match outcome {
        PriceOutcome::Priced { cost_atoms, .. } => cost_atoms.as_str().into(),
        other => panic!("{other:?}"),
    }
}
fn preserved(db: &Database) -> (i64, i64, i64, i64) {
    db.snapshot(|tx,r|Ok((r.data,tx.query_row("SELECT total_tokens FROM usage_events WHERE event_id='event'",[],|row|row.get(0))?,tx.query_row("SELECT committed_offset FROM file_generations WHERE file_generation_id='generation'",[],|row|row.get(0))?,tx.query_row("SELECT checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|row|row.get(0))?))).unwrap()
}

#[test]
fn fixed_database_catalogs_select_old_or_new_conditions_without_fallback_or_token_changes() {
    let (directory, db) = setup();
    let old = OfflinePriceCatalog::bundled().unwrap();
    let at = old.verified_at_ms.value();
    let (batch, observation) = usage_fixture("gpt-6.1-sol", at + 500, 20);
    db.commit(batch).unwrap();
    db.install_offline_price_catalog(old.clone(), at).unwrap();
    let before = preserved(&db);
    let mut next = old.clone();
    next.catalog_id = "openai-text-history-fixture".into();
    next.verified_at_ms = EpochMs::new(at + 1000).unwrap();
    next.entries.retain(|entry| {
        !(entry.model_exact == "gpt-6.1-sol" && entry.tier == OfflinePriceTier::Fast)
    });
    for entry in next.entries.iter_mut().filter(|entry| {
        entry.model_exact == "gpt-6.1-sol"
            && entry.tier == OfflinePriceTier::Standard
            && entry.context == OfflineContextBand::Short
    }) {
        entry.input_per_million = "9".into();
        entry.cached_per_million = Some("3".into());
        entry.cache_write_per_million = Some("7".into());
        entry.output_per_million = "11".into();
    }
    db.snapshot(|tx, r| {
        assert_eq!(r.price, 1);
        assert_eq!(
            db.install_offline_price_catalog(next.clone(), at + 1001)?,
            2
        );
        let captured = catalog_at(tx, r.price)?;
        // Independent: 20*2 + 60*.1 + 20*2.5 + 10*10 = 196 / million USD.
        assert_eq!(
            atoms(estimate(
                &captured,
                &observation,
                "gpt-6.1-sol",
                at + 2000,
                None,
                Some(OfflinePriceTier::Standard)
            )),
            "196000000000"
        );
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, r| {
        let catalog = catalog_at(tx, r.price)?;
        assert_eq!(
            atoms(estimate(
                &catalog,
                &observation,
                "gpt-6.1-sol",
                at + 500,
                None,
                Some(OfflinePriceTier::Standard)
            )),
            "196000000000"
        );
        // Independent new fixture: 20*9 + 60*3 + 20*7 + 10*11 = 610.
        assert_eq!(
            atoms(estimate(
                &catalog,
                &observation,
                "gpt-6.1-sol",
                at + 1000,
                None,
                Some(OfflinePriceTier::Standard)
            )),
            "610000000000"
        );
        assert_eq!(
            atoms(estimate(
                &catalog,
                &observation,
                "gpt-6.1-sol",
                at + 500,
                Some(at + 1000),
                Some(OfflinePriceTier::Standard)
            )),
            "610000000000"
        );
        assert_eq!(
            atoms(estimate(
                &catalog,
                &observation,
                "gpt-6.1-sol",
                at + 2000,
                Some(at + 999),
                Some(OfflinePriceTier::Standard)
            )),
            "196000000000"
        );
        let old_fast = estimate(
            &catalog,
            &observation,
            "gpt-6.1-sol",
            at + 500,
            None,
            Some(OfflinePriceTier::Fast),
        );
        assert_eq!(atoms(old_fast.clone()), "392000000000");
        let PriceOutcome::Priced { rule_id, .. } = old_fast else {
            panic!()
        };
        assert_eq!(
            rule_id,
            "offline/openai-text-2026-10-02/gpt-6.1-sol/fast/short"
        );
        assert_eq!(
            tx.query_row(
                "SELECT request_conditional FROM price_rules WHERE rule_id=?1",
                [rule_id],
                |row| row.get::<_, i64>(0)
            )?,
            1
        );
        assert!(matches!(
            estimate(
                &catalog,
                &observation,
                "gpt-6.1-sol",
                at + 1000,
                None,
                Some(OfflinePriceTier::Fast)
            ),
            PriceOutcome::Unpriced {
                reason: UnpricedCode::IncompletePricingConditions
            }
        ));
        assert!(matches!(
            estimate(&catalog, &observation, "gpt-6.1-sol", at + 500, None, None),
            PriceOutcome::Unpriced {
                reason: UnpricedCode::IncompletePricingConditions
            }
        ));
        Ok(())
    })
    .unwrap();
    assert_eq!(preserved(&db), before);
    drop(db);
    let db = Database::open(directory.path()).unwrap();
    assert_eq!(preserved(&db), before);
    db.snapshot(|tx, r| {
        assert_eq!(
            atoms(estimate(
                &catalog_at(tx, r.price)?,
                &observation,
                "gpt-6.1-sol",
                at + 500,
                None,
                Some(OfflinePriceTier::Fast)
            )),
            "392000000000"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn closed_flat_history_resolves_aliases_and_preserves_confirmed_modes_across_revision_gaps() {
    let (_directory, db) = setup();
    let old = OfflinePriceCatalog::bundled().unwrap();
    let at = old.verified_at_ms.value();
    let (batch, observation) = usage_fixture("gpt-5.3-codex", at + 500, 0);
    db.commit(batch).unwrap();
    db.install_offline_price_catalog(old.clone(), at).unwrap();
    db.mutate_model_alias_snapshot(
        ModelAliasMutation::Create {
            draft: ModelAliasDraft {
                provider: "openai".into(),
                alias: "synthetic-snapshot".into(),
                canonical_model: "gpt-5.3-codex".into(),
            },
        },
        1,
        at + 1,
    )
    .unwrap();
    let mut next = old;
    next.catalog_id = "openai-text-history-flat-fixture".into();
    next.verified_at_ms = EpochMs::new(at + 1000).unwrap();
    next.entries.retain(|entry| {
        !(entry.model_exact == "gpt-5.3-codex" && entry.tier == OfflinePriceTier::Fast)
    });
    assert_eq!(
        db.install_offline_price_catalog(next, at + 1001).unwrap(),
        3
    );
    db.snapshot(|tx, r| {
        let catalog = catalog_at(tx, r.price)?;
        assert_eq!(
            atoms(estimate(
                &catalog,
                &observation,
                "synthetic-snapshot",
                at + 500,
                None,
                None
            )),
            "220500000000"
        );
        assert_eq!(
            atoms(estimate(
                &catalog,
                &observation,
                "synthetic-snapshot",
                at + 500,
                None,
                Some(OfflinePriceTier::Fast)
            )),
            "441000000000"
        );
        assert!(matches!(
            estimate(
                &catalog,
                &observation,
                "synthetic-snapshot",
                at + 1000,
                None,
                Some(OfflinePriceTier::Fast)
            ),
            PriceOutcome::Unpriced {
                reason: UnpricedCode::IncompletePricingConditions
            }
        ));
        assert_eq!(
            atoms(estimate(
                &catalog,
                &observation,
                "synthetic-snapshot",
                at + 1000,
                Some(at + 500),
                Some(OfflinePriceTier::Fast)
            )),
            "441000000000"
        );
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM price_rules WHERE rule_id LIKE 'offline-history/%'",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            37
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(preserved(&db).1, 110);
}

#[test]
fn history_read_rejects_corrupt_older_facts_instead_of_using_only_the_latest_catalog() {
    let (_directory, db) = setup();
    let old = OfflinePriceCatalog::bundled().unwrap();
    let at = old.verified_at_ms.value();
    db.install_offline_price_catalog(old.clone(), at).unwrap();
    let mut next = old.clone();
    next.catalog_id = "openai-text-history-validation-fixture".into();
    next.verified_at_ms = EpochMs::new(at + 1000).unwrap();
    db.install_offline_price_catalog(next, at + 1000).unwrap();
    db.write(move |conn| {
        conn.execute(
            "UPDATE offline_price_catalogs SET content_sha256=?1 WHERE catalog_id=?2",
            params!["0".repeat(64), old.catalog_id],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(db.offline_price_catalog_at(None).unwrap().catalog.is_some());
    assert_eq!(
        db.snapshot(|tx, r| catalog_at(tx, r.price).map(|_| ()))
            .unwrap_err()
            .code,
        ErrorCode::DbCorrupt
    );
}
