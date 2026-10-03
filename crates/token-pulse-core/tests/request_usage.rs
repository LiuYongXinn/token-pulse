//! Exact response evidence fixtures; never a real account or request.
use serde_json::{Value, json};
use token_pulse_core::{
    adapter::{AdaptedRecord, adapt},
    domain::*,
};

fn context() -> ReaderContext {
    ReaderContext {
        provider_session_id: Some("thread".into()),
        session_key: Some("codex:thread".into()),
        metadata: EffectiveMetadata {
            provider: Some("synthetic".into()),
            model: Some("synthetic".into()),
            turn_id: Some("turn".into()),
            ..Default::default()
        },
        ..Default::default()
    }
}
fn position(offset: u64) -> PhysicalPosition {
    PhysicalPosition {
        file_generation_id: "g".into(),
        byte_offset: offset,
        byte_end: offset + 1,
    }
}
fn vector(input: i64, output: i64) -> Value {
    json!({"input_tokens":input,"cached_input_tokens":1,"cache_write_input_tokens":2,"output_tokens":output,"reasoning_output_tokens":0,"total_tokens":input+output})
}
fn record(input: i64) -> Value {
    json!({"type":"token_usage_record","payload":{"thread_id":"thread","turn_id":"turn","root_turn_id":"root","session_id":"runtime-session","response_id":"response","usage":vector(input,10),"turn_token_usage":vector(input,10),"thread_token_usage":vector(500000,30)}})
}
fn count(input: i64) -> Value {
    json!({"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":vector(input,10),"total_token_usage":vector(500000,30),"model_context_window":1000000}}})
}
fn feed(value: &Value, offset: u64, context: &mut ReaderContext) -> AdaptedRecord {
    adapt(
        &serde_json::to_vec(value).unwrap(),
        position(offset),
        context,
    )
}
fn usage(value: AdaptedRecord) -> UsageObservation {
    match value {
        AdaptedRecord::Observation(observation) => match *observation {
            NormalizedObservation::Usage(u) => u,
            _ => panic!("usage expected"),
        },
        _ => panic!("usage expected"),
    }
}

#[test]
fn durable_response_input_survives_checkpoint_and_binds_exact_single_response_not_total_or_window()
{
    for input in [272000, 272001] {
        let mut c = context();
        assert!(matches!(
            feed(&record(input), 0, &mut c),
            AdaptedRecord::Ignored
        ));
        let mut reopened: ReaderContext =
            serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
        let observed = usage(feed(&count(input), 1, &mut reopened));
        let evidence = observed.request_usage.as_ref().unwrap();
        assert_eq!(evidence.usage.input_total, Some(input));
        assert_eq!(evidence.thread_usage.input_total, Some(500000));
        assert_eq!(observed.model_context_window, Some(1000000));
        assert_eq!(evidence.physical_position, position(0));
        assert_eq!(evidence.response_id, "response");
        assert!(observed.request_identity.is_none());
        assert!(reopened.pending_request_usage.is_none());
        assert!(
            usage(feed(&count(input), 2, &mut reopened))
                .request_usage
                .is_none()
        );
        let necessary = serde_json::to_string(&observed).unwrap();
        assert!(!necessary.contains("runtime-session") && !necessary.contains("root_turn_id"));
    }
}

#[test]
fn mismatched_identity_vector_generation_position_or_intervening_record_never_binds_usage() {
    for case in 0..10 {
        let mut c = context();
        feed(&record(272001), 0, &mut c);
        let mut incoming = count(272001);
        let mut pos = position(1);
        match case {
            0 => incoming["payload"]["info"]["last_token_usage"]["input_tokens"] = 272000.into(),
            1 => incoming["payload"]["info"]["total_token_usage"]["cached_input_tokens"] = 2.into(),
            2 => c.metadata.turn_id = Some("different".into()),
            3 => pos.file_generation_id = "another".into(),
            4 => pos.byte_offset = 0,
            5 => {
                feed(
                    &json!({"type":"response_item","payload":{"text":"discarded synthetic body"}}),
                    1,
                    &mut c,
                );
                pos = position(2);
            }
            6 => {
                adapt(b"{invalid}", position(1), &mut c);
                pos = position(2);
            }
            7 => {
                incoming["payload"]["info"]["last_token_usage"]["cache_write_input_tokens"] =
                    Value::Null
            }
            8 => incoming["payload"]["info"]["last_token_usage"]["total_tokens"] = Value::Null,
            _ => incoming["payload"]["info"]["last_token_usage"] = Value::Null,
        }
        let observed = usage(adapt(&serde_json::to_vec(&incoming).unwrap(), pos, &mut c));
        assert!(
            observed.request_usage.is_none() && observed.request_identity.is_none(),
            "case {case}"
        );
    }
}

#[test]
fn invalid_auxiliary_records_do_not_invent_evidence_and_legacy_serialization_is_unchanged() {
    for case in 0..8 {
        let mut c = context();
        let mut value = record(272001);
        match case {
            0 => value["payload"]["thread_id"] = "another".into(),
            1 => value["payload"]["turn_id"] = "another".into(),
            2 => value["payload"]["response_id"] = "".into(),
            3 => value["payload"]["usage"]["input_tokens"] = "272001".into(),
            4 => value["payload"]["usage"]["cache_write_input_tokens"] = 272002.into(),
            5 => value["payload"]["usage"]["unknown_usage"] = 1.into(),
            6 => value["payload"]["thread_token_usage"] = Value::Null,
            _ => value["payload"]["session_id"] = "bad\nidentity".into(),
        }
        assert!(matches!(
            feed(&value, 0, &mut c),
            AdaptedRecord::Diagnostic(_)
        ));
        let observed = usage(feed(&count(272001), 1, &mut c));
        assert!(observed.request_usage.is_none());
        assert!(
            !serde_json::to_string(&observed)
                .unwrap()
                .contains("request_usage")
        );
    }
    assert!(
        !serde_json::to_string(&context())
            .unwrap()
            .contains("pending_request_usage")
    );
}

#[test]
fn sparse_auxiliary_evidence_cannot_change_mirror_or_inherited_prefix_identity() {
    use token_pulse_core::sequence::{
        SequenceDecision, SequenceIdentity, SessionSequence, UsageSignature, align_lineage,
        align_mirror,
    };
    let mut c = context();
    feed(&record(272001), 0, &mut c);
    let mut observed = usage(feed(&count(272001), 1, &mut c));
    observed.event_time_ms = Some(1000);
    let mut legacy = observed.clone();
    legacy.request_usage = None;
    let current = [UsageSignature::from(&observed)];
    let sparse = [UsageSignature::from(&legacy)];
    assert_eq!(current, sparse);
    let parent = SequenceIdentity {
        provider_namespace: "synthetic".into(),
        provider_session_id: "thread".into(),
        created_at_ms: Some(1),
        parent_provider_id: None,
    };
    let reference = SessionSequence {
        identity: &parent,
        records: &current,
        starts_at_session_head: true,
        scanned_to_upper_bound: true,
    };
    let mirror = SessionSequence {
        identity: &parent,
        records: &sparse,
        starts_at_session_head: true,
        scanned_to_upper_bound: true,
    };
    assert_eq!(
        align_mirror(&reference, &mirror),
        SequenceDecision::Mirror {
            aligned_prefix: 1,
            incoming_continuation: 0
        }
    );
    let child = SequenceIdentity {
        provider_session_id: "child".into(),
        parent_provider_id: Some("thread".into()),
        ..parent.clone()
    };
    let inherited = SessionSequence {
        identity: &child,
        records: &sparse,
        starts_at_session_head: true,
        scanned_to_upper_bound: true,
    };
    assert_eq!(
        align_lineage(&[reference], &inherited),
        SequenceDecision::Inherited {
            aligned_prefix: 1,
            child_continuation: 0
        }
    );
    // A canonical response still retains its own input proof for later price binding.
    assert_eq!(
        observed.request_usage.unwrap().usage.input_total,
        Some(272001)
    );
}
