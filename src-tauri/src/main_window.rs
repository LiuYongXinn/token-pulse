//! Native-only main placement. Coordinates are relative to monitor work areas, in DIP.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::{Emitter, Manager, WebviewWindow};
use token_pulse_core::{
    error::ErrorCode,
    numeric::EpochMs,
    placement::{WindowPlacement, WorkArea},
};

#[derive(Default)]
pub(super) struct PlacementRuntime {
    pub(super) enabled: AtomicBool,
    sequence: AtomicU64,
    worker: AtomicBool,
    fit_requested: AtomicBool,
    last_normal: std::sync::Mutex<Option<WindowPlacement>>,
    pub(super) fitted_area: std::sync::Mutex<Option<MonitorWorkArea>>,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MonitorWorkArea {
    pub(super) monitor: Option<String>,
    pub(super) work: WorkArea,
}
fn area(monitor: &tauri::Monitor) -> WorkArea {
    let work = monitor.work_area();
    WorkArea {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
        scale: monitor.scale_factor(),
    }
}
fn current_area(window: &WebviewWindow) -> Result<MonitorWorkArea, ErrorCode> {
    let monitor = window
        .current_monitor()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        .or(window
            .primary_monitor()
            .map_err(|_| ErrorCode::WindowUnavailable)?)
        .ok_or(ErrorCode::WindowUnavailable)?;
    Ok(MonitorWorkArea {
        monitor: monitor.name().cloned(),
        work: area(&monitor),
    })
}
fn fitted(window: &WebviewWindow, current: MonitorWorkArea) -> Result<(), ErrorCode> {
    // Do not hold the runtime mutex while invoking native window APIs.
    *window
        .app_handle()
        .state::<super::RuntimeState>()
        .main_geometry
        .fitted_area
        .lock()
        .map_err(|_| ErrorCode::WindowUnavailable)? = Some(current);
    Ok(())
}
fn work_area_changed(window: &WebviewWindow) -> Result<bool, ErrorCode> {
    if !normal(window)? {
        return Ok(false);
    }
    let current = current_area(window)?;
    Ok(window
        .app_handle()
        .state::<super::RuntimeState>()
        .main_geometry
        .fitted_area
        .lock()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        .as_ref()
        != Some(&current))
}
fn normal(window: &WebviewWindow) -> Result<bool, ErrorCode> {
    Ok(!window
        .is_minimized()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        && !window
            .is_maximized()
            .map_err(|_| ErrorCode::WindowUnavailable)?)
}
fn outer_dip(window: &WebviewWindow) -> Result<(f64, f64), ErrorCode> {
    // Main has decorations: client size would omit the title bar / frame from the fit check.
    let size = window
        .outer_size()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        .to_logical::<f64>(
            window
                .scale_factor()
                .map_err(|_| ErrorCode::WindowUnavailable)?,
        );
    Ok((size.width, size.height))
}
fn restore(window: &WebviewWindow, placement: &WindowPlacement) -> Result<(), ErrorCode> {
    let monitors = window
        .available_monitors()
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    let monitor = monitors
        .iter()
        .find(|m| {
            placement
                .monitor
                .as_ref()
                .is_some_and(|name| m.name() == Some(name))
        })
        .cloned()
        .or(window
            .primary_monitor()
            .map_err(|_| ErrorCode::WindowUnavailable)?)
        .ok_or(ErrorCode::WindowUnavailable)?;
    let (width, height) = outer_dip(window)?;
    let (x, y) = area(&monitor).restore(placement, width, height)?;
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    // Moving between monitors can change the native frame and DPI. Measure after the move.
    fit_in_area(window, area(&monitor))?;
    let (width, height) = outer_dip(window)?;
    let (x, y) = area(&monitor).restore(placement, width, height)?;
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    fitted(
        window,
        MonitorWorkArea {
            monitor: monitor.name().cloned(),
            work: area(&monitor),
        },
    )
}
pub(super) fn fit_current(window: &WebviewWindow) -> Result<(), ErrorCode> {
    if !normal(window)? {
        return Ok(());
    }
    let current = current_area(window)?;
    fit_in_area(window, current.work)?;
    fitted(window, current)
}
pub(super) fn fit_in_area(window: &WebviewWindow, work: WorkArea) -> Result<(), ErrorCode> {
    if !normal(window)? {
        return Ok(());
    }
    let position = window
        .outer_position()
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    let placement = work.capture(position.x, position.y, None)?;
    let scale = window
        .scale_factor()
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    let client = window
        .inner_size()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        .to_logical::<f64>(scale);
    let (width, height) = outer_dip(window)?;
    let fit = work.fit_client(
        (client.width, client.height),
        (960.0, 680.0),
        (
            (width - client.width).max(0.0),
            (height - client.height).max(0.0),
        ),
    )?;
    window
        .set_min_size(Some(tauri::LogicalSize::new(fit.minimum.0, fit.minimum.1)))
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    if (client.width - fit.size.0).abs() > 0.01 || (client.height - fit.size.1).abs() > 0.01 {
        window
            .set_size(tauri::LogicalSize::new(fit.size.0, fit.size.1))
            .map_err(|_| ErrorCode::WindowUnavailable)?;
    }
    let (width, height) = outer_dip(window)?;
    let (x, y) = work.restore(&placement, width, height)?;
    if (x, y) != (position.x, position.y) {
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|_| ErrorCode::WindowUnavailable)?;
    }
    Ok(())
}
pub(super) fn initialize(app: &tauri::AppHandle) {
    let state = app.state::<super::RuntimeState>();
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let preferences = state
        .database
        .as_ref()
        .map_err(|e| e.code)
        .and_then(|db| db.main_window_preferences().map_err(|e| e.code));
    let result = match preferences {
        Ok(preferences) => match preferences.placement {
            Some(placement) => restore(&window, &placement),
            None => fit_current(&window),
        },
        Err(_) => {
            eprintln!("MAIN_PLACEMENT_READ_FAILED");
            fit_current(&window)
        }
    };
    if result.is_err() {
        eprintln!("MAIN_PLACEMENT_UNAVAILABLE");
    }
    #[cfg(all(debug_assertions, windows))]
    if super::navigation_smoke::secondary_probe()
        && (super::navigation_smoke::place_on_secondary(app).is_err()
            || fit_current(&window).is_err())
    {
        eprintln!("NATIVE_SECONDARY_PLACEMENT_FAILED");
        return;
    }
    #[cfg(all(debug_assertions, windows))]
    let shown = if super::navigation_smoke::secondary_probe() {
        // initialize runs on the native main thread; do not dispatch and wait on itself.
        super::mini_window::nonactivating_show_owned(&window)
    } else {
        window.show().map_err(|e| e.to_string())
    };
    #[cfg(not(all(debug_assertions, windows)))]
    let shown = window.show();
    if shown.is_err() {
        eprintln!("MAIN_WINDOW_SHOW_FAILED");
    }
    if remember(&window).is_err() {
        eprintln!("MAIN_PLACEMENT_UNAVAILABLE");
    }
    state.main_geometry.enabled.store(true, Ordering::Release);
}
fn capture(window: &WebviewWindow) -> Result<Option<WindowPlacement>, ErrorCode> {
    if !normal(window)? {
        return Ok(None);
    }
    let monitor = window
        .current_monitor()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        .ok_or(ErrorCode::WindowUnavailable)?;
    let position = window
        .outer_position()
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    Ok(Some(area(&monitor).capture(
        position.x,
        position.y,
        monitor.name().cloned(),
    )?))
}
fn remember(window: &WebviewWindow) -> Result<(), ErrorCode> {
    if let Some(placement) = capture(window)? {
        *window
            .app_handle()
            .state::<super::RuntimeState>()
            .main_geometry
            .last_normal
            .lock()
            .map_err(|_| ErrorCode::WindowUnavailable)? = Some(placement);
    }
    Ok(())
}
pub(super) fn save_current(window: &WebviewWindow) -> Result<(), ErrorCode> {
    let app = window.app_handle();
    let state = app.state::<super::RuntimeState>();
    let placement = capture(window)?.or(state
        .main_geometry
        .last_normal
        .lock()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        .clone());
    let Some(placement) = placement else {
        return Ok(());
    };
    let db = state.database.as_ref().map_err(|e| e.code)?;
    let at = EpochMs::new(token_pulse_collector::jobs::now_ms().map_err(|e| e.code)?)?;
    let (revision, changed) = db
        .update_main_window_placement(placement, at)
        .map_err(|e| e.code)?;
    if changed {
        let _ = app.emit(
            "settings_changed",
            token_pulse_core::settings::SettingsChanged {
                settings_revision: revision,
            },
        );
    }
    Ok(())
}
pub(super) fn schedule(app: &tauri::AppHandle, fit: bool) {
    let Some(state) = app.try_state::<super::RuntimeState>() else {
        return;
    };
    let runtime = &state.main_geometry;
    if !runtime.enabled.load(Ordering::Acquire) {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        if remember(&window).is_err() {
            eprintln!("MAIN_PLACEMENT_UNAVAILABLE");
        }
    }
    if fit {
        runtime.fit_requested.store(true, Ordering::Release);
    }
    runtime.sequence.fetch_add(1, Ordering::AcqRel);
    if runtime
        .worker
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<super::RuntimeState>();
        let runtime = &state.main_geometry;
        let sequence = loop {
            let sequence = runtime.sequence.load(Ordering::Acquire);
            std::thread::sleep(std::time::Duration::from_millis(300));
            if !runtime.enabled.load(Ordering::Acquire) {
                break sequence;
            }
            if sequence != runtime.sequence.load(Ordering::Acquire) {
                continue;
            }
            if let Some(window) = app.get_webview_window("main") {
                // Same-DPI monitor moves do not emit ScaleFactorChanged. Compare the last
                // successfully fitted work area after the ordinary move/resize debounce.
                let forced = runtime.fit_requested.swap(false, Ordering::AcqRel);
                let changed = work_area_changed(&window);
                if changed.is_err()
                    || ((forced || changed.unwrap_or(false)) && fit_current(&window).is_err())
                {
                    eprintln!("MAIN_PLACEMENT_UNAVAILABLE");
                }
                if sequence != runtime.sequence.load(Ordering::Acquire) {
                    continue;
                }
                if runtime.enabled.load(Ordering::Acquire) && save_current(&window).is_err() {
                    eprintln!("MAIN_PLACEMENT_SAVE_FAILED");
                }
            }
            break sequence;
        };
        runtime.worker.store(false, Ordering::Release);
        if sequence != runtime.sequence.load(Ordering::Acquire) {
            schedule(&app, false);
        }
    });
}
pub(super) fn stop_and_save(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<super::RuntimeState>() else {
        return;
    };
    if state.main_geometry.enabled.swap(false, Ordering::AcqRel) {
        state.main_geometry.sequence.fetch_add(1, Ordering::AcqRel);
        if let Some(window) = app.get_webview_window("main") {
            if save_current(&window).is_err() {
                eprintln!("MAIN_PLACEMENT_SAVE_FAILED");
            }
        }
    }
}
