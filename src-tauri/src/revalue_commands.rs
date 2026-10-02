use super::price_commands::{authorized, blocking, database};
use tauri::{State, WebviewWindow};
use token_pulse_core::{
    error::AppError, jobs::CancelJobResult, pricing::revalue::*, privacy::PrivateResponse,
};

fn service(
    state: &State<'_, super::RuntimeState>,
    id: &str,
) -> Result<std::sync::Arc<token_pulse_store::revalue_service::RevalueService>, Box<AppError>> {
    state
        .revaluations
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, id.into())))
}
#[tauri::command]
pub async fn get_price_revalue_status(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<PriceRevalueStatus>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let service = service(&state, &request_id)?;
    if let Some(error) = service.last_error() {
        return Err(Box::new(AppError::new(error, request_id)));
    }
    let db = database(&state, &request_id)?;
    let status = blocking(&request_id, move || db.price_revalue_status()).await?;
    Ok(PrivateResponse::new(
        request_id,
        status,
        state.privacy.clone(),
    ))
}
#[tauri::command]
pub async fn start_price_revalue(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: PriceRevalueRequest,
    request_id: String,
) -> Result<PrivateResponse<PriceRevalueJob>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let service = service(&state, &request_id)?;
    let job = blocking(&request_id, move || {
        service.start_job(uuid::Uuid::new_v4().to_string(), request)
    })
    .await?;
    Ok(PrivateResponse::new(request_id, job, state.privacy.clone()))
}
#[tauri::command]
pub async fn cancel_price_revalue(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    job_id: String,
    request_id: String,
) -> Result<PrivateResponse<CancelJobResult>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let service = service(&state, &request_id)?;
    let result = blocking(&request_id, move || service.cancel_job(job_id)).await?;
    Ok(PrivateResponse::new(
        request_id,
        result,
        state.privacy.clone(),
    ))
}
