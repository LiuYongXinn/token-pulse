//! Register on the main HWND owner thread; prepare a second slot before retiring the old key.
use tauri::{Emitter, Manager, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    numeric::EpochMs,
    protocol::{Response, validate_request_id},
    shortcuts::*,
};
pub(super) const SLOTS: [i32; 2] = [0x5451, 0x5452];
pub(super) struct RecoveryRuntime {
    pub active: Option<(i32, RecoveryShortcut)>,
    pub status: ShortcutRegistration,
}
impl Default for RecoveryRuntime {
    fn default() -> Self {
        Self {
            active: None,
            status: ShortcutRegistration::Unavailable,
        }
    }
}
fn snapshot(app: &tauri::AppHandle) -> Result<RecoveryShortcutSnapshot, ErrorCode> {
    let runtime = app.state::<super::RuntimeState>();
    let hotkey = runtime
        .recovery_shortcut
        .lock()
        .map_err(|_| ErrorCode::ShortcutUnavailable)?;
    let (shortcut, settings_revision) = runtime
        .database
        .as_ref()
        .map_err(|e| e.code)?
        .recovery_shortcut()
        .map_err(|e| e.code)?;
    let registration = if hotkey
        .active
        .as_ref()
        .is_some_and(|(_, key)| key == &shortcut)
    {
        ShortcutRegistration::Ready
    } else {
        if hotkey.status == ShortcutRegistration::Ready {
            ShortcutRegistration::Unavailable
        } else {
            hotkey.status
        }
    };
    Ok(RecoveryShortcutSnapshot {
        shortcut,
        registration,
        settings_revision,
    })
}
#[cfg(windows)]
fn register(app: &tauri::AppHandle, id: i32, key: &RecoveryShortcut) -> Result<(), ErrorCode> {
    use windows_sys::Win32::{
        Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, GetLastError},
        UI::Input::KeyboardAndMouse::{MOD_NOREPEAT, RegisterHotKey},
    };
    let window = app
        .get_webview_window("main")
        .ok_or(ErrorCode::ShortcutUnavailable)?;
    let hwnd = window.hwnd().map_err(|_| ErrorCode::ShortcutUnavailable)?.0 as _;
    if unsafe { RegisterHotKey(hwnd, id, key.modifiers() | MOD_NOREPEAT, key.virtual_key()?) } == 0
    {
        return Err(
            if unsafe { GetLastError() } == ERROR_HOTKEY_ALREADY_REGISTERED {
                ErrorCode::ShortcutConflict
            } else {
                ErrorCode::ShortcutUnavailable
            },
        );
    }
    Ok(())
}
#[cfg(not(windows))]
fn register(_: &tauri::AppHandle, _: i32, _: &RecoveryShortcut) -> Result<(), ErrorCode> {
    Err(ErrorCode::ShortcutUnavailable)
}
fn unregister(app: &tauri::AppHandle, id: i32) {
    #[cfg(windows)]
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey(hwnd.0 as _, id);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (app, id);
    }
}
pub(super) fn initialize(app: &tauri::AppHandle) {
    let runtime = app.state::<super::RuntimeState>();
    let result = (|| {
        let (key, _) = runtime
            .database
            .as_ref()
            .map_err(|e| e.code)?
            .recovery_shortcut()
            .map_err(|e| e.code)?;
        register(app, SLOTS[0], &key)?;
        let mut state = runtime
            .recovery_shortcut
            .lock()
            .map_err(|_| ErrorCode::ShortcutUnavailable)?;
        state.active = Some((SLOTS[0], key));
        state.status = ShortcutRegistration::Ready;
        Ok::<_, ErrorCode>(())
    })();
    if let Err(error) = result {
        if let Ok(mut state) = runtime.recovery_shortcut.lock() {
            state.status = if cfg!(windows) {
                if error == ErrorCode::ShortcutConflict {
                    ShortcutRegistration::Conflict
                } else {
                    ShortcutRegistration::Unavailable
                }
            } else {
                ShortcutRegistration::Unsupported
            };
        }
        eprintln!("{error}");
    }
}
fn configure(
    app: &tauri::AppHandle,
    request: RecoveryShortcutMutation,
) -> Result<RecoveryShortcutSnapshot, ErrorCode> {
    request.validate()?;
    let runtime = app.state::<super::RuntimeState>();
    let mut state = runtime
        .recovery_shortcut
        .lock()
        .map_err(|_| ErrorCode::ShortcutUnavailable)?;
    let db = runtime.database.as_ref().map_err(|e| e.code)?;
    let (_, revision) = db.recovery_shortcut().map_err(|e| e.code)?;
    if revision.value() != request.expected_settings_revision.value() {
        return Err(ErrorCode::RevisionConflict);
    }
    let at = EpochMs::new(token_pulse_collector::jobs::now_ms().map_err(|e| e.code)?)?;
    let replacement = if state
        .active
        .as_ref()
        .is_some_and(|(_, key)| key == &request.shortcut)
    {
        None
    } else {
        let slot = if state.active.as_ref().is_some_and(|(id, _)| *id == SLOTS[0]) {
            SLOTS[1]
        } else {
            SLOTS[0]
        };
        // The inactive slot belongs to this HWND only. Clear any earlier cleanup residue.
        unregister(app, slot);
        register(app, slot, &request.shortcut)?;
        Some(slot)
    };
    let result = db.mutate_recovery_shortcut(request, at).map_err(|e| e.code);
    let (shortcut, settings_revision, changed) = match result {
        Ok(value) => value,
        Err(error) => {
            if let Some(slot) = replacement {
                unregister(app, slot);
            }
            return Err(error);
        }
    };
    if let Some(slot) = replacement {
        if let Some((old, _)) = &state.active {
            unregister(app, *old);
        }
        state.active = Some((slot, shortcut.clone()));
    }
    state.status = ShortcutRegistration::Ready;
    if changed {
        let _ = app.emit(
            "settings_changed",
            token_pulse_core::settings::SettingsChanged {
                settings_revision: settings_revision.clone(),
            },
        );
    }
    Ok(RecoveryShortcutSnapshot {
        shortcut,
        registration: state.status,
        settings_revision,
    })
}
fn authorize(window: &WebviewWindow, request_id: &str, write: bool) -> Result<(), Box<AppError>> {
    validate_request_id(request_id)
        .map_err(|e| Box::new(AppError::new(e, "invalid-request".into())))?;
    if window.label() != "main" && (write || window.label() != "mini") {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id.into(),
        )));
    }
    Ok(())
}
#[tauri::command]
pub async fn get_recovery_shortcut(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<Response<RecoveryShortcutSnapshot>, Box<AppError>> {
    authorize(&window, &request_id, false)?;
    let _ = state;
    let app = window.app_handle().clone();
    let result = tauri::async_runtime::spawn_blocking(move || snapshot(&app))
        .await
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::ShortcutUnavailable,
                request_id.clone(),
            ))
        })?
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    Ok(Response::new(request_id, result))
}
#[tauri::command]
pub async fn set_recovery_shortcut(
    window: WebviewWindow,
    request: RecoveryShortcutMutation,
    request_id: String,
) -> Result<Response<RecoveryShortcutSnapshot>, Box<AppError>> {
    authorize(&window, &request_id, true)?;
    request
        .validate()
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    let app = window.app_handle().clone();
    let owner = app.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _ = sender.send(configure(&owner, request));
    })
    .map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::ShortcutUnavailable,
            request_id.clone(),
        ))
    })?;
    let result = tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::ShortcutUnavailable,
                request_id.clone(),
            ))
        })?
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::ShortcutUnavailable,
                request_id.clone(),
            ))
        })?
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    Ok(Response::new(request_id, result))
}
pub(super) fn dispatch(app: &tauri::AppHandle, id: usize, packed: isize) {
    let runtime = app.state::<super::RuntimeState>();
    let accepted = runtime.recovery_shortcut.lock().is_ok_and(|state| {
        state.active.as_ref().is_some_and(|(owned, key)| {
            id == *owned as usize
                && (packed as u32 & 0xffff) == key.modifiers()
                && (packed as u32 >> 16) == key.virtual_key().unwrap_or(0)
        })
    });
    if accepted {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if super::mini_window::show(&app).is_err() {
                eprintln!("WINDOW_UNAVAILABLE");
            }
        });
    }
}
