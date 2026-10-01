//! Rust is the authority for DTOs, generated TypeScript and JSON Schema.
use crate::{
    ServiceState,
    error::{AppError, ErrorCode},
    numeric::{DecimalInt, DecimalMoney, EpochMs},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AppStatus {
    pub version: String,
    pub development: bool,
    pub data_directory: String,
    pub collector: ServiceState,
    pub storage: ServiceState,
    pub storage_error: Option<ErrorCode>,
    pub quota: ServiceState,
    pub taskbar: ServiceState,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct Response<T> {
    #[ts(type = "1")]
    #[schemars(range(min = 1, max = 1))]
    #[serde(deserialize_with = "deserialize_api_version")]
    pub api_version: u32,
    pub request_id: String,
    pub data: T,
}
impl<T> Response<T> {
    pub fn new(request_id: String, data: T) -> Self {
        Self {
            api_version: crate::API_VERSION,
            request_id,
            data,
        }
    }
}

fn deserialize_api_version<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let value = u32::deserialize(d)?;
    if value != crate::API_VERSION {
        return Err(serde::de::Error::custom("UNSUPPORTED_API"));
    }
    Ok(value)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SnapshotMeta {
    pub snapshot_id: String,
    pub data_revision: DecimalInt,
    pub price_revision: DecimalInt,
    pub generated_at_ms: EpochMs,
    pub parser_versions: Vec<String>,
    pub accounting_versions: Vec<String>,
    pub display_timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DateRange {
    pub start_ms: EpochMs,
    pub end_ms: EpochMs,
    pub timezone: String,
}
impl DateRange {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.start_ms >= self.end_ms || self.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DimensionSelection {
    All {},
    Ids {
        #[schemars(length(max = 100))]
        ids: Vec<String>,
        include_unknown: bool,
    },
}
impl DimensionSelection {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if let Self::Ids { ids, .. } = self {
            if ids.len() > 100
                || ids
                    .iter()
                    .any(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
            {
                return Err(ErrorCode::InvalidQuery);
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct UsageFilter {
    pub range: DateRange,
    pub sources: DimensionSelection,
    pub models: DimensionSelection,
    pub projects: DimensionSelection,
    pub sessions: DimensionSelection,
}
impl UsageFilter {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        self.range.validate()?;
        for selection in [&self.sources, &self.models, &self.projects, &self.sessions] {
            selection.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum PriceBasis {
    EventTime {},
    SpecifiedTime { specified_at_ms: EpochMs },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TokenMeasure {
    pub value: Option<DecimalInt>,
    pub covered_total_tokens: DecimalInt,
    pub complete: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TokenTotals {
    pub total_tokens: DecimalInt,
    pub input_total: TokenMeasure,
    pub cached_input: TokenMeasure,
    pub noncached_input: TokenMeasure,
    pub output_total: TokenMeasure,
    pub reasoning_output: TokenMeasure,
    pub session_count: DecimalInt,
    pub usage_event_count: DecimalInt,
    pub reliable_turn_count: Option<DecimalInt>,
    pub reliable_turns_complete: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct CurrencyEstimate {
    pub currency: String,
    pub estimated_cost: Option<DecimalMoney>,
    pub priced_total_tokens: DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct UnpricedReason {
    pub code: String,
    pub total_tokens: DecimalInt,
    pub event_count: DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct PricingSummary {
    pub redacted: bool,
    pub basis: PriceBasis,
    pub currencies: Vec<CurrencyEstimate>,
    pub priced_total_tokens: DecimalInt,
    pub unpriced_total_tokens: DecimalInt,
    pub reasons: Vec<UnpricedReason>,
    pub calculating: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    Complete,
    Partial,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SourceIssue {
    pub source_id: String,
    pub code: String,
    pub last_success_ms: Option<EpochMs>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FormatIssue {
    pub format: String,
    pub count: DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub state: CoverageState,
    pub pending_observation_count: DecimalInt,
    pub unattributed_observation_count: DecimalInt,
    pub unattributed_total_tokens: Option<DecimalInt>,
    pub pending_file_count: DecimalInt,
    pub source_issues: Vec<SourceIssue>,
    pub format_issues: Vec<FormatIssue>,
    pub breakdown_complete: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct ContextSnapshot {
    pub context_tokens: Option<DecimalInt>,
    pub model_context_window: Option<DecimalInt>,
    pub percentage: Option<f64>,
    pub observed_at_ms: Option<EpochMs>,
    pub quality: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScopeStart {
    Today {},
    Fixed { start_ms: EpochMs },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MiniScope {
    TodayAllSources {},
    Session {
        session_key: String,
        start: ScopeStart,
    },
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum QuotaState {
    Disconnected,
    Connecting,
    AuthorizationRequired,
    Unsupported,
    Ready,
    Stale,
    Error,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct QuotaWindow {
    pub window_id: String,
    pub duration_mins: Option<u32>,
    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub resets_at_ms: Option<EpochMs>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct QuotaLimit {
    pub limit_id: String,
    pub display_name: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct QuotaSnapshot {
    pub connection_epoch: String,
    pub quota_revision: DecimalInt,
    pub state: QuotaState,
    pub selected_limit_id: Option<String>,
    pub available_limits: Vec<QuotaLimit>,
    pub fetched_at_ms: Option<EpochMs>,
    pub last_attempt_at_ms: Option<EpochMs>,
    pub windows: Vec<QuotaWindow>,
    pub error_code: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniSnapshot {
    pub usage_meta: SnapshotMeta,
    pub mini_scope: MiniScope,
    pub scope_display_name: Option<String>,
    pub usage: TokenTotals,
    pub pricing: PricingSummary,
    pub coverage: Coverage,
    pub quota: QuotaSnapshot,
    pub privacy: bool,
    pub usage_last_success_ms: Option<EpochMs>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Import,
    Reconcile,
    Rebuild,
    Export,
    Backup,
    Restore,
    PriceRevalue,
    Clear,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Validating,
    Publishing,
    Cancelling,
    Succeeded,
    Cancelled,
    Failed,
    Interrupted,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub job_id: String,
    pub kind: JobKind,
    pub state: JobState,
    pub phase: String,
    pub discovered_files: DecimalInt,
    pub discovery_complete: bool,
    pub processed_files: DecimalInt,
    pub processed_bytes: DecimalInt,
    pub accepted_events: DecimalInt,
    pub pending_observations: DecimalInt,
    pub can_cancel: bool,
    pub error: Option<AppError>,
    pub created_at_ms: EpochMs,
    pub updated_at_ms: EpochMs,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum WindowAction {
    OpenStats,
    HideMain,
    Quit,
}

pub fn validate_request_id(id: &str) -> Result<(), ErrorCode> {
    if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
        return Err(ErrorCode::InvalidQuery);
    }
    Ok(())
}
