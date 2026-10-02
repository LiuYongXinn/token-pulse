use tauri::{AppHandle, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    privacy::PrivateResponse,
    protocol::validate_request_id,
    updates::{UpdateActionRequest, UpdateSnapshot},
};

fn authorize(window: &WebviewWindow, id: &str) -> Result<(), Box<AppError>> {
    validate_request_id(id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            id.into(),
        )));
    }
    Ok(())
}
fn response(
    state: &super::RuntimeState,
    id: String,
    result: Result<UpdateSnapshot, ErrorCode>,
) -> Result<PrivateResponse<UpdateSnapshot>, Box<AppError>> {
    result
        .map(|snapshot| PrivateResponse::new(id.clone(), snapshot, state.privacy.clone()))
        .map_err(|code| Box::new(AppError::new(code, id)))
}
#[tauri::command]
pub fn get_update_status(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<UpdateSnapshot>, Box<AppError>> {
    authorize(&window, &request_id)?;
    response(&state, request_id, state.updates.snapshot())
}
#[tauri::command]
pub fn check_for_updates(
    window: WebviewWindow,
    app: AppHandle,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<UpdateSnapshot>, Box<AppError>> {
    authorize(&window, &request_id)?;
    response(&state, request_id, state.updates.start_check(app))
}
#[tauri::command]
pub fn download_update(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: UpdateActionRequest,
    request_id: String,
) -> Result<PrivateResponse<UpdateSnapshot>, Box<AppError>> {
    authorize(&window, &request_id)?;
    response(
        &state,
        request_id,
        state
            .updates
            .start_download(&request.expected_update_revision),
    )
}
