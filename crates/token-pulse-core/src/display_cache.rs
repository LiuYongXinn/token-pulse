//! Only successful display DTOs can cross a process restart. No cursor or editor state.
use crate::{numeric::DecimalInt, protocol::SnapshotMeta, query::*};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UsageDisplayRequest {
    Dashboard { request: DashboardRequest },
    Groups { request: GroupedUsageRequest },
    Sessions { request: SessionsQuery },
    Events { request: UsageEventsQuery },
}
impl UsageDisplayRequest {
    pub fn validate(&self) -> Result<(), crate::error::ErrorCode> {
        match self {
            Self::Dashboard { request } => request.validate(),
            Self::Groups { request } => request.validate(),
            Self::Sessions { request } => request.validate(),
            Self::Events { request } => request.validate(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UsageDisplayData {
    Dashboard { value: DashboardBundle },
    Groups { value: GroupedUsageBundle },
    Sessions { value: SessionsPage },
    Events { value: UsageEventsPage },
}
impl UsageDisplayData {
    pub fn meta(&self) -> &SnapshotMeta {
        match self {
            Self::Dashboard { value } => &value.meta,
            Self::Groups { value } => &value.meta,
            Self::Sessions { value } => &value.meta,
            Self::Events { value } => &value.meta,
        }
    }
    pub fn strip_cursor(&mut self) -> bool {
        match self {
            Self::Sessions { value } => value.next_cursor.take().is_some(),
            Self::Groups { value } => value.next_cursor.take().is_some(),
            Self::Events { value } => value.next_cursor.take().is_some(),
            _ => false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct UsageDisplaySnapshot {
    pub data: Option<UsageDisplayData>,
    pub has_more: bool,
}
impl crate::privacy::PrivacyRedact for UsageDisplaySnapshot {
    fn redact(&mut self) {
        self.data = None;
        self.has_more = false;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayCacheStamp {
    pub usage: UsageRevision,
    pub settings_revision: DecimalInt,
    pub privacy: bool,
}
