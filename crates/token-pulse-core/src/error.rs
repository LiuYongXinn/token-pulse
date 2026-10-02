use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidQuery,
    UnsupportedApi,
    SourceUnreadable,
    UnsupportedFormat,
    UnsupportedSettingsVersion,
    AmbiguousUsage,
    CheckpointConflict,
    CandidateObsolete,
    DbWriteFailed,
    DiskFull,
    DbCorrupt,
    MigrationFailed,
    SnapshotExpired,
    CursorInvalid,
    RevisionConflict,
    StaleConfirmation,
    RequestKeyConflict,
    PriceRuleConflict,
    JobCancelled,
    JobInterrupted,
    QuotaDisconnected,
    QuotaUnsupported,
    QuotaTimeout,
    QuotaAuthRequired,
    QuotaProtocolError,
    QuotaServiceUnavailable,
    TaskbarUnsupported,
    TaskbarNoSpace,
    TaskbarEmbedFailed,
    NumericOverflow,
    PermissionDenied,
    InvalidUsage,
    WindowUnavailable,
    ShortcutConflict,
    ShortcutUnavailable,
    NotifyIntegrationFailed,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_value(self)
                .expect("enum serialization")
                .as_str()
                .expect("enum string")
        )
    }
}
impl std::error::Error for ErrorCode {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum ErrorDetail {
    Text(String),
    Number(f64),
    Boolean(bool),
    Null(()),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AppError {
    pub code: ErrorCode,
    pub message_key: String,
    pub retryable: bool,
    pub correlation_id: String,
    pub source_id: Option<String>,
    pub job_id: Option<String>,
    pub details: BTreeMap<String, ErrorDetail>,
}

impl AppError {
    pub fn new(code: ErrorCode, correlation_id: String) -> Self {
        Self {
            code,
            message_key: format!("errors.{code}"),
            retryable: matches!(
                code,
                ErrorCode::SourceUnreadable
                    | ErrorCode::QuotaTimeout
                    | ErrorCode::CheckpointConflict
            ),
            correlation_id,
            source_id: None,
            job_id: None,
            details: BTreeMap::new(),
        }
    }
}
