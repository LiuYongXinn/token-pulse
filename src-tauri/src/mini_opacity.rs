//! Native whole-window alpha. Only the HWND owner thread touches Win32 attributes.
use tauri::{Emitter, Manager, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    mini_opacity::*,
    numeric::EpochMs,
    protocol::{Response, validate_request_id},
};

#[cfg(windows)]
const OPACITY_SUBCLASS: usize = 0x54504f50;
#[cfg(windows)]
unsafe extern "system" fn retain_layered_style(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    _id: usize,
    _data: usize,
) -> isize {
    use windows_sys::Win32::UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass},
        WindowsAndMessaging::*,
    };
    // Tao rebuilds the extended styles on show/hide, pin and decoration changes. Preserve
    // only our layered bit while letting Tao manage cursor transparency and every other bit.
    let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    if message == WM_STYLECHANGING && wparam as isize == GWL_EXSTYLE as isize && lparam != 0 {
        let styles = unsafe { &mut *(lparam as *mut STYLESTRUCT) };
        styles.styleNew |= WS_EX_LAYERED;
    }
    if message == WM_NCDESTROY {
        unsafe {
            RemoveWindowSubclass(hwnd, Some(retain_layered_style), OPACITY_SUBCLASS);
        }
    }
    result
}

#[cfg(windows)]
fn apply_owned(window: &WebviewWindow, percent: u8) -> Result<(), ErrorCode> {
    use windows_sys::Win32::{
        Foundation::{GetLastError, SetLastError},
        UI::Shell::{GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass},
        UI::WindowsAndMessaging::*,
    };
    let alpha = native_alpha(percent)?;
    let hwnd = window.hwnd().map_err(|_| ErrorCode::WindowUnavailable)?.0 as _;
    // Retain topmost/tool-window and all other unrelated extended style bits.
    unsafe {
        SetLastError(0);
        let old = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if old == 0 && GetLastError() != 0 {
            return Err(ErrorCode::WindowUnavailable);
        }
        let mut old_alpha = 255;
        let mut old_key = 0;
        let mut old_flags = 0;
        let old_attributes = old & WS_EX_LAYERED as isize != 0
            && GetLayeredWindowAttributes(hwnd, &mut old_key, &mut old_alpha, &mut old_flags) != 0;
        let mut data = 0;
        let installed = GetWindowSubclass(
            hwnd,
            Some(retain_layered_style),
            OPACITY_SUBCLASS,
            &mut data,
        ) != 0;
        if !installed
            && SetWindowSubclass(hwnd, Some(retain_layered_style), OPACITY_SUBCLASS, 0) == 0
        {
            return Err(ErrorCode::WindowUnavailable);
        }
        SetLastError(0);
        let previous = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, old | WS_EX_LAYERED as isize);
        if previous == 0 && GetLastError() != 0 {
            if !installed {
                RemoveWindowSubclass(hwnd, Some(retain_layered_style), OPACITY_SUBCLASS);
            }
            return Err(ErrorCode::WindowUnavailable);
        }
        if SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA) == 0 {
            if !installed {
                RemoveWindowSubclass(hwnd, Some(retain_layered_style), OPACITY_SUBCLASS);
            }
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, old);
            if old_attributes {
                SetLayeredWindowAttributes(hwnd, old_key, old_alpha, old_flags);
            }
            return Err(ErrorCode::WindowUnavailable);
        }
    }
    Ok(())
}
#[cfg(not(windows))]
fn apply_owned(_window: &WebviewWindow, percent: u8) -> Result<(), ErrorCode> {
    if percent == 100 {
        Ok(())
    } else {
        Err(ErrorCode::WindowUnavailable)
    }
}
/// Call from a blocking worker, never while occupying the HWND owner thread.
pub(super) fn apply(window: &WebviewWindow, percent: u8) -> Result<(), ErrorCode> {
    validate_opacity(percent)?;
    let owner = window.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    window
        .run_on_main_thread(move || {
            let _ = sender.send(apply_owned(&owner, percent));
        })
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    receiver.recv().map_err(|_| ErrorCode::WindowUnavailable)?
}
fn configure(
    app: &tauri::AppHandle,
    request: MiniOpacityMutation,
) -> Result<MiniOpacitySnapshot, ErrorCode> {
    request.validate()?;
    if !cfg!(windows) {
        return Err(ErrorCode::WindowUnavailable);
    }
    let state = app.state::<super::RuntimeState>();
    // Shares the creation lock: a concurrent show cannot reapply an older persisted alpha.
    let _creation = state
        .mini_creation
        .lock()
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    let db = state.database.as_ref().map_err(|e| e.code)?;
    let before = db.mini_opacity(true).map_err(|e| e.code)?;
    if before.settings_revision != request.expected_settings_revision {
        return Err(ErrorCode::RevisionConflict);
    }
    let at = EpochMs::new(token_pulse_collector::jobs::now_ms().map_err(|e| e.code)?)?;
    let window = app.get_webview_window("mini");
    if let Some(window) = &window {
        apply(window, request.opacity_percent)?;
    }
    let (saved, changed) = match db.mutate_mini_opacity(request, true, at) {
        Ok(value) => value,
        Err(error) => {
            if let Some(window) = &window {
                if apply(window, before.opacity_percent).is_err() {
                    eprintln!("MINI_OPACITY_ROLLBACK_FAILED");
                    return Err(ErrorCode::WindowUnavailable);
                }
            }
            return Err(error.code);
        }
    };
    if changed {
        let _ = app.emit(
            "settings_changed",
            token_pulse_core::settings::SettingsChanged {
                settings_revision: saved.settings_revision.clone(),
            },
        );
    }
    Ok(saved)
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
pub async fn get_mini_opacity(
    window: WebviewWindow,
    request_id: String,
) -> Result<Response<MiniOpacitySnapshot>, Box<AppError>> {
    authorize(&window, &request_id, false)?;
    let app = window.app_handle().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<super::RuntimeState>();
        state
            .database
            .as_ref()
            .map_err(|e| e.code)?
            .mini_opacity(cfg!(windows))
            .map_err(|e| e.code)
    })
    .await
    .map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::WindowUnavailable,
            request_id.clone(),
        ))
    })?
    .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    Ok(Response::new(request_id, result))
}
#[tauri::command]
pub async fn set_mini_opacity(
    window: WebviewWindow,
    request: MiniOpacityMutation,
    request_id: String,
) -> Result<Response<MiniOpacitySnapshot>, Box<AppError>> {
    authorize(&window, &request_id, true)?;
    request
        .validate()
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    let app = window.app_handle().clone();
    let result = tauri::async_runtime::spawn_blocking(move || configure(&app, request))
        .await
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::WindowUnavailable,
                request_id.clone(),
            ))
        })?
        .map_err(|e| Box::new(AppError::new(e, request_id.clone())))?;
    Ok(Response::new(request_id, result))
}
