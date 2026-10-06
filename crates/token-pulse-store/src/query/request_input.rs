//! Read only retained fields necessary to project exact per-response input.
use serde::{Deserialize, Serialize};
use token_pulse_core::{
    domain::{PhysicalPosition, RequestUsageEvidence, UsageVector},
    pricing::request::{RequestInputContext, RequestInputEvidence},
};
pub(super) const SQL: &str = "CASE WHEN json_type(o.normalized_json,'$.request_usage')='object' THEN json_object('evidence',json_extract(o.normalized_json,'$.request_usage'),'position',json_extract(o.normalized_json,'$.physical_position'),'turn_id',json_extract(o.normalized_json,'$.effective_metadata.turn_id'),'last',json_extract(o.normalized_json,'$.last'),'cumulative',json_extract(o.normalized_json,'$.cumulative')) ELSE NULL END";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NecessaryInput {
    evidence: RequestUsageEvidence,
    position: PhysicalPosition,
    turn_id: Option<String>,
    last: Option<UsageVector>,
    cumulative: Option<UsageVector>,
}
/// Internal cache identity. Never serialize response IDs or positions into public DTOs.
#[derive(Serialize)]
pub(crate) struct PricingRequestInput {
    pub input: RequestInputEvidence,
    response_id: String,
    turn_id: String,
    position: PhysicalPosition,
}
pub(crate) fn pricing(json: Option<&str>, consumption: UsageVector) -> Option<PricingRequestInput> {
    let input: NecessaryInput = serde_json::from_str(json?).ok()?;
    let projected = input.evidence.project_input(
        RequestInputContext {
            position: &input.position,
            turn_id: input.turn_id.as_deref(),
            last: input.last,
            cumulative: input.cumulative,
        },
        consumption,
    )?;
    Some(PricingRequestInput {
        input: projected,
        response_id: input.evidence.response_id,
        turn_id: input.evidence.turn_id,
        position: input.evidence.physical_position,
    })
}
