use super::*;
use crate::{
    SourceRecord,
    batch::tests::{fixture, setup},
    query::tests::{extra, filter, ids},
};
use rusqlite::params;
use token_pulse_core::{
    pricing::{PriceOutcome, PriceRuleDraft, PriceRuleMutation},
    protocol::PriceBasis,
};

fn request(sort: UsageEventSort, size: u16) -> UsageEventsRequest {
    UsageEventsRequest {
        query: UsageEventsQuery {
            filter: filter(),
            price_basis: PriceBasis::EventTime {},
            sort,
            page_size: size,
        },
        cursor: None,
    }
}
fn fetch(db: &Database, request: &UsageEventsRequest) -> UsageEventsPage {
    db.query_usage_events("main", request, EpochMs::new(1234).unwrap())
        .unwrap()
}
fn install(db: &Database) {
    db.mutate_price_rule(
        PriceRuleMutation::Create {
            draft: PriceRuleDraft {
                provider: "P".into(),
                model_exact: "M".into(),
                source_id: None,
                currency: "USD".into(),
                effective_from_ms: EpochMs::new(0).unwrap(),
                effective_to_ms: None,
                priority: 0,
                input_rate_atoms: DecimalInt::from_nonnegative(1_000_000_000).unwrap(),
                cache_write_rate_atoms: None,
                cached_rate_atoms: Some(DecimalInt::from_nonnegative(1_000_000_000).unwrap()),
                output_rate_atoms: DecimalInt::from_nonnegative(1_000_000_000).unwrap(),
                origin_reference: Some("synthetic only".into()),
            },
        },
        0,
        1,
    )
    .unwrap();
}

#[test]
fn exact_request_input_is_snapshot_consistent_and_distinct_from_consumption_and_cumulative() {
    use token_pulse_core::{
        domain::{NormalizedObservation, PhysicalPosition, RequestUsageEvidence},
        pricing::request::RequestConsumptionBinding,
    };
    for full in [true, false] {
        let (_dir, db) = setup();
        let mut batch = fixture();
        let NormalizedObservation::Usage(observed) = &mut batch.observations[0].record else {
            panic!("fixture usage")
        };
        observed.physical_position.byte_offset = 10;
        observed.effective_metadata.turn_id = Some("turn".into());
        observed.effective_metadata.cwd =
            Some("E:\\private-synthetic-parent\\VisibleProject".into());
        let response_usage = observed.last.unwrap();
        let cumulative = UsageVector {
            input_total: Some(500000),
            reported_total: Some(500010),
            ..response_usage
        };
        observed.cumulative = Some(cumulative);
        observed.model_context_window = Some(1000000);
        observed.request_usage = Some(Box::new(RequestUsageEvidence {
            response_id: "response-internal-only".into(),
            turn_id: "turn".into(),
            usage: response_usage,
            thread_usage: cumulative,
            physical_position: PhysicalPosition {
                file_generation_id: "generation".into(),
                byte_offset: 0,
                byte_end: 10,
            },
        }));
        if !full {
            batch.events[0].usage = UsageVector {
                input_total: Some(20),
                cached_input: Some(5),
                output_total: Some(5),
                reasoning_output: Some(1),
                reported_total: Some(25),
                ..Default::default()
            };
        }
        db.commit(batch).unwrap();
        let page = fetch(&db, &request_for_test());
        let evidence = page.events[0].request_input.as_ref().unwrap();
        assert_eq!(evidence.input_tokens.as_str(), "100");
        assert_eq!(
            evidence.binding,
            if full {
                RequestConsumptionBinding::FullRequest
            } else {
                RequestConsumptionBinding::DifferentConsumption
            }
        );
        assert_eq!(
            page.summary.total_tokens.as_str(),
            if full { "110" } else { "25" }
        );
        let encoded = serde_json::to_string(&page).unwrap();
        assert!(
            !encoded.contains("response-internal-only")
                && !encoded.contains("private-synthetic-parent")
        );
        extra(&db, "newer-zero", 2000, 0, (None, None), None, None);
        let mut continuation = request(UsageEventSort::TimeDesc, 1);
        let first = fetch(&db, &continuation);
        assert!(first.events[0].request_input.is_none());
        continuation.cursor = first.next_cursor;
        db.write(|connection| {connection.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.request_usage',NULL) WHERE observation_id='observation'",[])?;Ok(())}).unwrap();
        let old_page = fetch(&db, &continuation);
        assert_eq!(
            old_page.events[0].request_input.as_ref().unwrap().binding,
            evidence.binding
        );
        assert_eq!(old_page.meta.data_revision, first.meta.data_revision);
        // Optional malformed/stale evidence must not invalidate independently published Tokens.
        for value in ["null", "{}", "{\"response_id\":\"bad\"}"] {
            let json = value.to_owned();
            db.write(move|connection| {connection.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.request_usage',json(?1)) WHERE observation_id='observation'",[json])?;Ok(())}).unwrap();
            let page = fetch(&db, &request_for_test());
            assert!(page.events[0].request_input.is_none());
            assert_eq!(
                page.summary.total_tokens.as_str(),
                if full { "110" } else { "25" }
            );
        }
    }
}
fn request_for_test() -> UsageEventsRequest {
    request(UsageEventSort::TimeDesc, 200)
}

#[test]
fn known_cache_write_pricing_and_persistent_fee_cache_do_not_charge_ordinary_input_twice() {
    use token_pulse_core::{domain::NormalizedObservation, pricing::UnpricedCode};
    for writes in [0, 20] {
        let (_dir, db) = setup();
        install(&db);
        let mut b = fixture();
        b.events[0].model = Some("M".into());
        b.events[0].usage.cache_write_input = Some(writes);
        b.streams[0].baseline.cache_write_input = Some(writes);
        if let NormalizedObservation::Usage(u) = &mut b.observations[0].record {
            u.effective_metadata.provider = Some("P".into());
            u.effective_metadata.model = Some("M".into());
            u.last.as_mut().unwrap().cache_write_input = Some(writes);
            u.cumulative.as_mut().unwrap().cache_write_input = Some(writes);
        }
        db.commit(b).unwrap();
        let req = request(UsageEventSort::TimeDesc, 50);
        let before = fetch(&db, &req);
        db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 2000)
            .unwrap();
        let cached = fetch(&db, &req);
        for page in [before, cached] {
            assert_eq!(page.summary.total_tokens.as_str(), "110");
            assert_eq!(
                page.summary
                    .cache_write_input
                    .value
                    .as_ref()
                    .unwrap()
                    .as_str(),
                writes.to_string()
            );
            if writes == 0 {
                assert!(
                    matches!(&page.events[0].price, PriceOutcome::Priced { cost_atoms, .. } if cost_atoms.as_str()=="110000000000")
                );
                assert_eq!(page.pricing.unpriced_total_tokens.as_str(), "0");
            } else {
                assert!(matches!(
                    page.events[0].price,
                    PriceOutcome::Unpriced {
                        reason: UnpricedCode::InsufficientUsage
                    }
                ));
                assert_eq!(page.pricing.unpriced_total_tokens.as_str(), "110");
                assert!(page.pricing.currencies.is_empty());
            }
        }
    }
}

#[test]
fn event_pages_preserve_exact_integer_order_and_source_vectors_without_private_json() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(
        &db,
        "a",
        2000,
        9007199254740992,
        (Some("M"), Some("P")),
        None,
        None,
    );
    extra(
        &db,
        "b",
        2000,
        9007199254740993,
        (Some("M"), Some("P")),
        None,
        None,
    );
    extra(&db, "max", 3000, i64::MAX, (None, None), None, None);
    db.write(|conn| { conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.last.input_total',-1,'$.private_synthetic_field','DO_NOT_EXPOSE_SYNTHETIC') WHERE observation_id='observation'", [])?; Ok(()) }).unwrap();
    let mut req = request(UsageEventSort::TotalDesc, 1);
    let first = fetch(&db, &req);
    assert_eq!(first.events[0].event_id, "max");
    assert_eq!(first.events[0].total_tokens.as_str(), "9223372036854775807");
    let mut keys = vec![first.events[0].event_id.clone()];
    req.cursor = first.next_cursor;
    while req.cursor.is_some() {
        let page = fetch(&db, &req);
        assert_eq!(
            serde_json::to_value(&page.meta).unwrap(),
            serde_json::to_value(&first.meta).unwrap()
        );
        keys.push(page.events[0].event_id.clone());
        if page.events[0].event_id == "event" {
            let event = &page.events[0];
            assert_eq!(event.usage.input_total.unwrap().value(), 100);
            assert_eq!(event.usage.cached_input.unwrap().value(), 60);
            assert_eq!(
                event
                    .raw_last
                    .as_ref()
                    .unwrap()
                    .input_total
                    .unwrap()
                    .value(),
                -1
            );
            assert_eq!(
                event
                    .raw_cumulative
                    .as_ref()
                    .unwrap()
                    .input_total
                    .unwrap()
                    .value(),
                100
            );
            assert_eq!(event.quality_flags, ["confirmed"]);
            assert_eq!(event.calculation_method, "last_new_stream");
            let json = serde_json::to_string(event).unwrap();
            assert!(!json.contains("DO_NOT_EXPOSE") && !json.contains("normalized_json"));
        }
        req.cursor = page.next_cursor;
    }
    assert_eq!(keys, ["max", "b", "a", "event"]);
    let time = fetch(&db, &request(UsageEventSort::TimeDesc, 200));
    assert_eq!(
        time.events
            .iter()
            .map(|e| e.event_id.as_str())
            .collect::<Vec<_>>(),
        ["max", "a", "b", "event"]
    );
    assert!(time.events[1].raw_last.is_none() && time.events[1].raw_cumulative.is_none());
    assert_eq!(time.summary.total_tokens.as_str(), "9241386435364257902");
}

#[test]
fn old_pages_keep_raw_vectors_price_rules_and_attribution_after_writer_publication() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(&db, "next", 2000, 20, (Some("M"), Some("P")), None, None);
    db.write(|conn| {
        conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.effective_metadata.provider','P') WHERE observation_id='observation'", [])?;
        conn.execute("UPDATE usage_events SET model='M' WHERE event_id='event'", [])?; Ok(())
    }).unwrap();
    let mut req = request(UsageEventSort::TimeDesc, 1);
    let first = fetch(&db, &req);
    req.cursor = first.next_cursor.clone();
    install(&db);
    db.write(|conn| {
        conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.cumulative.input_total',123) WHERE observation_id='observation'", [])?;
        conn.execute("UPDATE usage_events SET calculation_method='changed_synthetic_method' WHERE event_id='event'", [])?;
        conn.execute("UPDATE sessions SET provider_session_id='changed' WHERE session_key='session'", [])?;
        conn.execute("UPDATE app_state SET data_revision=data_revision+1", [])?; Ok(())
    }).unwrap();
    let old = fetch(&db, &req);
    assert_eq!(old.events[0].event_id, "event");
    assert_eq!(old.events[0].session_display_name, "provider-session");
    assert_eq!(old.events[0].calculation_method, "last_new_stream");
    assert_eq!(
        old.events[0]
            .raw_cumulative
            .as_ref()
            .unwrap()
            .input_total
            .unwrap()
            .value(),
        100
    );
    assert!(matches!(old.events[0].price, PriceOutcome::Unpriced { .. }));
    assert_eq!(old.meta.price_revision.as_str(), "0");
    assert_eq!(old.meta.data_revision, first.meta.data_revision);
    let fresh = fetch(&db, &request(UsageEventSort::TimeDesc, 200));
    let event = &fresh.events[1];
    assert_eq!(event.session_display_name, "changed");
    assert_eq!(
        event
            .raw_cumulative
            .as_ref()
            .unwrap()
            .input_total
            .unwrap()
            .value(),
        123
    );
    assert_eq!(event.calculation_method, "changed_synthetic_method");
    assert_eq!(fresh.meta.price_revision.as_str(), "1");
    match &event.price {
        PriceOutcome::Priced { estimated_cost, .. } => {
            assert_eq!(estimated_cost.as_str(), "0.000110000000000")
        }
        _ => panic!("expected priced event"),
    }
}

#[test]
fn mirrored_source_evidence_selects_one_fact_and_lists_distinct_sources() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.add_source(SourceRecord {
        source_id: "mirror".into(),
        root_path: "synthetic-mirror".into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    db.write(|conn| {
        conn.execute("INSERT INTO source_files(file_id,source_id,canonical_path,status) VALUES('mf','mirror','synthetic-mirror.jsonl','known')", [])?;
        conn.execute("INSERT INTO file_generations SELECT 'mg','mf',state,identity_json,observed_size,committed_offset,checkpoint_revision,anchor_json,reader_context_json,parser_version,created_at_ms FROM file_generations WHERE file_generation_id='generation'", [])?;
        for index in 1..=2 {
            conn.execute("INSERT INTO observations SELECT ?1,'mg',?2,?3,session_key,kind,observed_at_ms,stable_record_id,turn_id,stream_hint,model,project_id,normalized_json,payload_fingerprint,format_version FROM observations WHERE observation_id='observation'", params![format!("copy-{index}"), index*100, index*100+100])?;
            conn.execute("INSERT INTO event_provenance VALUES('event',?1,'mirror')", [format!("copy-{index}")])?;
        } Ok(())
    }).unwrap();
    let mut req = request(UsageEventSort::TimeDesc, 200);
    req.query.filter.sources = ids(&["mirror"], false);
    let page = fetch(&db, &req);
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.summary.total_tokens.as_str(), "110");
    assert_eq!(page.events[0].source_ids, ["mirror", "source"]);
    req.query.filter.range.end_ms = EpochMs::new(1000).unwrap();
    let empty = fetch(&db, &req);
    assert!(empty.events.is_empty() && empty.next_cursor.is_none());
    assert_eq!(empty.summary.total_tokens.as_str(), "0");
    assert!(empty.summary.input_total.value.is_none());
}

#[test]
fn event_cursors_cannot_be_rebound_or_leak_slots_after_close_or_bad_rows() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(&db, "next", 2000, 20, (None, None), None, None);
    let mut req = request(UsageEventSort::TimeDesc, 1);
    req.cursor = fetch(&db, &req).next_cursor;
    assert_eq!(
        db.query_usage_events("mini", &req, EpochMs::new(1000).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::CursorInvalid
    );
    for field in 0..4 {
        let mut changed = req.clone();
        match field {
            0 => changed.query.sort = UsageEventSort::TotalDesc,
            1 => changed.query.page_size = 2,
            2 => changed.query.filter.sessions = ids(&["session"], false),
            _ => {
                changed.query.price_basis = PriceBasis::SpecifiedTime {
                    specified_at_ms: EpochMs::new(2000).unwrap(),
                }
            }
        }
        assert_eq!(
            db.query_usage_events("main", &changed, EpochMs::new(1000).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::CursorInvalid
        );
        assert_eq!(
            db.close_usage_events("main", &changed).unwrap_err().code,
            ErrorCode::CursorInvalid
        );
    }
    db.close_usage_events("main", &req).unwrap();
    db.close_usage_events("main", &req).unwrap();
    assert_eq!(
        db.query_usage_events("main", &req, EpochMs::new(1000).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::SnapshotExpired
    );
    db.write(|conn| {
        conn.execute(
            "UPDATE usage_events SET quality_json='[1]' WHERE event_id='event'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    for _ in 0..3 {
        assert_eq!(
            db.query_usage_events(
                "main",
                &request(UsageEventSort::TimeDesc, 200),
                EpochMs::new(1000).unwrap()
            )
            .unwrap_err()
            .code,
            ErrorCode::DbCorrupt
        );
    }
    db.write(|conn| {
        conn.execute(
            "UPDATE usage_events SET quality_json='[\"confirmed\"]' WHERE event_id='event'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        fetch(&db, &request(UsageEventSort::TimeDesc, 200))
            .events
            .len(),
        2
    );
}

mod matched_price;
