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

mod sessions;
pub use sessions::{
    ClassificationKind, SessionActivity, SessionBundle, SessionBundleRequest,
    SessionClassification, SessionIdentity, SessionRow, SessionSort, SessionsPage, SessionsQuery,
    SessionsRequest,
};
mod events;
mod turns;
pub use events::{
    RawTokenCount, RawUsageVector, UsageEventRow, UsageEventSort, UsageEventsPage,
    UsageEventsQuery, UsageEventsRequest,
};
pub use turns::{TurnRow, TurnsPage, TurnsQuery, TurnsRequest};

/// Anonymous version vector; no source labels or event content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct UsageRevision {
    pub database_id: String,
    pub data_revision: crate::numeric::DecimalInt,
    pub price_revision: crate::numeric::DecimalInt,
    pub usage_view_revision: crate::numeric::DecimalInt,
}

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
pub struct GroupedUsageRequest {
    pub filter: UsageFilter,
    pub price_basis: PriceBasis,
    pub dimension: GroupDimension,
    pub sort: GroupSort,
    #[schemars(range(min = 1, max = 200))]
    pub limit: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    #[schemars(length(min = 151, max = 151))]
    pub cursor: Option<String>,
}
impl GroupedUsageRequest {
    pub fn validate(&self) -> Result<(), crate::error::ErrorCode> {
        self.filter.validate()?;
        if !(1..=200).contains(&self.limit) {
            return Err(crate::error::ErrorCode::InvalidQuery);
        }
        if let Some(cursor) = &self.cursor {
            if cursor.len() != 151
                || !cursor
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(crate::error::ErrorCode::CursorInvalid);
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct PricedUsageGroup {
    pub key: Option<String>,
    pub display_name: String,
    pub totals: TokenTotals,
    pub pricing: PricingSummary,
    pub coverage: Coverage,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct GroupedUsageBundle {
    pub meta: SnapshotMeta,
    pub summary: TokenTotals,
    pub pricing: PricingSummary,
    pub coverage: Coverage,
    pub total_group_count: crate::numeric::DecimalInt,
    pub truncated: bool,
    #[schemars(length(max = 200))]
    pub groups: Vec<PricedUsageGroup>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum FacetDimension {
    Sources,
    Models,
    Projects,
    Sessions,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FilterOptionsQuery {
    pub filter: UsageFilter,
    pub dimension: FacetDimension,
    #[schemars(length(max = 256))]
    pub search: String,
    #[schemars(range(min = 1, max = 200))]
    pub page_size: u16,
}
impl FilterOptionsQuery {
    pub fn validate(&self) -> Result<(), crate::error::ErrorCode> {
        self.filter.validate()?;
        if self.search.chars().count() > 256
            || self.search.chars().any(char::is_control)
            || !(1..=200).contains(&self.page_size)
        {
            return Err(crate::error::ErrorCode::InvalidQuery);
        }
        Ok(())
    }
    pub fn facet_filter(&self) -> Result<UsageFilter, crate::error::ErrorCode> {
        self.validate()?;
        let mut filter = self.filter.clone();
        let all = crate::protocol::DimensionSelection::All {};
        match self.dimension {
            FacetDimension::Sources => filter.sources = all,
            FacetDimension::Models => filter.models = all,
            FacetDimension::Projects => filter.projects = all,
            FacetDimension::Sessions => filter.sessions = all,
        }
        Ok(filter)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FilterOptionsRequest {
    pub query: FilterOptionsQuery,
    #[schemars(length(min = 151, max = 151))]
    pub cursor: Option<String>,
}
impl FilterOptionsRequest {
    pub fn validate(&self) -> Result<(), crate::error::ErrorCode> {
        self.query.validate()?;
        if let Some(cursor) = &self.cursor {
            if cursor.len() != 151
                || !cursor
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(crate::error::ErrorCode::CursorInvalid);
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FilterOption {
    pub key: Option<String>,
    pub display_name: String,
    /// Confirmed selected usage events, not an import-completeness assertion.
    pub count: crate::numeric::DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FilterOptionsPage {
    pub meta: SnapshotMeta,
    pub dimension: FacetDimension,
    #[schemars(length(max = 200))]
    pub options: Vec<FilterOption>,
    #[schemars(length(min = 151, max = 151))]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CloseQuerySnapshotRequest {
    Groups {
        request: GroupedUsageRequest,
    },
    MiniSessions {
        request: crate::mini::MiniSessionsRequest,
    },
    FilterOptions {
        request: FilterOptionsRequest,
    },
    Sessions {
        request: SessionsRequest,
    },
    UsageEvents {
        request: UsageEventsRequest,
    },
    Turns {
        request: TurnsRequest,
    },
}
impl CloseQuerySnapshotRequest {
    pub fn validate(&self) -> Result<(), crate::error::ErrorCode> {
        match self {
            Self::Groups { request } => {
                request.validate()?;
                if request.cursor.is_none() {
                    return Err(crate::error::ErrorCode::InvalidQuery);
                }
                Ok(())
            }
            Self::MiniSessions { request } => {
                request.validate()?;
                if request.cursor.is_none() {
                    return Err(crate::error::ErrorCode::InvalidQuery);
                }
                Ok(())
            }
            Self::Turns { request } => {
                request.validate()?;
                if request.cursor.is_none() {
                    return Err(crate::error::ErrorCode::InvalidQuery);
                }
                Ok(())
            }
            Self::FilterOptions { request } => {
                request.validate()?;
                if request.cursor.is_none() {
                    return Err(crate::error::ErrorCode::InvalidQuery);
                }
                Ok(())
            }
            Self::Sessions { request } => {
                request.validate()?;
                if request.cursor.is_none() {
                    return Err(crate::error::ErrorCode::InvalidQuery);
                }
                Ok(())
            }
            Self::UsageEvents { request } => {
                request.validate()?;
                if request.cursor.is_none() {
                    return Err(crate::error::ErrorCode::InvalidQuery);
                }
                Ok(())
            }
        }
    }
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
