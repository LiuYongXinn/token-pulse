//! Debug-only input preflight. Never unlock, switch desktops or release user-held keys.
use windows_sys::Win32::{
    Foundation::GetLastError,
    System::{StationsAndDesktops::*, Threading::GetCurrentThreadId},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::GetForegroundWindow},
};

fn desktop_name(handle: HDESK) -> Result<Vec<u16>, String> {
    let mut name = [0u16; 256];
    let mut needed = 0;
    if unsafe {
        GetUserObjectInformationW(
            handle,
            UOI_NAME,
            name.as_mut_ptr().cast(),
            std::mem::size_of_val(&name) as u32,
            &mut needed,
        )
    } == 0
    {
        return Err(format!("INPUT_DESKTOP_NAME_UNAVAILABLE:{}", unsafe {
            GetLastError()
        }));
    }
    let length = name
        .iter()
        .position(|c| *c == 0)
        .ok_or("invalid desktop name")?;
    Ok(name[..length].to_vec())
}

pub(super) fn require_interactive() -> Result<(), String> {
    let input = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
    if input.is_null() {
        return Err(format!(
            "INPUT_DESKTOP_UNAVAILABLE:{}; unlock an interactive Windows desktop before input acceptance",
            unsafe { GetLastError() }
        ));
    }
    let input_name = desktop_name(input);
    unsafe {
        CloseDesktop(input);
    }
    let input_name = input_name?;
    // The thread's borrowed handle must not be closed. Both handles refer to this process station.
    let thread_desktop = unsafe { GetThreadDesktop(GetCurrentThreadId()) };
    if thread_desktop.is_null() || desktop_name(thread_desktop)? != input_name {
        return Err("INPUT_DESKTOP_DIFFERS_FROM_APP; interactive input acceptance deferred".into());
    }
    if unsafe { GetForegroundWindow() }.is_null() {
        return Err("INPUT_FOREGROUND_UNAVAILABLE; interactive input acceptance deferred".into());
    }
    for key in [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN] {
        if unsafe { GetAsyncKeyState(i32::from(key)) } < 0 {
            return Err("INPUT_MODIFIER_ALREADY_HELD; release keys before input acceptance".into());
        }
    }
    Ok(())
}
