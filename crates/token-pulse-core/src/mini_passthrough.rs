//! Explicit cursor pass-through authorization is bound to the acknowledged recovery key.
use crate::{
    error::ErrorCode,
    numeric::DecimalInt,
    shortcuts::{RecoveryShortcut, ShortcutRegistration},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniPassthroughSnapshot {
    pub enabled: bool,
    pub persisted_enabled: bool,
    pub window_present: bool,
    pub supported: bool,
    pub recovery_shortcut: RecoveryShortcut,
    pub recovery_registration: ShortcutRegistration,
    pub settings_revision: DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniPassthroughMutation {
    pub enabled: bool,
    pub acknowledged_recovery: Option<RecoveryShortcut>,
    pub expected_settings_revision: DecimalInt,
}
impl MiniPassthroughMutation {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        i64::try_from(self.expected_settings_revision.value())
            .map_err(|_| ErrorCode::InvalidQuery)?;
        match (&self.acknowledged_recovery, self.enabled) {
            (Some(key), true) => {
                key.virtual_key()?;
                Ok(())
            }
            (None, false) => Ok(()),
            _ => Err(ErrorCode::InvalidQuery),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enable_requires_explicit_valid_key_and_disable_needs_no_recovery() {
        let mut request = MiniPassthroughMutation {
            enabled: true,
            acknowledged_recovery: None,
            expected_settings_revision: DecimalInt::parse("9007199254740993").unwrap(),
        };
        assert_eq!(request.validate(), Err(ErrorCode::InvalidQuery));
        request.acknowledged_recovery = Some(RecoveryShortcut::default());
        assert!(request.validate().is_ok());
        request.enabled = false;
        assert!(request.validate().is_err());
        request.acknowledged_recovery = None;
        assert!(request.validate().is_ok());
        request.expected_settings_revision = DecimalInt::parse("9223372036854775808").unwrap();
        assert!(request.validate().is_err());
        assert!(serde_json::from_str::<MiniPassthroughMutation>(r#"{"enabled":true,"acknowledged_recovery":null,"expected_settings_revision":"1","hwnd":1}"#).is_err());
    }
}
