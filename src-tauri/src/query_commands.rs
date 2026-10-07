use tauri::{State, WebviewWindow};
use token_pulse_core::{
    calendar::{CalendarSelectionRequest, CalendarSelectionResult},
    error::{AppError, ErrorCode},
    privacy::PrivateResponse,
    protocol::{ContextSnapshot, Response, validate_request_id},
    query::{
        CloseQuerySnapshotRequest, DashboardBundle, DashboardRequest, FilterOptionsPage,
        FilterOptionsRequest, GroupedUsageBundle, GroupedUsageRequest, SessionBundle,
        SessionBundleRequest, SessionsPage, SessionsRequest, TurnsPage, TurnsRequest,
        UsageEventsPage, UsageEventsRequest,
    },
};

#[tauri::command]
pub async fn restore_usage_display(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: token_pulse_core::display_cache::UsageDisplayRequest,
    request_id: String,
) -> Result<PrivateResponse<token_pulse_core::display_cache::UsageDisplaySnapshot>, Box<AppError>> {
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
    let data =
        tauri::async_runtime::spawn_blocking(move || database.restore_usage_display(&request))
            .await
            .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
            .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}

#[tauri::command]
pub async fn get_usage_revision(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<Response<token_pulse_core::query::UsageRevision>, Box<AppError>> {
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
    let data = tauri::async_runtime::spawn_blocking(move || database.usage_revision())
        .await
        .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    Ok(Response::new(request_id, data))
}

#[tauri::command]
pub fn resolve_calendar_selection(
    window: WebviewWindow,
    request: CalendarSelectionRequest,
    request_id: String,
) -> Result<Response<CalendarSelectionResult>, Box<AppError>> {
    validate_request_id(&request_id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    let at = token_pulse_collector::jobs::now_ms()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    let at = token_pulse_core::numeric::EpochMs::new(at)
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    let data = token_pulse_core::calendar::resolve_selection(&request, at)
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    Ok(Response::new(request_id, data))
}

#[tauri::command]
pub async fn query_turns(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: TurnsRequest,
    request_id: String,
) -> Result<PrivateResponse<TurnsPage>, Box<AppError>> {
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
        database.query_turns(&owner, &request, at)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}

#[tauri::command]
pub async fn get_session_bundle(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: SessionBundleRequest,
    request_id: String,
) -> Result<PrivateResponse<SessionBundle>, Box<AppError>> {
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
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}

#[tauri::command]
pub async fn query_usage_events(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: UsageEventsRequest,
    request_id: String,
) -> Result<PrivateResponse<UsageEventsPage>, Box<AppError>> {
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
        #[cfg(all(debug_assertions, windows))]
        super::navigation_smoke::wait_if_blocked();
        let stamp = database.display_cache_stamp().ok();
        let data = database.query_usage_events(&owner, &request, at)?;
        if let Some(stamp) = stamp {
            if request.cursor.is_none() {
                let _ = database.remember_usage_display(
                    &token_pulse_core::display_cache::UsageDisplayRequest::Events {
                        request: request.query.clone(),
                    },
                    token_pulse_core::display_cache::UsageDisplayData::Events {
                        value: data.clone(),
                    },
                    stamp,
                );
            }
        }
        Ok(data)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}

#[tauri::command]
pub async fn query_sessions(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: SessionsRequest,
    request_id: String,
) -> Result<PrivateResponse<SessionsPage>, Box<AppError>> {
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
        #[cfg(all(debug_assertions, windows))]
        super::navigation_smoke::wait_if_blocked();
        let stamp = database.display_cache_stamp().ok();
        let data = database.query_sessions(&owner, &request, at)?;
        if let Some(stamp) = stamp {
            if request.cursor.is_none() {
                let _ = database.remember_usage_display(
                    &token_pulse_core::display_cache::UsageDisplayRequest::Sessions {
                        request: request.query.clone(),
                    },
                    token_pulse_core::display_cache::UsageDisplayData::Sessions {
                        value: data.clone(),
                    },
                    stamp,
                );
            }
        }
        Ok(data)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}

#[tauri::command]
pub async fn get_filter_options(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: FilterOptionsRequest,
    request_id: String,
) -> Result<PrivateResponse<FilterOptionsPage>, Box<AppError>> {
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
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}

#[tauri::command]
pub async fn close_query_snapshot(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: CloseQuerySnapshotRequest,
    request_id: String,
) -> Result<PrivateResponse<()>, Box<AppError>> {
    validate_request_id(&request_id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if window.label() != "main"
        && !(window.label() == "mini"
            && matches!(&request, CloseQuerySnapshotRequest::MiniSessions { .. }))
    {
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
        CloseQuerySnapshotRequest::MiniSessions { request } => {
            database.close_mini_sessions(&owner, &request)
        }
        CloseQuerySnapshotRequest::Turns { request } => database.close_turns(&owner, &request),
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
    Ok(PrivateResponse::new(request_id, (), state.privacy.clone()))
}

#[tauri::command]
pub async fn get_grouped_usage(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: GroupedUsageRequest,
    request_id: String,
) -> Result<PrivateResponse<GroupedUsageBundle>, Box<AppError>> {
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
    let dimension = request.dimension;
    let data = tauri::async_runtime::spawn_blocking(move || {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        #[cfg(all(debug_assertions, windows))]
        super::navigation_smoke::wait_if_blocked();
        let stamp = database.display_cache_stamp().ok();
        let data = database.grouped_usage_bundle(&request, at, &snapshot_id)?;
        if let Some(stamp) = stamp {
            let _ = database.remember_usage_display(
                &token_pulse_core::display_cache::UsageDisplayRequest::Groups {
                    request: request.clone(),
                },
                token_pulse_core::display_cache::UsageDisplayData::Groups {
                    value: data.clone(),
                },
                stamp,
            );
        }
        Ok(data)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(PrivateResponse::groups(
        request_id,
        data,
        state.privacy.clone(),
        dimension,
    ))
}

#[tauri::command]
pub async fn get_dashboard_bundle(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: DashboardRequest,
    request_id: String,
) -> Result<PrivateResponse<DashboardBundle>, Box<AppError>> {
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
    // Native acceptance observes the validated real IPC request; release builds contain no probe state.
    #[cfg(debug_assertions)]
    if std::env::args().any(|arg| arg == "--native-smoke") {
        if let Ok(mut latest) = state.native_dashboard_request.lock() {
            *latest = Some(request.clone());
        }
    }
    let database = state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    let snapshot_id = request_id.clone();
    let data = tauri::async_runtime::spawn_blocking(move || {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        #[cfg(all(debug_assertions, windows))]
        super::navigation_smoke::wait_if_blocked();
        let stamp = database.display_cache_stamp().ok();
        let data = database.dashboard_bundle(&request, at, &snapshot_id)?;
        if let Some(stamp) = stamp {
            let _ = database.remember_usage_display(
                &token_pulse_core::display_cache::UsageDisplayRequest::Dashboard {
                    request: request.clone(),
                },
                token_pulse_core::display_cache::UsageDisplayData::Dashboard {
                    value: data.clone(),
                },
                stamp,
            );
        }
        Ok(data)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}

#[tauri::command]
pub async fn get_context_snapshot(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    session_key: String,
    request_id: String,
) -> Result<PrivateResponse<ContextSnapshot>, Box<AppError>> {
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
    Ok(PrivateResponse::new(
        request_id,
        context,
        state.privacy.clone(),
    ))
}
