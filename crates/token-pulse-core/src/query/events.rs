//! Invalid raw counters may be negative; only published consumption is nonnegative.
use crate::{
    domain::UsageVector,
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
    pricing::PriceOutcome,
    pricing::request::RequestInputEvidence,
    protocol::{Coverage, PriceBasis, PricingSummary, SnapshotMeta, TokenTotals, UsageFilter},
};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(type = "string")]
pub struct RawTokenCount(i64);
impl RawTokenCount {
    pub fn value(self) -> i64 {
        self.0
    }
}
impl Serialize for RawTokenCount {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for RawTokenCount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let number = value.parse::<i64>().map_err(serde::de::Error::custom)?;
        if number.to_string() != value {
            return Err(serde::de::Error::custom("noncanonical raw counter"));
        }
        Ok(Self(number))
    }
}
impl JsonSchema for RawTokenCount {
    fn schema_name() -> Cow<'static, str> {
        "RawTokenCount".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type":"string","pattern":"^(0|-?[1-9][0-9]*)$","maxLength":20})
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RawUsageVector {
    pub input_total: Option<RawTokenCount>,
    pub cached_input: Option<RawTokenCount>,
    pub cache_write_input: Option<RawTokenCount>,
    pub output_total: Option<RawTokenCount>,
    pub reasoning_output: Option<RawTokenCount>,
    pub reported_total: Option<RawTokenCount>,
}
impl From<UsageVector> for RawUsageVector {
    fn from(value: UsageVector) -> Self {
        Self {
            input_total: value.input_total.map(RawTokenCount),
            cached_input: value.cached_input.map(RawTokenCount),
            cache_write_input: value.cache_write_input.map(RawTokenCount),
            output_total: value.output_total.map(RawTokenCount),
            reasoning_output: value.reasoning_output.map(RawTokenCount),
            reported_total: value.reported_total.map(RawTokenCount),
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum UsageEventSort {
    TimeDesc,
    TotalDesc,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct UsageEventsQuery {
    pub filter: UsageFilter,
    pub price_basis: PriceBasis,
    pub sort: UsageEventSort,
    #[schemars(range(min = 1, max = 200))]
    pub page_size: u16,
}
impl UsageEventsQuery {
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
pub struct UsageEventsRequest {
    pub query: UsageEventsQuery,
    #[schemars(length(min = 151, max = 151))]
    pub cursor: Option<String>,
}
impl UsageEventsRequest {
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
pub struct UsageEventRow {
    pub event_id: String,
    pub session_key: String,
    pub session_display_name: String,
    pub occurred_at_ms: EpochMs,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub project_id: Option<String>,
    pub project_display_name: Option<String>,
    #[schemars(length(max = 32))]
    pub source_ids: Vec<String>,
    pub turn_id: Option<String>,
    pub total_tokens: DecimalInt,
    /// Published increment; raw_last/cumulative retain original source vectors.
    pub usage: RawUsageVector,
    pub raw_last: Option<RawUsageVector>,
    pub raw_cumulative: Option<RawUsageVector>,
    pub request_input: Option<RequestInputEvidence>,
    pub calculation_method: String,
    #[schemars(length(max = 16))]
    pub quality_flags: Vec<String>,
    pub price: PriceOutcome,
    pub parser_version: String,
    pub accounting_version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct UsageEventsPage {
    pub meta: SnapshotMeta,
    pub summary: TokenTotals,
    pub pricing: PricingSummary,
    pub coverage: Coverage,
    #[schemars(length(max = 200))]
    pub events: Vec<UsageEventRow>,
    #[schemars(length(min = 151, max = 151))]
    pub next_cursor: Option<String>,
}
