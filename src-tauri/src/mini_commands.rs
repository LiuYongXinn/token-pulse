use tauri::{Emitter, Manager, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    mini::{MiniScopeMutation, MiniScopeSnapshot, MiniUsageSnapshot},
    privacy::PrivateResponse,
    protocol::validate_request_id,
};

fn authorized(window: &WebviewWindow, id: &str) -> Result<(), Box<AppError>> {
    validate_request_id(id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if !matches!(window.label(), "main" | "mini") {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            id.into(),
        )));
    }
    Ok(())
}
fn database(
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
    work: impl FnOnce() -> token_pulse_store::StoreResult<T> + Send + 'static,
) -> Result<T, Box<AppError>> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, id.into())))?
        .map_err(|e| Box::new(AppError::new(e.code, id.into())))
}
#[tauri::command]
pub async fn get_mini_scope(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<MiniScopeSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = database(&state, &request_id)?;
    let data = blocking(&request_id, move || db.mini_scope()).await?;
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}
#[tauri::command]
pub async fn get_mini_usage(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<MiniUsageSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = database(&state, &request_id)?;
    let snapshot_id = request_id.clone();
    let data = blocking(&request_id, move || {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        db.mini_usage(at, &snapshot_id)
    })
    .await?;
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}
#[tauri::command]
pub async fn set_mini_scope(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: MiniScopeMutation,
    request_id: String,
) -> Result<PrivateResponse<MiniScopeSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = database(&state, &request_id)?;
    let (data, changed) = blocking(&request_id, move || {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        db.mutate_mini_scope(request, at)
    })
    .await?;
    if changed {
        let _ = window.app_handle().emit("mini_scope_changed", &data);
        let _ = window.app_handle().emit(
            "settings_changed",
            token_pulse_core::settings::SettingsChanged {
                settings_revision: data.settings_revision.clone(),
            },
        );
    }
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}
