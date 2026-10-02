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

/// Main-window location is independent of the floating display's scope and interaction state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MainWindowPreferences {
    pub placement: Option<WindowPlacement>,
}
impl MainWindowPreferences {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        self.placement
            .as_ref()
            .map_or(Ok(()), WindowPlacement::validate)
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
    /// Fit the client and its preferred minimum inside the work area, including decorations.
    /// Round available physical pixels down so fractional DPI cannot put an edge off screen.
    pub fn fit_client(
        &self,
        desired: (f64, f64),
        preferred_minimum: (f64, f64),
        frame: (f64, f64),
    ) -> Result<ClientFit, ErrorCode> {
        self.validate()?;
        if [
            desired.0,
            desired.1,
            preferred_minimum.0,
            preferred_minimum.1,
        ]
        .into_iter()
        .any(|n| !n.is_finite() || n <= 0.0 || n > 1_000_000.0)
            || [frame.0, frame.1]
                .into_iter()
                .any(|n| !n.is_finite() || !(0.0..=1_000_000.0).contains(&n))
        {
            return Err(ErrorCode::InvalidQuery);
        }
        let available = (
            (f64::from(self.width) - frame.0 * self.scale).floor() / self.scale,
            (f64::from(self.height) - frame.1 * self.scale).floor() / self.scale,
        );
        if available.0 < 1.0 || available.1 < 1.0 {
            return Err(ErrorCode::WindowUnavailable);
        }
        let minimum = (
            preferred_minimum.0.min(available.0),
            preferred_minimum.1.min(available.1),
        );
        Ok(ClientFit {
            minimum,
            size: (
                desired.0.clamp(minimum.0, available.0),
                desired.1.clamp(minimum.1, available.1),
            ),
        })
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
                (f64::from(self.width) - (width_dip * self.scale).ceil()).max(0.0),
            );
        let y = f64::from(self.y)
            + (placement.offset_y_dip * self.scale).clamp(
                0.0,
                (f64::from(self.height) - (height_dip * self.scale).ceil()).max(0.0),
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClientFit {
    pub minimum: (f64, f64),
    pub size: (f64, f64),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decorated_main_fits_all_dpi_levels_without_hiding_edges_or_permanently_lowering_minimum() {
        // Fixed 2560x1376 physical work area; independent expected client geometry.
        for (scale, expected) in [
            (1.0, (1280.0, 860.0)),
            (1.25, (1280.0, 860.0)),
            (1.5, (1280.0, 860.0)),
            (2.0, (1264.0, 649.0)),
        ] {
            let area = WorkArea {
                x: -2560,
                y: 40,
                width: 2560,
                height: 1376,
                scale,
            };
            let fit = area
                .fit_client((1280.0, 860.0), (960.0, 680.0), (16.0, 39.0))
                .unwrap();
            assert_eq!(fit.size, expected);
            assert_eq!(fit.minimum, (960.0, 680.0_f64.min(expected.1)));
            let saved = WindowPlacement {
                monitor: None,
                offset_x_dip: 8000.0,
                offset_y_dip: 8000.0,
            };
            let (x, y) = area
                .restore(&saved, fit.size.0 + 16.0, fit.size.1 + 39.0)
                .unwrap();
            assert!(f64::from(x) + (fit.size.0 + 16.0) * scale <= 0.0);
            assert!(f64::from(y) + (fit.size.1 + 39.0) * scale <= 1416.0);
        }
        let small = WorkArea {
            x: 0,
            y: 0,
            width: 1201,
            height: 751,
            scale: 1.25,
        };
        let fit = small
            .fit_client((1280.0, 860.0), (960.0, 680.0), (16.0, 39.0))
            .unwrap();
        assert_eq!(fit.size, (944.8, 561.6));
        let larger = WorkArea {
            width: 2560,
            height: 1376,
            scale: 1.0,
            ..small
        };
        let restored = larger
            .fit_client(fit.size, (960.0, 680.0), (16.0, 39.0))
            .unwrap();
        assert_eq!(restored.minimum, (960.0, 680.0));
        assert_eq!(restored.size, (960.0, 680.0));
    }
    #[test]
    fn invalid_client_geometry_and_work_area_smaller_than_frame_are_rejected() {
        let area = WorkArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
            scale: 1.0,
        };
        for desired in [(f64::NAN, 860.0), (1280.0, 0.0), (f64::INFINITY, 860.0)] {
            assert!(
                area.fit_client(desired, (960.0, 680.0), (16.0, 39.0))
                    .is_err()
            );
        }
        assert!(
            area.fit_client((1280.0, 860.0), (960.0, 680.0), (-1.0, 39.0))
                .is_err()
        );
        assert!(
            WorkArea { height: 20, ..area }
                .fit_client((1280.0, 860.0), (960.0, 680.0), (16.0, 39.0))
                .is_err()
        );
    }
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
