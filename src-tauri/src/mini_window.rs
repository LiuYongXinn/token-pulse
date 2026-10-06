//! Dedicated local WebView. All native actions remain behind window-specific commands.
use tauri::{Emitter, Manager, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    mini::{MiniWindowAction, MiniWindowState},
    placement::{MiniPreferenceChange, WindowPlacement, WorkArea},
    protocol::{Response, validate_request_id},
};

pub fn show(app: &tauri::AppHandle) -> Result<(), String> {
    show_internal(app, None, None)
}
pub(super) fn show_taskbar(
    app: &tauri::AppHandle,
    request: &super::taskbar_service::ActionRequest,
) -> Result<(), String> {
    show_internal(app, None, Some(request))
}
pub(super) fn show_fallback(
    app: &tauri::AppHandle,
    request: &super::taskbar_service::FallbackRequest,
) -> Result<(), String> {
    show_internal(app, Some(request), None)
}
fn show_internal(
    app: &tauri::AppHandle,
    fallback: Option<&super::taskbar_service::FallbackRequest>,
    action: Option<&super::taskbar_service::ActionRequest>,
) -> Result<(), String> {
    let current = || {
        fallback.is_none_or(|request| request.current())
            && action.is_none_or(|request| request.current())
    };
    let runtime = app.state::<super::RuntimeState>();
    let _creation = runtime
        .mini_creation
        .lock()
        .map_err(|_| "WINDOW_STATE_UNAVAILABLE")?;
    if !current() {
        if let Some(request) = fallback {
            request.cancel();
        }
        return Ok(());
    }
    let window = if let Some(window) = app.get_webview_window("mini") {
        if fallback.is_some() && window.is_visible().map_err(|e| e.to_string())? {
            return Ok(());
        }
        if fit_current(&window).is_err() {
            eprintln!("MINI_PLACEMENT_UNAVAILABLE");
        }
        window
    } else {
        let state = app.state::<super::RuntimeState>();
        let preferences = state
            .database
            .as_ref()
            .map_err(|e| e.code.to_string())?
            .mini_window_preferences()
            .map_err(|e| e.code.to_string())?;
        let interaction = preferences.interaction;
        let (width, height) = dimensions(interaction.expanded);
        let theme = state
            .database
            .as_ref()
            .ok()
            .and_then(|db| db.display_settings().ok())
            .map(|s| s.preferences.theme)
            .unwrap_or_default();
        let theme = match theme {
            token_pulse_core::settings::AppTheme::Dark => Some(tauri::Theme::Dark),
            token_pulse_core::settings::AppTheme::Light => Some(tauri::Theme::Light),
            token_pulse_core::settings::AppTheme::System => None,
        };
        let window = tauri::WebviewWindowBuilder::new(
            app,
            "mini",
            tauri::WebviewUrl::App("index.html?window=mini".into()),
        )
        .title("TokenPulse · 悬浮窗")
        .inner_size(width, height)
        .resizable(false)
        .decorations(false)
        .skip_taskbar(true)
        .always_on_top(interaction.pinned)
        .theme(theme)
        .center()
        .focused(fallback.is_none() && action.is_none())
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
        *state
            .mini_window
            .lock()
            .map_err(|_| "WINDOW_STATE_UNAVAILABLE")? = interaction;
        if let Some(placement) = preferences.placement {
            restore_placement(&window, &placement, width, height)?;
        } else {
            fit_current(&window)?;
        }
        window
    };
    if !current() {
        if let Some(request) = fallback {
            request.cancel();
        }
        return Ok(());
    }
    // Explicit show, tray and recovery key all restore interaction, including when saving fails.
    super::mini_passthrough::recover(&window).map_err(|e| e.to_string())?;
    if !current() {
        return Ok(());
    }
    if save_current_placement(&window).is_err() {
        eprintln!("MINI_PLACEMENT_SAVE_FAILED");
    }
    if let Some(request) = fallback {
        show_without_activation(&window, request)?;
    } else if let Some(request) = action {
        let target = window.clone();
        let owned_request = request.clone();
        super::taskbar_commands::on_main_thread(app, request, move || {
            {
                // Do not hold the interaction mutex while waiting for a main-thread dispatch.
                let state = target.app_handle().state::<super::RuntimeState>();
                let mut interaction = state
                    .mini_window
                    .lock()
                    .map_err(|_| ErrorCode::WindowUnavailable)?;
                set_expanded(&target, &mut interaction, true)?;
            }
            if !owned_request.current() {
                return Ok(());
            }
            target.show().map_err(|_| ErrorCode::WindowUnavailable)?;
            super::quota_commands::update_visibility(&target);
            target.set_focus().map_err(|_| ErrorCode::WindowUnavailable)
        })
        .map_err(|e| e.to_string())?;
    } else {
        window.show().map_err(|e| e.to_string())?;
    }
    // The native frame can finish changing while a hidden WebView is being created.
    // Clamp again after showing, using its actual outer bounds rather than the fixed client size.
    if fit_current(&window).is_err() {
        eprintln!("MINI_PLACEMENT_UNAVAILABLE");
    }
    super::quota_commands::update_visibility(&window);
    if fallback.is_some() || action.is_some() {
        Ok(())
    } else {
        window.set_focus().map_err(|e| e.to_string())
    }
}
#[cfg(windows)]
fn show_without_activation(
    window: &WebviewWindow,
    request: &super::taskbar_service::FallbackRequest,
) -> Result<(), String> {
    let target = window.clone();
    let owned_request = request.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    window
        .app_handle()
        .run_on_main_thread(move || {
            let result = (|| {
                if !owned_request.current() {
                    owned_request.cancel();
                    return Ok(());
                }
                nonactivating_show_owned(&target).map_err(|_| "WINDOW_STATE_UNAVAILABLE")
            })();
            let _ = sender.send(result);
        })
        .map_err(|e| e.to_string())?;
    match receiver.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(result) => result.map_err(str::to_owned),
        Err(_) => {
            request.cancel();
            Err("WINDOW_STATE_UNAVAILABLE".into())
        }
    }
}
#[cfg(windows)]
const NONACTIVATING_SHOW_SUBCLASS: usize = 0x54504642;
#[cfg(windows)]
unsafe extern "system" fn keep_nonactivating_show(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    _id: usize,
    _data: usize,
) -> isize {
    use windows_sys::Win32::UI::{Shell::*, WindowsAndMessaging::*};
    let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    if message == WM_STYLECHANGING && wparam as isize == GWL_EXSTYLE as isize && lparam != 0 {
        unsafe {
            (&mut *(lparam as *mut STYLESTRUCT)).styleNew |= WS_EX_NOACTIVATE;
        }
    }
    if message == WM_NCDESTROY {
        unsafe {
            RemoveWindowSubclass(
                hwnd,
                Some(keep_nonactivating_show),
                NONACTIVATING_SHOW_SUBCLASS,
            );
        }
    }
    result
}
#[cfg(windows)]
thread_local! {
    static FALLBACK_ACTIVATION_TARGET: std::cell::Cell<windows_sys::Win32::Foundation::HWND> = const { std::cell::Cell::new(std::ptr::null_mut()) };
}
#[cfg(windows)]
unsafe extern "system" fn prevent_fallback_activation(
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CallNextHookEx, HCBT_ACTIVATE};
    if code == HCBT_ACTIVATE as i32
        && FALLBACK_ACTIVATION_TARGET.with(|target| target.get() == wparam as _)
    {
        return 1;
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}
#[cfg(windows)]
fn nonactivating_show_owned(window: &WebviewWindow) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{GetLastError, SetLastError},
        UI::{Shell::*, WindowsAndMessaging::*},
    };
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0.cast();
    unsafe {
        SetLastError(0);
        let previous = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if previous == 0 && GetLastError() != 0 {
            return Err("WINDOW_STATE_UNAVAILABLE".into());
        }
        if SetWindowSubclass(
            hwnd,
            Some(keep_nonactivating_show),
            NONACTIVATING_SHOW_SUBCLASS,
            0,
        ) == 0
        {
            return Err("WINDOW_STATE_UNAVAILABLE".into());
        }
        SetLastError(0);
        let old = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, previous | WS_EX_NOACTIVATE as isize);
        let prepared = old != 0 || GetLastError() == 0;
        // SW_SHOW explicitly activates even with NOACTIVATE styles. Veto only this
        // owned window's activation, on its own thread, during the synchronous show.
        let hook = SetWindowsHookExW(
            WH_CBT,
            Some(prevent_fallback_activation),
            std::ptr::null_mut(),
            windows_sys::Win32::System::Threading::GetCurrentThreadId(),
        );
        FALLBACK_ACTIVATION_TARGET.with(|target| target.set(hwnd));
        // Tauri/tao must observe the visibility transition so later hide/style changes work.
        // The temporary subclass retains NOACTIVATE through tao's style reconstruction.
        let result = if prepared && !hook.is_null() {
            window.show().map_err(|e| e.to_string())
        } else {
            Err("WINDOW_STATE_UNAVAILABLE".into())
        };
        let hook_removed = hook.is_null() || UnhookWindowsHookEx(hook) != 0;
        FALLBACK_ACTIVATION_TARGET.with(|target| target.set(std::ptr::null_mut()));
        let removed = RemoveWindowSubclass(
            hwnd,
            Some(keep_nonactivating_show),
            NONACTIVATING_SHOW_SUBCLASS,
        ) != 0;
        SetLastError(0);
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let readable = current != 0 || GetLastError() == 0;
        let restored = if readable && removed {
            let restored =
                (current & !(WS_EX_NOACTIVATE as isize)) | (previous & WS_EX_NOACTIVATE as isize);
            SetLastError(0);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, restored) != 0 || GetLastError() == 0
        } else {
            false
        };
        if !removed || !restored || !hook_removed {
            return Err("WINDOW_STATE_UNAVAILABLE".into());
        }
        result?;
    }
    if window.is_visible().map_err(|e| e.to_string())? {
        Ok(())
    } else {
        Err("WINDOW_STATE_UNAVAILABLE".into())
    }
}
#[cfg(not(windows))]
fn show_without_activation(
    _window: &WebviewWindow,
    _request: &super::taskbar_service::FallbackRequest,
) -> Result<(), String> {
    Err("WINDOW_STATE_UNAVAILABLE".into())
}
fn dimensions(expanded: bool) -> (f64, f64) {
    if expanded {
        (360.0, 380.0)
    } else {
        (280.0, 220.0)
    }
}

#[tauri::command]
pub fn mini_window_action(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request: MiniWindowAction,
    request_id: String,
) -> Result<Response<MiniWindowState>, Box<AppError>> {
    validate_request_id(&request_id)
        .map_err(|code| AppError::new(code, "invalid-request".into()))?;
    if window.label() != "mini" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    let mut interaction = state.mini_window.lock().map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::WindowUnavailable,
            request_id.clone(),
        ))
    })?;
    let result = match request {
        MiniWindowAction::Read {} => Ok(()),
        MiniWindowAction::SetExpanded { expanded } => {
            set_expanded(&window, &mut interaction, expanded)
                .map_err(|error| Box::new(AppError::new(error, request_id.clone())))?;
            Ok(())
        }
        MiniWindowAction::SetPinned { pinned } => {
            window.set_always_on_top(pinned).map_err(|_| {
                Box::new(AppError::new(
                    ErrorCode::WindowUnavailable,
                    request_id.clone(),
                ))
            })?;
            if let Err(error) = persist(window.app_handle(), MiniPreferenceChange::Pinned(pinned)) {
                let _ = window.set_always_on_top(interaction.pinned);
                return Err(Box::new(AppError::new(error, request_id)));
            }
            interaction.pinned = pinned;
            Ok(())
        }
        MiniWindowAction::Drag {} => window.start_dragging(),
        MiniWindowAction::Hide {} => {
            if save_current_placement(&window).is_err() {
                eprintln!("MINI_PLACEMENT_SAVE_FAILED");
            }
            window
                .hide()
                .map(|_| super::quota_commands::update_visibility(&window))
        }
    };
    result.map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::WindowUnavailable,
            request_id.clone(),
        ))
    })?;
    Ok(Response::new(request_id, *interaction))
}

fn set_expanded(
    window: &WebviewWindow,
    interaction: &mut MiniWindowState,
    expanded: bool,
) -> Result<(), ErrorCode> {
    let (width, height) = dimensions(expanded);
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    if let Err(error) = persist(
        window.app_handle(),
        MiniPreferenceChange::Expanded(expanded),
    ) {
        let (width, height) = dimensions(interaction.expanded);
        let _ = window.set_size(tauri::LogicalSize::new(width, height));
        return Err(error);
    }
    interaction.expanded = expanded;
    if fit_current(window).is_err() {
        eprintln!("MINI_PLACEMENT_UNAVAILABLE");
    }
    if save_current_placement(window).is_err() {
        eprintln!("MINI_PLACEMENT_SAVE_FAILED");
    }
    let _ = window.emit("mini_interaction_changed", ());
    Ok(())
}
fn area(monitor: &tauri::Monitor) -> WorkArea {
    let work = monitor.work_area();
    WorkArea {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
        scale: monitor.scale_factor(),
    }
}
fn restore_placement(
    window: &WebviewWindow,
    placement: &WindowPlacement,
    width_dip: f64,
    height_dip: f64,
) -> Result<(), String> {
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let monitor = monitors
        .iter()
        .find(|m| {
            placement
                .monitor
                .as_ref()
                .is_some_and(|name| m.name() == Some(name))
        })
        .cloned()
        .or(window.primary_monitor().map_err(|e| e.to_string())?)
        .ok_or("MONITOR_UNAVAILABLE")?;
    // A hidden Win32 window can still report its intermediate client height before
    // decoration removal completes. The floating display has two fixed logical sizes.
    let (x, y) = area(&monitor)
        .restore(placement, width_dip, height_dip)
        .map_err(|e| e.to_string())?;
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}
pub(super) fn fit_current(window: &WebviewWindow) -> Result<(), String> {
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .or(window.primary_monitor().map_err(|e| e.to_string())?)
        .ok_or("MONITOR_UNAVAILABLE")?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let placement = area(&monitor)
        .capture(position.x, position.y, monitor.name().cloned())
        .map_err(|e| e.to_string())?;
    // An undecorated Win32 WebView can still have a native resize / shadow frame.
    // Placement uses the outer origin, so fitting only its client leaves that frame off screen.
    let logical = window
        .outer_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);
    let (x, y) = area(&monitor)
        .restore(&placement, logical.width, logical.height)
        .map_err(|e| e.to_string())?;
    if (x, y) != (position.x, position.y) {
        move_for_fit(window, x, y)?;
    }
    Ok(())
}
#[cfg(windows)]
fn move_for_fit(window: &WebviewWindow, x: i32, y: i32) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };
    // tao's set_position also reapplies visibility flags, which calls ShowWindow(SW_SHOW)
    // after the fallback's temporary NOACTIVATE guard has been removed. Only move this
    // already-sized owned window; keep its visibility, z-order and foreground unchanged.
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0.cast();
    if unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            0,
            0,
            SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
        )
    } == 0
    {
        return Err("WINDOW_STATE_UNAVAILABLE".into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn move_for_fit(window: &WebviewWindow, x: i32, y: i32) -> Result<(), String> {
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}
fn persist(app: &tauri::AppHandle, change: MiniPreferenceChange) -> Result<(), ErrorCode> {
    let runtime = app.state::<super::RuntimeState>();
    let db = runtime.database.as_ref().map_err(|e| e.code)?;
    let at = token_pulse_core::numeric::EpochMs::new(
        token_pulse_collector::jobs::now_ms().map_err(|e| e.code)?,
    )?;
    let (revision, changed) = db
        .update_mini_window_preferences(change, at)
        .map_err(|e| e.code)?;
    if changed {
        let _ = app.emit(
            "settings_changed",
            token_pulse_core::settings::SettingsChanged {
                settings_revision: revision,
            },
        );
    }
    Ok(())
}
pub(super) fn save_current_placement(window: &WebviewWindow) -> Result<(), ErrorCode> {
    let monitor = window
        .current_monitor()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        .ok_or(ErrorCode::WindowUnavailable)?;
    let position = window
        .outer_position()
        .map_err(|_| ErrorCode::WindowUnavailable)?;
    let placement = area(&monitor).capture(position.x, position.y, monitor.name().cloned())?;
    persist(
        window.app_handle(),
        MiniPreferenceChange::Placement(placement),
    )
}
pub(super) fn schedule_placement(app: &tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    let Some(runtime) = app.try_state::<super::RuntimeState>() else {
        return;
    };
    runtime
        .mini_geometry_sequence
        .fetch_add(1, Ordering::Relaxed);
    if runtime
        .mini_geometry_worker
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<super::RuntimeState>();
        let sequence = loop {
            let sequence = runtime.mini_geometry_sequence.load(Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_millis(250));
            if sequence != runtime.mini_geometry_sequence.load(Ordering::Relaxed) {
                continue;
            }
            if let Some(window) = app.get_webview_window("mini") {
                // Windows' suggested DPI-change rectangle can scale the old outer
                // frame as if it were the client. Restore our fixed logical client
                // first; the resulting Resized event schedules fitting / persistence
                // after the real native frame settles.
                match restore_client_size(&window) {
                    Ok(false) => break sequence,
                    Err(_) => {
                        eprintln!("MINI_DIMENSIONS_UNAVAILABLE");
                        break sequence;
                    }
                    Ok(true) => {}
                }
                if fit_current(&window).is_err() {
                    eprintln!("MINI_PLACEMENT_UNAVAILABLE");
                } else if sequence == runtime.mini_geometry_sequence.load(Ordering::Relaxed)
                    && save_current_placement(&window).is_err()
                {
                    eprintln!("MINI_PLACEMENT_SAVE_FAILED");
                }
            }
            break sequence;
        };
        runtime.mini_geometry_worker.store(false, Ordering::Release);
        // An event arriving during the final write/reset must schedule another bounded worker.
        if sequence != runtime.mini_geometry_sequence.load(Ordering::Relaxed) {
            schedule_placement(&app);
        }
    });
}
fn restore_client_size(window: &WebviewWindow) -> Result<bool, String> {
    let runtime = window.app_handle().state::<super::RuntimeState>();
    let expanded = runtime
        .mini_window
        .lock()
        .map_err(|_| "WINDOW_STATE_UNAVAILABLE")?
        .expanded;
    let (width, height) = dimensions(expanded);
    let logical = tauri::LogicalSize::new(width, height);
    let expected = logical.to_physical::<u32>(window.scale_factor().map_err(|e| e.to_string())?);
    if window.inner_size().map_err(|e| e.to_string())? != expected {
        window.set_size(logical).map_err(|e| e.to_string())?;
        return Ok(false);
    }
    Ok(true)
}
