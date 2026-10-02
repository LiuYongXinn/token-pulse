//! Price-only background work. The public progress contains no source paths or monetary values.
use crate::{
    error::ErrorCode,
    jobs::JobScope,
    numeric::{DecimalInt, EpochMs},
    protocol::{PriceBasis, validate_request_id},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct PriceRevalueRequest {
    pub scope: JobScope,
    pub basis: PriceBasis,
    pub expected_price_revision: DecimalInt,
    pub request_key: String,
}
impl PriceRevalueRequest {
    pub fn validate(&mut self) -> Result<(), ErrorCode> {
        self.scope.canonicalize()?;
        validate_request_id(&self.request_key)?;
        i64::try_from(self.expected_price_revision.value())
            .map_err(|_| ErrorCode::NumericOverflow)?;
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum PriceRevalueState {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Cancelled,
    Failed,
    Interrupted,
}
impl PriceRevalueState {
    pub fn finished(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Cancelled | Self::Failed | Self::Interrupted
        )
    }
    pub fn can_cancel(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Cancelling)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct PriceRevalueJob {
    pub job_id: String,
    pub state: PriceRevalueState,
    pub automatic: bool,
    pub price_revision: DecimalInt,
    pub basis: PriceBasis,
    pub total_ledgers: DecimalInt,
    pub completed_ledgers: DecimalInt,
    pub total_events: DecimalInt,
    pub processed_events: DecimalInt,
    pub can_cancel: bool,
    pub error: Option<ErrorCode>,
    pub created_at_ms: EpochMs,
    pub updated_at_ms: EpochMs,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct PriceRevalueStatus {
    pub current_price_revision: DecimalInt,
    pub active_job: Option<PriceRevalueJob>,
    pub latest_job: Option<PriceRevalueJob>,
    pub uncached_ledgers: DecimalInt,
}
