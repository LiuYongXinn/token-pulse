use tauri::{Emitter, Manager, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    privacy::{DisplayPolicyStamp, PrivacyState, PrivateResponse},
    protocol::validate_request_id,
    settings::{
        DisplayPrivacyMutation, DisplaySettingsSnapshot, SettingsChanged, TimezoneMutation,
    },
};
fn authorized(window: &WebviewWindow, request_id: &str) -> Result<(), Box<AppError>> {
    validate_request_id(request_id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id.into(),
        )));
    }
    Ok(())
}
#[tauri::command]
pub async fn get_display_settings(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<DisplaySettingsSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    let data = tauri::async_runtime::spawn_blocking(move || db.display_settings())
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
pub async fn set_display_timezone(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: TimezoneMutation,
    request_id: String,
) -> Result<PrivateResponse<DisplaySettingsSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    request
        .validate()
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    let db = state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    let (data, changed) = tauri::async_runtime::spawn_blocking(move || {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        db.mutate_display_timezone(request, at)
    })
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e: token_pulse_store::StoreError| {
        Box::new(AppError::new(e.code, request_id.clone()))
    })?;
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

/// Shared coordinator for IPC and subsequent native actions. Never write privacy around this lock.
pub(super) fn update_privacy(
    app: &tauri::AppHandle,
    db: token_pulse_store::Database,
    policy: &PrivacyState,
    request: DisplayPrivacyMutation,
) -> token_pulse_store::StoreResult<DisplaySettingsSnapshot> {
    let (data, changed) = policy.commit_update(|| {
        let at = token_pulse_core::numeric::EpochMs::new(token_pulse_collector::jobs::now_ms()?)?;
        let (data, changed) = db.mutate_display_privacy(request, at)?;
        let stamp = DisplayPolicyStamp {
            settings_revision: data.settings_revision.clone(),
            privacy: data.preferences.privacy,
        };
        Ok::<_, token_pulse_store::StoreError>(((data, changed), stamp))
    })?;
    if changed {
        // Clients clear retained views before re-reading configuration or statistics.
        let _ = app.emit(
            "display_policy_changed",
            DisplayPolicyStamp {
                settings_revision: data.settings_revision.clone(),
                privacy: data.preferences.privacy,
            },
        );
        let _ = app.emit(
            "settings_changed",
            SettingsChanged {
                settings_revision: data.settings_revision.clone(),
            },
        );
    }
    Ok(data)
}

#[tauri::command]
pub async fn set_display_privacy(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: DisplayPrivacyMutation,
    request_id: String,
) -> Result<PrivateResponse<DisplaySettingsSnapshot>, Box<AppError>> {
    // Floating window capability is added together with the real window module.
    authorized(&window, &request_id)?;
    request
        .validate()
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    let db = state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    let policy = state.privacy.clone();
    let app = window.app_handle().clone();
    let data =
        tauri::async_runtime::spawn_blocking(move || update_privacy(&app, db, &policy, request))
            .await
            .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
            .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    Ok(PrivateResponse::new(
        request_id,
        data,
        state.privacy.clone(),
    ))
}
