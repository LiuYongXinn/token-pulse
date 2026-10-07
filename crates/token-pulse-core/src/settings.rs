//! Application display configuration, independent of data and price revisions.
use crate::{error::ErrorCode, numeric::DecimalInt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub const SETTINGS_VERSION: u32 = 1;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AppTheme {
    Dark,
    #[default]
    Light,
    System,
}

#[cfg(test)]
mod theme_tests {
    use super::{AppTheme, DisplayPreferences};

    #[test]
    fn new_preferences_use_silver_mist_without_replacing_saved_themes() {
        assert_eq!(DisplayPreferences::default().theme, AppTheme::Light);
        for (saved, expected) in [
            ("dark", AppTheme::Dark),
            ("light", AppTheme::Light),
            ("system", AppTheme::System),
        ] {
            let preferences: DisplayPreferences = serde_json::from_str(&format!(
                r#"{{"display_timezone":"Asia/Shanghai","privacy":true,"theme":"{saved}"}}"#
            ))
            .unwrap();
            assert_eq!(preferences.theme, expected);
            assert!(preferences.privacy);
            assert_eq!(
                preferences.display_timezone.as_deref(),
                Some("Asia/Shanghai")
            );
        }
    }
}
pub fn validate_timezone(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty() || value.len() > 128 || value.parse::<chrono_tz::Tz>().is_err() {
        return Err(ErrorCode::InvalidQuery);
    }
    Ok(())
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DisplayPreferences {
    /// None means not initialized, never an implicit UTC/system fallback.
    #[schemars(length(min = 1, max = 128))]
    pub display_timezone: Option<String>,
    pub privacy: bool,
    pub theme: AppTheme,
}
impl DisplayPreferences {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if let Some(value) = &self.display_timezone {
            validate_timezone(value)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DisplaySettingsSnapshot {
    pub settings_version: u32,
    pub settings_revision: DecimalInt,
    pub preferences: DisplayPreferences,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimezoneMutation {
    Initialize {
        system_timezone: String,
    },
    Set {
        display_timezone: String,
        expected_settings_revision: DecimalInt,
    },
}
impl TimezoneMutation {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        match self {
            Self::Initialize { system_timezone } => validate_timezone(system_timezone),
            Self::Set {
                display_timezone,
                expected_settings_revision,
            } => {
                validate_timezone(display_timezone)?;
                i64::try_from(expected_settings_revision.value())
                    .map_err(|_| ErrorCode::InvalidQuery)?;
                Ok(())
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SettingsChanged {
    pub settings_revision: DecimalInt,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DisplayPrivacyMutation {
    pub privacy: bool,
    pub expected_settings_revision: DecimalInt,
}
impl DisplayPrivacyMutation {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        i64::try_from(self.expected_settings_revision.value())
            .map_err(|_| ErrorCode::InvalidQuery)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DisplayThemeMutation {
    pub theme: AppTheme,
    pub expected_settings_revision: DecimalInt,
}
impl DisplayThemeMutation {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        i64::try_from(self.expected_settings_revision.value())
            .map_err(|_| ErrorCode::InvalidQuery)?;
        Ok(())
    }
}
