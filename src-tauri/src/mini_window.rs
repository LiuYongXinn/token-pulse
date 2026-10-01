//! Dedicated local WebView. All native actions remain behind window-specific commands.
use tauri::{Emitter, Manager, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    mini::{MiniWindowAction, MiniWindowState},
    placement::{MiniPreferenceChange, WindowPlacement, WorkArea},
    protocol::{Response, validate_request_id},
};

pub fn show(app: &tauri::AppHandle) -> Result<(), String> {
    let runtime = app.state::<super::RuntimeState>();
    let _creation = runtime
        .mini_creation
        .lock()
        .map_err(|_| "WINDOW_STATE_UNAVAILABLE")?;
    let window = if let Some(window) = app.get_webview_window("mini") {
        fit_current(&window)?;
        window
    } else {
        let state = app.state::<super::RuntimeState>();
        let preferences = state
            .database
            .as_ref()
            .map_err(|e| e.code.to_string())?
            .mini_window_preferences()
            .map_err(|e| e.code.to_string())?;
        let interaction = preferences.interaction;
        let (width, height) = dimensions(interaction.expanded);
        let theme = state
            .database
            .as_ref()
            .ok()
            .and_then(|db| db.display_settings().ok())
            .map(|s| s.preferences.theme)
            .unwrap_or_default();
        let theme = match theme {
            token_pulse_core::settings::AppTheme::Dark => Some(tauri::Theme::Dark),
            token_pulse_core::settings::AppTheme::Light => Some(tauri::Theme::Light),
            token_pulse_core::settings::AppTheme::System => None,
        };
        let window = tauri::WebviewWindowBuilder::new(
            app,
            "mini",
            tauri::WebviewUrl::App("index.html?window=mini".into()),
        )
        .title("TokenPulse · 悬浮窗")
        .inner_size(width, height)
        .resizable(false)
        .decorations(false)
        .skip_taskbar(true)
        .always_on_top(interaction.pinned)
        .theme(theme)
        .center()
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
        *state
            .mini_window
            .lock()
            .map_err(|_| "WINDOW_STATE_UNAVAILABLE")? = interaction;
        if let Some(placement) = preferences.placement {
            restore_placement(&window, &placement, width, height)?;
        } else {
            fit_current(&window)?;
        }
        window
    };
    // Tray entry restores interaction even if a future integration has hidden/disabled the surface.
    window
        .set_ignore_cursor_events(false)
        .map_err(|e| e.to_string())?;
    save_current_placement(&window).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}
fn dimensions(expanded: bool) -> (f64, f64) {
    if expanded {
        (360.0, 380.0)
    } else {
        (280.0, 220.0)
    }
}

#[tauri::command]
pub fn mini_window_action(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: MiniWindowAction,
    request_id: String,
) -> Result<Response<MiniWindowState>, Box<AppError>> {
    validate_request_id(&request_id)
        .map_err(|code| AppError::new(code, "invalid-request".into()))?;
    if window.label() != "mini" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    let mut interaction = state.mini_window.lock().map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::WindowUnavailable,
            request_id.clone(),
        ))
    })?;
    let result = match request {
        MiniWindowAction::Read {} => Ok(()),
        MiniWindowAction::SetExpanded { expanded } => {
            let (width, height) = dimensions(expanded);
            window
                .set_size(tauri::LogicalSize::new(width, height))
                .map_err(|_| {
                    Box::new(AppError::new(
                        ErrorCode::WindowUnavailable,
                        request_id.clone(),
                    ))
                })?;
            if let Err(error) = persist(
                window.app_handle(),
                MiniPreferenceChange::Expanded(expanded),
            ) {
                let (width, height) = dimensions(interaction.expanded);
                let _ = window.set_size(tauri::LogicalSize::new(width, height));
                return Err(Box::new(AppError::new(error, request_id)));
            }
            interaction.expanded = expanded;
            // Growing the window near an edge must not hide its buttons below the taskbar.
            if fit_current(&window).is_err() {
                eprintln!("MINI_PLACEMENT_UNAVAILABLE");
            }
            if save_current_placement(&window).is_err() {
                eprintln!("MINI_PLACEMENT_SAVE_FAILED");
            }
            Ok(())
        }
        MiniWindowAction::SetPinned { pinned } => {
            window.set_always_on_top(pinned).map_err(|_| {
                Box::new(AppError::new(
                    ErrorCode::WindowUnavailable,
                    request_id.clone(),
                ))
            })?;
            if let Err(error) = persist(window.app_handle(), MiniPreferenceChange::Pinned(pinned)) {
                let _ = window.set_always_on_top(interaction.pinned);
                return Err(Box::new(AppError::new(error, request_id)));
            }
            interaction.pinned = pinned;
            Ok(())
        }
        MiniWindowAction::Drag {} => window.start_dragging(),
        MiniWindowAction::Hide {} => {
            if save_current_placement(&window).is_err() {
                eprintln!("MINI_PLACEMENT_SAVE_FAILED");
            }
            window.hide()
        }
    };
    result.map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::WindowUnavailable,
            request_id.clone(),
        ))
    })?;
    Ok(Response::new(request_id, *interaction))
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
fn restore_placement(
    window: &WebviewWindow,
    placement: &WindowPlacement,
    width_dip: f64,
    height_dip: f64,
) -> Result<(), String> {
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let monitor = monitors
        .iter()
        .find(|m| {
            placement
                .monitor
                .as_ref()
                .is_some_and(|name| m.name() == Some(name))
        })
        .cloned()
        .or(window.primary_monitor().map_err(|e| e.to_string())?)
        .ok_or("MONITOR_UNAVAILABLE")?;
    // A hidden Win32 window can still report its intermediate client height before
    // decoration removal completes. The floating display has two fixed logical sizes.
    let (x, y) = area(&monitor)
        .restore(placement, width_dip, height_dip)
        .map_err(|e| e.to_string())?;
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}
pub(super) fn fit_current(window: &WebviewWindow) -> Result<(), String> {
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .or(window.primary_monitor().map_err(|e| e.to_string())?)
        .ok_or("MONITOR_UNAVAILABLE")?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let placement = area(&monitor)
        .capture(position.x, position.y, monitor.name().cloned())
        .map_err(|e| e.to_string())?;
    let logical = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);
    let (x, y) = area(&monitor)
        .restore(&placement, logical.width, logical.height)
        .map_err(|e| e.to_string())?;
    if (x, y) != (position.x, position.y) {
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn persist(app: &tauri::AppHandle, change: MiniPreferenceChange) -> Result<(), ErrorCode> {
    let runtime = app.state::<super::RuntimeState>();
    let db = runtime.database.as_ref().map_err(|e| e.code)?;
    let at = token_pulse_core::numeric::EpochMs::new(
        token_pulse_collector::jobs::now_ms().map_err(|e| e.code)?,
    )?;
    let (revision, changed) = db
        .update_mini_window_preferences(change, at)
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
pub(super) fn save_current_placement(window: &WebviewWindow) -> Result<(), ErrorCode> {
    let monitor = window
        .current_monitor()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        .ok_or(ErrorCode::WindowUnavailable)?;
    let position = window
        .outer_position()
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    let placement = area(&monitor).capture(position.x, position.y, monitor.name().cloned())?;
    persist(
        window.app_handle(),
        MiniPreferenceChange::Placement(placement),
    )
}
pub(super) fn schedule_placement(app: &tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    let Some(runtime) = app.try_state::<super::RuntimeState>() else {
        return;
    };
    runtime
        .mini_geometry_sequence
        .fetch_add(1, Ordering::Relaxed);
    if runtime
        .mini_geometry_worker
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<super::RuntimeState>();
        let sequence = loop {
            let sequence = runtime.mini_geometry_sequence.load(Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_millis(250));
            if sequence != runtime.mini_geometry_sequence.load(Ordering::Relaxed) {
                continue;
            }
            if let Some(window) = app.get_webview_window("mini") {
                if fit_current(&window).is_err() {
                    eprintln!("MINI_PLACEMENT_UNAVAILABLE");
                } else if sequence == runtime.mini_geometry_sequence.load(Ordering::Relaxed)
                    && save_current_placement(&window).is_err()
                {
                    eprintln!("MINI_PLACEMENT_SAVE_FAILED");
                }
            }
            break sequence;
        };
        runtime.mini_geometry_worker.store(false, Ordering::Release);
        // An event arriving during the final write/reset must schedule another bounded worker.
        if sequence != runtime.mini_geometry_sequence.load(Ordering::Relaxed) {
            schedule_placement(&app);
        }
    });
}
