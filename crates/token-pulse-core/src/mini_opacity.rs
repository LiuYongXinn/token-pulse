//! Whole-window native opacity; it does not change the application's theme or privacy.
use crate::{error::ErrorCode, numeric::DecimalInt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const fn default_opacity() -> u8 {
    100
}
pub fn validate_opacity(percent: u8) -> Result<(), ErrorCode> {
    if (70..=100).contains(&percent) {
        Ok(())
    } else {
        Err(ErrorCode::InvalidQuery)
    }
}
pub fn native_alpha(percent: u8) -> Result<u8, ErrorCode> {
    validate_opacity(percent)?;
    Ok(((u16::from(percent) * 255 + 50) / 100) as u8)
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniOpacitySnapshot {
    #[schemars(range(min = 70, max = 100))]
    pub opacity_percent: u8,
    pub supported: bool,
    pub settings_revision: DecimalInt,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MiniOpacityMutation {
    #[schemars(range(min = 70, max = 100))]
    pub opacity_percent: u8,
    pub expected_settings_revision: DecimalInt,
}
impl MiniOpacityMutation {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validate_opacity(self.opacity_percent)?;
        i64::try_from(self.expected_settings_revision.value())
            .map_err(|_| ErrorCode::InvalidQuery)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opacity_has_a_readable_floor_and_literal_independent_native_alpha_expectations() {
        for (percent, alpha) in [
            (70, 179),
            (75, 191),
            (80, 204),
            (90, 230),
            (99, 252),
            (100, 255),
        ] {
            assert_eq!(native_alpha(percent).unwrap(), alpha);
        }
        for percent in [0, 1, 69, 101, 255] {
            assert_eq!(native_alpha(percent), Err(ErrorCode::InvalidQuery));
        }
        assert!(
            serde_json::from_str::<MiniOpacityMutation>(
                r#"{"opacity_percent":80.5,"expected_settings_revision":"1"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<MiniOpacityMutation>(
                r#"{"opacity_percent":80,"expected_settings_revision":"1","extra":true}"#
            )
            .is_err()
        );
    }
}
