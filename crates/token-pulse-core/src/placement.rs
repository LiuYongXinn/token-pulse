//! Saved native placement is relative to a monitor's working area in DIP.
use crate::{error::ErrorCode, mini::MiniWindowState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowPlacement {
    pub monitor: Option<String>,
    pub offset_x_dip: f64,
    pub offset_y_dip: f64,
}
impl WindowPlacement {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.monitor.as_ref().is_some_and(|name| {
            name.is_empty() || name.len() > 256 || name.chars().any(char::is_control)
        }) || [self.offset_x_dip, self.offset_y_dip]
            .into_iter()
            .any(|n| !n.is_finite() || n.abs() > 1_000_000.0)
        {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MiniWindowPreferences {
    pub interaction: MiniWindowState,
    pub placement: Option<WindowPlacement>,
    #[serde(default = "crate::mini_opacity::default_opacity")]
    pub opacity_percent: u8,
    #[serde(default)]
    pub passthrough: bool,
}
impl Default for MiniWindowPreferences {
    fn default() -> Self {
        Self {
            interaction: Default::default(),
            placement: None,
            opacity_percent: 100,
            passthrough: false,
        }
    }
}
impl MiniWindowPreferences {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        crate::mini_opacity::validate_opacity(self.opacity_percent)?;
        self.placement
            .as_ref()
            .map_or(Ok(()), WindowPlacement::validate)
    }
}
/// Internal native updates change one field and preserve the latest unrelated configuration.
pub enum MiniPreferenceChange {
    Passthrough(bool),
    Expanded(bool),
    Pinned(bool),
    Placement(WindowPlacement),
}

#[derive(Debug, Clone, Copy)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}
impl WorkArea {
    fn validate(&self) -> Result<(), ErrorCode> {
        if self.width == 0
            || self.height == 0
            || self.width > 1_000_000
            || self.height > 1_000_000
            || !self.scale.is_finite()
            || !(0.25..=8.0).contains(&self.scale)
        {
            return Err(ErrorCode::WindowUnavailable);
        }
        Ok(())
    }
    pub fn capture(
        &self,
        x: i32,
        y: i32,
        monitor: Option<String>,
    ) -> Result<WindowPlacement, ErrorCode> {
        self.validate()?;
        let placement = WindowPlacement {
            monitor,
            offset_x_dip: (f64::from(x) - f64::from(self.x)) / self.scale,
            offset_y_dip: (f64::from(y) - f64::from(self.y)) / self.scale,
        };
        placement.validate()?;
        Ok(placement)
    }
    /// Rounded physical coordinates retain the native title area even on a very small work area.
    pub fn restore(
        &self,
        placement: &WindowPlacement,
        width_dip: f64,
        height_dip: f64,
    ) -> Result<(i32, i32), ErrorCode> {
        self.validate()?;
        placement.validate()?;
        if [width_dip, height_dip]
            .into_iter()
            .any(|n| !n.is_finite() || n <= 0.0 || n > 1_000_000.0)
        {
            return Err(ErrorCode::InvalidQuery);
        }
        let x = f64::from(self.x)
            + (placement.offset_x_dip * self.scale).clamp(
                0.0,
                (f64::from(self.width) - width_dip * self.scale).max(0.0),
            );
        let y = f64::from(self.y)
            + (placement.offset_y_dip * self.scale).clamp(
                0.0,
                (f64::from(self.height) - height_dip * self.scale).max(0.0),
            );
        if [x, y]
            .into_iter()
            .any(|n| n.round() < f64::from(i32::MIN) || n.round() > f64::from(i32::MAX))
        {
            return Err(ErrorCode::NumericOverflow);
        }
        Ok((x.round() as i32, y.round() as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dpi_work_area_changes_and_disconnected_monitor_keep_exact_dip_offsets_visible() {
        let old = WorkArea {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1040,
            scale: 1.0,
        };
        let saved = old.capture(-1820, 150, Some("left".into())).unwrap();
        assert_eq!(saved.offset_x_dip, 100.0);
        let new = WorkArea {
            x: 0,
            y: 50,
            width: 2560,
            height: 1390,
            scale: 2.0,
        };
        assert_eq!(new.restore(&saved, 360.0, 380.0).unwrap(), (200, 350));
        let docked = WorkArea {
            x: -1280,
            y: 40,
            width: 1280,
            height: 640,
            scale: 1.25,
        };
        let distant = WindowPlacement {
            monitor: Some("disconnected".into()),
            offset_x_dip: 8000.0,
            offset_y_dip: 8000.0,
        };
        assert_eq!(docked.restore(&distant, 360.0, 380.0).unwrap(), (-450, 205));
        let outside = WindowPlacement {
            offset_x_dip: -200.0,
            offset_y_dip: -20.0,
            ..saved
        };
        assert_eq!(docked.restore(&outside, 280.0, 220.0).unwrap(), (-1280, 40));
        let tiny = WorkArea {
            width: 300,
            height: 100,
            ..new
        };
        assert_eq!(tiny.restore(&outside, 360.0, 380.0).unwrap(), (0, 50));
    }
    #[test]
    fn invalid_coordinates_and_unknown_fields_never_become_default_positions() {
        let area = WorkArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
            scale: 1.0,
        };
        for scale in [f64::NAN, f64::INFINITY, 0.0, 9.0] {
            assert!(WorkArea { scale, ..area }.capture(0, 0, None).is_err());
        }
        for x in [f64::NAN, f64::INFINITY, 1_000_001.0] {
            assert!(
                WindowPlacement {
                    monitor: None,
                    offset_x_dip: x,
                    offset_y_dip: 0.0
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            serde_json::from_str::<WindowPlacement>(
                r#"{"monitor":null,"offset_x_dip":0,"offset_y_dip":0,"path":"other"}"#
            )
            .is_err()
        );
        assert!(
            WindowPlacement {
                monitor: Some("bad\nname".into()),
                offset_x_dip: 0.0,
                offset_y_dip: 0.0
            }
            .validate()
            .is_err()
        );
    }
}
