//! Persistent taskbar intent; native capability is a separate runtime result.
use crate::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarDisplayLayout {
    TwoRows,
    SingleRow,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskbarDisplayPreferences {
    pub layout: TaskbarDisplayLayout,
    pub show_tokens: bool,
    pub show_costs: bool,
    pub show_quota: bool,
    pub show_weekly_reset: bool,
}
impl Default for TaskbarDisplayPreferences {
    fn default() -> Self {
        Self {
            layout: TaskbarDisplayLayout::TwoRows,
            show_tokens: true,
            show_costs: true,
            show_quota: true,
            show_weekly_reset: true,
        }
    }
}
impl TaskbarDisplayPreferences {
    pub fn validate(self) -> Result<(), ErrorCode> {
        if self.show_tokens || self.show_costs || self.show_quota || self.show_weekly_reset {
            Ok(())
        } else {
            Err(ErrorCode::InvalidQuery)
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarPosition {
    #[default]
    NotificationLeft,
    ApplicationRight,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskbarPreferences {
    pub enabled: bool,
    pub display: TaskbarDisplayPreferences,
    pub position: TaskbarPosition,
    pub fallback_to_mini: bool,
}
impl Default for TaskbarPreferences {
    fn default() -> Self {
        Self {
            enabled: false,
            display: Default::default(),
            position: Default::default(),
            fallback_to_mini: true,
        }
    }
}
impl TaskbarPreferences {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        self.display.validate()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskbarPreferencesSnapshot {
    pub preferences: TaskbarPreferences,
    pub settings_revision: DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskbarPreferencesMutation {
    pub preferences: TaskbarPreferences,
    pub expected_settings_revision: DecimalInt,
}
impl TaskbarPreferencesMutation {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        self.preferences.validate()?;
        i64::try_from(self.expected_settings_revision.value())
            .map_err(|_| ErrorCode::InvalidQuery)?;
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarRuntimeState {
    Disabled,
    Probing,
    WaitingSnapshot,
    Embedded,
    Unavailable,
    Recovering,
    Suspended,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarRuntimeIssue {
    UnsupportedVersion,
    MissingTaskbar,
    UnexpectedStructure,
    UnsafeGeometry,
    InsufficientSpace,
    BackgroundUnavailable,
    HostUnavailable,
    HostTimeout,
    ProtocolError,
    UnsupportedPosition,
    InputUnavailable,
    CleanupUncertain,
    CleanupFailed,
    ExternalLayoutChange,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarCleanupOutcome {
    NoRecord,
    Restored,
    AlreadyRestored,
    ExternalChange,
    IdentityLost,
    Failed,
    Uncertain,
    Timeout,
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskbarRuntimeSnapshot {
    pub revision: DecimalInt,
    pub state: TaskbarRuntimeState,
    pub applied_settings_revision: Option<DecimalInt>,
    pub issue: Option<TaskbarRuntimeIssue>,
    pub error: Option<ErrorCode>,
    pub compact: Option<bool>,
    pub fallback_visible: Option<bool>,
    pub last_cleanup: Option<TaskbarCleanupOutcome>,
    pub last_snapshot_at_ms: Option<EpochMs>,
}
