//! Exact request input is independent of accumulated consumption and configured windows.
use crate::{
    domain::{PhysicalPosition, RequestUsageEvidence, UsageObservation, UsageVector},
    numeric::DecimalInt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RequestConsumptionBinding {
    FullRequest,
    DifferentConsumption,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RequestInputEvidence {
    pub input_tokens: DecimalInt,
    pub binding: RequestConsumptionBinding,
}
/// Necessary association fields only; response identity stays out of the public projection.
pub struct RequestInputContext<'a> {
    pub position: &'a PhysicalPosition,
    pub turn_id: Option<&'a str>,
    pub last: Option<UsageVector>,
    pub cumulative: Option<UsageVector>,
}
impl<'a> From<&'a UsageObservation> for RequestInputContext<'a> {
    fn from(observation: &'a UsageObservation) -> Self {
        Self {
            position: &observation.physical_position,
            turn_id: observation.effective_metadata.turn_id.as_deref(),
            last: observation.last,
            cumulative: observation.cumulative,
        }
    }
}
impl RequestUsageEvidence {
    /// Unusable auxiliary evidence is unknown without invalidating independent Token facts.
    pub fn project_input(
        &self,
        context: RequestInputContext<'_>,
        consumption: UsageVector,
    ) -> Option<RequestInputEvidence> {
        let identity_valid =
            |s: &str| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control);
        if !identity_valid(&self.response_id)
            || !identity_valid(&self.turn_id)
            || context.turn_id != Some(self.turn_id.as_str())
            || self.physical_position.validate().is_err()
            || context.position.validate().is_err()
            || self.physical_position.file_generation_id != context.position.file_generation_id
            || self.physical_position.byte_end > context.position.byte_offset
            || context.last != Some(self.usage)
            || context.cumulative != Some(self.thread_usage)
            || self.usage.output_total.is_none()
        {
            return None;
        }
        let response_total = self.usage.validated_total().ok().flatten()?;
        let thread_total = self.thread_usage.validated_total().ok().flatten()?;
        if thread_total < response_total || [self.usage.input_total, self.usage.cached_input, self.usage.cache_write_input, self.usage.output_total, self.usage.reasoning_output]
            .into_iter().zip([self.thread_usage.input_total, self.thread_usage.cached_input, self.thread_usage.cache_write_input, self.thread_usage.output_total, self.thread_usage.reasoning_output])
            .any(|(response, thread)| matches!((response, thread), (Some(response), Some(thread)) if response > thread)) { return None; }
        consumption.validated_total().ok().flatten()?;
        Some(RequestInputEvidence {
            input_tokens: DecimalInt::from_nonnegative(i128::from(self.usage.input_total?)).ok()?,
            binding: if consumption == self.usage {
                RequestConsumptionBinding::FullRequest
            } else {
                RequestConsumptionBinding::DifferentConsumption
            },
        })
    }
}
