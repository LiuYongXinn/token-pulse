use token_pulse_core::{accounting::*, domain::*, error::ErrorCode};

#[test]
fn partial_lower_bounds_reject_contradictions_without_filling_missing_parents() {
    let bad = [
        UsageVector {
            input_total: Some(100),
            reported_total: Some(10),
            ..Default::default()
        },
        UsageVector {
            cached_input: Some(7),
            reasoning_output: Some(8),
            reported_total: Some(10),
            ..Default::default()
        },
        UsageVector {
            input_total: Some(7),
            reasoning_output: Some(4),
            reported_total: Some(10),
            ..Default::default()
        },
        UsageVector {
            cached_input: Some(4),
            output_total: Some(7),
            reported_total: Some(10),
            ..Default::default()
        },
        UsageVector {
            input_total: Some(i64::MAX),
            reasoning_output: Some(1),
            reported_total: Some(i64::MAX),
            ..Default::default()
        },
        UsageVector {
            reasoning_output: Some(1),
            reported_total: Some(0),
            ..Default::default()
        },
    ];
    for usage in bad {
        assert_eq!(usage.validated_total(), Err(ErrorCode::InvalidUsage));
        assert_eq!(
            usage.published_total(LEGACY_ACCOUNTING_VERSION).unwrap(),
            usage.reported_total
        );
        assert_eq!(
            usage.published_total(ACCOUNTING_VERSION),
            Err(ErrorCode::InvalidUsage)
        );
    }
    let children = UsageVector {
        cached_input: Some(7),
        reasoning_output: Some(8),
        reported_total: Some(15),
        ..Default::default()
    };
    assert_eq!(children.validated_total().unwrap(), Some(15));
    assert_eq!(children.noncached_input().unwrap(), None);
    assert_eq!(children.input_total, None);
    assert_eq!(children.output_total, None);
    assert_eq!(
        UsageVector {
            reported_total: None,
            ..children
        }
        .validated_total()
        .unwrap(),
        None
    );
    let inclusive = UsageVector {
        input_total: Some(100),
        cache_write_input: None,
        cached_input: Some(60),
        output_total: Some(10),
        reasoning_output: Some(2),
        reported_total: Some(110),
    };
    assert_eq!(inclusive.validated_total().unwrap(), Some(110));
    assert_eq!(inclusive.noncached_input().unwrap(), Some(40));
    assert_eq!(
        inclusive.published_total("accounting-v999"),
        Err(ErrorCode::UnsupportedFormat)
    );
    assert!(!can_upgrade_accounting_version("accounting-v999"));
}

#[test]
fn independent_completion_oracle_checks_all_small_nullable_vectors() {
    let values = [None, Some(0), Some(1), Some(2)];
    for input in values {
        for cache in values {
            for output in values {
                for reason in values {
                    for total in 0..=4 {
                        let usage = UsageVector {
                            input_total: input,
                            cache_write_input: None,
                            cached_input: cache,
                            output_total: output,
                            reasoning_output: reason,
                            reported_total: Some(total),
                        };
                        // Independently enumerate concrete full histories consistent with all
                        // supplied facts, including the two containment relationships.
                        let possible = (0..=total).any(|full_input| {
                            (0..=total).any(|full_output| {
                                full_input + full_output == total
                                    && input.is_none_or(|n| n == full_input)
                                    && output.is_none_or(|n| n == full_output)
                                    && cache.is_none_or(|n| n <= full_input)
                                    && reason.is_none_or(|n| n <= full_output)
                            })
                        });
                        assert_eq!(usage.validated_total().is_ok(), possible, "{usage:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn invalid_partial_last_or_cumulative_never_advances_accounting_state() {
    let usage = UsageVector {
        input_total: Some(10),
        cache_write_input: None,
        cached_input: Some(0),
        output_total: Some(0),
        reasoning_output: Some(0),
        reported_total: Some(10),
    };
    let mut observation = UsageObservation {
        request_usage: None,
        physical_position: PhysicalPosition {
            file_generation_id: "synthetic".into(),
            byte_offset: 0,
            byte_end: 100,
        },
        session_key: "session".into(),
        event_time_ms: Some(1000),
        request_identity: None,
        stream_hint: Some("stream".into()),
        last: Some(usage),
        cumulative: Some(usage),
        effective_metadata: EffectiveMetadata::default(),
        explicit_episode_start: true,
        model_context_window: None,
    };
    let state = account(
        &AccountingState::new("session".into()),
        &observation,
        &AccountingEvidence {
            independent_new_stream: true,
            ..Default::default()
        },
    )
    .state;
    let bad = UsageVector {
        cached_input: Some(7),
        reasoning_output: Some(8),
        reported_total: Some(10),
        ..Default::default()
    };
    observation.physical_position.byte_offset = 100;
    observation.physical_position.byte_end = 200;
    observation.explicit_episode_start = false;
    observation.cumulative = Some(bad);
    let result = account(&state, &observation, &AccountingEvidence::default());
    assert_eq!(result.error, Some(ErrorCode::InvalidUsage));
    assert_eq!(result.state, state);
    assert!(result.event_usage.is_none());
    assert!(result.context.is_some());
    observation.last = Some(bad);
    observation.cumulative = Some(usage);
    let result = account(&state, &observation, &AccountingEvidence::default());
    assert_eq!(result.error, Some(ErrorCode::InvalidUsage));
    assert_eq!(result.state, state);
    assert!(result.event_usage.is_none());
    assert!(result.context.is_none());
}
