//! Query dimensions use stable opaque keys, never display labels as SQL input.
use crate::{
    calendar::Grain,
    numeric::EpochMs,
    protocol::{
        Coverage, DateRange, PriceBasis, PricingSummary, SnapshotMeta, TokenTotals, UsageFilter,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

pub fn model_key(provider: Option<&str>, model: Option<&str>) -> Option<String> {
    model.map(|name| {
        // JSON encodes null and component boundaries without separator ambiguity.
        let identity = serde_json::to_vec(&(provider, name)).expect("string pair JSON");
        format!("model:{:x}", Sha256::digest(identity))
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum GroupDimension {
    Models,
    Projects,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum GroupSort {
    TotalDesc,
    NameAsc,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct GroupedUsage {
    pub key: Option<String>,
    pub display_name: String,
    pub totals: TokenTotals,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DashboardRequest {
    pub filter: UsageFilter,
    pub price_basis: PriceBasis,
    pub grain: Grain,
    pub heatmap_range: DateRange,
}
impl DashboardRequest {
    pub fn validate(&self) -> Result<(), crate::error::ErrorCode> {
        self.filter.validate()?;
        self.heatmap_range.validate()?;
        if self.heatmap_range.timezone != self.filter.range.timezone {
            return Err(crate::error::ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct UsageSeriesBucket {
    pub start_ms: EpochMs,
    pub end_ms: EpochMs,
    pub display_label: String,
    pub utc_offset: String,
    pub totals: TokenTotals,
    pub coverage: Coverage,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RecentSession {
    pub session_key: String,
    pub display_name: String,
    pub latest_at_ms: EpochMs,
    pub latest_model: Option<String>,
    pub latest_project_id: Option<String>,
    pub latest_project_name: Option<String>,
    pub summary: TokenTotals,
    pub pricing: PricingSummary,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DashboardBundle {
    pub meta: SnapshotMeta,
    pub summary: TokenTotals,
    pub pricing: PricingSummary,
    pub coverage: Coverage,
    #[schemars(length(max = 2000))]
    pub series: Vec<UsageSeriesBucket>,
    #[schemars(length(max = 2000))]
    pub heatmap: Vec<UsageSeriesBucket>,
    #[schemars(length(max = 10))]
    pub recent_sessions: Vec<RecentSession>,
}
