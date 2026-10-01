//! Dedicated local WebView. All native actions remain behind window-specific commands.
use tauri::{Manager, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    mini::{MiniWindowAction, MiniWindowState},
    protocol::{Response, validate_request_id},
};

pub fn show(app: &tauri::AppHandle) -> Result<(), String> {
    let runtime = app.state::<super::RuntimeState>();
    let _creation = runtime
        .mini_creation
        .lock()
        .map_err(|_| "WINDOW_STATE_UNAVAILABLE")?;
    let window = if let Some(window) = app.get_webview_window("mini") {
        window
    } else {
        let state = app.state::<super::RuntimeState>();
        let interaction = *state
            .mini_window
            .lock()
            .map_err(|_| "WINDOW_STATE_UNAVAILABLE")?;
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
        tauri::WebviewWindowBuilder::new(
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
        .map_err(|e| e.to_string())?
    };
    // Tray entry restores interaction even if a future integration has hidden/disabled the surface.
    window
        .set_ignore_cursor_events(false)
        .map_err(|e| e.to_string())?;
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
                .map(|()| interaction.expanded = expanded)
        }
        MiniWindowAction::SetPinned { pinned } => window
            .set_always_on_top(pinned)
            .map(|()| interaction.pinned = pinned),
        MiniWindowAction::Drag {} => window.start_dragging(),
        MiniWindowAction::Hide {} => window.hide(),
    };
    result.map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::WindowUnavailable,
            request_id.clone(),
        ))
    })?;
    Ok(Response::new(request_id, *interaction))
}
