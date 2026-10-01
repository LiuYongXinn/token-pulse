use tauri::{State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    protocol::{ContextSnapshot, Response, validate_request_id},
    query::{DashboardBundle, DashboardRequest},
};

#[tauri::command]
pub async fn get_dashboard_bundle(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: DashboardRequest,
    request_id: String,
) -> Result<Response<DashboardBundle>, Box<AppError>> {
    validate_request_id(&request_id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    request
        .validate()
        .map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    let database = state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    let snapshot_id = request_id.clone();
    let data = tauri::async_runtime::spawn_blocking(move || {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        database.dashboard_bundle(&request, at, &snapshot_id)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(Response::new(request_id, data))
}

#[tauri::command]
pub async fn get_context_snapshot(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    session_key: String,
    request_id: String,
) -> Result<Response<ContextSnapshot>, Box<AppError>> {
    validate_request_id(&request_id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    let database = state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    let context =
        tauri::async_runtime::spawn_blocking(move || database.latest_context(&session_key))
            .await
            .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
            .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    Ok(Response::new(request_id, context))
}
