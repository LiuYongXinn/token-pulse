//! Independent storage oracles; quote rows do not prove real request modes.
use super::*;
use crate::batch::tests::{fixture, setup};
use token_pulse_core::protocol::PriceBasis;

const LONG_ID: &str = "offline/openai-text-2026-10-02/gpt-6.1-sol/standard/long";

fn counts(db: &Database) -> (i64, i64, i64) {
    db.snapshot(|tx, _| {
        Ok((
            tx.query_row("SELECT COUNT(*) FROM price_rules", [], |r| r.get(0))?,
            tx.query_row("SELECT COUNT(*) FROM conditional_price_rules", [], |r| {
                r.get(0)
            })?,
            tx.query_row("SELECT COUNT(*) FROM offline_price_catalogs", [], |r| {
                r.get(0)
            })?,
        ))
    })
    .unwrap()
}

fn install(db: &Database) -> OfflinePriceCatalog {
    let catalog = OfflinePriceCatalog::bundled().unwrap();
    db.install_offline_price_catalog(catalog.clone(), catalog.verified_at_ms.value())
        .unwrap();
    catalog
}

#[test]
fn quotes_are_persisted_with_exact_four_rates_but_never_enter_flat_matching() {
    let (_directory, db) = setup();
    db.commit(fixture()).unwrap();
    let catalog = install(&db);
    assert_eq!(counts(&db), (209, 172, 1));
    let flat = db.price_rules().unwrap();
    assert_eq!(flat.rules.len(), 37);
    assert!(!flat.rules.iter().any(|r| r.rule_id == LONG_ID));
    db.snapshot(|tx, revision| {
        assert_eq!((revision.data, revision.price), (1, 1));
        let mut statement = tx.prepare(&format!(
            "SELECT {COLUMNS} FROM price_rules WHERE rule_id=?1"
        ))?;
        let mut rows = statement.query([LONG_ID])?;
        let rule = read_rule(rows.next()?.unwrap())?;
        assert_eq!(rule.input_rate_atoms.as_str(), "4000000000");
        assert_eq!(
            rule.cached_rate_atoms.as_ref().unwrap().as_str(),
            "200000000"
        );
        assert_eq!(
            rule.cache_write_rate_atoms.as_ref().unwrap().as_str(),
            "5000000000"
        );
        assert_eq!(rule.output_rate_atoms.as_str(), "15000000000");
        assert_eq!(rule.introduced_revision.as_str(), "1");
        assert_eq!(rule.created_at_ms, catalog.verified_at_ms);
        assert_eq!(rule.retired_revision, None);
        let metadata: (String, String, String) = tx.query_row(
            "SELECT model_exact,tier,context_band FROM conditional_price_rules WHERE rule_id=?1",
            [LONG_ID],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        assert_eq!(
            metadata,
            ("gpt-6.1-sol".into(), "standard".into(), "long".into())
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            0
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.mutate_price_rule(
            token_pulse_core::pricing::PriceRuleMutation::Retire {
                rule_id: LONG_ID.into()
            },
            1,
            catalog.verified_at_ms.value()
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.usage_totals(&crate::query::tests::filter())
            .unwrap()
            .total_tokens
            .as_str(),
        "110"
    );
}

#[test]
fn persisted_quotes_do_not_consume_the_active_flat_rule_limit() {
    let (_directory, db) = setup();
    let catalog = install(&db);
    db.write(|conn| {
        conn.execute_batch("WITH RECURSIVE n(v) AS(VALUES(0) UNION ALL SELECT v+1 FROM n WHERE v<4057)
        INSERT INTO price_rules(rule_id,introduced_revision,provider,model_exact,currency,effective_from_ms,priority,input_rate_atoms,output_rate_atoms,origin,created_at_ms)
        SELECT 'capacity-'||v,1,'fixture-provider','model-'||v,'USD',0,0,'0','0','custom',0 FROM n;")?;
        Ok(())
    }).unwrap();
    assert_eq!(db.price_rules().unwrap().rules.len(), 4095);
    let draft = crate::pricing::tests::draft();
    assert_eq!(
        db.mutate_price_rule(
            token_pulse_core::pricing::PriceRuleMutation::Create {
                draft: draft.clone()
            },
            1,
            catalog.verified_at_ms.value()
        )
        .unwrap(),
        2
    );
    assert_eq!(db.price_rules().unwrap().rules.len(), 4096);
    let mut other = draft;
    other.model_exact = "another-capacity-model".into();
    assert_eq!(
        db.mutate_price_rule(
            token_pulse_core::pricing::PriceRuleMutation::Create { draft: other },
            2,
            catalog.verified_at_ms.value()
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(counts(&db), (4268, 172, 1));
    assert_eq!(db.price_rules().unwrap().price_revision.as_str(), "2");
}

#[test]
fn schema_thirteen_boot_materialization_keeps_original_revisions_and_read_snapshot() {
    let (directory, db) = setup();
    db.commit(fixture()).unwrap();
    let catalog = install(&db);
    let original = serde_json::to_value(db.price_rules().unwrap()).unwrap();
    let (data, price, settings) = db
        .snapshot(|_, r| Ok((r.data, r.price, r.settings)))
        .unwrap();
    // Model an actual old v13 publication: it has the catalog and 37 flat rules only.
    // Remove the child rows first with foreign keys kept enabled.
    db.write(|conn| {conn.execute_batch("CREATE TEMP TABLE old_quote_ids AS SELECT rule_id FROM conditional_price_rules; DELETE FROM conditional_price_rules; DELETE FROM price_rules WHERE rule_id IN (SELECT rule_id FROM old_quote_ids); DROP TABLE old_quote_ids; DROP TABLE conditional_price_rules; ALTER TABLE price_rules DROP COLUMN request_conditional; DELETE FROM schema_migrations WHERE version>=14; UPDATE app_state SET schema_version=13; PRAGMA user_version=13;")?;Ok(())}).unwrap();
    drop(db);
    let db = Database::open(directory.path()).unwrap();
    assert_eq!(counts(&db), (37, 0, 1));
    db.snapshot(|tx, old| {
        assert_eq!((old.data, old.price, old.settings), (data, price, settings));
        assert_eq!(
            db.install_offline_price_catalog(
                catalog.clone(),
                catalog.verified_at_ms.value() + 9999
            )?,
            price
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM conditional_price_rules", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(serde_json::to_value(rules_at(tx, old.price)?)?, original);
        Ok(())
    })
    .unwrap();
    assert_eq!(counts(&db), (209, 172, 1));
    assert_eq!(
        serde_json::to_value(db.price_rules().unwrap()).unwrap(),
        original
    );
    db.snapshot(|tx, r| {
        assert_eq!((r.data, r.price, r.settings), (data, price, settings));
        assert_eq!(
            tx.query_row(
                "SELECT created_at_ms FROM price_rules WHERE rule_id=?1",
                [LONG_ID],
                |r| r.get::<_, i64>(0)
            )?,
            catalog.verified_at_ms.value()
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            0
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.install_offline_price_catalog(catalog.clone(), catalog.verified_at_ms.value() + 10000)
            .unwrap(),
        price
    );
    assert_eq!(counts(&db), (209, 172, 1));
}

#[test]
fn partial_publication_failure_rolls_back_catalog_parent_quotes_and_price_revision() {
    let (_directory, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {conn.execute_batch("CREATE TRIGGER quote_publication_failure BEFORE INSERT ON conditional_price_rules WHEN NEW.tier='fast' BEGIN SELECT RAISE(ABORT,'synthetic quote failure'); END;")?;Ok(())}).unwrap();
    let catalog = OfflinePriceCatalog::bundled().unwrap();
    assert!(
        db.install_offline_price_catalog(catalog.clone(), catalog.verified_at_ms.value())
            .is_err()
    );
    assert_eq!(counts(&db), (0, 0, 0));
    db.snapshot(|_, r| {
        assert_eq!((r.data, r.price), (1, 0));
        Ok(())
    })
    .unwrap();
    db.write(|conn| {
        conn.execute_batch("DROP TRIGGER quote_publication_failure;")?;
        Ok(())
    })
    .unwrap();
    install(&db);
    assert_eq!(counts(&db), (209, 172, 1));
}

#[test]
fn new_catalog_retains_old_quote_ids_for_foreign_keys_and_captured_revisions() {
    let (_directory, db) = setup();
    db.commit(fixture()).unwrap();
    let catalog = install(&db);
    db.build_event_valuation(
        "ledger",
        &PriceBasis::EventTime {},
        catalog.verified_at_ms.value(),
    )
    .unwrap();
    // Only verify the real FK contract here; this synthetic row is not a computed quote.
    db.write(|conn| {
        assert_eq!(conn.execute("UPDATE event_valuations SET status='priced',rule_id=?1,currency='USD',cost_atoms='0' WHERE event_id='event'",[LONG_ID])?,1);
        let error = conn.execute("UPDATE event_valuations SET rule_id='nonexistent-condition-quote' WHERE event_id='event'",[]).unwrap_err();
        assert_eq!(error.sqlite_error().unwrap().extended_code,787);
        Ok(())
    }).unwrap();
    let mut newer = catalog.clone();
    newer.catalog_id = "openai-text-condition-next-fixture".into();
    newer.verified_at_ms = EpochMs::new(catalog.verified_at_ms.value() + 1000).unwrap();
    for entry in &mut newer.entries {
        entry.input_per_million = "9.00".into();
    }
    db.snapshot(|tx, r| {
        assert_eq!(r.price, 1);
        assert_eq!(
            db.install_offline_price_catalog(newer.clone(), newer.verified_at_ms.value())?,
            2
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM conditional_price_rules", [], |r| r
                .get::<_, i64>(0))?,
            172
        );
        assert_eq!(
            tx.query_row(
                "SELECT input_rate_atoms FROM price_rules WHERE rule_id=?1",
                [LONG_ID],
                |r| r.get::<_, String>(0)
            )?,
            "4000000000"
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(db.price_rules_at(Some(1)).unwrap().rules.len(), 37);
    assert_eq!(db.price_rules().unwrap().rules.len(), 74);
    db.snapshot(|tx,r| {
        assert_eq!((r.data,r.price),(1,2));
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM conditional_price_rules",[],|r|r.get::<_,i64>(0))?,344);
        assert_eq!(tx.query_row("SELECT input_rate_atoms FROM price_rules WHERE rule_id='offline/openai-text-condition-next-fixture/gpt-6.1-sol/standard/long'",[],|r|r.get::<_,String>(0))?,"9000000000");
        assert_eq!(tx.query_row("SELECT rule_id FROM event_valuations WHERE event_id='event'",[],|r|r.get::<_,String>(0))?,LONG_ID);
        assert_eq!(tx.query_row("SELECT retired_revision FROM price_rules WHERE rule_id=?1",[LONG_ID],|r|r.get::<_,Option<i64>>(0))?,None);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check",[],|r|r.get::<_,i64>(0))?,0);
        Ok(())
    }).unwrap();
}

#[test]
fn inconsistent_quote_metadata_or_rate_is_rejected_without_repair_or_new_publication() {
    for change in 0..4 {
        let (_directory, db) = setup();
        let catalog = install(&db);
        db.write(move |conn| {
            match change {
                0 => {conn.execute("UPDATE conditional_price_rules SET model_exact='wrong-fixture' WHERE rule_id=?1",[LONG_ID])?;},
                1 => {conn.execute("UPDATE price_rules SET input_rate_atoms='999' WHERE rule_id=?1",[LONG_ID])?;},
                2 => {conn.execute("DELETE FROM conditional_price_rules WHERE rule_id=?1",[LONG_ID])?;},
                _ => {conn.execute("UPDATE price_rules SET request_conditional=0 WHERE rule_id=?1",[LONG_ID])?;},
            }
            Ok(())
        }).unwrap();
        let before = counts(&db);
        assert_eq!(db.price_rules().unwrap().rules.len(), 37);
        assert_eq!(
            db.install_offline_price_catalog(catalog.clone(), catalog.verified_at_ms.value())
                .unwrap_err()
                .code,
            ErrorCode::DbCorrupt
        );
        let mut newer = catalog.clone();
        newer.catalog_id = "openai-text-condition-failed-fixture".into();
        newer.verified_at_ms = EpochMs::new(catalog.verified_at_ms.value() + 1).unwrap();
        assert_eq!(
            db.install_offline_price_catalog(newer, catalog.verified_at_ms.value() + 1)
                .unwrap_err()
                .code,
            ErrorCode::DbCorrupt
        );
        assert_eq!(counts(&db), before);
        assert_eq!(db.price_rules().unwrap().price_revision.as_str(), "1");
        db.snapshot(|tx, _| {
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
}
