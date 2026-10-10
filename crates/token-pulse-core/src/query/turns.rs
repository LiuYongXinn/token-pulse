//! A reliable turn groups explicit nonempty turn IDs within one canonical session.
use crate::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
    protocol::{Coverage, PriceBasis, PricingSummary, SnapshotMeta, TokenTotals, UsageFilter},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TurnsQuery {
    #[schemars(length(min = 1, max = 256))]
    pub session_key: String,
    pub filter: UsageFilter,
    pub price_basis: PriceBasis,
    #[schemars(range(min = 1, max = 200))]
    pub page_size: u16,
}
impl TurnsQuery {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        super::SessionBundleRequest {
            session_key: self.session_key.clone(),
            filter: self.filter.clone(),
            price_basis: self.price_basis.clone(),
        }
        .validate()?;
        if !(1..=200).contains(&self.page_size) {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TurnsRequest {
    pub query: TurnsQuery,
    #[schemars(length(min = 151, max = 151))]
    pub cursor: Option<String>,
}
impl TurnsRequest {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        self.query.validate()?;
        if self.cursor.as_ref().is_some_and(|s| {
            s.len() != 151
                || !s
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
pub struct TurnRow {
    #[schemars(length(min = 1, max = 256))]
    pub turn_id: String,
    pub first_at_ms: EpochMs,
    pub last_at_ms: EpochMs,
    /// Whole completed turn, including model calls and tool execution; never inferred from usage times.
    #[serde(default)]
    pub duration_ms: Option<DecimalInt>,
    /// Turn-level first-token wait reported by the source, not per-request latency.
    #[serde(default)]
    pub time_to_first_token_ms: Option<DecimalInt>,
    /// Only selected events in this turn, not its lifetime consumption.
    pub summary: TokenTotals,
    pub pricing: PricingSummary,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TurnsPage {
    pub meta: SnapshotMeta,
    pub session_key: String,
    /// All selected session consumption, including events without turn identity.
    pub summary: TokenTotals,
    pub pricing: PricingSummary,
    pub coverage: Coverage,
    pub unidentified_usage_event_count: DecimalInt,
    #[schemars(length(max = 200))]
    pub turns: Vec<TurnRow>,
    #[schemars(length(min = 151, max = 151))]
    pub next_cursor: Option<String>,
}
