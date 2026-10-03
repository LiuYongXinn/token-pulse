//! Window-subclass lifetime follows the native HWND; messages only set nonblocking service flags.
use tauri::Manager;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, SPI_SETWORKAREA,
            WM_DISPLAYCHANGE, WM_HOTKEY, WM_NCDESTROY, WM_POWERBROADCAST, WM_SETTINGCHANGE,
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
    if message == WM_DISPLAYCHANGE
        || (message == WM_SETTINGCHANGE && wparam as u32 == SPI_SETWORKAREA)
    {
        let app = unsafe { &*(data as *const tauri::AppHandle) };
        super::mini_window::schedule_placement(app);
        super::main_window::schedule(app, true);
    }
    if message == WM_POWERBROADCAST {
        // The only writer of dwRefData is install above; no message payload is dereferenced.
        let app = unsafe { &*(data as *const tauri::AppHandle) };
        #[cfg(debug_assertions)]
        super::power_resume_smoke::record_event(app, wparam as u32);
        if let Some(state) = app.try_state::<super::RuntimeState>() {
            if let Some(taskbar) = super::taskbar_commands::service(app) {
                match wparam as u32 {
                    PBT_APMSUSPEND => taskbar.suspend(),
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => taskbar.resume(),
                    _ => {}
                }
            }
            if let Ok(quota) = &state.quota {
                match wparam as u32 {
                    PBT_APMSUSPEND => quota.suspend(),
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => quota.resume(),
                    _ => {}
                }
            }
            if let Ok(collector) = &state.collector {
                match wparam as u32 {
                    PBT_APMSUSPEND => collector.suspend(),
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => collector.resume(),
                    _ => {}
                }
            }
            if matches!(wparam as u32, PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND) {
                if let Ok(service) = &state.revaluations {
                    service.wake();
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
