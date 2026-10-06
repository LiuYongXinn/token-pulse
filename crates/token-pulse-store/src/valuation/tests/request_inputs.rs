//! Request evidence is a cache input even before actual response modes are available.
use super::*;
use token_pulse_core::{
    domain::{PhysicalPosition, RequestUsageEvidence, UsageVector},
    pricing::request::RequestConsumptionBinding,
};

fn evidence(response: &str, input: i64) -> serde_json::Value {
    let usage = UsageVector {
        input_total: Some(input),
        cached_input: Some(60),
        output_total: Some(10),
        reasoning_output: Some(2),
        reported_total: Some(input + 10),
        ..Default::default()
    };
    serde_json::json!({
        "evidence": RequestUsageEvidence {
            response_id: response.into(),
            turn_id: "turn".into(),
            usage,
            thread_usage: usage,
            physical_position: PhysicalPosition {
                file_generation_id: "generation".into(), byte_offset: 0, byte_end: 10,
            },
        },
        "position": {"file_generation_id":"generation", "byte_offset":10, "byte_end":100},
        "turn_id":"turn", "last":usage, "cumulative":usage,
    })
}
fn event(sources: &[String]) -> PricingEvent<'_> {
    PricingEvent {
        provider: Some("P"),
        model: Some("M"),
        source_ids: sources,
        occurred_at_ms: EpochMs::new(1000).unwrap(),
        usage: UsageVector {
            input_total: Some(100),
            cached_input: Some(60),
            output_total: Some(10),
            reasoning_output: Some(2),
            reported_total: Some(110),
            ..Default::default()
        },
    }
}
fn request(
    value: &serde_json::Value,
    consumption: UsageVector,
) -> Option<crate::query::request_input::PricingRequestInput> {
    crate::query::request_input::pricing(Some(&value.to_string()), consumption)
}

#[test]
fn request_cache_identity_separates_unknown_length_binding_and_response_identity() {
    let sources = vec!["source".into()];
    let event = event(&sources);
    let unknown = fingerprint(&event, "v1", None).unwrap();
    let value = evidence("response-a", 100);
    let full = request(&value, event.usage).unwrap();
    assert_eq!(full.input.binding, RequestConsumptionBinding::FullRequest);
    let full_key = fingerprint(&event, "v1", Some(&full)).unwrap();
    assert_ne!(unknown, full_key);
    let mut different = event.usage;
    different.reported_total = None;
    let partial = request(&value, different).unwrap();
    assert_eq!(
        partial.input.binding,
        RequestConsumptionBinding::DifferentConsumption
    );
    assert_ne!(full_key, fingerprint(&event, "v1", Some(&partial)).unwrap());
    for value in [
        evidence("response-b", 100),
        evidence("response-a", 272000),
        evidence("response-a", 272001),
        {
            let mut moved = value.clone();
            moved["evidence"]["physical_position"]["byte_offset"] = 1.into();
            moved
        },
        {
            let mut other_turn = value.clone();
            other_turn["evidence"]["turn_id"] = "other".into();
            other_turn["turn_id"] = "other".into();
            other_turn
        },
    ] {
        let projected = request(&value, event.usage).unwrap();
        assert_ne!(
            full_key,
            fingerprint(&event, "v1", Some(&projected)).unwrap()
        );
    }
    for malformed in [
        serde_json::Value::Null,
        serde_json::json!({}),
        {
            let mut stale = value.clone();
            stale["evidence"]["response_id"] = "".into();
            stale
        },
        {
            let mut stale = value.clone();
            stale["position"]["file_generation_id"] = "other".into();
            stale
        },
    ] {
        assert!(request(&malformed, event.usage).is_none());
    }
    let duplicates = vec!["source".into(), "source".into()];
    let reversed = PricingEvent {
        source_ids: &duplicates,
        ..event
    };
    assert_eq!(full_key, fingerprint(&reversed, "v1", Some(&full)).unwrap());
}

fn attach(db: &Database, value: serde_json::Value, source_total: Option<i64>) {
    db.write(move |conn| {
        let evidence = value["evidence"].to_string();
        let last = value["last"].to_string();
        conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.request_usage',json(?1),'$.physical_position',json(?2),'$.effective_metadata.turn_id','turn','$.last',json(?3),'$.cumulative',json(?3)) WHERE observation_id='observation'",params![evidence, value["position"].to_string(), last])?;
        conn.execute("UPDATE usage_events SET source_total_tokens=?1 WHERE event_id='event'",[source_total])?;
        conn.execute("UPDATE ledger_usage_versions SET revision=revision+1 WHERE ledger_id='ledger'",[])?;
        Ok(())
    }).unwrap();
}

#[test]
fn new_request_evidence_misses_old_cache_and_publishes_identical_summary_and_page_inputs() {
    for source_total in [Some(110), None] {
        let (dir, db) = priced();
        let old = db
            .build_event_valuation("ledger", &PriceBasis::EventTime {}, 3)
            .unwrap();
        let before = db.snapshot(|tx, r| Ok((r.data, r.price, tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?))).unwrap();
        attach(&db, evidence("response-a", 100), source_total);
        db.snapshot(|tx, r| {
            assert!(cached(tx, r.price, &PriceBasis::EventTime {})?.is_none());
            Ok(())
        })
        .unwrap();
        let built = db
            .build_event_valuation("ledger", &PriceBasis::EventTime {}, 4)
            .unwrap();
        assert_ne!(old.set_id, built.set_id);
        db.snapshot(|tx, r| {
            assert_eq!(cost(cached(tx, r.price, &PriceBasis::EventTime {})?.unwrap()), "900");
            assert_eq!(tx.query_row("SELECT COUNT(DISTINCT input_sha256) FROM valuation_cache_inputs WHERE event_id='event'",[],|row|row.get::<_,i64>(0))?, 2);
            Ok(())
        }).unwrap();
        // Poison just the disposable current estimate: page + aggregate must both find
        // exactly the same input identity, including a nullable original source total.
        let id = built.set_id.clone();
        db.write(move |conn| {
            conn.execute(
                "UPDATE event_valuations SET cost_atoms='12345' WHERE valuation_set_id=?1",
                [id],
            )?;
            Ok(())
        })
        .unwrap();
        let page = db
            .query_usage_events(
                "main",
                &UsageEventsRequest {
                    query: UsageEventsQuery {
                        filter: filter(),
                        price_basis: PriceBasis::EventTime {},
                        sort: UsageEventSort::TimeDesc,
                        page_size: 10,
                    },
                    cursor: None,
                },
                EpochMs::new(5).unwrap(),
            )
            .unwrap();
        assert_eq!(cost(page.events[0].price.clone()), "12345");
        assert_eq!(
            page.pricing.currencies[0]
                .estimated_cost
                .as_ref()
                .unwrap()
                .as_str(),
            "0.000000000012345"
        );
        assert_eq!(
            page.events[0].request_input.as_ref().unwrap().binding,
            if source_total.is_some() {
                RequestConsumptionBinding::FullRequest
            } else {
                RequestConsumptionBinding::DifferentConsumption
            }
        );
        let encoded = serde_json::to_string(&page).unwrap();
        assert!(!encoded.contains("response-a") && !encoded.contains("byte_offset"));
        db.snapshot(|tx, r| {assert_eq!((r.data,r.price,tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?),before); Ok(())}).unwrap();
        drop(db);
        let db = Database::open(dir.path()).unwrap();
        db.snapshot(|tx, r| {
            assert_eq!(
                cost(cached(tx, r.price, &PriceBasis::EventTime {})?.unwrap()),
                "12345"
            );
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn v4_and_v5_ready_sets_cannot_supply_current_input_and_legacy_rows_are_retained() {
    for legacy_version in [4, 5] {
        let (_dir, db) = priced();
        let built = db
            .build_event_valuation("ledger", &PriceBasis::EventTime {}, 3)
            .unwrap();
        let id = built.set_id.clone();
        db.write(move |conn| {
        let tx=conn.transaction()?;
        tx.execute("INSERT INTO valuation_sets SELECT 'legacy-v4',price_revision,mode,specified_at_ms,state,created_at_ms FROM valuation_sets WHERE valuation_set_id=?1",[&id])?;
        tx.execute("INSERT INTO valuation_cache_sets SELECT 'legacy-v4',ledger_id,evidence_revision,?2,parser_version,accounting_version,event_count,content_sha256,published_at_ms FROM valuation_cache_sets WHERE valuation_set_id=?1",params![id,legacy_version])?;
        tx.execute("INSERT INTO event_valuations SELECT 'legacy-v4',event_id,rule_id,currency,'999999',status FROM event_valuations WHERE valuation_set_id=?1",[&id])?;
        tx.execute("INSERT INTO valuation_cache_inputs SELECT 'legacy-v4',event_id,input_sha256 FROM valuation_cache_inputs WHERE valuation_set_id=?1",[&id])?;
        tx.execute("DELETE FROM event_valuations WHERE valuation_set_id=?1",[&id])?;
        tx.execute("DELETE FROM valuation_sets WHERE valuation_set_id=?1",[id])?;
        tx.commit()?;
        Ok(())
    }).unwrap();
        db.snapshot(|tx, r| {
            assert!(cached(tx, r.price, &PriceBasis::EventTime {})?.is_none());
            Ok(())
        })
        .unwrap();
        assert_eq!(summary_cost(&db), "0.000000000000900");
        db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 4)
            .unwrap();
        db.snapshot(|tx, r| {
            assert_eq!(
                cost(cached(tx, r.price, &PriceBasis::EventTime {})?.unwrap()),
                "900"
            );
            assert_eq!(
                tx.query_row(
                    "SELECT cost_atoms FROM event_valuations WHERE valuation_set_id='legacy-v4'",
                    [],
                    |r| r.get::<_, String>(0)
                )?,
                "999999"
            );
            Ok(())
        })
        .unwrap();
    }
}
