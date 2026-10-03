use super::*;
use crate::batch::tests::{fixture, setup};
use crate::query::tests::{extra, filter, ids};
use token_pulse_core::{
    pricing::{PriceRuleDraft, PriceRuleMutation},
    query::{UsageEventSort, UsageEventsQuery, UsageEventsRequest},
};

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
    // Emulate a ready v1 cache under its real version-specific ID. It cannot
    // satisfy v2 lookup or prevent the automatic builder from creating a v2 set.
    db.write(|conn| {
        let tx = conn.transaction()?;
        let old_input = input(&tx, "ledger", 2, &PriceBasis::EventTime {})?;
        let current = old_input.id()?;
        let legacy = format!("valuation:{:x}", Sha256::digest(serde_json::to_vec(&(1_i64, &old_input))?));
        tx.execute("INSERT INTO valuation_sets SELECT ?1,price_revision,mode,specified_at_ms,state,created_at_ms FROM valuation_sets WHERE valuation_set_id=?2", params![legacy,current])?;
        tx.execute("INSERT INTO event_valuations SELECT ?1,event_id,rule_id,currency,'999999',status FROM event_valuations WHERE valuation_set_id=?2", params![legacy,current])?;
        tx.execute("INSERT INTO valuation_cache_inputs SELECT ?1,event_id,input_sha256 FROM valuation_cache_inputs WHERE valuation_set_id=?2", params![legacy,current])?;
        tx.execute("INSERT INTO valuation_cache_sets SELECT ?1,ledger_id,evidence_revision,1,parser_version,accounting_version,event_count,content_sha256,published_at_ms FROM valuation_cache_sets WHERE valuation_set_id=?2", params![legacy,current])?;
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
