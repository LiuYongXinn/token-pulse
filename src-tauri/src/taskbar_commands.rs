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
    if let Ok(path) = std::env::current_exe() {
        initialize_at(app, path.with_file_name("token-pulse-taskbar-host.exe"));
    }
}
#[cfg(debug_assertions)]
pub(super) fn missing_host_fixture(app: &tauri::AppHandle, missing: bool) -> Result<(), String> {
    let runtime = app.state::<super::RuntimeState>();
    if !runtime
        .data_directory
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("native-probe-"))
        || !std::env::args().any(|arg| arg == "--native-smoke")
    {
        return Err("host fixture requires explicit isolated native smoke".into());
    }
    let previous = runtime.taskbar.lock().map_err(|_| "taskbar lock")?.take();
    if let Some(previous) = previous {
        previous.shutdown();
    }
    let executable = if missing {
        runtime.data_directory.join("token-pulse-taskbar-host.exe")
    } else {
        std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("token-pulse-taskbar-host.exe")
    };
    if missing && executable.exists() {
        return Err("missing host fixture unexpectedly exists".into());
    }
    initialize_at(app, executable);
    service(app).ok_or("replacement taskbar service missing")?;
    Ok(())
}
fn initialize_at(app: &tauri::AppHandle, executable: std::path::PathBuf) {
    let read_app = app.clone();
    let notify_app = app.clone();
    let visible_app = app.clone();
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
                    input.theme,
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
        {
            let action_app = app.clone();
            Arc::new(move |request| execute_action(&action_app, &request))
        },
    );
    if let Ok(service) = result {
        if let Ok(mut slot) = app.state::<super::RuntimeState>().taskbar.lock() {
            *slot = Some(Arc::new(service));
        }
    }
}
fn execute_action(
    app: &tauri::AppHandle,
    request: &super::taskbar_service::ActionRequest,
) -> Result<(), ErrorCode> {
    use token_pulse_taskbar::HostAction;
    if !request.current() {
        return Ok(());
    }
    let runtime = app.state::<super::RuntimeState>();
    let db = runtime.database.as_ref().map_err(|e| e.code)?;
    let configuration = db.taskbar_preferences().map_err(|e| e.code)?;
    if !configuration.preferences.enabled
        || configuration.settings_revision != request.settings_revision
    {
        return Err(ErrorCode::RevisionConflict);
    }
    if !request.current() {
        return Ok(());
    }
    match &request.action {
        HostAction::OpenFloat {} => {
            super::mini_window::show_taskbar(app, request).map_err(|_| ErrorCode::WindowUnavailable)
        }
        HostAction::OpenStats {} => {
            let id = uuid::Uuid::new_v4().to_string();
            let at = token_pulse_core::numeric::EpochMs::new(
                token_pulse_collector::jobs::now_ms().map_err(|e| e.code)?,
            )?;
            let usage = db.mini_usage(at, &id).map_err(|e| e.code)?;
            let data = token_pulse_core::mini::open_stats_request(
                &usage,
                &token_pulse_core::mini::MiniStatsOpenRequest {
                    expected_settings_revision: request.settings_revision.clone(),
                },
                id,
            )?;
            let owned_app = app.clone();
            on_main_thread(app, request, move || {
                super::navigation::publish(
                    &owned_app,
                    token_pulse_core::navigation::MainNavigationIntent::MiniStats {
                        request: Box::new(data),
                    },
                )?;
                super::show_main(&owned_app).map_err(|_| ErrorCode::WindowUnavailable)
            })
        }
        HostAction::OpenTaskbarSettings {} => {
            let owned_app = app.clone();
            on_main_thread(app, request, move || {
                super::navigation::publish(
                    &owned_app,
                    token_pulse_core::navigation::MainNavigationIntent::TaskbarSettings {},
                )?;
                super::show_main(&owned_app).map_err(|_| ErrorCode::WindowUnavailable)
            })
        }
        HostAction::SetPrivacy { enabled } => {
            let _ = super::settings_commands::update_privacy_from_taskbar(
                app,
                db.clone(),
                &runtime.privacy,
                token_pulse_core::settings::DisplayPrivacyMutation {
                    privacy: *enabled,
                    expected_settings_revision: request.settings_revision.clone(),
                },
                request,
            )
            .map_err(|e| e.code)?;
            Ok(())
        }
        HostAction::DisableTaskbar {} => {
            let owner = service(app).ok_or(ErrorCode::TaskbarEmbedFailed)?;
            let _pause = owner.pause_publication()?;
            if !request.mutation_current() {
                return Ok(());
            }
            request.mark_coordinated();
            let mut preferences = configuration.preferences;
            preferences.enabled = false;
            let at = token_pulse_core::numeric::EpochMs::new(
                token_pulse_collector::jobs::now_ms().map_err(|e| e.code)?,
            )?;
            let (data, changed) = db
                .mutate_taskbar_preferences(
                    TaskbarPreferencesMutation {
                        preferences,
                        expected_settings_revision: request.settings_revision.clone(),
                    },
                    at,
                )
                .map_err(|e| e.code)?;
            if changed {
                let _ = app.emit(
                    "settings_changed",
                    token_pulse_core::settings::SettingsChanged {
                        settings_revision: data.settings_revision,
                    },
                );
            }
            Ok(())
        }
    }
}
pub(super) fn on_main_thread(
    app: &tauri::AppHandle,
    request: &super::taskbar_service::ActionRequest,
    work: impl FnOnce() -> Result<(), ErrorCode> + Send + 'static,
) -> Result<(), ErrorCode> {
    let owned_request = request.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result = if owned_request.current() {
            work()
        } else {
            Ok(())
        };
        let _ = sender.send(result);
    })
    .map_err(|_| ErrorCode::WindowUnavailable)?;
    match receiver.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(result) => result,
        Err(_) => {
            request.timeout();
            Err(ErrorCode::WindowUnavailable)
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
