use super::QuotaRefreshDecision;
use crate::{
    error::ErrorCode,
    numeric::DecimalInt,
    protocol::{QuotaSnapshot, QuotaState},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum QuotaRefreshStatus {
    Started,
    InFlight,
    RateLimited,
    NotDue,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct QuotaRefreshResult {
    pub status: QuotaRefreshStatus,
    pub retry_after_ms: Option<u32>,
    pub quota: QuotaSnapshot,
}
impl QuotaRefreshResult {
    pub fn from_decision(
        decision: QuotaRefreshDecision,
        quota: QuotaSnapshot,
    ) -> Result<Self, ErrorCode> {
        let (status, retry_after_ms) = match decision {
            QuotaRefreshDecision::Started(_) => (QuotaRefreshStatus::Started, None),
            QuotaRefreshDecision::InFlight => (QuotaRefreshStatus::InFlight, None),
            QuotaRefreshDecision::NotDue => (QuotaRefreshStatus::NotDue, None),
            QuotaRefreshDecision::RateLimited { retry_after_ms } => (
                QuotaRefreshStatus::RateLimited,
                Some(u32::try_from(retry_after_ms).map_err(|_| ErrorCode::NumericOverflow)?),
            ),
        };
        Ok(Self {
            status,
            retry_after_ms,
            quota,
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct QuotaChanged {
    pub connection_epoch: String,
    pub quota_revision: DecimalInt,
    pub state: QuotaState,
}
impl From<&QuotaSnapshot> for QuotaChanged {
    fn from(snapshot: &QuotaSnapshot) -> Self {
        Self {
            connection_epoch: snapshot.connection_epoch.clone(),
            quota_revision: snapshot.quota_revision.clone(),
            state: snapshot.state,
        }
    }
}
