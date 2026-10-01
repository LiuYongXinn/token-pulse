//! Shared floating/taskbar usage scope. It never accepts the main window's filter.
use crate::{
    calendar::{CalendarSelection, CalendarSelectionRequest, resolve_selection},
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
    protocol::{
        Coverage, DateRange, DimensionSelection, MiniScope, PriceBasis, PricingSummary, ScopeStart,
        SnapshotMeta, TokenTotals, UsageFilter,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Geometry and pin state. Cursor pass-through has a separate recovery-bound contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniWindowState {
    pub expanded: bool,
    pub pinned: bool,
}
impl Default for MiniWindowState {
    fn default() -> Self {
        Self {
            expanded: false,
            pinned: true,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MiniWindowAction {
    Read {},
    SetExpanded { expanded: bool },
    SetPinned { pinned: bool },
    Drag {},
    Hide {},
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniSessionsQuery {
    #[schemars(length(max = 256))]
    pub search: String,
    #[schemars(range(min = 1, max = 100))]
    pub page_size: u16,
}
impl MiniSessionsQuery {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.search.chars().count() > 256
            || self.search.chars().any(char::is_control)
            || !(1..=100).contains(&self.page_size)
        {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniSessionsRequest {
    pub query: MiniSessionsQuery,
    #[schemars(length(min = 151, max = 151))]
    pub cursor: Option<String>,
}
impl MiniSessionsRequest {
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
pub struct MiniSessionOption {
    pub session_key: String,
    pub display_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniSessionsPage {
    pub meta: SnapshotMeta,
    #[schemars(length(max = 100))]
    pub options: Vec<MiniSessionOption>,
    pub next_cursor: Option<String>,
}
impl crate::privacy::PrivacyRedact for MiniSessionsPage {
    fn redact(&mut self) {
        for option in &mut self.options {
            option.display_name = crate::privacy::alias("会话", Some(&option.session_key));
        }
    }
}

impl MiniScope {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        match self {
            Self::TodayAllSources {} => Ok(()),
            Self::Session { session_key, .. } => DimensionSelection::Ids {
                ids: vec![session_key.clone()],
                include_unknown: false,
            }
            .validate(),
        }
    }
}
impl Default for MiniScope {
    fn default() -> Self {
        Self::TodayAllSources {}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniScopeMutation {
    pub mini_scope: MiniScope,
    pub expected_settings_revision: DecimalInt,
}
impl MiniScopeMutation {
    pub fn validate(&self, at: EpochMs) -> Result<(), ErrorCode> {
        self.mini_scope.validate()?;
        i64::try_from(self.expected_settings_revision.value())
            .map_err(|_| ErrorCode::InvalidQuery)?;
        if let MiniScope::Session {
            start: ScopeStart::Fixed { start_ms },
            ..
        } = self.mini_scope
        {
            if start_ms > at {
                return Err(ErrorCode::InvalidQuery);
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniScopeSnapshot {
    pub settings_revision: DecimalInt,
    pub mini_scope: MiniScope,
}
/// Atomic local usage part. Account service data is deliberately composed outside this transaction.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniUsageSnapshot {
    pub meta: SnapshotMeta,
    pub settings_revision: DecimalInt,
    pub mini_scope: MiniScope,
    pub scope_display_name: Option<String>,
    pub range: DateRange,
    pub usage: TokenTotals,
    pub pricing: PricingSummary,
    pub coverage: Coverage,
}
/// Explicit navigation intent. It contains no names, paths, prices or account fields.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniStatsRequest {
    pub request_id: String,
    pub mini_scope: MiniScope,
    pub calendar: crate::calendar::CalendarSelectionResult,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniStatsOpenRequest {
    pub expected_settings_revision: DecimalInt,
}
pub fn open_stats_request(
    usage: &MiniUsageSnapshot,
    request: &MiniStatsOpenRequest,
    id: String,
) -> Result<MiniStatsRequest, ErrorCode> {
    if request.expected_settings_revision.value() != usage.settings_revision.value() {
        return Err(ErrorCode::RevisionConflict);
    }
    stats_request(usage, id)
}
pub fn stats_request(usage: &MiniUsageSnapshot, id: String) -> Result<MiniStatsRequest, ErrorCode> {
    crate::protocol::validate_request_id(&id)?;
    usage.range.validate()?;
    usage.mini_scope.validate()?;
    let mut calendar = resolve_selection(
        &CalendarSelectionRequest {
            timezone: usage.range.timezone.clone(),
            selection: CalendarSelection::Today {},
        },
        usage.meta.generated_at_ms,
    )?;
    // Preserve the exact UTC bounds already read with the mini scope. Never round a fixed start.
    calendar.range = usage.range.clone();
    Ok(MiniStatsRequest {
        request_id: id,
        mini_scope: usage.mini_scope.clone(),
        calendar,
    })
}
pub fn usage_filter(
    scope: &MiniScope,
    timezone: &str,
    at: EpochMs,
) -> Result<UsageFilter, ErrorCode> {
    scope.validate()?;
    let today = resolve_selection(
        &CalendarSelectionRequest {
            timezone: timezone.into(),
            selection: CalendarSelection::Today {},
        },
        at,
    )?;
    let start = match scope {
        MiniScope::Session {
            start: ScopeStart::Fixed { start_ms },
            ..
        } => *start_ms,
        _ => today.range.start_ms,
    };
    if start > at {
        return Err(ErrorCode::InvalidQuery);
    }
    // Include the captured millisecond explicitly; use a half-open bound for all SQL reads.
    let end = EpochMs::new(
        at.value()
            .checked_add(1)
            .ok_or(ErrorCode::NumericOverflow)?,
    )?;
    let filter = UsageFilter {
        range: DateRange {
            start_ms: start,
            end_ms: end,
            timezone: today.range.timezone,
        },
        sources: DimensionSelection::All {},
        models: DimensionSelection::All {},
        projects: DimensionSelection::All {},
        sessions: match scope {
            MiniScope::TodayAllSources {} => DimensionSelection::All {},
            MiniScope::Session { session_key, .. } => DimensionSelection::Ids {
                ids: vec![session_key.clone()],
                include_unknown: false,
            },
        },
    };
    filter.validate()?;
    Ok(filter)
}
pub fn price_basis() -> PriceBasis {
    PriceBasis::EventTime {}
}

impl crate::privacy::PrivacyRedact for MiniScopeSnapshot {
    fn redact(&mut self) {}
}
impl crate::privacy::PrivacyRedact for MiniUsageSnapshot {
    fn redact(&mut self) {
        self.pricing.redact();
        match &self.mini_scope {
            MiniScope::TodayAllSources {} => {}
            MiniScope::Session { session_key, .. } => {
                if self.scope_display_name.is_some() {
                    self.scope_display_name =
                        Some(crate::privacy::alias("会话", Some(session_key)));
                }
            }
        }
    }
}
