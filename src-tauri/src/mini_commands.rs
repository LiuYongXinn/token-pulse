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
pub async fn query_mini_sessions(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: token_pulse_core::mini::MiniSessionsRequest,
    request_id: String,
) -> Result<PrivateResponse<token_pulse_core::mini::MiniSessionsPage>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = database(&state, &request_id)?;
    let owner = window.label().to_owned();
    let data = blocking(&request_id, move || {
        db.mini_sessions(
            &owner,
            &request,
            token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?,
        )
    })
    .await?;
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
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

#[tauri::command]
pub async fn open_mini_stats(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: token_pulse_core::mini::MiniStatsOpenRequest,
    request_id: String,
) -> Result<
    token_pulse_core::protocol::Response<token_pulse_core::mini::MiniStatsRequest>,
    Box<AppError>,
> {
    authorized(&window, &request_id)?;
    if window.label() != "mini" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    let db = database(&state, &request_id)?;
    let id = request_id.clone();
    let data = blocking(&request_id, move || {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        let usage = db.mini_usage(at, &id)?;
        Ok(token_pulse_core::mini::open_stats_request(
            &usage, &request, id,
        )?)
    })
    .await?;
    *state.mini_stats_request.lock().map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::WindowUnavailable,
            request_id.clone(),
        ))
    })? = Some(data.clone());
    let app = window.app_handle();
    // Only an invalidation is emitted. The main window retrieves the latest retained intent.
    let _ = app.emit_to("main", "mini_stats_requested", ());
    super::show_main(app).map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::WindowUnavailable,
            request_id.clone(),
        ))
    })?;
    Ok(token_pulse_core::protocol::Response::new(request_id, data))
}
#[tauri::command]
pub fn get_mini_stats_request(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<
    token_pulse_core::protocol::Response<Option<token_pulse_core::mini::MiniStatsRequest>>,
    Box<AppError>,
> {
    authorized(&window, &request_id)?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    let data = state
        .mini_stats_request
        .lock()
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::WindowUnavailable,
                request_id.clone(),
            ))
        })?
        .clone();
    Ok(token_pulse_core::protocol::Response::new(request_id, data))
}
