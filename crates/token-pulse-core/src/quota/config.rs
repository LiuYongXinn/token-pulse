use crate::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

/// Internal configuration. The renderer never submits or receives this executable target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountServiceTarget {
    pub executable_path: String,
    pub home_path: Option<String>,
    pub executable_sha256: String,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountServicePreferences {
    pub target: Option<AccountServiceTarget>,
    pub auto_connect: bool,
}
impl AccountServiceTarget {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        fn path(value: &str) -> Result<(), ErrorCode> {
            if value.len() > 32767
                || value.chars().any(char::is_control)
                || !Path::new(value).is_absolute()
            {
                return Err(ErrorCode::InvalidQuery);
            }
            #[cfg(windows)]
            crate::sources::validate_root(Path::new(value), crate::sources::SourceOrigin::Custom)?;
            Ok(())
        }
        path(&self.executable_path)?;
        if let Some(home) = &self.home_path {
            path(home)?;
        }
        #[cfg(windows)]
        if !Path::new(&self.executable_path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return Err(ErrorCode::InvalidQuery);
        }
        if !valid_fingerprint(&self.executable_sha256) {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}
pub fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl AccountServicePreferences {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if let Some(target) = &self.target {
            target.validate()?;
        }
        if self.auto_connect && self.target.is_none() {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AccountServiceConfigSnapshot {
    pub settings_revision: DecimalInt,
    pub executable_display_path: Option<String>,
    pub home_display_path: Option<String>,
    pub executable_sha256: Option<String>,
    pub configured: bool,
    pub auto_connect: bool,
}
impl AccountServiceConfigSnapshot {
    pub fn from_preferences(preferences: &AccountServicePreferences, revision: DecimalInt) -> Self {
        Self {
            settings_revision: revision,
            executable_display_path: preferences
                .target
                .as_ref()
                .map(|t| t.executable_path.clone()),
            home_display_path: preferences
                .target
                .as_ref()
                .and_then(|t| t.home_path.clone()),
            executable_sha256: preferences
                .target
                .as_ref()
                .map(|t| t.executable_sha256.clone()),
            configured: preferences.target.is_some(),
            auto_connect: preferences.auto_connect,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AccountServiceSelectionKind {
    DetectLocal,
    Current,
    Executable,
    Home,
    DefaultHome,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AccountServiceSelectionRequest {
    pub kind: AccountServiceSelectionKind,
    pub base_selection_handle: Option<String>,
    pub expected_settings_revision: DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AccountServiceSelection {
    pub selection_handle: String,
    pub preview: AccountServiceConfigSnapshot,
    pub expires_at_ms: EpochMs,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AccountServiceConfigMutation {
    pub selection_handle: String,
    pub auto_connect: bool,
    pub expected_settings_revision: DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccountConnectionRequest {
    Connect {
        expected_settings_revision: DecimalInt,
        expected_connection_epoch: String,
        acknowledged_executable_sha256: String,
    },
    Disconnect {
        expected_connection_epoch: String,
    },
    SelectLimit {
        expected_connection_epoch: String,
        expected_quota_revision: DecimalInt,
        limit_id: String,
    },
}
impl AccountConnectionRequest {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        let (epoch, revision) = match self {
            Self::Connect {
                expected_settings_revision,
                expected_connection_epoch,
                acknowledged_executable_sha256,
            } => {
                if !valid_fingerprint(acknowledged_executable_sha256) {
                    return Err(ErrorCode::InvalidQuery);
                }
                (expected_connection_epoch, Some(expected_settings_revision))
            }
            Self::Disconnect {
                expected_connection_epoch,
            } => (expected_connection_epoch, None),
            Self::SelectLimit {
                expected_connection_epoch,
                expected_quota_revision,
                limit_id,
            } => {
                if limit_id.is_empty()
                    || limit_id.len() > 256
                    || limit_id.chars().any(char::is_control)
                {
                    return Err(ErrorCode::InvalidQuery);
                }
                (expected_connection_epoch, Some(expected_quota_revision))
            }
        };
        if epoch.is_empty() || epoch.len() > 128 || epoch.chars().any(char::is_control) {
            return Err(ErrorCode::InvalidQuery);
        }
        if matches!(self, Self::Connect { .. }) {
            i64::try_from(revision.expect("connect revision").value())
                .map_err(|_| ErrorCode::InvalidQuery)?;
        }
        Ok(())
    }
}
