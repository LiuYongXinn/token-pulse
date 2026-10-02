use std::sync::Arc;
use tauri::{Emitter, Manager, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    protocol::{Response, validate_request_id},
    taskbar::*,
};
pub(super) fn service(
    app: &tauri::AppHandle,
) -> Option<Arc<super::taskbar_service::TaskbarService>> {
    app.try_state::<super::RuntimeState>()?
        .taskbar
        .lock()
        .ok()?
        .clone()
}
pub(super) fn initialize(app: &tauri::AppHandle) {
    let read_app = app.clone();
    let notify_app = app.clone();
    let visible_app = app.clone();
    let executable = match std::env::current_exe() {
        Ok(path) => path.with_file_name("token-pulse-taskbar-host.exe"),
        Err(_) => return,
    };
    let result = super::taskbar_service::TaskbarService::start(
        executable,
        Arc::new(move || {
            let state = read_app.state::<super::RuntimeState>();
            let db = state.database.as_ref().map_err(|e| e.code)?;
            let at = token_pulse_core::numeric::EpochMs::new(
                token_pulse_collector::jobs::now_ms().map_err(|e| e.code)?,
            )?;
            let input = db
                .taskbar_input(at, &uuid::Uuid::new_v4().to_string())
                .map_err(|e| e.code)?;
            let quota = state.quota.as_ref().ok().and_then(|q| q.snapshot().ok());
            let view = input.usage.as_ref().map(|usage| {
                token_pulse_taskbar::TaskbarView::from_optional_snapshots(
                    usage,
                    quota.as_ref(),
                    input.privacy,
                )
            });
            Ok(super::taskbar_service::Input {
                configuration: input.configuration,
                privacy: input.privacy,
                view,
            })
        }),
        Arc::new(move |snapshot| {
            let _ = notify_app.emit("taskbar_status_changed", snapshot);
        }),
        Arc::new(move |visible| {
            if let Some(state) = visible_app.try_state::<super::RuntimeState>() {
                if let Ok(quota) = &state.quota {
                    quota.set_visible(token_pulse_quota::service::DisplayEntry::Taskbar, visible);
                }
            }
        }),
        {
            let fallback_app = app.clone();
            Arc::new(move |request| {
                if !request.current() {
                    request.cancel();
                    return Ok(false);
                }
                if !request.show {
                    return fallback_app
                        .get_webview_window("mini")
                        .map(|window| {
                            window
                                .is_visible()
                                .map_err(|_| ErrorCode::WindowUnavailable)
                        })
                        .unwrap_or(Ok(false));
                }
                let state = fallback_app.state::<super::RuntimeState>();
                let preferences = state
                    .database
                    .as_ref()
                    .map_err(|e| e.code)?
                    .taskbar_preferences()
                    .map_err(|e| e.code)?
                    .preferences;
                if !preferences.enabled || !preferences.fallback_to_mini || !request.current() {
                    request.cancel();
                    return Ok(false);
                }
                super::mini_window::show_fallback(&fallback_app, &request)
                    .map_err(|_| ErrorCode::WindowUnavailable)?;
                fallback_app
                    .get_webview_window("mini")
                    .ok_or(ErrorCode::WindowUnavailable)?
                    .is_visible()
                    .map_err(|_| ErrorCode::WindowUnavailable)
            })
        },
    );
    if let Ok(service) = result {
        if let Ok(mut slot) = app.state::<super::RuntimeState>().taskbar.lock() {
            *slot = Some(Arc::new(service));
        }
    }
}
fn authorized(window: &WebviewWindow, id: &str) -> Result<(), Box<AppError>> {
    validate_request_id(id).map_err(|e| Box::new(AppError::new(e, "invalid-request".into())))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            id.into(),
        )));
    }
    Ok(())
}
#[tauri::command]
pub async fn get_taskbar_preferences(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<Response<TaskbarPreferencesSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    let data = tauri::async_runtime::spawn_blocking(move || db.taskbar_preferences())
        .await
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::TaskbarEmbedFailed,
                request_id.clone(),
            ))
        })?
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    Ok(Response::new(request_id, data))
}
#[tauri::command]
pub async fn set_taskbar_preferences(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: TaskbarPreferencesMutation,
    request_id: String,
) -> Result<Response<TaskbarPreferencesSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    request
        .validate()
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    let db = state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    let owner = service(window.app_handle()).ok_or_else(|| {
        Box::new(AppError::new(
            ErrorCode::TaskbarEmbedFailed,
            request_id.clone(),
        ))
    })?;
    let app = window.app_handle().clone();
    let (data, changed) = tauri::async_runtime::spawn_blocking(move || {
        let _pause = owner.pause_publication()?;
        db.mutate_taskbar_preferences(
            request,
            token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?,
        )
    })
    .await
    .map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::TaskbarEmbedFailed,
            request_id.clone(),
        ))
    })?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
    if changed {
        let _ = app.emit(
            "settings_changed",
            token_pulse_core::settings::SettingsChanged {
                settings_revision: data.settings_revision.clone(),
            },
        );
    }
    Ok(Response::new(request_id, data))
}
#[tauri::command]
pub fn get_taskbar_status(
    window: WebviewWindow,
    request_id: String,
) -> Result<Response<TaskbarRuntimeSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let snapshot = service(window.app_handle())
        .ok_or_else(|| {
            Box::new(AppError::new(
                ErrorCode::TaskbarEmbedFailed,
                request_id.clone(),
            ))
        })?
        .snapshot()
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    Ok(Response::new(request_id, snapshot))
}
#[tauri::command]
pub fn retry_taskbar_embed(
    window: WebviewWindow,
    request_id: String,
) -> Result<Response<()>, Box<AppError>> {
    authorized(&window, &request_id)?;
    service(window.app_handle())
        .ok_or_else(|| {
            Box::new(AppError::new(
                ErrorCode::TaskbarEmbedFailed,
                request_id.clone(),
            ))
        })?
        .invalidate();
    Ok(Response::new(request_id, ()))
}
