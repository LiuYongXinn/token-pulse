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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionBundleRequest {
    #[schemars(length(min = 1, max = 256))]
    pub session_key: String,
    pub filter: UsageFilter,
    pub price_basis: PriceBasis,
}
impl SessionBundleRequest {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        self.filter.validate()?;
        crate::protocol::DimensionSelection::Ids {
            ids: vec![self.session_key.clone()],
            include_unknown: false,
        }
        .validate()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionIdentity {
    pub session_key: String,
    pub display_name: String,
    pub parent_key: Option<String>,
    pub parent_display_name: Option<String>,
    pub parent_provider_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionActivity {
    pub occurred_at_ms: EpochMs,
    pub model: Option<String>,
    pub project_id: Option<String>,
    pub project_display_name: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationKind {
    Pending,
    Inherited,
    Duplicate,
    Unattributed,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionClassification {
    pub kind: ClassificationKind,
    #[schemars(length(min = 1, max = 128))]
    pub reason_code: String,
    /// Observation classifications across the active ledger, never consumption.
    pub observation_count: DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionBundle {
    pub meta: SnapshotMeta,
    pub identity: SessionIdentity,
    pub summary: TokenTotals,
    pub pricing: PricingSummary,
    pub coverage: Coverage,
    pub latest_selected_activity: Option<SessionActivity>,
    pub latest_context: ContextSnapshot,
    pub child_count: DecimalInt,
    #[schemars(length(max = 100))]
    pub children: Vec<SessionIdentity>,
    pub children_truncated: bool,
    #[schemars(length(max = 64))]
    pub classifications: Vec<SessionClassification>,
}
