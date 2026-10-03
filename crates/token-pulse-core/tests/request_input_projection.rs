//! Independent public evidence expectations, without prices or real requests.
use token_pulse_core::{domain::*, pricing::request::*};
fn vector(input: i64, output: i64) -> UsageVector {
    UsageVector {
        input_total: Some(input),
        cached_input: Some(0),
        cache_write_input: Some(0),
        output_total: Some(output),
        reasoning_output: Some(0),
        reported_total: Some(input + output),
    }
}
fn observed(input: i64) -> UsageObservation {
    let usage = vector(input, 10);
    let cumulative = vector(input.max(500000), 30);
    UsageObservation {
        physical_position: PhysicalPosition {
            file_generation_id: "g".into(),
            byte_offset: 10,
            byte_end: 20,
        },
        session_key: "canonical".into(),
        event_time_ms: Some(1000),
        request_identity: None,
        request_usage: Some(Box::new(RequestUsageEvidence {
            response_id: "response".into(),
            turn_id: "turn".into(),
            usage,
            thread_usage: cumulative,
            physical_position: PhysicalPosition {
                file_generation_id: "g".into(),
                byte_offset: 0,
                byte_end: 10,
            },
        })),
        stream_hint: None,
        last: Some(usage),
        cumulative: Some(cumulative),
        effective_metadata: EffectiveMetadata {
            turn_id: Some("turn".into()),
            ..Default::default()
        },
        explicit_episode_start: false,
        model_context_window: Some(1000000),
    }
}
#[test]
fn exact_response_input_and_full_consumption_are_not_session_totals_or_model_windows() {
    for input in [0, 272000, 272001, 9007199254740993] {
        let o = observed(input);
        let proof = o.request_usage.as_ref().unwrap();
        let projected = proof.project_input((&o).into(), proof.usage).unwrap();
        assert_eq!(projected.input_tokens.as_str(), input.to_string());
        assert_eq!(projected.binding, RequestConsumptionBinding::FullRequest);
        let json = serde_json::to_string(&projected).unwrap();
        assert!(!json.contains("response") && !json.contains("turn") && !json.contains("window"));
    }
}
#[test]
fn partial_or_different_component_consumption_keeps_input_but_cannot_prove_full_request() {
    let o = observed(272001);
    let proof = o.request_usage.as_ref().unwrap();
    for consumption in [
        vector(20, 5),
        vector(272001, 9),
        UsageVector {
            cached_input: Some(1),
            ..proof.usage
        },
        UsageVector {
            cache_write_input: None,
            ..proof.usage
        },
        UsageVector {
            reported_total: None,
            ..proof.usage
        },
    ] {
        let projection = proof.project_input((&o).into(), consumption).unwrap();
        assert_eq!(projection.input_tokens.as_str(), "272001");
        assert_eq!(
            projection.binding,
            RequestConsumptionBinding::DifferentConsumption
        );
    }
}
#[test]
fn missing_mismatched_or_invalid_proofs_are_unknown_without_guessing_from_last() {
    for case in 0..12 {
        let mut o = observed(272001);
        let consumption = o.last.unwrap();
        match case {
            0 => o.effective_metadata.turn_id = None,
            1 => o.effective_metadata.turn_id = Some("other".into()),
            2 => o.physical_position.file_generation_id = "other".into(),
            3 => o.physical_position.byte_offset = 9,
            4 => o.last = None,
            5 => o.cumulative = None,
            6 => o.last.as_mut().unwrap().cache_write_input = None,
            7 => o.request_usage.as_mut().unwrap().response_id = "".into(),
            8 => o.request_usage.as_mut().unwrap().usage.input_total = None,
            9 => {
                o.request_usage
                    .as_mut()
                    .unwrap()
                    .thread_usage
                    .reported_total = Some(1)
            }
            10 => o.request_usage.as_mut().unwrap().physical_position.byte_end = 0,
            _ => {
                let cumulative = vector(20, 10);
                o.request_usage.as_mut().unwrap().thread_usage = cumulative;
                o.cumulative = Some(cumulative);
            }
        }
        assert!(
            o.request_usage
                .as_ref()
                .unwrap()
                .project_input((&o).into(), consumption)
                .is_none(),
            "case {case}"
        );
    }
}
