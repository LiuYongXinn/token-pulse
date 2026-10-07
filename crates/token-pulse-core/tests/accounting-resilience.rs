use token_pulse_core::{accounting::*, domain::*, error::ErrorCode};

fn vector(input: i64, cache: Option<i64>, output: i64, total: i64) -> UsageVector {
    UsageVector {
        input_total: Some(input),
        cached_input: cache,
        cache_write_input: Some(0),
        output_total: Some(output),
        reasoning_output: Some(0),
        reported_total: Some(total),
    }
}
fn observation(index: u64, last: Option<UsageVector>, cumulative: UsageVector) -> UsageObservation {
    UsageObservation {
        request_usage: None,
        physical_position: PhysicalPosition {
            file_generation_id: "synthetic".into(),
            byte_offset: index,
            byte_end: index + 1,
        },
        session_key: "session".into(),
        event_time_ms: Some(1000 + index as i64),
        request_identity: None,
        stream_hint: None,
        last,
        cumulative: Some(cumulative),
        effective_metadata: EffectiveMetadata::default(),
        explicit_episode_start: false,
        model_context_window: None,
    }
}
fn first() -> (UsageObservation, AccountingState) {
    let v = vector(100, Some(60), 10, 110);
    let u = observation(0, Some(v), v);
    let state = account(
        &AccountingState::new("session".into()),
        &u,
        &AccountingEvidence {
            independent_new_stream: true,
            ..Default::default()
        },
    )
    .state;
    (u, state)
}
fn continuation(previous: Option<UsageVector>) -> AccountingEvidence {
    AccountingEvidence {
        ordered_cumulative: true,
        previous_cumulative: previous,
        ..Default::default()
    }
}

#[test]
fn repeated_unhinted_rollout_snapshot_never_remains_pending_or_counts_twice() {
    let (first, state) = first();
    let repeated = observation(1, first.last, first.cumulative.unwrap());
    let result = account(&state, &repeated, &AccountingEvidence::default());
    assert_eq!(result.method, CalculationMethod::RepeatedSnapshot);
    assert_eq!(result.quality, ObservationQuality::Duplicate);
    assert_eq!(result.state, state);
    assert!(result.event_usage.is_none());
}

#[test]
fn unchanged_counter_with_invalid_context_last_is_a_non_consuming_diagnostic() {
    let (first, state) = first();
    let refresh = observation(
        1,
        Some(vector(0, Some(0), 0, 27)),
        first.cumulative.unwrap(),
    );
    let result = account(&state, &refresh, &continuation(None));
    assert_eq!(result.method, CalculationMethod::UnchangedCumulative);
    assert_eq!(result.quality, ObservationQuality::Duplicate);
    assert_eq!(result.error, Some(ErrorCode::InvalidUsage));
    assert_eq!(result.state, state);
    assert!(result.context.is_none());
    assert!(result.event_usage.is_none());
    let next = observation(
        2,
        Some(vector(20, Some(10), 5, 25)),
        vector(120, Some(70), 15, 135),
    );
    assert_eq!(
        account(&result.state, &next, &continuation(refresh.cumulative))
            .event_usage
            .unwrap()
            .validated_total()
            .unwrap(),
        Some(25)
    );
}

#[test]
fn invalid_last_recovers_from_adjacent_valid_counter_and_keeps_the_diagnostic() {
    let (first, state) = first();
    let bad = observation(
        1,
        Some(vector(20, Some(10), 5, 999)),
        vector(120, Some(70), 15, 135),
    );
    let result = account(&state, &bad, &continuation(first.cumulative));
    assert_eq!(result.method, CalculationMethod::CumulativeDelta);
    assert_eq!(result.quality, ObservationQuality::Confirmed);
    assert_eq!(result.error, Some(ErrorCode::InvalidUsage));
    assert_eq!(
        result.event_usage.unwrap().validated_total().unwrap(),
        Some(25)
    );
    let next = observation(
        2,
        Some(vector(10, Some(5), 5, 15)),
        vector(130, Some(75), 20, 150),
    );
    assert_eq!(
        account(&result.state, &next, &continuation(bad.cumulative))
            .event_usage
            .unwrap()
            .validated_total()
            .unwrap(),
        Some(15)
    );
}

#[test]
fn missing_optional_breakdown_does_not_freeze_total_or_fill_unknown_cache() {
    let (first, state) = first();
    let missing = observation(1, Some(vector(20, None, 5, 25)), vector(120, None, 15, 135));
    let result = account(&state, &missing, &AccountingEvidence::default());
    assert_eq!(
        result.event_usage.unwrap().validated_total().unwrap(),
        Some(25)
    );
    assert_eq!(result.event_usage.unwrap().cached_input, None);
    let next = observation(
        2,
        Some(vector(10, Some(5), 5, 15)),
        vector(130, Some(75), 20, 150),
    );
    assert_eq!(
        account(&result.state, &next, &continuation(missing.cumulative))
            .event_usage
            .unwrap()
            .validated_total()
            .unwrap(),
        Some(15)
    );
    assert_eq!(first.cumulative.unwrap().cached_input, Some(60));
}

#[test]
fn invalid_counter_is_isolated_and_next_request_rebases_without_charging_the_old_gap() {
    let (first, state) = first();
    let bad = observation(
        1,
        Some(vector(20, Some(10), 5, 25)),
        vector(120, Some(70), 15, 999),
    );
    let result = account(&state, &bad, &continuation(first.cumulative));
    assert_eq!(result.quality, ObservationQuality::Pending);
    assert_eq!(result.state, state);
    let next = observation(
        2,
        Some(vector(10, Some(5), 5, 15)),
        vector(130, Some(75), 20, 150),
    );
    let recovered = account(&state, &next, &continuation(None));
    assert_eq!(recovered.method, CalculationMethod::LastRebased);
    assert_eq!(
        recovered.event_usage.unwrap().validated_total().unwrap(),
        Some(15)
    );
    assert_eq!(
        recovered.state.streams.values().next().unwrap().cumulative,
        next.cumulative.unwrap()
    );
    assert!(recovered.prior_anchor.is_none());
}

#[test]
fn absent_adjacency_or_stream_proof_never_assigns_a_counter_gap_to_the_latest_time() {
    let (_, state) = first();
    let bad = observation(
        1,
        Some(vector(20, Some(10), 5, 999)),
        vector(120, Some(70), 15, 135),
    );
    assert!(
        account(&state, &bad, &continuation(None))
            .event_usage
            .is_none()
    );
    assert!(
        account(&state, &bad, &AccountingEvidence::default())
            .event_usage
            .is_none()
    );
}

#[test]
fn a_proven_new_stream_or_unresolved_lineage_cannot_be_swallowed_as_a_snapshot() {
    let (first, state) = first();
    for evidence in [
        AccountingEvidence {
            independent_new_stream: true,
            ..Default::default()
        },
        AccountingEvidence {
            lineage: LineageEvidence::Pending,
            ordered_cumulative: true,
            ..Default::default()
        },
    ] {
        let result = account(
            &state,
            &observation(1, first.last, first.cumulative.unwrap()),
            &evidence,
        );
        if evidence.independent_new_stream {
            assert_eq!(result.event_usage, first.last);
            assert_eq!(result.state.streams.len(), 2);
        } else {
            assert_eq!(result.method, CalculationMethod::LineagePending);
            assert!(result.event_usage.is_none());
        }
    }
}

#[test]
fn unknown_counter_reset_and_two_equal_streams_remain_pending() {
    let (first, mut state) = first();
    let reset = observation(
        1,
        Some(vector(10, Some(5), 5, 15)),
        vector(10, Some(5), 5, 15),
    );
    assert!(
        account(&state, &reset, &continuation(first.cumulative))
            .event_usage
            .is_none()
    );
    let mut second = state.streams.values().next().unwrap().clone();
    second.stream_key = "another".into();
    state.streams.insert(second.stream_key.clone(), second);
    assert_eq!(
        account(
            &state,
            &observation(1, first.last, first.cumulative.unwrap()),
            &continuation(first.cumulative)
        )
        .quality,
        ObservationQuality::Pending
    );
}

#[test]
fn corrected_optional_counters_do_not_invalidate_a_known_parent_delta() {
    let (first, state) = first();
    let mut corrected = vector(120, Some(100), 15, 135);
    corrected.reasoning_output = Some(10);
    let u = observation(1, None, corrected);
    let result = account(&state, &u, &continuation(first.cumulative));
    assert_eq!(result.method, CalculationMethod::CumulativeDelta);
    let delta = result.event_usage.unwrap();
    assert_eq!(delta.validated_total().unwrap(), Some(25));
    assert_eq!(delta.cached_input, None);
    assert_eq!(delta.reasoning_output, None);
}
