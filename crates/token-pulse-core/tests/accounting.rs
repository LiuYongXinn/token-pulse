use proptest::prelude::*;
use serde::Deserialize;
use token_pulse_core::{accounting::*, domain::*, error::ErrorCode};

#[derive(Deserialize)]
struct Fixture {
    scenarios: Vec<Scenario>,
}
#[derive(Deserialize)]
struct Scenario {
    name: String,
    steps: Vec<Step>,
    confirmed_total: i128,
}
#[derive(Deserialize)]
struct Step {
    last: Option<[Option<i64>; 5]>,
    total: Option<[Option<i64>; 5]>,
    stream: Option<String>,
    #[serde(default)]
    head: bool,
    #[serde(default)]
    reset: bool,
    #[serde(default)]
    missing_time: bool,
    expected: String,
    event: Option<[Option<i64>; 5]>,
}
fn vector(v: [Option<i64>; 5]) -> UsageVector {
    UsageVector {
        input_total: v[0],
        cached_input: v[1],
        output_total: v[2],
        reasoning_output: v[3],
        reported_total: v[4],
    }
}
fn observation(index: usize, step: &Step) -> UsageObservation {
    UsageObservation {
        physical_position: PhysicalPosition {
            file_generation_id: "synthetic-generation".into(),
            byte_offset: index as u64 * 100,
            byte_end: index as u64 * 100 + 100,
        },
        session_key: "session".into(),
        event_time_ms: if step.missing_time { None } else { Some(1000) },
        request_identity: None,
        stream_hint: step.stream.clone(),
        last: step.last.map(vector),
        cumulative: step.total.map(vector),
        effective_metadata: EffectiveMetadata::default(),
        explicit_episode_start: step.reset,
        model_context_window: Some(1000),
    }
}
#[test]
fn independent_fixture_checks_each_classification_vector_and_total() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../../../fixtures/accounting-streams.json")).unwrap();
    for scenario in fixture.scenarios {
        let mut state = AccountingState::new("session".into());
        let mut sum = 0i128;
        for (index, step) in scenario.steps.iter().enumerate() {
            let result = account(
                &state,
                &observation(index, step),
                &AccountingEvidence {
                    independent_new_stream: step.head,
                    ..Default::default()
                },
            );
            assert_eq!(
                serde_json::to_value(result.method)
                    .unwrap()
                    .as_str()
                    .unwrap(),
                step.expected,
                "{} step {index}",
                scenario.name
            );
            assert_eq!(
                result.event_usage,
                step.event.map(vector),
                "{} step {index}",
                scenario.name
            );
            if let Some(v) = result.event_usage {
                sum += i128::from(v.validated_total().unwrap().unwrap());
            }
            state = serde_json::from_str(&serde_json::to_string(&result.state).unwrap()).unwrap();
        }
        assert_eq!(sum, scenario.confirmed_total, "{}", scenario.name);
    }
}
#[test]
fn arbitrary_batch_boundaries_preserve_event_identity_set_and_state() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../../../fixtures/accounting-streams.json")).unwrap();
    for scenario in fixture.scenarios {
        let run = |chunk: usize| {
            let mut state = AccountingState::new("session".into());
            let mut events = vec![];
            for (batch_index, batch) in scenario.steps.chunks(chunk).enumerate() {
                for (i, step) in batch.iter().enumerate() {
                    let position = batch_index * chunk + i;
                    let result = account(
                        &state,
                        &observation(position, step),
                        &AccountingEvidence {
                            independent_new_stream: step.head,
                            ..Default::default()
                        },
                    );
                    if let Some(v) = result.event_usage {
                        events.push((position, v, result.episode_id.clone()));
                    }
                    state = result.state;
                }
                state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
            }
            (state, events)
        };
        for chunk in 1..=scenario.steps.len() {
            assert_eq!(
                run(chunk),
                run(scenario.steps.len()),
                "{} chunk {chunk}",
                scenario.name
            );
        }
    }
}
#[test]
fn overflow_invalid_cumulative_and_missing_mask_do_not_advance_baseline() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../../../fixtures/accounting-streams.json")).unwrap();
    let step = &fixture.scenarios[0].steps[0];
    let first = observation(0, step);
    let state = account(
        &AccountingState::new("session".into()),
        &first,
        &AccountingEvidence {
            independent_new_stream: true,
            ..Default::default()
        },
    )
    .state;
    let mut next = observation(1, &fixture.scenarios[0].steps[1]);
    next.cumulative.as_mut().unwrap().reported_total = Some(999);
    let result = account(&state, &next, &AccountingEvidence::default());
    assert_eq!(result.state, state);
    assert_eq!(result.error, Some(ErrorCode::InvalidUsage));
    assert!(result.context.is_some());
    next.cumulative = Some(UsageVector {
        input_total: Some(i64::MAX),
        output_total: Some(1),
        ..Default::default()
    });
    assert_eq!(
        account(&state, &next, &AccountingEvidence::default()).error,
        Some(ErrorCode::NumericOverflow)
    );
    next = observation(1, &fixture.scenarios[0].steps[1]);
    next.cumulative.as_mut().unwrap().cached_input = None;
    let result = account(&state, &next, &AccountingEvidence::default());
    assert_eq!(result.state, state);
    assert!(result.event_usage.is_none());
}
#[test]
fn proven_duplicate_and_inheritance_do_not_advance_a_baseline_twice() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../../../fixtures/accounting-streams.json")).unwrap();
    let usage = observation(0, &fixture.scenarios[0].steps[0]);
    let state = AccountingState::new("session".into());
    let reference = CanonicalReference {
        event_id: "canonical".into(),
        usage: usage.last.unwrap(),
        event_time_ms: usage.event_time_ms,
        request_identity: None,
    };
    for (evidence, method) in [
        (
            AccountingEvidence {
                physical_duplicate: true,
                ..Default::default()
            },
            CalculationMethod::PhysicalDuplicate,
        ),
        (
            AccountingEvidence {
                verified_duplicate: Some(reference.clone()),
                ..Default::default()
            },
            CalculationMethod::VerifiedDuplicate,
        ),
        (
            AccountingEvidence {
                lineage: LineageEvidence::VerifiedInherited(reference.clone()),
                ..Default::default()
            },
            CalculationMethod::Inherited,
        ),
        (
            AccountingEvidence {
                lineage: LineageEvidence::Pending,
                ..Default::default()
            },
            CalculationMethod::LineagePending,
        ),
    ] {
        let result = account(&state, &usage, &evidence);
        assert_eq!(result.method, method);
        assert_eq!(result.state, state);
        assert!(result.event_usage.is_none());
    }
    let mut mismatched = reference;
    mismatched.usage.input_total = Some(99);
    assert_eq!(
        account(
            &state,
            &usage,
            &AccountingEvidence {
                verified_duplicate: Some(mismatched),
                ..Default::default()
            }
        )
        .method,
        CalculationMethod::AmbiguousUsage
    );
}
#[test]
fn stream_limit_retains_all_baselines_and_diagnoses_new_candidates() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../../../fixtures/accounting-streams.json")).unwrap();
    let mut state = AccountingState::new("session".into());
    for index in 0..MAX_STREAMS {
        let mut usage = observation(index, &fixture.scenarios[0].steps[0]);
        usage.stream_hint = Some(index.to_string());
        state = account(
            &state,
            &usage,
            &AccountingEvidence {
                independent_new_stream: true,
                ..Default::default()
            },
        )
        .state;
    }
    let mut next = observation(MAX_STREAMS, &fixture.scenarios[0].steps[0]);
    next.stream_hint = Some("overflow".into());
    let result = account(
        &state,
        &next,
        &AccountingEvidence {
            independent_new_stream: true,
            ..Default::default()
        },
    );
    assert_eq!(result.method, CalculationMethod::StreamCapacityExceeded);
    assert_eq!(result.state, state);
    assert!(result.event_usage.is_none());
}

proptest! {
    #[test]
    fn checked_single_stream_matches_independent_sum(calls in prop::collection::vec((0i64..10_000,0i64..1000,0i64..5000,0i64..1000),1..200)) {
        let mut state=AccountingState::new("session".into());
        let mut totals=[0i64;5]; let mut engine_sum=0i128; let mut oracle_sum=0i128;
        for (i,(input,cache,output,reasoning)) in calls.iter().enumerate() {
            let last=UsageVector {input_total:Some(*input),cached_input:Some((*cache).min(*input)),output_total:Some(*output),reasoning_output:Some((*reasoning).min(*output)),reported_total:Some(*input+*output)};
            let components=[last.input_total,last.cached_input,last.output_total,last.reasoning_output,last.reported_total];
            for j in 0..5 {totals[j]+=components[j].unwrap();}
            let usage=UsageObservation {physical_position:PhysicalPosition{file_generation_id:"g".into(),byte_offset:i as u64,byte_end:i as u64+1},session_key:"session".into(),event_time_ms:Some(123),request_identity:None,stream_hint:Some("trusted".into()),last:Some(last),cumulative:Some(vector(totals.map(Some))),effective_metadata:EffectiveMetadata{model:Some(format!("model-{i}")),..Default::default()},explicit_episode_start:false,model_context_window:None};
            let result=account(&state,&usage,&AccountingEvidence{independent_new_stream:i==0,..Default::default()});
            prop_assert_eq!(result.quality,ObservationQuality::Confirmed);
            if let Some(event)=result.event_usage {engine_sum+=i128::from(event.input_total.unwrap())+i128::from(event.output_total.unwrap());}
            oracle_sum+=i128::from(*input)+i128::from(*output);
            state=result.state;
        }
        prop_assert_eq!(engine_sum,oracle_sum);
    }
}

#[test]
fn unknown_last_and_corrupted_state_cannot_be_confirmed_as_zero() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../../../fixtures/accounting-streams.json")).unwrap();
    let mut usage = observation(0, &fixture.scenarios[0].steps[0]);
    usage.last = Some(UsageVector::default());
    usage.cumulative = Some(UsageVector::default());
    let state = AccountingState::new("session".into());
    let result = account(
        &state,
        &usage,
        &AccountingEvidence {
            independent_new_stream: true,
            ..Default::default()
        },
    );
    assert_eq!(result.quality, ObservationQuality::Pending);
    assert_eq!(result.state, state);
    assert!(result.event_usage.is_none());
    let mut corrupt = state;
    corrupt.streams.insert(
        "key".into(),
        StreamBaseline {
            stream_key: "other".into(),
            episode_id: "episode".into(),
            cumulative: UsageVector::default(),
            last_snapshot: None,
        },
    );
    assert_eq!(
        account(&corrupt, &usage, &AccountingEvidence::default()).error,
        Some(ErrorCode::InvalidQuery)
    );
}
