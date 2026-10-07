use super::*;
use crate::batch::tests::{fixture, setup};
use crate::query::tests::{extra, filter, ids};
use token_pulse_core::{
    pricing::{PriceRuleDraft, PriceRuleMutation},
    query::{UsageEventSort, UsageEventsQuery, UsageEventsRequest},
};

mod request_inputs;

fn draft(input: i128) -> PriceRuleDraft {
    PriceRuleDraft {
        provider: "P".into(),
        model_exact: "M".into(),
        source_id: None,
        currency: "USD".into(),
        effective_from_ms: EpochMs::new(0).unwrap(),
        effective_to_ms: None,
        priority: 0,
        input_rate_atoms: DecimalInt::from_nonnegative(input).unwrap(),
        cache_write_rate_atoms: None,
        cached_rate_atoms: Some(DecimalInt::parse("5").unwrap()),
        output_rate_atoms: DecimalInt::parse("20").unwrap(),
        origin_reference: Some("synthetic only".into()),
    }
}
fn priced() -> (tempfile::TempDir, Database) {
    let (directory, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.effective_metadata.provider','P') WHERE observation_id='observation'",[])?;conn.execute("UPDATE usage_events SET model='M' WHERE event_id='event'",[])?;Ok(())}).unwrap();
    db.mutate_price_rule(PriceRuleMutation::Create { draft: draft(10) }, 0, 1)
        .unwrap();
    (directory, db)
}
fn cached(
    tx: &Transaction<'_>,
    revision: i64,
    basis: &PriceBasis,
) -> StoreResult<Option<PriceOutcome>> {
    let catalog = crate::pricing::catalog_at(tx, revision)?;
    let mut result = None;
    crate::query::pricing::visit_ledger_uncached(tx, "ledger", basis, &catalog, |event| {
        if event.event_id == "event" {
            result = lookup(
                tx,
                &event.event_id,
                &event.cache_fingerprint,
                &catalog.revision,
                basis,
            )?;
        }
        Ok(())
    })?;
    Ok(result)
}
fn cost(outcome: PriceOutcome) -> String {
    match outcome {
        PriceOutcome::Priced { cost_atoms, .. } => cost_atoms.as_str().into(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn aggregate_prefetch_equals_point_lookup_and_discards_a_partial_overflow() {
    let (_directory, db) = priced();
    let basis = PriceBasis::EventTime {};
    db.build_event_valuation("ledger", &basis, 2).unwrap();
    db.snapshot(|tx, revision| {
        let catalog = crate::pricing::catalog_at(tx, revision.price)?;
        let scope = crate::query::predicate(&filter())?;
        let mut prefetched = CacheReader::new(tx, &catalog.revision, &basis)?;
        prefetched.prefetch(tx, &scope)?;
        assert!(prefetched.prefetched.is_some());
        let mut overflow = CacheReader::new(tx, &catalog.revision, &basis)?;
        overflow.prefetch_bounded(tx, &scope, 1)?;
        assert!(overflow.prefetched.is_none());
        crate::query::pricing::visit_ledger_uncached(tx, "ledger", &basis, &catalog, |event| {
            let exact = lookup(
                tx,
                &event.event_id,
                &event.cache_fingerprint,
                &catalog.revision,
                &basis,
            )?;
            assert_eq!(
                prefetched.lookup(&event.event_id, &event.cache_fingerprint, &event.ledger_id)?,
                exact
            );
            assert_eq!(
                overflow.lookup(&event.event_id, &event.cache_fingerprint, &event.ledger_id)?,
                exact
            );
            assert!(
                prefetched
                    .lookup(&event.event_id, "wrong-fingerprint", &event.ledger_id)?
                    .is_none()
            );
            assert!(
                prefetched
                    .lookup(&event.event_id, &event.cache_fingerprint, "other-ledger")?
                    .is_none()
            );
            Ok(())
        })?;
        Ok(())
    })
    .unwrap();
}
fn summary_cost(db: &Database) -> String {
    db.pricing_summary(&filter(), &PriceBasis::EventTime {})
        .unwrap()
        .currencies[0]
        .estimated_cost
        .as_ref()
        .unwrap()
        .as_str()
        .into()
}

#[test]
fn aggregate_prefetch_does_not_visit_cached_events_outside_the_fact_range() {
    let (_directory, db) = priced();
    db.write(|conn| {
        let tx = conn.transaction()?;
        tx.execute_batch("WITH RECURSIVE n(v) AS (VALUES(0) UNION ALL SELECT v+1 FROM n WHERE v<1999)
          INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,observed_at_ms,model,normalized_json,payload_fingerprint,format_version)
          SELECT 'outside-'||v,'generation',10000+v*100,10100+v*100,'session','usage',10000+v,'M','{\"kind\":\"usage\",\"effective_metadata\":{\"provider\":\"P\",\"model\":\"M\"}}','outside-'||v,'fixture' FROM n;
          INSERT INTO usage_events(event_id,ledger_id,origin_observation_id,occurred_at_ms,episode_id,model,source_total_tokens,total_tokens,calculation_method,quality_json)
          SELECT observation_id,'ledger',observation_id,observed_at_ms,'episode',model,1,1,'fixture','[\"confirmed\"]' FROM observations WHERE observation_id LIKE 'outside-%';
          INSERT INTO event_provenance SELECT event_id,origin_observation_id,'origin' FROM usage_events WHERE event_id LIKE 'outside-%';
          UPDATE app_state SET data_revision=data_revision+1;")?;
        tx.commit()?;
        Ok(())
    }).unwrap();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 2)
        .unwrap();
    db.snapshot(|tx, revision| {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let mut reader = CacheReader::new(
            tx,
            &DecimalInt::from_nonnegative(revision.price.into())?,
            &PriceBasis::EventTime {},
        )?;
        let steps = Arc::new(AtomicUsize::new(0));
        let measured = steps.clone();
        tx.progress_handler(
            1,
            Some(move || {
                measured.fetch_add(1, Ordering::Relaxed);
                false
            }),
        )?;
        reader.prefetch(tx, &crate::query::predicate(&filter())?)?;
        tx.progress_handler(0, None::<fn() -> bool>)?;
        assert_eq!(reader.prefetched.as_ref().unwrap().len(), 1);
        assert!(
            steps.load(Ordering::Relaxed) < 1000,
            "out-of-range cache rows were traversed: {}",
            steps.load(Ordering::Relaxed)
        );
        let input: String = tx.query_row(
            "SELECT input_sha256 FROM valuation_cache_inputs WHERE event_id='event' LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(
            reader.lookup("event", &input, "ledger")?,
            lookup(
                tx,
                "event",
                &input,
                &DecimalInt::from_nonnegative(revision.price.into())?,
                &PriceBasis::EventTime {}
            )?
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(summary_cost(&db), "0.000000000000900");
}

#[test]
fn assumed_reference_survives_cache_revalue_history_and_reopen_without_token_changes() {
    use token_pulse_core::pricing::{PriceMatchBasis, offline::OfflinePriceCatalog};
    let (directory, db) = setup();
    db.commit(fixture()).unwrap();
    let reference = OfflinePriceCatalog::bundled().unwrap();
    let at = reference.verified_at_ms.value();
    db.install_offline_price_catalog(reference, at).unwrap();
    db.write(move |conn| {
        conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.effective_metadata.provider','openai') WHERE observation_id='observation'",[])?;
        conn.execute("UPDATE usage_events SET model='gpt-6.1-sol',occurred_at_ms=?1 WHERE event_id='event'",[at])?; Ok(())
    }).unwrap();
    extra(
        &db,
        "newer",
        at + 1,
        1,
        (Some("gpt-6.1-sol"), Some("openai")),
        None,
        None,
    );
    let mut range = filter();
    range.range.end_ms = EpochMs::new(at + 5000).unwrap();
    let query = UsageEventsRequest {
        query: UsageEventsQuery {
            filter: range.clone(),
            price_basis: PriceBasis::EventTime {},
            sort: UsageEventSort::TimeDesc,
            page_size: 1,
        },
        cursor: None,
    };
    let before=db.snapshot(|tx,r| Ok((r.data,tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?))).unwrap();
    let first = db
        .query_usage_events("main", &query, EpochMs::new(at + 2).unwrap())
        .unwrap();
    assert_eq!(first.pricing.priced_total_tokens.as_str(), "110");
    assert_eq!(first.pricing.unpriced_total_tokens.as_str(), "1");
    assert_eq!(first.pricing.reasons[0].code, "insufficient_usage");
    assert_eq!(
        first.pricing.currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap()
            .as_str(),
        "0.000186000000000"
    );
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, at + 3)
        .unwrap();
    db.snapshot(|tx, _| {
        assert!(matches!(
            cached(tx, 1, &PriceBasis::EventTime {})?,
            Some(PriceOutcome::Priced { .. })
        ));
        Ok(())
    })
    .unwrap();
    let mut custom = draft(10);
    custom.provider = "openai".into();
    custom.model_exact = "gpt-6.1-sol".into();
    db.mutate_price_rule(PriceRuleMutation::Create { draft: custom }, 1, at + 4)
        .unwrap();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, at + 5)
        .unwrap();
    let second = db
        .query_usage_events(
            "main",
            &UsageEventsRequest {
                cursor: first.next_cursor.clone(),
                ..query.clone()
            },
            EpochMs::new(at + 6).unwrap(),
        )
        .unwrap();
    assert_eq!(second.meta.price_revision, first.meta.price_revision);
    assert_eq!(second.events[0].event_id, "event");
    assert_eq!(cost(second.events[0].price.clone()), "186000000000");
    assert!(matches!(
        second.events[0].matched_price.as_ref().unwrap().basis,
        PriceMatchBasis::OfflineAssumedReference {
            context_assumed: true,
            cache_write_assumed_zero: true,
            ..
        }
    ));
    assert!(second.events[0].usage.cache_write_input.is_none());
    assert!(matches!(
        second.events[0].price,
        PriceOutcome::Priced { .. }
    ));
    let fresh = db
        .query_usage_events(
            "main",
            &UsageEventsRequest {
                query: UsageEventsQuery {
                    page_size: 50,
                    ..query.query.clone()
                },
                cursor: None,
            },
            EpochMs::new(at + 7).unwrap(),
        )
        .unwrap();
    let event = fresh.events.iter().find(|e| e.event_id == "event").unwrap();
    assert_eq!(cost(event.price.clone()), "900");
    assert_eq!(fresh.summary.total_tokens.as_str(), "111");
    db.snapshot(|tx,r| { assert!(matches!(cached(tx,1,&PriceBasis::EventTime {})?,Some(PriceOutcome::Priced { .. }))); assert_eq!(cost(cached(tx,2,&PriceBasis::EventTime {})?.unwrap()),"900");
        assert_eq!((r.data,tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?),before); Ok(()) }).unwrap();
    drop(db);
    let db = Database::open(directory.path()).unwrap();
    assert_eq!(
        db.usage_totals(&range).unwrap().total_tokens.as_str(),
        "111"
    );
    db.snapshot(|tx, _| {
        assert!(matches!(
            cached(tx, 1, &PriceBasis::EventTime {})?,
            Some(PriceOutcome::Priced { .. })
        ));
        assert_eq!(
            cost(cached(tx, 2, &PriceBasis::EventTime {})?.unwrap()),
            "900"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn historical_reference_excludes_v3_cache_and_rebuilds_without_changing_consumption() {
    use token_pulse_core::pricing::offline::OfflinePriceCatalog;
    let (directory, db) = setup();
    db.commit(fixture()).unwrap();
    let old = OfflinePriceCatalog::bundled().unwrap();
    let at = old.verified_at_ms.value();
    db.install_offline_price_catalog(old.clone(), at).unwrap();
    db.write(move |conn| {
        conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.effective_metadata.provider','openai') WHERE observation_id='observation'",[])?;
        conn.execute("UPDATE usage_events SET model='gpt-6.1-sol',occurred_at_ms=?1 WHERE event_id='event'",[at+500])?;
        Ok(())
    }).unwrap();
    let mut next = old;
    next.catalog_id = "openai-text-cache-history-fixture".into();
    next.verified_at_ms = EpochMs::new(at + 1000).unwrap();
    db.install_offline_price_catalog(next, at + 1000).unwrap();
    let before=db.snapshot(|tx,r|Ok((r.data,
        tx.query_row("SELECT total_tokens FROM usage_events WHERE event_id='event'",[],|row|row.get::<_,i64>(0))?,
        tx.query_row("SELECT normalized_json FROM observations WHERE observation_id='observation'",[],|row|row.get::<_,String>(0))?,
        tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?))).unwrap();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, at + 1100)
        .unwrap();
    // A synthetic legacy cache carries the prior missing-rule explanation. Copy the
    // current fingerprint deliberately so this verifies the version gate independently.
    db.write(|conn| {
        let tx=conn.transaction()?;
        let current_input=input(&tx,"ledger",2,&PriceBasis::EventTime {})?;
        let current=current_input.id()?;
        let legacy=format!("valuation:{:x}",Sha256::digest(serde_json::to_vec(&(3_i64,&current_input))?));
        tx.execute("INSERT INTO valuation_sets SELECT ?1,price_revision,mode,specified_at_ms,state,created_at_ms FROM valuation_sets WHERE valuation_set_id=?2",params![legacy,current])?;
        tx.execute("INSERT INTO event_valuations SELECT ?1,event_id,NULL,NULL,NULL,'missing_rule' FROM event_valuations WHERE valuation_set_id=?2",params![legacy,current])?;
        tx.execute("INSERT INTO valuation_cache_inputs SELECT ?1,event_id,input_sha256 FROM valuation_cache_inputs WHERE valuation_set_id=?2",params![legacy,current])?;
        tx.execute("INSERT INTO valuation_cache_sets SELECT ?1,ledger_id,evidence_revision,3,parser_version,accounting_version,event_count,content_sha256,published_at_ms FROM valuation_cache_sets WHERE valuation_set_id=?2",params![legacy,current])?;
        tx.execute("DELETE FROM event_valuations WHERE valuation_set_id=?1",[&current])?;
        tx.execute("DELETE FROM valuation_sets WHERE valuation_set_id=?1",[&current])?;
        tx.commit()?;Ok(())
    }).unwrap();
    db.snapshot(|tx, _| {
        assert!(cached(tx, 2, &PriceBasis::EventTime {})?.is_none());
        Ok(())
    })
    .unwrap();
    let mut range = filter();
    range.range.end_ms = EpochMs::new(at + 2000).unwrap();
    let summary = db
        .pricing_summary(&range, &PriceBasis::EventTime {})
        .unwrap();
    assert!(summary.reasons.is_empty());
    assert_eq!(summary.priced_total_tokens.as_str(), "110");
    assert_eq!(
        summary.currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap()
            .as_str(),
        "0.000186000000000"
    );
    let result = db
        .build_event_valuation("ledger", &PriceBasis::EventTime {}, at + 1200)
        .unwrap();
    assert!(!result.already_ready);
    db.snapshot(|tx,r| {
        assert!(matches!(cached(tx,2,&PriceBasis::EventTime {})?,Some(PriceOutcome::Priced { .. })));
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM valuation_cache_sets WHERE cache_version=3",[],|row|row.get::<_,i64>(0))?,1);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM valuation_cache_sets WHERE cache_version=?1",[CACHE_VERSION],|row|row.get::<_,i64>(0))?,1);
        assert_eq!((r.data,
            tx.query_row("SELECT total_tokens FROM usage_events WHERE event_id='event'",[],|row|row.get::<_,i64>(0))?,
            tx.query_row("SELECT normalized_json FROM observations WHERE observation_id='observation'",[],|row|row.get::<_,String>(0))?,
            tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?),before);
        Ok(())
    }).unwrap();
    drop(db);
    let db = Database::open(directory.path()).unwrap();
    db.snapshot(|tx, _| {
        assert!(matches!(
            cached(tx, 2, &PriceBasis::EventTime {})?,
            Some(PriceOutcome::Priced { .. })
        ));
        Ok(())
    })
    .unwrap();
}

#[test]
fn conditional_reason_migration_preserves_legacy_valuation_rows_and_foreign_key_inputs() {
    let (directory, db) = priced();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 2)
        .unwrap();
    let before=db.snapshot(|tx,r| Ok((r.data,r.price,
        tx.query_row("SELECT valuation_set_id,event_id,rule_id,currency,cost_atoms,status FROM event_valuations",[],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?)))?,
        tx.query_row("SELECT valuation_set_id,event_id,input_sha256 FROM valuation_cache_inputs",[],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))?))).unwrap();
    // Own schema-12 layout: restore its actual status CHECK, keeping the two FK-linked tables.
    db.write(|conn| { crate::migration::remove_usage_revision_fixture(conn)?; conn.execute_batch("CREATE TEMP TABLE saved_inputs AS SELECT * FROM valuation_cache_inputs; DROP TABLE valuation_cache_inputs;
        CREATE TABLE old_valuations(valuation_set_id TEXT NOT NULL REFERENCES valuation_sets(valuation_set_id),event_id TEXT NOT NULL REFERENCES usage_events(event_id) ON DELETE CASCADE,rule_id TEXT REFERENCES price_rules(rule_id),currency TEXT,cost_atoms TEXT,status TEXT NOT NULL CHECK(status IN ('priced','unknown_model','missing_rule','ambiguous_rule','insufficient_usage','overflow')),PRIMARY KEY(valuation_set_id,event_id),CHECK((status='priced' AND cost_atoms IS NOT NULL AND currency IS NOT NULL) OR (status!='priced' AND cost_atoms IS NULL)));
        INSERT INTO old_valuations SELECT * FROM event_valuations; DROP TABLE event_valuations; ALTER TABLE old_valuations RENAME TO event_valuations;
        CREATE TABLE valuation_cache_inputs(valuation_set_id TEXT NOT NULL,event_id TEXT NOT NULL,input_sha256 TEXT NOT NULL CHECK(length(input_sha256)=64),PRIMARY KEY(valuation_set_id,event_id),FOREIGN KEY(valuation_set_id,event_id) REFERENCES event_valuations(valuation_set_id,event_id) ON DELETE CASCADE); INSERT INTO valuation_cache_inputs SELECT * FROM saved_inputs; DROP TABLE saved_inputs; CREATE INDEX valuation_cache_input_lookup ON valuation_cache_inputs(event_id,input_sha256,valuation_set_id);
        DROP TABLE conditional_price_rules; ALTER TABLE price_rules DROP COLUMN request_conditional; DELETE FROM schema_migrations WHERE version>=13; UPDATE app_state SET schema_version=12; PRAGMA user_version=12;")?; Ok(()) }).unwrap();
    drop(db);
    let db = Database::open(directory.path()).unwrap();
    db.snapshot(|tx,r| { assert_eq!((r.data,r.price,
        tx.query_row("SELECT valuation_set_id,event_id,rule_id,currency,cost_atoms,status FROM event_valuations",[],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?)))?,
        tx.query_row("SELECT valuation_set_id,event_id,input_sha256 FROM valuation_cache_inputs",[],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))?),before);
        assert!(!tx.prepare("PRAGMA foreign_key_check")?.exists([])?);
        assert_eq!(cost(cached(tx,r.price,&PriceBasis::EventTime {})?.unwrap()),"900"); Ok(()) }).unwrap();
}
fn replace(db: &Database) {
    let id = db.price_rules().unwrap().rules[0].rule_id.clone();
    db.mutate_price_rule(
        PriceRuleMutation::Replace {
            rule_id: id,
            draft: draft(20),
        },
        1,
        2,
    )
    .unwrap();
}

#[test]
fn write_price_publication_retains_old_read_snapshot_and_exact_reopened_fee_cache() {
    use token_pulse_core::pricing::{PriceOutcome, UnpricedCode};
    let (directory, db) = priced();
    // Only this isolated synthetic fixture changes; production logs/DB are untouched.
    db.write(|conn| {
        conn.execute(
            "UPDATE usage_events SET cache_write_input_tokens=20 WHERE event_id='event'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let before = db
        .snapshot(|tx, r| {
            Ok((
                r.data,
                tx.query_row(
                    "SELECT committed_offset,checkpoint_revision FROM file_generations",
                    [],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
                )?,
            ))
        })
        .unwrap();
    let unpriced = || PriceOutcome::Unpriced {
        reason: UnpricedCode::InsufficientUsage,
    };
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 2)
        .unwrap();
    let old_id = db.price_rules().unwrap().rules[0].rule_id.clone();
    db.snapshot(|tx, r| {
        assert_eq!(
            serde_json::to_value(cached(tx, r.price, &PriceBasis::EventTime {})?.unwrap())?,
            serde_json::to_value(unpriced())?
        );
        let mut replacement = draft(10);
        replacement.cache_write_rate_atoms = Some(DecimalInt::parse("30")?);
        db.mutate_price_rule(
            PriceRuleMutation::Replace {
                rule_id: old_id,
                draft: replacement,
            },
            1,
            3,
        )?;
        db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 4)?;
        assert_eq!(
            crate::pricing::rules_at(tx, 1)?.rules[0].cache_write_rate_atoms,
            None
        );
        assert_eq!(
            serde_json::to_value(cached(tx, 1, &PriceBasis::EventTime {})?.unwrap())?,
            serde_json::to_value(unpriced())?
        );
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, r| {
        assert_eq!(r.price, 2);
        assert_eq!(
            cost(cached(tx, 2, &PriceBasis::EventTime {})?.unwrap()),
            "1300"
        );
        assert_eq!(
            crate::pricing::rules_at(tx, 2)?.rules[0]
                .cache_write_rate_atoms
                .as_ref()
                .unwrap()
                .as_str(),
            "30"
        );
        assert_eq!(
            (
                r.data,
                tx.query_row(
                    "SELECT committed_offset,checkpoint_revision FROM file_generations",
                    [],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
                )?
            ),
            before
        );
        Ok(())
    })
    .unwrap();
    drop(db);
    let db = Database::open(directory.path()).unwrap();
    assert_eq!(summary_cost(&db), "0.000000000001300");
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "110"
    );
    // Emulate a ready v2 cache under its real version-specific ID. It cannot
    // satisfy the current lookup or prevent automatic construction of its version-specific set.
    db.write(|conn| {
        let tx = conn.transaction()?;
        let old_input = input(&tx, "ledger", 2, &PriceBasis::EventTime {})?;
        let current = old_input.id()?;
        let legacy = format!("valuation:{:x}", Sha256::digest(serde_json::to_vec(&(2_i64, &old_input))?));
        tx.execute("INSERT INTO valuation_sets SELECT ?1,price_revision,mode,specified_at_ms,state,created_at_ms FROM valuation_sets WHERE valuation_set_id=?2", params![legacy,current])?;
        tx.execute("INSERT INTO event_valuations SELECT ?1,event_id,rule_id,currency,'999999',status FROM event_valuations WHERE valuation_set_id=?2", params![legacy,current])?;
        tx.execute("INSERT INTO valuation_cache_inputs SELECT ?1,event_id,input_sha256 FROM valuation_cache_inputs WHERE valuation_set_id=?2", params![legacy,current])?;
        tx.execute("INSERT INTO valuation_cache_sets SELECT ?1,ledger_id,evidence_revision,2,parser_version,accounting_version,event_count,content_sha256,published_at_ms FROM valuation_cache_sets WHERE valuation_set_id=?2", params![legacy,current])?;
        tx.execute("DELETE FROM event_valuations WHERE valuation_set_id=?1", [&current])?;
        tx.execute("DELETE FROM valuation_sets WHERE valuation_set_id=?1", [current])?;
        tx.commit()?;
        Ok(())
    })
    .unwrap();
    assert_eq!(summary_cost(&db), "0.000000000001300");
    db.snapshot(|tx, _| {
        assert!(cached(tx, 2, &PriceBasis::EventTime {})?.is_none());
        Ok(())
    })
    .unwrap();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 5)
        .unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(
            cost(cached(tx, 2, &PriceBasis::EventTime {})?.unwrap()),
            "1300"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn pinned_revision_survives_price_publication_during_build() {
    let (_directory, db) = priced();
    let stop = Arc::new(AtomicBool::new(false));
    let result = db
        .build_event_valuation_at_revision_interruptible(
            "ledger",
            &PriceBasis::EventTime {},
            1,
            3,
            &stop,
            |done, _| {
                if done == 0 {
                    replace(&db);
                }
            },
        )
        .unwrap();
    assert_eq!(result.price_revision, 1);
    db.snapshot(|tx, r| {
        assert_eq!(r.price, 2);
        assert_eq!(
            cost(cached(tx, 1, &PriceBasis::EventTime {})?.unwrap()),
            "900"
        );
        assert!(cached(tx, 2, &PriceBasis::EventTime {})?.is_none());
        Ok(())
    })
    .unwrap();
    assert_eq!(summary_cost(&db), "0.000000000001300");
}

#[test]
fn precise_cache_reopens_and_does_not_change_consumption_or_checkpoint() {
    let (directory, db) = priced();
    let before=db.snapshot(|tx,r|Ok((r.data,r.price,r.settings,tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?))).unwrap();
    let result = db
        .build_event_valuation("ledger", &PriceBasis::EventTime {}, 3)
        .unwrap();
    assert_eq!(result.event_count, 1);
    assert!(!result.already_ready);
    db.snapshot(|tx,r| {assert_eq!(cost(cached(tx,r.price,&PriceBasis::EventTime {})?.unwrap()),"900"); // 40*10 + 60*5 + 10*20
        assert_eq!((r.data,r.price,r.settings),(before.0,before.1,before.2));assert_eq!(tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?,before.3);Ok(())}).unwrap();
    assert_eq!(summary_cost(&db), "0.000000000000900");
    assert!(
        db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 4)
            .unwrap()
            .already_ready
    );
    let page = db
        .query_usage_events(
            "main",
            &UsageEventsRequest {
                query: UsageEventsQuery {
                    filter: filter(),
                    price_basis: PriceBasis::EventTime {},
                    sort: UsageEventSort::TimeDesc,
                    page_size: 20,
                },
                cursor: None,
            },
            EpochMs::new(1234).unwrap(),
        )
        .unwrap();
    assert_eq!(cost(page.events[0].price.clone()), "900");
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "110"
    );
    drop(db);
    let reopened = Database::open(directory.path()).unwrap();
    reopened
        .snapshot(|tx, r| {
            assert_eq!(
                cost(cached(tx, r.price, &PriceBasis::EventTime {})?.unwrap()),
                "900"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn mode_and_exact_time_are_separate_and_unpriced_amounts_remain_null() {
    let (_dir, db) = priced();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 3)
        .unwrap();
    let earlier = PriceBasis::SpecifiedTime {
        specified_at_ms: EpochMs::new(-1).unwrap(),
    };
    db.snapshot(|tx, r| {
        assert!(cached(tx, r.price, &earlier)?.is_none());
        Ok(())
    })
    .unwrap();
    let result = db.build_event_valuation("ledger", &earlier, 4).unwrap();
    db.snapshot(|tx,r| {assert!(matches!(cached(tx,r.price,&earlier)?.unwrap(),PriceOutcome::Unpriced {reason:UnpricedCode::MissingRule}));
        assert_eq!(tx.query_row("SELECT rule_id,currency,cost_atoms FROM event_valuations WHERE valuation_set_id=?1",[result.set_id],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<String>>(2)?)))?,(None,None,None));
        assert!(cached(tx,r.price,&PriceBasis::SpecifiedTime {specified_at_ms:EpochMs::new(1500)?})?.is_none());Ok(())}).unwrap();
}
#[test]
fn old_real_snapshot_keeps_ready_cost_after_new_prices_and_changed_inputs() {
    let (_dir, db) = priced();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 3)
        .unwrap();
    db.snapshot(|tx, r| {
        assert_eq!(
            cost(cached(tx, r.price, &PriceBasis::EventTime {})?.unwrap()),
            "900"
        );
        replace(&db);
        db.write(|conn| {
            conn.execute(
                "UPDATE usage_events SET cached_input_tokens=40 WHERE event_id='event'",
                [],
            )?;
            Ok(())
        })?;
        assert_eq!(
            cost(cached(tx, r.price, &PriceBasis::EventTime {})?.unwrap()),
            "900"
        );
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, r| {
        assert!(cached(tx, r.price, &PriceBasis::EventTime {})?.is_none());
        Ok(())
    })
    .unwrap();
    assert_eq!(summary_cost(&db), "0.000000000001600"); // 60*20 + 40*5 + 10*20
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 4)
        .unwrap();
    db.snapshot(|tx, r| {
        assert_eq!(
            cost(cached(tx, r.price, &PriceBasis::EventTime {})?.unwrap()),
            "1600"
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn mirror_evidence_invalidates_cache_and_filters_keep_actual_source_ambiguity() {
    let (_dir, db) = priced();
    db.add_source(crate::SourceRecord {
        source_id: "mirror".into(),
        root_path: "synthetic-mirror".into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    let mut specific = draft(20);
    specific.source_id = Some("source".into());
    db.mutate_price_rule(
        PriceRuleMutation::Create {
            draft: specific.clone(),
        },
        1,
        1,
    )
    .unwrap();
    specific.source_id = Some("mirror".into());
    db.mutate_price_rule(PriceRuleMutation::Create { draft: specific }, 2, 1)
        .unwrap();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 2)
        .unwrap();
    db.snapshot(|tx, r| {
        assert_eq!(
            cost(cached(tx, r.price, &PriceBasis::EventTime {})?.unwrap()),
            "1300"
        );
        Ok(())
    })
    .unwrap();
    db.write(|conn| {
        conn.execute("INSERT INTO source_files(file_id,source_id,canonical_path,status) VALUES('mirror-file','mirror','synthetic-mirror.jsonl','known')",[])?;
        conn.execute("INSERT INTO file_generations SELECT 'mirror-generation','mirror-file',state,identity_json,observed_size,committed_offset,checkpoint_revision,anchor_json,reader_context_json,parser_version,created_at_ms FROM file_generations WHERE file_generation_id='generation'",[])?;
        conn.execute("INSERT INTO observations SELECT 'copy','mirror-generation',100,200,session_key,kind,observed_at_ms,stable_record_id,turn_id,stream_hint,model,project_id,normalized_json,payload_fingerprint,format_version FROM observations WHERE observation_id='observation'",[])?;conn.execute("INSERT INTO event_provenance VALUES('event','copy','mirror')",[])?;Ok(())}).unwrap();
    db.snapshot(|tx, r| {
        assert!(cached(tx, r.price, &PriceBasis::EventTime {})?.is_none());
        Ok(())
    })
    .unwrap();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 3)
        .unwrap();
    for selected in ["source", "mirror"] {
        let mut f = filter();
        f.sources = ids(&[selected], false);
        let result = db.pricing_summary(&f, &PriceBasis::EventTime {}).unwrap();
        assert!(result.currencies.is_empty());
        assert_eq!(result.reasons[0].code, "ambiguous_rule");
        assert_eq!(result.reasons[0].total_tokens.as_str(), "110");
    }
}
#[test]
fn batch_boundary_cancellation_never_exposes_partial_estimates() {
    let (_dir, db) = priced();
    // Cross 500-row transactions. This is a functional fixture, not a benchmark.
    for i in 0..600 {
        extra(
            &db,
            &format!("unknown-{i:04}"),
            2000,
            7,
            (None, None),
            None,
            None,
        );
    }
    let mut progress = Vec::new();
    let stop = Arc::new(AtomicBool::new(false));
    let old = db
        .build_event_valuation_interruptible(
            "ledger",
            &PriceBasis::EventTime {},
            3,
            &stop,
            |done, total| progress.push((done, total)),
        )
        .unwrap();
    assert_eq!(progress, [(0, 601), (500, 601), (601, 601)]);
    assert_eq!(old.event_count, 601);
    replace(&db);
    let error = db
        .build_event_valuation_interruptible(
            "ledger",
            &PriceBasis::EventTime {},
            4,
            &stop,
            |done, _| {
                if done == 500 {
                    stop.store(true, Ordering::Release)
                }
            },
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::JobCancelled);
    db.snapshot(|tx, r| {
        assert!(cached(tx, r.price, &PriceBasis::EventTime {})?.is_none());
        assert_eq!(
            cost(cached(tx, 1, &PriceBasis::EventTime {})?.unwrap()),
            "900"
        );
        assert_eq!(
            tx.query_row(
                "SELECT state FROM valuation_sets WHERE price_revision=2",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "failed"
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(summary_cost(&db), "0.000000000001300");
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "4310"
    );
    stop.store(false, Ordering::Release);
    assert_eq!(
        db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 5)
            .unwrap()
            .event_count,
        601
    );
}
#[test]
fn changed_facts_discard_candidate_and_new_events_keep_live_estimates() {
    let (_dir, db) = priced();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 3)
        .unwrap();
    replace(&db);
    let stop = Arc::new(AtomicBool::new(false));
    let error=db.build_event_valuation_interruptible("ledger",&PriceBasis::EventTime {},4,&stop,|done,_|if done==0 {db.write(|conn| {conn.execute("UPDATE usage_events SET output_tokens_total=20,total_tokens=120,source_total_tokens=120 WHERE event_id='event'",[])?;Ok(())}).unwrap();}).unwrap_err();
    assert_eq!(error.code, ErrorCode::CandidateObsolete);
    assert_eq!(summary_cost(&db), "0.000000000001500"); // 40*20 + 60*5 + 20*20
    extra(&db, "new-unpriced", 2100, 7, (None, None), None, None);
    let s = db
        .pricing_summary(&filter(), &PriceBasis::EventTime {})
        .unwrap();
    assert_eq!(s.priced_total_tokens.as_str(), "120");
    assert_eq!(s.reasons[0].total_tokens.as_str(), "7");
}
#[test]
fn failed_publication_preserves_old_ready_set_and_consumption() {
    let (_dir, db) = priced();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 3)
        .unwrap();
    replace(&db);
    db.write(|conn| {conn.execute_batch("CREATE TRIGGER fail_valuation_publish BEFORE UPDATE OF state ON valuation_sets WHEN NEW.state='ready' BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;Ok(())}).unwrap();
    assert!(
        db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 4)
            .is_err()
    );
    db.snapshot(|tx,r| {assert!(cached(tx,r.price,&PriceBasis::EventTime {})?.is_none());assert_eq!(cost(cached(tx,1,&PriceBasis::EventTime {})?.unwrap()),"900");assert_eq!(tx.query_row("SELECT content_sha256,published_at_ms FROM valuation_cache_sets cs JOIN valuation_sets vs USING(valuation_set_id) WHERE vs.price_revision=2",[],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,Option<i64>>(1)?)))?,(None,None));assert_eq!(r.data,1);Ok(())}).unwrap();
    assert_eq!(summary_cost(&db), "0.000000000001300");
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "110"
    );
}

#[test]
fn cache_keeps_integer_and_amount_precision_beyond_javascript_safe_integer() {
    let (_dir, db) = priced();
    db.write(|conn| {
        conn.execute("UPDATE usage_events SET input_tokens_total=9007199254740993,cached_input_tokens=0,output_tokens_total=0,reasoning_output_tokens=0,total_tokens=9007199254740993,source_total_tokens=9007199254740993 WHERE event_id='event'",[])?;
        Ok(())
    }).unwrap();
    let built = db
        .build_event_valuation("ledger", &PriceBasis::EventTime {}, 3)
        .unwrap();
    db.snapshot(|tx,r| {
        assert_eq!(cost(cached(tx,r.price,&PriceBasis::EventTime {})?.unwrap()),"90071992547409930");
        assert_eq!(tx.query_row("SELECT typeof(cost_atoms),cost_atoms FROM event_valuations WHERE valuation_set_id=?1",[built.set_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?,("text".into(),"90071992547409930".into()));
        Ok(())
    }).unwrap();
    assert_eq!(summary_cost(&db), "90.071992547409930");
}

#[test]
fn event_lookup_work_does_not_grow_with_unrelated_ready_history() {
    let (_directory, db) = priced();
    let built = db
        .build_event_valuation("ledger", &PriceBasis::EventTime {}, 2)
        .unwrap();
    let id = built.set_id;
    db.write(move |conn| {
        let tx = conn.transaction()?;
        for n in 0..1000 {
            let unrelated = format!("unrelated-history-{n}");
            tx.execute("INSERT INTO valuation_sets SELECT ?1,price_revision,mode,specified_at_ms,state,created_at_ms FROM valuation_sets WHERE valuation_set_id=?2", params![unrelated,id])?;
            tx.execute("INSERT INTO valuation_cache_sets SELECT ?1,ledger_id,evidence_revision,cache_version,parser_version,accounting_version,event_count,content_sha256,published_at_ms FROM valuation_cache_sets WHERE valuation_set_id=?2", params![unrelated,id])?;
        }
        tx.commit()?; Ok(())
    }).unwrap();
    assert_eq!(summary_cost(&db), "0.000000000000900");
    db.snapshot(|tx, revision| {
        let fingerprint: String = tx.query_row(
            "SELECT input_sha256 FROM valuation_cache_inputs WHERE event_id='event' LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        let mut reader = CacheReader::new(
            tx,
            &DecimalInt::from_nonnegative(revision.price.into())?,
            &PriceBasis::EventTime {},
        )?;
        assert!(reader.lookup("event", &fingerprint, "ledger")?.is_none());
        // SQLite VM work is deterministic; a wall-clock threshold would be flaky.
        // Starting at all 1,001 ready sets violates this bound by several orders of magnitude.
        let statement = reader.statement.as_ref().unwrap();
        assert!(statement.get_status(rusqlite::StatementStatus::VmStep) < 500);
        assert!(reader.lookup("event", &"0".repeat(64), "ledger")?.is_none());
        Ok(())
    })
    .unwrap();
}
