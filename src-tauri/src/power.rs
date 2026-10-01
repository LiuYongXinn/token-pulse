//! Window-subclass lifetime follows the native HWND; messages only set nonblocking service flags.
use tauri::Manager;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, WM_DISPLAYCHANGE,
            WM_HOTKEY, WM_NCDESTROY, WM_POWERBROADCAST,
        },
    },
};
const SUBCLASS_ID: usize = 0x54505057;
pub fn install(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("main window missing")?;
    let hwnd = window
        .hwnd()
        .map_err(|_| "main native handle unavailable")?;
    let owned = Box::into_raw(Box::new(app.clone()));
    // Setup runs on the owner thread. The pointer stays live until WM_NCDESTROY removes the subclass.
    if unsafe {
        SetWindowSubclass(
            hwnd.0 as HWND,
            Some(power_message),
            SUBCLASS_ID,
            owned as usize,
        )
    } == 0
    {
        unsafe {
            drop(Box::from_raw(owned));
        }
        return Err("power subclass registration failed".into());
    }
    Ok(())
}
unsafe extern "system" fn power_message(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    if message == WM_HOTKEY {
        let app = unsafe { &*(data as *const tauri::AppHandle) };
        super::shortcuts::dispatch(app, wparam, lparam);
    }
    if message == WM_DISPLAYCHANGE {
        let app = unsafe { &*(data as *const tauri::AppHandle) };
        super::mini_window::schedule_placement(app);
    }
    if message == WM_POWERBROADCAST {
        // The only writer of dwRefData is install above; no message payload is dereferenced.
        let app = unsafe { &*(data as *const tauri::AppHandle) };
        if let Some(state) = app.try_state::<super::RuntimeState>() {
            if let Ok(collector) = &state.collector {
                match wparam as u32 {
                    PBT_APMSUSPEND => collector.suspend(),
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => collector.resume(),
                    _ => {}
                }
            }
        }
    }
    if message == WM_NCDESTROY {
        for id in super::shortcuts::SLOTS {
            unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey(hwnd, id);
            }
        }
        unsafe {
            RemoveWindowSubclass(hwnd, Some(power_message), SUBCLASS_ID);
            drop(Box::from_raw(data as *mut tauri::AppHandle));
        }
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
