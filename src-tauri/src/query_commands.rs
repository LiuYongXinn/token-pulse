use tauri::{State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    protocol::{ContextSnapshot, Response, validate_request_id},
    query::{
        CloseQuerySnapshotRequest, DashboardBundle, DashboardRequest, FilterOptionsPage,
        FilterOptionsRequest, GroupedUsageBundle, GroupedUsageRequest, SessionBundle,
        SessionBundleRequest, SessionsPage, SessionsRequest, UsageEventsPage, UsageEventsRequest,
    },
};

#[tauri::command]
pub async fn get_session_bundle(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: SessionBundleRequest,
    request_id: String,
) -> Result<Response<SessionBundle>, Box<AppError>> {
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
        database.session_bundle(&request, at, &snapshot_id)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(Response::new(request_id, data))
}

#[tauri::command]
pub async fn query_usage_events(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: UsageEventsRequest,
    request_id: String,
) -> Result<Response<UsageEventsPage>, Box<AppError>> {
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
    let owner = window.label().to_owned();
    let data = tauri::async_runtime::spawn_blocking(move || {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        database.query_usage_events(&owner, &request, at)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(Response::new(request_id, data))
}

#[tauri::command]
pub async fn query_sessions(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: SessionsRequest,
    request_id: String,
) -> Result<Response<SessionsPage>, Box<AppError>> {
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
    let owner = window.label().to_owned();
    let data = tauri::async_runtime::spawn_blocking(move || {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        database.query_sessions(&owner, &request, at)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(Response::new(request_id, data))
}

#[tauri::command]
pub async fn get_filter_options(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: FilterOptionsRequest,
    request_id: String,
) -> Result<Response<FilterOptionsPage>, Box<AppError>> {
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
    let owner = window.label().to_owned();
    let data = tauri::async_runtime::spawn_blocking(move || {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        database.filter_options(&owner, &request, at)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(Response::new(request_id, data))
}

#[tauri::command]
pub async fn close_query_snapshot(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: CloseQuerySnapshotRequest,
    request_id: String,
) -> Result<Response<()>, Box<AppError>> {
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
    let owner = window.label().to_owned();
    tauri::async_runtime::spawn_blocking(move || match request {
        CloseQuerySnapshotRequest::FilterOptions { request } => {
            database.close_filter_options(&owner, &request)
        }
        CloseQuerySnapshotRequest::Sessions { request } => {
            database.close_sessions(&owner, &request)
        }
        CloseQuerySnapshotRequest::UsageEvents { request } => {
            database.close_usage_events(&owner, &request)
        }
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    Ok(Response::new(request_id, ()))
}

#[tauri::command]
pub async fn get_grouped_usage(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: GroupedUsageRequest,
    request_id: String,
) -> Result<Response<GroupedUsageBundle>, Box<AppError>> {
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
        database.grouped_usage_bundle(&request, at, &snapshot_id)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(Response::new(request_id, data))
}

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
