//! Clip the native floating window to the same outline as its WebView panel.
use tauri::WebviewWindow;
use token_pulse_core::error::ErrorCode;

#[cfg(windows)]
const SHAPE_SUBCLASS: usize = 0x54505348;
// Keep in sync with .mini-window in ui/src/mini/mini.css.
#[cfg(windows)]
const CORNER_RADIUS_DIP: i32 = 15;

#[cfg(windows)]
fn update_owned(hwnd: windows_sys::Win32::Foundation::HWND) -> Result<(), ErrorCode> {
    use windows_sys::Win32::{
        Foundation::{POINT, RECT},
        Graphics::Gdi::{ClientToScreen, CreateRoundRectRgn, DeleteObject, SetWindowRgn},
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{GetClientRect, GetWindowRect},
        },
    };
    unsafe {
        let mut client = RECT::default();
        let mut outer = RECT::default();
        let mut origin = POINT::default();
        if GetClientRect(hwnd, &mut client) == 0
            || GetWindowRect(hwnd, &mut outer) == 0
            || ClientToScreen(hwnd, &mut origin) == 0
        {
            return Err(ErrorCode::WindowUnavailable);
        }
        let width = client.right - client.left;
        let height = client.bottom - client.top;
        if width <= 0 || height <= 0 {
            return Ok(());
        }
        // Regions use outer-window coordinates; the CSS panel fills only the client area.
        let left = origin.x - outer.left;
        let top = origin.y - outer.top;
        let diameter = (CORNER_RADIUS_DIP * 2 * GetDpiForWindow(hwnd) as i32 + 48) / 96;
        let region = CreateRoundRectRgn(
            left,
            top,
            left + width + 1,
            top + height + 1,
            diameter,
            diameter,
        );
        if region.is_null() {
            return Err(ErrorCode::WindowUnavailable);
        }
        if SetWindowRgn(hwnd, region, 1) == 0 {
            DeleteObject(region);
            return Err(ErrorCode::WindowUnavailable);
        }
        // SetWindowRgn transfers ownership to Windows on success.
    }
    Ok(())
}

#[cfg(windows)]
unsafe extern "system" fn retain_shape(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    _id: usize,
    _data: usize,
) -> isize {
    use windows_sys::Win32::UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass},
        WindowsAndMessaging::{WM_DPICHANGED, WM_NCDESTROY, WM_SIZE},
    };
    let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    // Run after Tao updates the client bounds, including expand/collapse and monitor changes.
    if matches!(message, WM_SIZE | WM_DPICHANGED) && update_owned(hwnd).is_err() {
        eprintln!("MINI_SHAPE_UPDATE_FAILED");
    }
    if message == WM_NCDESTROY {
        unsafe {
            RemoveWindowSubclass(hwnd, Some(retain_shape), SHAPE_SUBCLASS);
        }
    }
    result
}

#[cfg(windows)]
fn install_owned(window: &WebviewWindow) -> Result<(), ErrorCode> {
    use windows_sys::Win32::UI::Shell::{RemoveWindowSubclass, SetWindowSubclass};
    let hwnd = window
        .hwnd()
        .map_err(|_| ErrorCode::WindowUnavailable)?
        .0
        .cast();
    if unsafe { SetWindowSubclass(hwnd, Some(retain_shape), SHAPE_SUBCLASS, 0) } == 0 {
        return Err(ErrorCode::WindowUnavailable);
    }
    if let Err(error) = update_owned(hwnd) {
        unsafe {
            RemoveWindowSubclass(hwnd, Some(retain_shape), SHAPE_SUBCLASS);
        }
        return Err(error);
    }
    Ok(())
}

/// Called by the creation worker; Win32 mutations stay on the HWND owner thread.
pub(super) fn install(window: &WebviewWindow) -> Result<(), ErrorCode> {
    #[cfg(windows)]
    {
        let owner = window.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        window
            .run_on_main_thread(move || {
                let _ = sender.send(install_owned(&owner));
            })
            .map_err(|_| ErrorCode::WindowUnavailable)?;
        receiver.recv().map_err(|_| ErrorCode::WindowUnavailable)?
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        Ok(())
    }
}
