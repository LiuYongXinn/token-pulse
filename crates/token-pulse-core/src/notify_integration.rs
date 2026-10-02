//! Public notify management contracts. Commands, credentials and full configs are never inputs.
use crate::{error::ErrorCode, numeric::DecimalInt, privacy::PrivacyRedact};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum NotifyIssue {
    Unavailable,
    TransactionUnavailable,
    Busy,
    PermissionDenied,
    UnsafePath,
    UnsafeFile,
    UnsafePermissions,
    InvalidConfig,
    InvalidNotify,
    AlreadyManaged,
    ConfigChanged,
    OwnershipChanged,
    InvalidRegistration,
    InvalidMarker,
    LimitReached,
    AlreadyExists,
    NotFound,
    NoOriginalCommand,
    PlanNotFound,
    PlanExpired,
    PlanLimit,
    ActiveConfiguration,
    CleanupFailed,
    WrongExecutable,
    ChannelUnavailable,
    WorkerUnavailable,
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum NotifyConfigOperation {
    Enable,
    Disable,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NotifyPrepareAction {
    EnableSource {
        source_id: String,
        chain_original: Option<bool>,
    },
    ChooseHome {
        chain_original: Option<bool>,
    },
    Disable {
        registration_id: String,
    },
}
impl NotifyPrepareAction {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        match self {
            Self::EnableSource { source_id, .. }
                if source_id.is_empty()
                    || source_id.len() > 512
                    || source_id.chars().any(char::is_control) =>
            {
                Err(ErrorCode::InvalidQuery)
            }
            Self::Disable { registration_id } => validate_notify_id(registration_id),
            _ => Ok(()),
        }
    }
}
pub fn validate_notify_id(id: &str) -> Result<(), ErrorCode> {
    if id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(ErrorCode::InvalidQuery)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct NotifyIntegrationRow {
    pub registration_id: String,
    pub home_path: Option<String>,
    pub configured: Option<bool>,
    pub current_executable: Option<bool>,
    pub chain_original: Option<bool>,
    pub issue: Option<NotifyIssue>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct NotifyIntegrationsSnapshot {
    pub ready: bool,
    pub listener_count: Option<u8>,
    pub service_issue: Option<NotifyIssue>,
    /// null means enumeration failed, distinct from no registrations.
    pub registrations: Option<Vec<NotifyIntegrationRow>>,
    pub registry_issue: Option<NotifyIssue>,
    pub redacted: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct NotifyConfigPreview {
    pub plan_id: String,
    pub registration_id: String,
    pub operation: NotifyConfigOperation,
    pub home_path: Option<String>,
    pub before_notify: Option<String>,
    pub after_notify: Option<String>,
    pub creates_config: bool,
    pub can_chain_original: bool,
    pub chain_original: bool,
    pub settings_revision: DecimalInt,
    pub expires_in_seconds: u16,
    pub redacted: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct NotifyApplyResult {
    pub registration_id: String,
    pub configured: Option<bool>,
    pub retired: bool,
    pub cleanup_issue: Option<NotifyIssue>,
}
impl PrivacyRedact for NotifyIntegrationsSnapshot {
    fn redact(&mut self) {
        self.redacted = true;
        if let Some(rows) = &mut self.registrations {
            for row in rows {
                row.home_path = None;
            }
        }
    }
}
impl PrivacyRedact for NotifyConfigPreview {
    fn redact(&mut self) {
        self.redacted = true;
        self.home_path = None;
        self.before_notify = None;
        self.after_notify = None;
    }
}
impl PrivacyRedact for NotifyApplyResult {
    fn redact(&mut self) {}
}
