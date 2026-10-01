//! Recovery keys exclude Windows-reserved keys and arbitrary native key codes.
use crate::{error::ErrorCode, numeric::DecimalInt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RecoveryShortcut {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    #[schemars(
        length(min = 1, max = 3),
        regex(pattern = r"^(?:[A-Z0-9]|F(?:[1-9]|10|11))$")
    )]
    pub key: String,
}
impl Default for RecoveryShortcut {
    fn default() -> Self {
        Self {
            control: true,
            alt: true,
            shift: true,
            key: "T".into(),
        }
    }
}
impl RecoveryShortcut {
    pub fn virtual_key(&self) -> Result<u32, ErrorCode> {
        if !self.control && !self.alt {
            return Err(ErrorCode::InvalidQuery);
        }
        if self.key.len() == 1 {
            let key = self.key.as_bytes()[0];
            if key.is_ascii_uppercase() || key.is_ascii_digit() {
                return Ok(u32::from(key));
            }
        }
        match self.key.as_str() {
            "F1" => Ok(0x70),
            "F2" => Ok(0x71),
            "F3" => Ok(0x72),
            "F4" => Ok(0x73),
            "F5" => Ok(0x74),
            "F6" => Ok(0x75),
            "F7" => Ok(0x76),
            "F8" => Ok(0x77),
            "F9" => Ok(0x78),
            "F10" => Ok(0x79),
            "F11" => Ok(0x7a),
            _ => Err(ErrorCode::InvalidQuery),
        }
    }
    pub fn modifiers(&self) -> u32 {
        u32::from(self.alt) | (u32::from(self.control) << 1) | (u32::from(self.shift) << 2)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutRegistration {
    Ready,
    Conflict,
    Unsupported,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RecoveryShortcutSnapshot {
    pub shortcut: RecoveryShortcut,
    pub registration: ShortcutRegistration,
    pub settings_revision: DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RecoveryShortcutMutation {
    pub shortcut: RecoveryShortcut,
    pub expected_settings_revision: DecimalInt,
}
impl RecoveryShortcutMutation {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        self.shortcut.virtual_key()?;
        i64::try_from(self.expected_settings_revision.value())
            .map_err(|_| ErrorCode::InvalidQuery)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_allowed_keys_and_modifiers_reject_reserved_or_ambiguous_inputs() {
        let mut key = RecoveryShortcut::default();
        assert_eq!(key.virtual_key().unwrap(), 0x54);
        assert_eq!(key.modifiers(), 7);
        for (value, expected) in [("A", 0x41), ("0", 0x30), ("F1", 0x70), ("F11", 0x7a)] {
            key.key = value.into();
            assert_eq!(key.virtual_key().unwrap(), expected);
        }
        for value in [
            "F12",
            "F01",
            "t",
            "WIN",
            "PrintScreen",
            "",
            "Ｔ",
            "A\n",
            "0x54",
        ] {
            key.key = value.into();
            assert!(key.virtual_key().is_err());
        }
        key.key = "T".into();
        let request = RecoveryShortcutMutation {
            shortcut: RecoveryShortcut::default(),
            expected_settings_revision: DecimalInt::parse("9223372036854775808").unwrap(),
        };
        assert_eq!(request.validate().unwrap_err(), ErrorCode::InvalidQuery);
        key.control = false;
        key.alt = false;
        assert!(key.virtual_key().is_err());
        assert!(
            serde_json::from_str::<RecoveryShortcut>(
                r#"{"control":true,"alt":true,"shift":true,"key":"T","win":true}"#
            )
            .is_err()
        );
    }
}
