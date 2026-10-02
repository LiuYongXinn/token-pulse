use std::{
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use token_pulse_core::{
    error::{AppError, ErrorCode},
    numeric::EpochMs,
    privacy::PrivateResponse,
    protocol::{QuotaSnapshot, Response, validate_request_id},
    quota::*,
    settings::SettingsChanged,
};
use token_pulse_quota::NativeService;
fn main_only(window: &WebviewWindow, id: &str) -> Result<(), Box<AppError>> {
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
fn unhidden(privacy: &token_pulse_core::privacy::PrivacyState) -> Result<(), ErrorCode> {
    if privacy.current()?.privacy {
        return Err(ErrorCode::PermissionDenied);
    }
    Ok(())
}
fn now() -> Result<EpochMs, ErrorCode> {
    EpochMs::new(
        i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| ErrorCode::InvalidQuery)?
                .as_millis(),
        )
        .map_err(|_| ErrorCode::NumericOverflow)?,
    )
}
#[tauri::command]
pub async fn get_account_service_config(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<AccountServiceConfigSnapshot>, Box<AppError>> {
    main_only(&window, &request_id)?;
    let db = state
        .database
        .as_ref()
        .cloned()
        .map_err(|error| Box::new(AppError::new(error.code, request_id.clone())))?;
    let result = tauri::async_runtime::spawn_blocking(move || db.account_service_preferences())
        .await
        .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
        .map_err(|error| Box::new(AppError::new(error.code, request_id.clone())))?;
    Ok(PrivateResponse::new(
        request_id,
        AccountServiceConfigSnapshot::from_preferences(&result.0, result.1),
        state.privacy.clone(),
    ))
}
#[tauri::command]
pub async fn choose_account_service(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, super::RuntimeState>,
    request: AccountServiceSelectionRequest,
    request_id: String,
) -> Result<PrivateResponse<Option<AccountServiceSelection>>, Box<AppError>> {
    main_only(&window, &request_id)?;
    unhidden(&state.privacy).map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    let db = state
        .database
        .as_ref()
        .cloned()
        .map_err(|error| Box::new(AppError::new(error.code, request_id.clone())))?;
    let selections = state.quota_selections.clone();
    let privacy = state.privacy.clone();
    #[cfg(all(debug_assertions, windows))]
    let fixture_home = if std::env::args().any(|a| a == "--native-smoke")
        && std::env::args().any(|a| a == "--native-account-dialogs-smoke")
        && state
            .data_directory
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("native-probe-"))
    {
        Some(state.data_directory.join("synthetic-account-dialog-home"))
    } else {
        None
    };
    let result = tauri::async_runtime::spawn_blocking(
        move || -> Result<Option<AccountServiceSelection>, ErrorCode> {
            #[cfg(all(debug_assertions, windows))]
            if fixture_home.is_some()
                && matches!(request.kind, AccountServiceSelectionKind::DetectLocal)
            {
                return Err(ErrorCode::PermissionDenied);
            }
            let (preferences, revision) = db.account_service_preferences().map_err(|e| e.code)?;
            if revision != request.expected_settings_revision {
                return Err(ErrorCode::RevisionConflict);
            }
            let base = if let Some(handle) = &request.base_selection_handle {
                let candidate = selections
                    .lock()
                    .map_err(|_| ErrorCode::QuotaServiceUnavailable)?
                    .get(handle, "main", Instant::now())?;
                if candidate.settings_revision != revision {
                    return Err(ErrorCode::RevisionConflict);
                }
                Some(candidate.target)
            } else {
                preferences.target
            };
            let target = if matches!(request.kind, AccountServiceSelectionKind::DetectLocal) {
                token_pulse_quota::detect_local_service(base.as_ref())?
            } else {
                let (exe, home) = match request.kind {
                    AccountServiceSelectionKind::DetectLocal => unreachable!(),
                    AccountServiceSelectionKind::Executable => {
                        let Some(file) = app
                            .dialog()
                            .file()
                            .set_parent(&window)
                            .set_title("选择 Codex 原生 codex.exe")
                            .add_filter("原生程序", &["exe"])
                            .blocking_pick_file()
                        else {
                            return Ok(None);
                        };
                        (
                            file.into_path().map_err(|_| ErrorCode::InvalidQuery)?,
                            base.as_ref()
                                .and_then(|target| target.home_path.as_ref())
                                .map(PathBuf::from),
                        )
                    }
                    AccountServiceSelectionKind::Home => {
                        let base = base.as_ref().ok_or(ErrorCode::QuotaDisconnected)?;
                        let Some(folder) = app
                            .dialog()
                            .file()
                            .set_parent(&window)
                            .set_title("选择此账户服务的 Codex Home")
                            .blocking_pick_folder()
                        else {
                            return Ok(None);
                        };
                        (
                            PathBuf::from(&base.executable_path),
                            Some(folder.into_path().map_err(|_| ErrorCode::InvalidQuery)?),
                        )
                    }
                    AccountServiceSelectionKind::DefaultHome => {
                        let base = base.as_ref().ok_or(ErrorCode::QuotaDisconnected)?;
                        (PathBuf::from(&base.executable_path), None)
                    }
                    AccountServiceSelectionKind::Current => {
                        let base = base.as_ref().ok_or(ErrorCode::QuotaDisconnected)?;
                        (
                            PathBuf::from(&base.executable_path),
                            base.home_path.as_ref().map(PathBuf::from),
                        )
                    }
                };
                #[cfg(all(debug_assertions, windows))]
                if let Some(expected_home) = fixture_home {
                    let expected = expected_home
                        .canonicalize()
                        .map_err(|_| ErrorCode::PermissionDenied)?;
                    if exe
                        .canonicalize()
                        .map_err(|_| ErrorCode::PermissionDenied)?
                        != expected.join("synthetic-codex.exe")
                        || home.as_ref().is_some_and(|home| {
                            home.canonicalize().ok().as_ref() != Some(&expected)
                        })
                    {
                        return Err(ErrorCode::PermissionDenied);
                    }
                }
                NativeService::inspect(&exe, home.as_deref())?
            };
            if !matches!(
                request.kind,
                AccountServiceSelectionKind::Executable | AccountServiceSelectionKind::DetectLocal
            ) && base
                .as_ref()
                .is_some_and(|base| base.executable_sha256 != target.executable_sha256)
            {
                return Err(ErrorCode::StaleConfirmation);
            }
            unhidden(&privacy)?;
            if db.account_service_preferences().map_err(|e| e.code)?.1 != revision {
                return Err(ErrorCode::RevisionConflict);
            }
            let handle = uuid::Uuid::new_v4().to_string();
            let preview = AccountServiceConfigSnapshot::from_preferences(
                &AccountServicePreferences {
                    target: Some(target.clone()),
                    auto_connect: preferences.auto_connect,
                },
                revision.clone(),
            );
            let mut leases = selections
                .lock()
                .map_err(|_| ErrorCode::QuotaServiceUnavailable)?;
            leases.insert(
                handle.clone(),
                AccountServiceCandidate {
                    target,
                    settings_revision: revision,
                },
                Instant::now(),
            )?;
            if let Some(old) = &request.base_selection_handle {
                leases.remove(old);
            }
            Ok(Some(AccountServiceSelection {
                selection_handle: handle,
                preview,
                expires_at_ms: EpochMs::new(
                    now()?
                        .value()
                        .checked_add(300_000)
                        .ok_or(ErrorCode::NumericOverflow)?,
                )?,
            }))
        },
    )
    .await
    .map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::QuotaServiceUnavailable,
            request_id.clone(),
        ))
    })?
    .map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    Ok(PrivateResponse::new(
        request_id,
        result,
        state.privacy.clone(),
    ))
}
#[tauri::command]
pub fn cancel_account_service_selection(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    selection_handle: String,
    request_id: String,
) -> Result<Response<()>, Box<AppError>> {
    main_only(&window, &request_id)?;
    state
        .quota_selections
        .lock()
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::QuotaServiceUnavailable,
                request_id.clone(),
            ))
        })?
        .remove(&selection_handle);
    Ok(Response::new(request_id, ()))
}
#[tauri::command]
pub async fn save_account_service_config(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: AccountServiceConfigMutation,
    request_id: String,
) -> Result<PrivateResponse<AccountServiceConfigSnapshot>, Box<AppError>> {
    main_only(&window, &request_id)?;
    let db = state
        .database
        .as_ref()
        .cloned()
        .map_err(|error| Box::new(AppError::new(error.code, request_id.clone())))?;
    let leases = state.quota_selections.clone();
    let actions = state.quota_config_actions.clone();
    let privacy = state.privacy.clone();
    let (data, changed) = tauri::async_runtime::spawn_blocking(move || -> Result<_, ErrorCode> {
        let _action = actions
            .lock()
            .map_err(|_| ErrorCode::QuotaServiceUnavailable)?;
        unhidden(&privacy)?;
        let candidate = leases
            .lock()
            .map_err(|_| ErrorCode::QuotaServiceUnavailable)?
            .get(&request.selection_handle, "main", Instant::now())?;
        if candidate.settings_revision != request.expected_settings_revision {
            return Err(ErrorCode::RevisionConflict);
        }
        NativeService::from_target(&candidate.target)?;
        let result = db
            .mutate_account_service(
                AccountServicePreferences {
                    target: Some(candidate.target),
                    auto_connect: request.auto_connect,
                },
                request.expected_settings_revision,
                now()?,
            )
            .map_err(|e| e.code)?;
        leases
            .lock()
            .map_err(|_| ErrorCode::QuotaServiceUnavailable)?
            .remove(&request.selection_handle);
        Ok(result)
    })
    .await
    .map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::QuotaServiceUnavailable,
            request_id.clone(),
        ))
    })?
    .map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    if changed {
        let _ = window.app_handle().emit(
            "settings_changed",
            SettingsChanged {
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
pub async fn manage_account_connection(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: AccountConnectionRequest,
    request_id: String,
) -> Result<PrivateResponse<QuotaSnapshot>, Box<AppError>> {
    main_only(&window, &request_id)?;
    request
        .validate()
        .map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    let db = state.database.as_ref().cloned().map_err(|error| error.code);
    let service = state
        .quota
        .as_ref()
        .cloned()
        .map_err(|code| Box::new(AppError::new(*code, request_id.clone())))?;
    let actions = state.quota_config_actions.clone();
    let privacy = state.privacy.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || -> Result<QuotaSnapshot, ErrorCode> {
            let _action = actions
                .lock()
                .map_err(|_| ErrorCode::QuotaServiceUnavailable)?;
            match request {
                AccountConnectionRequest::Connect {
                    expected_settings_revision,
                    expected_connection_epoch,
                    acknowledged_executable_sha256,
                } => {
                    unhidden(&privacy)?;
                    let db = db?;
                    let (preferences, revision) =
                        db.account_service_preferences().map_err(|e| e.code)?;
                    if revision != expected_settings_revision {
                        return Err(ErrorCode::RevisionConflict);
                    }
                    let target = preferences.target.ok_or(ErrorCode::QuotaDisconnected)?;
                    if acknowledged_executable_sha256 != target.executable_sha256 {
                        return Err(ErrorCode::StaleConfirmation);
                    }
                    service.connect(
                        NativeService::from_target(&target)?,
                        &expected_connection_epoch,
                    )
                }
                AccountConnectionRequest::Disconnect {
                    expected_connection_epoch,
                } => service.disconnect(&expected_connection_epoch),
                AccountConnectionRequest::SelectLimit {
                    expected_connection_epoch,
                    expected_quota_revision,
                    limit_id,
                } => service.select_limit(
                    &limit_id,
                    &expected_connection_epoch,
                    expected_quota_revision,
                ),
            }
        })
        .await
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::QuotaServiceUnavailable,
                request_id.clone(),
            ))
        })?
        .map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    Ok(PrivateResponse::new(
        request_id,
        result,
        state.privacy.clone(),
    ))
}
pub fn initialize(app: &tauri::AppHandle) {
    let state = app.state::<super::RuntimeState>();
    let (Ok(db), Ok(service)) = (&state.database, &state.quota) else {
        return;
    };
    let db = db.clone();
    let service = service.clone();
    let actions = Arc::clone(&state.quota_config_actions);
    tauri::async_runtime::spawn_blocking(move || {
        let Ok(_action) = actions.lock() else {
            return;
        };
        let Ok((preferences, _)) = db.account_service_preferences() else {
            eprintln!("QUOTA_CONFIG_UNAVAILABLE");
            return;
        };
        if !preferences.auto_connect {
            return;
        }
        let Some(target) = preferences.target else {
            return;
        };
        let Ok(snapshot) = service.snapshot() else {
            return;
        };
        match NativeService::from_target(&target) {
            Ok(native) => {
                let _ = service.connect(native, &snapshot.connection_epoch);
            }
            Err(_) => {
                let _ = service.report_unavailable(&snapshot.connection_epoch);
            }
        }
    });
}
