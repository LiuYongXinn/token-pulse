//! Date-filtered consumption and latest context are explicitly separate.
use crate::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
    protocol::{
        ContextSnapshot, Coverage, PriceBasis, PricingSummary, SnapshotMeta, TokenTotals,
        UsageFilter,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum SessionSort {
    LatestDesc,
    TotalDesc,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionsQuery {
    pub filter: UsageFilter,
    pub price_basis: PriceBasis,
    pub sort: SessionSort,
    #[schemars(range(min = 1, max = 200))]
    pub page_size: u16,
}
impl SessionsQuery {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        self.filter.validate()?;
        if !(1..=200).contains(&self.page_size) {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionsRequest {
    pub query: SessionsQuery,
    #[schemars(length(min = 151, max = 151))]
    pub cursor: Option<String>,
}
impl SessionsRequest {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        self.query.validate()?;
        if self.cursor.as_ref().is_some_and(|cursor| {
            cursor.len() != 151
                || !cursor
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        }) {
            return Err(ErrorCode::CursorInvalid);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionRow {
    pub session_key: String,
    pub display_name: String,
    /// Latest selected usage event, not lifetime activity or latest context time.
    pub latest_at_ms: EpochMs,
    pub latest_model: Option<String>,
    pub latest_project_id: Option<String>,
    pub latest_project_name: Option<String>,
    pub parent_key: Option<String>,
    pub parent_display_name: Option<String>,
    pub parent_provider_id: Option<String>,
    /// Registered resolved children across dates, not inferred from request counts.
    pub child_count: DecimalInt,
    pub summary: TokenTotals,
    pub pricing: PricingSummary,
    pub coverage: Coverage,
    pub latest_context: ContextSnapshot,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionsPage {
    pub meta: SnapshotMeta,
    /// Whole filter totals, independent of the current page.
    pub summary: TokenTotals,
    pub pricing: PricingSummary,
    pub coverage: Coverage,
    #[schemars(length(max = 200))]
    pub sessions: Vec<SessionRow>,
    #[schemars(length(min = 151, max = 151))]
    pub next_cursor: Option<String>,
}
