//! Cursor transparency is a native action, gated by an actually owned recovery key.
use tauri::{Emitter, Manager, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    mini_passthrough::*,
    numeric::EpochMs,
    placement::MiniPreferenceChange,
    protocol::{Response, validate_request_id},
    shortcuts::ShortcutRegistration,
};
fn owner<T: Send + 'static>(
    app: &tauri::AppHandle,
    task: impl FnOnce(&tauri::AppHandle) -> Result<T, ErrorCode> + Send + 'static,
) -> Result<T, ErrorCode> {
    let target = app.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _ = sender.send(task(&target));
    })
    .map_err(|_| ErrorCode::WindowUnavailable)?;
    receiver.recv().map_err(|_| ErrorCode::WindowUnavailable)?
}
#[cfg(windows)]
fn observed(window: &WebviewWindow) -> Result<bool, ErrorCode> {
    use windows_sys::Win32::{
        Foundation::{GetLastError, SetLastError},
        UI::WindowsAndMessaging::*,
    };
    let hwnd = window.hwnd().map_err(|_| ErrorCode::WindowUnavailable)?.0 as _;
    unsafe {
        SetLastError(0);
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if style == 0 && GetLastError() != 0 {
            return Err(ErrorCode::WindowUnavailable);
        }
        Ok(style & WS_EX_TRANSPARENT as isize != 0)
    }
}
#[cfg(not(windows))]
fn observed(_window: &WebviewWindow) -> Result<bool, ErrorCode> {
    Ok(false)
}
fn snapshot_owned(app: &tauri::AppHandle) -> Result<MiniPassthroughSnapshot, ErrorCode> {
    let state = app.state::<super::RuntimeState>();
    let hotkey = state
        .recovery_shortcut
        .lock()
        .map_err(|_| ErrorCode::ShortcutUnavailable)?;
    let (prefs, key, revision) = state
        .database
        .as_ref()
        .map_err(|e| e.code)?
        .mini_recovery_preferences()
        .map_err(|e| e.code)?;
    let registration = if hotkey
        .active
        .as_ref()
        .is_some_and(|(_, active)| *active == key)
    {
        ShortcutRegistration::Ready
    } else if hotkey.status == ShortcutRegistration::Ready {
        ShortcutRegistration::Unavailable
    } else {
        hotkey.status
    };
    let window = app.get_webview_window("mini");
    Ok(MiniPassthroughSnapshot {
        enabled: window.as_ref().map(observed).transpose()?.unwrap_or(false),
        persisted_enabled: prefs.passthrough,
        window_present: window.is_some(),
        supported: cfg!(windows),
        recovery_shortcut: key,
        recovery_registration: registration,
        settings_revision: revision,
    })
}
fn cursor_owned(
    window: &WebviewWindow,
    enabled: bool,
    opacity: Option<u8>,
) -> Result<(), ErrorCode> {
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    if observed(window)? != enabled {
        return Err(ErrorCode::WindowUnavailable);
    }
    if let Some(opacity) = opacity {
        super::mini_opacity::apply_owned(window, opacity)?;
    }
    if observed(window)? != enabled {
        return Err(ErrorCode::WindowUnavailable);
    }
    Ok(())
}
pub(super) fn recover(window: &WebviewWindow) -> Result<(), ErrorCode> {
    let target = window.clone();
    // Called under the creation lock by a blocking worker. Even a failed/corrupt settings
    // read cannot prevent restoring interaction on an already existing native window.
    owner(window.app_handle(), move |app| {
        let state = app.state::<super::RuntimeState>();
        let opacity = state
            .database
            .as_ref()
            .ok()
            .and_then(|db| db.mini_window_preferences().ok())
            .map(|p| p.opacity_percent);
        cursor_owned(&target, false, None)?;
        if let Some(opacity) = opacity {
            if super::mini_opacity::apply_owned(&target, opacity).is_err() {
                eprintln!("MINI_OPACITY_RECOVERY_FAILED");
            }
        }
        Ok(())
    })?;
    let state = window.app_handle().state::<super::RuntimeState>();
    let save = (|| {
        let db = state.database.as_ref().map_err(|e| e.code)?;
        let at = EpochMs::new(token_pulse_collector::jobs::now_ms().map_err(|e| e.code)?)?;
        let (revision, changed) = db
            .update_mini_window_preferences(MiniPreferenceChange::Passthrough(false), at)
            .map_err(|e| e.code)?;
        if changed {
            let _ = window.app_handle().emit(
                "settings_changed",
                token_pulse_core::settings::SettingsChanged {
                    settings_revision: revision,
                },
            );
        }
        Ok::<_, ErrorCode>(())
    })();
    if save.is_err() {
        eprintln!("MINI_PASSTHROUGH_RECOVERY_SAVE_FAILED");
    }
    let _ = window.app_handle().emit("mini_interaction_changed", ());
    Ok(())
}
fn configure(
    app: &tauri::AppHandle,
    request: MiniPassthroughMutation,
) -> Result<MiniPassthroughSnapshot, ErrorCode> {
    request.validate()?;
    let state = app.state::<super::RuntimeState>();
    let _creation = state
        .mini_creation
        .lock()
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    let db = state.database.as_ref().map_err(|e| e.code)?;
    let (before, key, revision) = db.mini_recovery_preferences().map_err(|e| e.code)?;
    if revision != request.expected_settings_revision {
        return Err(ErrorCode::RevisionConflict);
    }
    if request.enabled && (!cfg!(windows) || request.acknowledged_recovery.as_ref() != Some(&key)) {
        return Err(ErrorCode::ShortcutUnavailable);
    }
    let at = EpochMs::new(token_pulse_collector::jobs::now_ms().map_err(|e| e.code)?)?;
    let enabled = request.enabled;
    let opacity = before.opacity_percent;
    let previous = owner(app, move |app| {
        let state = app.state::<super::RuntimeState>();
        // Never hold the key lock on a worker that is waiting for its owner thread.
        let hotkey = state
            .recovery_shortcut
            .lock()
            .map_err(|_| ErrorCode::ShortcutUnavailable)?;
        if enabled
            && !hotkey
                .active
                .as_ref()
                .is_some_and(|(_, active)| *active == key)
        {
            return Err(if hotkey.status == ShortcutRegistration::Conflict {
                ErrorCode::ShortcutConflict
            } else {
                ErrorCode::ShortcutUnavailable
            });
        }
        let Some(window) = app.get_webview_window("mini") else {
            return if enabled {
                Err(ErrorCode::WindowUnavailable)
            } else {
                Ok(false)
            };
        };
        let previous = observed(&window)?;
        if enabled {
            window.show().map_err(|_| ErrorCode::WindowUnavailable)?;
        }
        if let Err(error) = cursor_owned(&window, enabled, Some(opacity)) {
            // An incomplete native enable must never leave the window unreachable.
            let _ = cursor_owned(&window, false, None);
            return Err(error);
        }
        Ok(previous)
    })?;
    let (revision, changed) = match db.mutate_mini_passthrough(request, at) {
        Ok(value) => value,
        Err(error) => {
            // Disabling is a recovery operation: storage failure must never re-enable it.
            if enabled {
                let rollback = owner(app, move |app| {
                    if let Some(window) = app.get_webview_window("mini") {
                        cursor_owned(&window, previous, Some(opacity))
                    } else {
                        Ok(())
                    }
                });
                if rollback.is_err() {
                    eprintln!("MINI_PASSTHROUGH_ROLLBACK_FAILED");
                    let _ = owner(app, move |app| {
                        if let Some(window) = app.get_webview_window("mini") {
                            cursor_owned(&window, false, None)
                        } else {
                            Ok(())
                        }
                    });
                    let _ = app.emit("mini_interaction_changed", ());
                    return Err(ErrorCode::WindowUnavailable);
                }
            }
            let _ = app.emit("mini_interaction_changed", ());
            return Err(error.code);
        }
    };
    if changed {
        let _ = app.emit(
            "settings_changed",
            token_pulse_core::settings::SettingsChanged {
                settings_revision: revision,
            },
        );
    }
    let _ = app.emit("mini_interaction_changed", ());
    owner(app, snapshot_owned)
}
fn authorize(window: &WebviewWindow, id: &str, write: bool) -> Result<(), Box<AppError>> {
    validate_request_id(id).map_err(|e| Box::new(AppError::new(e, "invalid-request".into())))?;
    if window.label() != "main" && (write || window.label() != "mini") {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            id.into(),
        )));
    }
    Ok(())
}
#[tauri::command]
pub async fn get_mini_passthrough(
    window: WebviewWindow,
    request_id: String,
) -> Result<Response<MiniPassthroughSnapshot>, Box<AppError>> {
    authorize(&window, &request_id, false)?;
    let app = window.app_handle().clone();
    let value = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<super::RuntimeState>();
        let _creation = state
            .mini_creation
            .lock()
            .map_err(|_| ErrorCode::WindowUnavailable)?;
        owner(&app, snapshot_owned)
    })
    .await
    .map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::WindowUnavailable,
            request_id.clone(),
        ))
    })?
    .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    Ok(Response::new(request_id, value))
}
#[tauri::command]
pub async fn set_mini_passthrough(
    window: WebviewWindow,
    request: MiniPassthroughMutation,
    request_id: String,
) -> Result<Response<MiniPassthroughSnapshot>, Box<AppError>> {
    authorize(&window, &request_id, true)?;
    let app = window.app_handle().clone();
    let value = tauri::async_runtime::spawn_blocking(move || configure(&app, request))
        .await
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::WindowUnavailable,
                request_id.clone(),
            ))
        })?
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    Ok(Response::new(request_id, value))
}
