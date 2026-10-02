use tauri::{State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    jobs::{CancelJobResult, JobRequest},
    privacy::PrivateResponse,
    protocol::{Job, JobKind, validate_request_id},
};
fn authorized(window: &WebviewWindow, id: &str) -> Result<(), Box<AppError>> {
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
fn db(
    state: &State<'_, super::RuntimeState>,
    id: &str,
) -> Result<token_pulse_store::Database, Box<AppError>> {
    state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, id.into())))
}
async fn blocking<T: Send + 'static>(
    id: &str,
    f: impl FnOnce() -> token_pulse_store::StoreResult<T> + Send + 'static,
) -> Result<T, Box<AppError>> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, id.into())))?
        .map_err(|e| Box::new(AppError::new(e.code, id.into())))
}
#[tauri::command]
pub async fn start_job(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    mut request: JobRequest,
    request_id: String,
) -> Result<PrivateResponse<Job>, Box<AppError>> {
    authorized(&window, &request_id)?;
    request
        .validate()
        .map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    if request.kind != JobKind::Rebuild {
        return Err(Box::new(AppError::new(
            ErrorCode::UnsupportedApi,
            request_id,
        )));
    }
    state
        .jobs
        .as_ref()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    let db = db(&state, &request_id)?;
    let id = uuid::Uuid::new_v4().to_string();
    let job = blocking(&request_id, move || {
        db.create_job(id, request, token_pulse_collector::jobs::now_ms()?)
    })
    .await?;
    if let Ok(service) = &state.jobs {
        service.wake();
    }
    Ok(PrivateResponse::new(request_id, job, state.privacy.clone()))
}
#[tauri::command]
pub async fn list_jobs(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    limit: u32,
    request_id: String,
) -> Result<PrivateResponse<Vec<Job>>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = db(&state, &request_id)?;
    let jobs = blocking(&request_id, move || db.list_jobs(limit)).await?;
    Ok(PrivateResponse::new(
        request_id,
        jobs,
        state.privacy.clone(),
    ))
}
#[tauri::command]
pub async fn get_job(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    job_id: String,
    request_id: String,
) -> Result<PrivateResponse<Job>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = db(&state, &request_id)?;
    let job = blocking(&request_id, move || db.get_job(&job_id).map(|s| s.job)).await?;
    Ok(PrivateResponse::new(request_id, job, state.privacy.clone()))
}
#[tauri::command]
pub async fn get_rebuild_status(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<Option<Job>>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = db(&state, &request_id)?;
    let job = blocking(&request_id, move || db.rebuild_status()).await?;
    Ok(PrivateResponse::new(request_id, job, state.privacy.clone()))
}
#[tauri::command]
pub async fn cancel_job(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    job_id: String,
    request_id: String,
) -> Result<PrivateResponse<CancelJobResult>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = db(&state, &request_id)?;
    let result = blocking(&request_id, move || {
        db.cancel_job(job_id, token_pulse_collector::jobs::now_ms()?)
    })
    .await?;
    if let Ok(service) = &state.jobs {
        service.wake();
    }
    Ok(PrivateResponse::new(
        request_id,
        result,
        state.privacy.clone(),
    ))
}
