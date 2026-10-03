//! Synthetic counters; no account, log body, or market price is read.
use serde_json::{Value, json};
use token_pulse_core::{
    accounting::{
        AccountingEvidence, AccountingState, CalculationMethod, CanonicalReference,
        LineageEvidence, account,
    },
    adapter::{AdaptedRecord, adapt},
    domain::*,
    error::ErrorCode,
    sequence::UsageSignature,
};

fn vector(input: i64, read: i64, write: Option<i64>, output: i64) -> UsageVector {
    UsageVector {
        input_total: Some(input),
        cached_input: Some(read),
        cache_write_input: write,
        output_total: Some(output),
        reasoning_output: Some(0),
        reported_total: Some(input + output),
    }
}
fn observation(
    offset: u64,
    last: Option<UsageVector>,
    cumulative: UsageVector,
) -> UsageObservation {
    UsageObservation {
        physical_position: PhysicalPosition {
            file_generation_id: "synthetic".into(),
            byte_offset: offset,
            byte_end: offset + 100,
        },
        session_key: "session".into(),
        event_time_ms: Some(1000),
        request_identity: None,
        stream_hint: Some("stream".into()),
        last,
        cumulative: Some(cumulative),
        effective_metadata: EffectiveMetadata::default(),
        explicit_episode_start: false,
        model_context_window: None,
    }
}
fn adapted(usage: Value) -> AdaptedRecord {
    let record = json!({"timestamp":"2026-10-03T00:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":usage,"total_token_usage":usage}}});
    adapt(
        &serde_json::to_vec(&record).unwrap(),
        observation(0, None, UsageVector::default()).physical_position,
        &mut ReaderContext {
            session_key: Some("session".into()),
            ..Default::default()
        },
    )
}

#[test]
fn adapter_preserves_missing_null_zero_and_positive_and_rejects_conflicting_spellings() {
    let base = json!({"input_tokens":100,"cached_input_tokens":60,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":110});
    for name in ["cache_write_input_tokens", "cache_write_tokens"] {
        for (value, expected) in [
            (Value::Null, None),
            (json!(0), Some(0)),
            (json!(20), Some(20)),
        ] {
            let mut usage = base.clone();
            usage[name] = value;
            let AdaptedRecord::Observation(record) = adapted(usage) else {
                panic!("expected supported usage")
            };
            let NormalizedObservation::Usage(usage) = *record else {
                panic!("expected usage")
            };
            assert_eq!(usage.last.unwrap().cache_write_input, expected);
            assert_eq!(usage.cumulative.unwrap().cache_write_input, expected);
            assert_eq!(usage.last.unwrap().validated_total().unwrap(), Some(110));
        }
        for invalid in [
            json!("20"),
            json!(false),
            json!(1.5),
            json!(9223372036854775808u64),
        ] {
            let mut usage = base.clone();
            usage[name] = invalid;
            assert!(matches!(adapted(usage), AdaptedRecord::Diagnostic(_)));
        }
    }
    let AdaptedRecord::Observation(record) = adapted(base.clone()) else {
        panic!("legacy usage")
    };
    let NormalizedObservation::Usage(usage) = *record else {
        panic!("usage")
    };
    assert_eq!(usage.last.unwrap().cache_write_input, None);
    for other in [json!(21), Value::Null] {
        let mut usage = base.clone();
        usage["cache_write_input_tokens"] = json!(20);
        usage["cache_write_tokens"] = other;
        assert!(matches!(adapted(usage), AdaptedRecord::Diagnostic(_)));
    }
    let mut usage = base;
    usage["unknown_billing_tokens"] = json!(1);
    assert!(matches!(adapted(usage), AdaptedRecord::Diagnostic(_)));
}

#[test]
fn input_categories_are_disjoint_and_total_does_not_add_writes_or_reasoning() {
    for input in 0..=8 {
        for read in 0..=8 {
            for write in 0..=8 {
                let usage = vector(input, read, Some(write), 2);
                let expected = if read + write <= input {
                    Ok(Some(input + 2))
                } else {
                    Err(ErrorCode::InvalidUsage)
                };
                assert_eq!(usage.validated_total(), expected);
            }
        }
    }
    assert_eq!(
        vector(100, 60, Some(-1), 10).validated_total(),
        Err(ErrorCode::InvalidUsage)
    );
    let partial = UsageVector {
        cached_input: Some(60),
        cache_write_input: Some(20),
        reported_total: Some(79),
        ..Default::default()
    };
    assert_eq!(partial.validated_total(), Err(ErrorCode::InvalidUsage));
    let partial = UsageVector {
        reported_total: Some(80),
        ..partial
    };
    assert_eq!(partial.validated_total(), Ok(Some(80)));
    assert_eq!(partial.input_total, None);
    assert_eq!(partial.noncached_input(), Ok(None));
}

#[test]
fn cumulative_delta_preserves_write_counter_and_mask_changes_do_not_advance_baseline() {
    let old = vector(100, 60, Some(20), 10);
    let first = account(
        &AccountingState::new("session".into()),
        &observation(0, Some(old), old),
        &AccountingEvidence {
            independent_new_stream: true,
            ..Default::default()
        },
    );
    assert_eq!(first.event_usage, Some(old));
    let current = vector(120, 70, Some(25), 12);
    let next = account(
        &first.state,
        &observation(100, None, current),
        &AccountingEvidence::default(),
    );
    assert_eq!(next.method, CalculationMethod::CumulativeDelta);
    assert_eq!(next.event_usage, Some(vector(20, 10, Some(5), 2)));
    for writes in [None, Some(19)] {
        let inconsistent = UsageVector {
            cache_write_input: writes,
            ..current
        };
        let result = account(
            &first.state,
            &observation(100, None, inconsistent),
            &AccountingEvidence::default(),
        );
        assert!(result.event_usage.is_none());
        assert_eq!(result.state, first.state);
    }
    let reset_usage = vector(5, 1, Some(2), 1);
    let mut reset = observation(200, Some(reset_usage), reset_usage);
    reset.explicit_episode_start = true;
    let result = account(&next.state, &reset, &AccountingEvidence::default());
    assert_eq!(result.method, CalculationMethod::EpisodeReset);
    assert_eq!(result.event_usage, Some(reset_usage));
}

#[test]
fn write_evidence_participates_in_mirror_and_fork_identity_without_changing_legacy_encoding() {
    let usage = vector(100, 60, None, 10);
    let legacy = r#"{"input_total":100,"cached_input":60,"output_total":10,"reasoning_output":0,"reported_total":110}"#;
    assert_eq!(serde_json::to_string(&usage).unwrap(), legacy);
    assert_eq!(serde_json::from_str::<UsageVector>(legacy).unwrap(), usage);
    let observed = observation(
        0,
        Some(vector(100, 60, Some(20), 10)),
        vector(100, 60, Some(20), 10),
    );
    let reference = CanonicalReference {
        event_id: "canonical".into(),
        usage: observed.last.unwrap(),
        observation: UsageSignature::from(&observed),
    };
    for fork in [false, true] {
        let evidence = if fork {
            AccountingEvidence {
                lineage: LineageEvidence::VerifiedInherited {
                    reference: Box::new(reference.clone()),
                    baseline: None,
                },
                ..Default::default()
            }
        } else {
            AccountingEvidence {
                verified_duplicate: Some(reference.clone()),
                ..Default::default()
            }
        };
        let result = account(
            &AccountingState::new("session".into()),
            &observed,
            &evidence,
        );
        assert_eq!(result.canonical_event_id.as_deref(), Some("canonical"));
        assert!(result.event_usage.is_none());
        let different_usage = vector(100, 60, Some(21), 10);
        let different = observation(0, Some(different_usage), different_usage);
        assert_ne!(
            UsageSignature::from(&observed).candidate_fingerprint(),
            UsageSignature::from(&different).candidate_fingerprint()
        );
        let result = account(
            &AccountingState::new("session".into()),
            &different,
            &evidence,
        );
        assert!(result.canonical_event_id.is_none());
        assert!(result.event_usage.is_none());
    }
}
