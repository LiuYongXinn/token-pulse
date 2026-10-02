use tauri::{Emitter, Manager, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    navigation::{MainNavigationIntent, MainNavigationSnapshot},
    protocol::{Response, validate_request_id},
};
pub(super) fn publish(
    app: &tauri::AppHandle,
    intent: MainNavigationIntent,
) -> Result<(), ErrorCode> {
    app.state::<super::RuntimeState>()
        .main_navigation
        .lock()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        .publish(intent)?;
    let _ = app.emit_to("main", "main_navigation_changed", ());
    Ok(())
}
#[tauri::command]
pub fn get_main_navigation(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<Response<MainNavigationSnapshot>, Box<AppError>> {
    validate_request_id(&request_id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    let data = state
        .main_navigation
        .lock()
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::WindowUnavailable,
                request_id.clone(),
            ))
        })?
        .clone();
    Ok(Response::new(request_id, data))
}
