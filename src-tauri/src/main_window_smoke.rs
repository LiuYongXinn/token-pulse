//! Three opt-in debug processes share an isolated database; no real sources or accounts.
use std::time::{Duration, Instant};
use tauri::Manager;
use token_pulse_core::placement::WindowPlacement;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Seed,
    Restore,
    Missing,
}
pub struct Scene {
    pub directory: String,
    pub phase: Phase,
}
pub fn scene() -> Result<Option<Scene>, &'static str> {
    parse(std::env::args().skip(1))
}
fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Scene>, &'static str> {
    let mut id = None;
    let mut phase = None;
    let mut native = false;
    for arg in args {
        if arg == "--native-smoke" {
            native = true;
        }
        if let Some(value) = arg.strip_prefix("--native-main-window-id=") {
            if id.is_some() {
                return Err("duplicate main window probe UUID");
            }
            id = Some(uuid::Uuid::parse_str(value).map_err(|_| "invalid main window UUID")?);
        }
        if let Some(value) = arg.strip_prefix("--native-main-window-phase=") {
            if phase.is_some() {
                return Err("duplicate main window phase");
            }
            phase = Some(match value {
                "seed" => Phase::Seed,
                "restore" => Phase::Restore,
                "missing" => Phase::Missing,
                _ => return Err("invalid main window phase"),
            });
        }
    }
    match (id, phase, native) {
        (None, None, _) => Ok(None),
        (Some(id), Some(phase), true) => Ok(Some(Scene {
            directory: format!("native-main-placement-{}", id.simple()),
            phase,
        })),
        _ => Err("main window probe requires native-smoke, UUID and phase"),
    }
}
pub fn start(app: tauri::AppHandle, phase: Phase, before_restore: Option<WindowPlacement>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let result = verify(&app, phase, before_restore);
        match &result {
            Ok(()) => println!("NATIVE_MAIN_WINDOW_COLD_OK: {phase:?}"),
            Err(error) => eprintln!("NATIVE_MAIN_WINDOW_COLD_FAILED: {phase:?}: {error}"),
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}
fn wait(check: impl Fn() -> bool, label: &str) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(6);
    while !check() {
        if Instant::now() >= end {
            return Err(format!("main window condition: {label}"));
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}
fn verify(
    app: &tauri::AppHandle,
    phase: Phase,
    before_restore: Option<WindowPlacement>,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let state = app.state::<super::RuntimeState>();
    if !app.config().identifier.ends_with(".dev")
        || !state
            .data_directory
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("native-main-placement-"))
    {
        return Err("isolated main-window scene required".into());
    }
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let window = app.get_webview_window("main").ok_or("main missing")?;
    if !window.is_visible().map_err(|e| e.to_string())? {
        return Err("startup did not reveal main".into());
    }
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("monitor missing")?;
    let work = monitor.work_area();
    let scale = monitor.scale_factor();
    let size = window.outer_size().map_err(|e| e.to_string())?;
    if phase == Phase::Seed {
        let x = work.position.x
            + ((i64::from(work.size.width) - i64::from(size.width)).max(0) / 2).min(80) as i32;
        let y = work.position.y
            + ((i64::from(work.size.height) - i64::from(size.height)).max(0) / 2).min(20) as i32;
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        let expected = WindowPlacement {
            monitor: monitor.name().cloned(),
            offset_x_dip: f64::from(x - work.position.x) / scale,
            offset_y_dip: f64::from(y - work.position.y) / scale,
        };
        wait(
            || {
                db.main_window_preferences()
                    .is_ok_and(|p| p.placement.as_ref() == Some(&expected))
            },
            "ordinary move persisted",
        )?;
        if db
            .mini_window_preferences()
            .map_err(|e| e.to_string())?
            .placement
            .is_some()
        {
            return Err("main move changed mini placement".into());
        }
        // A final move followed immediately by maximize must not lose the move to debounce.
        let fast_expected = WindowPlacement {
            offset_x_dip: expected.offset_x_dip + 7.0 / scale,
            ..expected.clone()
        };
        window
            .set_position(tauri::PhysicalPosition::new(x + 7, y))
            .map_err(|e| e.to_string())?;
        window.maximize().map_err(|e| e.to_string())?;
        wait(|| window.is_maximized().unwrap_or(false), "maximize")?;
        wait(
            || {
                db.main_window_preferences()
                    .is_ok_and(|p| p.placement.as_ref() == Some(&fast_expected))
            },
            "move before maximize persisted",
        )?;
        std::thread::sleep(Duration::from_millis(650));
        if db
            .main_window_preferences()
            .map_err(|e| e.to_string())?
            .placement
            != Some(fast_expected.clone())
        {
            return Err("maximized bounds replaced ordinary placement".into());
        }
        window.minimize().map_err(|e| e.to_string())?;
        wait(|| window.is_minimized().unwrap_or(false), "minimize")?;
        std::thread::sleep(Duration::from_millis(650));
        if db
            .main_window_preferences()
            .map_err(|e| e.to_string())?
            .placement
            != Some(fast_expected.clone())
        {
            return Err("minimized sentinel replaced ordinary placement".into());
        }
        window.unminimize().map_err(|e| e.to_string())?;
        window.unmaximize().map_err(|e| e.to_string())?;
        wait(
            || !window.is_minimized().unwrap_or(true) && !window.is_maximized().unwrap_or(true),
            "normal state",
        )?;
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        wait(
            || window.outer_position().is_ok_and(|p| p.x == x && p.y == y),
            "restored native location",
        )?;
        window.close().map_err(|e| e.to_string())?;
        wait(
            || !window.is_visible().unwrap_or(true),
            "close hides rather than destroys",
        )?;
        if app.get_webview_window("main").is_none()
            || db
                .main_window_preferences()
                .map_err(|e| e.to_string())?
                .placement
                != Some(expected)
        {
            return Err("close lost window or location".into());
        }
        super::show_main(app)?;
        wait(|| window.is_visible().unwrap_or(false), "normal reopen")?;
    } else {
        let expected = if phase == Phase::Missing {
            let input = before_restore
                .as_ref()
                .ok_or("missing-monitor input absent")?;
            if input.monitor.as_deref() != Some("synthetic-disconnected-monitor")
                || input.offset_x_dip != 8000.0
                || input.offset_y_dip != 8000.0
            {
                return Err("missing-monitor input was not the synthetic fixture".into());
            }
            (
                work.position.x
                    + (i64::from(work.size.width) - i64::from(size.width)).max(0) as i32,
                work.position.y
                    + (i64::from(work.size.height) - i64::from(size.height)).max(0) as i32,
            )
        } else {
            let placement = before_restore
                .as_ref()
                .ok_or("cold input location missing")?;
            let exit_expected: WindowPlacement = serde_json::from_slice(
                &std::fs::read(state.data_directory.join("expected-main-placement.json"))
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            if placement != &exit_expected {
                return Err("normal exit lost the independently expected last move".into());
            }

            if placement.monitor.as_ref() != monitor.name() {
                return Err("cold restore did not select saved monitor".into());
            }
            (
                (f64::from(work.position.x) + placement.offset_x_dip * scale).round() as i32,
                (f64::from(work.position.y) + placement.offset_y_dip * scale).round() as i32,
            )
        };
        let actual = window.outer_position().map_err(|e| e.to_string())?;
        if (actual.x - expected.0).abs() > 1 || (actual.y - expected.1).abs() > 1 {
            return Err(format!(
                "cold actual placement mismatch: phase={phase:?}; expected={expected:?}; actual=({}, {})",
                actual.x, actual.y
            ));
        }
        if phase == Phase::Missing {
            let primary = window
                .primary_monitor()
                .map_err(|e| e.to_string())?
                .ok_or("primary missing")?;
            if monitor.name() != primary.name() {
                return Err("missing monitor did not use primary".into());
            }
        } else {
            // Prepare only the synthetic next-run input; do not claim a real monitor unplug.
            state.main_geometry.enabled.store(false, Ordering::Release);
            db.update_main_window_placement(
                WindowPlacement {
                    monitor: Some("synthetic-disconnected-monitor".into()),
                    offset_x_dip: 8000.0,
                    offset_y_dip: 8000.0,
                },
                token_pulse_core::numeric::EpochMs::new(
                    token_pulse_collector::jobs::now_ms().map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    super::mini_smoke::evaluate(
        app,
        &window,
        r#"
      if(!document.querySelector('.sidebar nav'))throw new Error('RESTORED_UI_MISSING');
      const status=await invoke('get_account_quota',{requestId:'main-placement-no-account'});
      const sources=await invoke('get_sources',{requestId:'main-placement-no-sources'});
      if(status.data.state!=='disconnected'||sources.data.sources.length)throw new Error('MAIN_PROBE_TOUCHED_REAL_DATA');
    "#,
    )?;
    if phase == Phase::Seed {
        // Leave a known final move pending; the next process proves the production exit save.
        let current = window.outer_position().map_err(|e| e.to_string())?;
        let final_x = current.x + 9;
        let expected = WindowPlacement {
            monitor: monitor.name().cloned(),
            offset_x_dip: f64::from(final_x - work.position.x) / scale,
            offset_y_dip: f64::from(current.y - work.position.y) / scale,
        };
        std::fs::write(
            state.data_directory.join("expected-main-placement.json"),
            serde_json::to_vec(&expected).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        window
            .set_position(tauri::PhysicalPosition::new(final_x, current.y))
            .map_err(|e| e.to_string())?;
        wait(
            || {
                window
                    .outer_position()
                    .is_ok_and(|p| p.x == final_x && p.y == current.y)
            },
            "final move before exit",
        )?;
        if db
            .main_window_preferences()
            .map_err(|e| e.to_string())?
            .placement
            .as_ref()
            == Some(&expected)
        {
            return Err("final move was already persisted; exit proof was not exercised".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_scene_requires_own_uuid_explicit_flag_and_finite_phase() {
        let valid = [
            "--native-smoke",
            "--native-main-window-id=00000000-0000-0000-0000-000000000001",
            "--native-main-window-phase=seed",
        ];
        assert_eq!(
            parse(valid.map(str::to_owned)).unwrap().unwrap().directory,
            "native-main-placement-00000000000000000000000000000001"
        );
        for args in [
            vec![valid[1], valid[2]],
            vec![valid[0], valid[1]],
            vec![valid[0], valid[1], valid[1], valid[2]],
            vec![valid[0], valid[1], "--native-main-window-phase=other"],
            vec![valid[0], "--native-main-window-id=../../outside", valid[2]],
        ] {
            assert!(parse(args.into_iter().map(str::to_owned)).is_err());
        }
    }
}
