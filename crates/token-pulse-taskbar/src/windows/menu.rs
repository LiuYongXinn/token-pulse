//! Standard Windows menu; returned IDs map only to the fixed host action whitelist.
use super::topology::wide;
use crate::{HostAction, WireError};
use std::mem;
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GA_ROOT, GetAncestor, GetWindowRect, HMENU,
        MF_CHECKED, MF_SEPARATOR, MF_STRING, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
        TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPMPARAMS, TrackPopupMenuEx,
    },
};
const FLOAT: u32 = 101;
const STATS: u32 = 102;
const SETTINGS: u32 = 103;
const PRIVACY: u32 = 104;
const DISABLE: u32 = 105;
struct Menu(HMENU);
impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            DestroyMenu(self.0);
        }
    }
}
fn create(privacy: bool) -> Result<Menu, WireError> {
    let menu = Menu(unsafe { CreatePopupMenu() });
    if menu.0.is_null() {
        return Err(WireError::InvalidState);
    }
    for (id, title) in [
        (FLOAT, "显示悬浮窗(&F)"),
        (STATS, "打开当前范围统计(&S)"),
        (SETTINGS, "任务栏设置(&T)"),
        (0, ""),
        (PRIVACY, "隐私模式(&P)"),
        (DISABLE, "隐藏任务栏显示(&H)"),
    ] {
        let flags = if id == 0 {
            MF_SEPARATOR
        } else {
            MF_STRING
                | if id == PRIVACY && privacy {
                    MF_CHECKED
                } else {
                    0
                }
        };
        if unsafe { AppendMenuW(menu.0, flags, id as usize, wide(title).as_ptr()) } == 0 {
            return Err(WireError::InvalidState);
        }
    }
    Ok(menu)
}
fn action(id: u32, privacy: bool) -> Option<HostAction> {
    match id {
        FLOAT => Some(HostAction::OpenFloat {}),
        STATS => Some(HostAction::OpenStats {}),
        SETTINGS => Some(HostAction::OpenTaskbarSettings {}),
        PRIVACY => Some(HostAction::SetPrivacy { enabled: !privacy }),
        DISABLE => Some(HostAction::DisableTaskbar {}),
        _ => None,
    }
}
pub(super) fn show(
    window: HWND,
    point: Option<POINT>,
    privacy: bool,
) -> Result<Option<HostAction>, WireError> {
    let _dpi = super::topology::DpiGuard::enter().map_err(|_| WireError::InvalidState)?;
    let menu = create(privacy)?;
    let mut rect: RECT = unsafe { mem::zeroed() };
    if unsafe { GetWindowRect(window, &mut rect) } == 0 {
        return Err(WireError::InvalidState);
    }
    let point = point.unwrap_or(POINT {
        x: rect.left,
        y: rect.top,
    });
    let root = unsafe { GetAncestor(window, GA_ROOT) };
    if root.is_null() {
        return Err(WireError::InvalidState);
    }
    // This is an explicit menu request. Standard child-window menus use their top-level parent.
    unsafe {
        SetForegroundWindow(root);
    }
    let params = TPMPARAMS {
        cbSize: mem::size_of::<TPMPARAMS>() as u32,
        rcExclude: rect,
    };
    let selected = unsafe {
        TrackPopupMenuEx(
            menu.0,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
            point.x,
            point.y,
            window,
            &params,
        )
    };
    Ok(action(selected as u32, privacy))
}
#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMenuItemCount, GetMenuState, GetMenuStringW, MF_BYCOMMAND, MF_BYPOSITION,
    };
    #[test]
    fn native_menu_has_named_accessible_items_checked_privacy_and_only_known_actions() {
        let menu = create(true).unwrap();
        assert_eq!(unsafe { GetMenuItemCount(menu.0) }, 6);
        let mut title = [0; 128];
        let n = unsafe { GetMenuStringW(menu.0, 2, title.as_mut_ptr(), 128, MF_BYPOSITION) };
        assert_eq!(
            String::from_utf16_lossy(&title[..n as usize]),
            "任务栏设置(&T)"
        );
        assert_ne!(
            unsafe { GetMenuState(menu.0, PRIVACY, MF_BYCOMMAND) } & MF_CHECKED,
            0
        );
        let unchecked = create(false).unwrap();
        assert_eq!(
            unsafe { GetMenuState(unchecked.0, PRIVACY, MF_BYCOMMAND) } & MF_CHECKED,
            0
        );
        assert_eq!(
            action(PRIVACY, true),
            Some(HostAction::SetPrivacy { enabled: false })
        );
        assert_eq!(
            action(PRIVACY, false),
            Some(HostAction::SetPrivacy { enabled: true })
        );
        assert_eq!(
            action(SETTINGS, false),
            Some(HostAction::OpenTaskbarSettings {})
        );
        assert_eq!(action(DISABLE, false), Some(HostAction::DisableTaskbar {}));
        assert_eq!(action(FLOAT, false), Some(HostAction::OpenFloat {}));
        assert_eq!(action(STATS, true), Some(HostAction::OpenStats {}));
        assert!(action(0, false).is_none() && action(u32::MAX, false).is_none());
    }
}
