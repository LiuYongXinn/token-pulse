//! Silver Mist items in a native Windows menu, retaining navigation and accessibility.
use super::{
    render::{NativeFont, rgb},
    topology::wide,
};
use crate::{HostAction, WireError};
use std::{
    cell::{Cell, RefCell},
    mem,
    rc::Rc,
};
use token_pulse_core::settings::AppTheme;
use windows_sys::Win32::{
    Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{
        CreatePen, CreateSolidBrush, DeleteObject, FillRect, GetStockObject, HDC, HGDIOBJ,
        IntersectClipRect, NULL_PEN, PS_SOLID, Polyline, RestoreDC, RoundRect, SaveDC,
        SelectObject,
    },
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW, MSAA_MENU_SIG, MSAAMENUINFO},
        Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_SELECTED, ODT_MENU},
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyMenu, GA_ROOT, GetAncestor, GetWindowRect, HMENU,
            MENUINFO, MENUITEMINFOW, MF_CHECKED, MF_SEPARATOR, MF_STRING, MFT_OWNERDRAW,
            MFT_SEPARATOR, MIIM_DATA, MIIM_FTYPE, MIM_BACKGROUND, MIM_STYLE, MNC_EXECUTE,
            MNS_NOCHECK, SPI_GETHIGHCONTRAST, SetForegroundWindow, SetMenuInfo, SetMenuItemInfoW,
            SystemParametersInfoW, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
            TPM_RIGHTBUTTON, TPMPARAMS, TrackPopupMenuEx, WM_DRAWITEM, WM_MEASUREITEM, WM_MENUCHAR,
        },
    },
};
const FLOAT: u32 = 101;
const STATS: u32 = 102;
const SETTINGS: u32 = 103;
const PRIVACY: u32 = 104;
const DISABLE: u32 = 105;
const ITEMS: [(u32, &str, &str, char); 6] = [
    (FLOAT, "显示悬浮窗(&F)", "显示悬浮窗", 'F'),
    (STATS, "打开当前范围统计(&S)", "打开当前范围统计", 'S'),
    (SETTINGS, "任务栏设置(&T)", "任务栏设置", 'T'),
    (0, "", "", '\0'),
    (PRIVACY, "隐私模式(&P)", "隐私模式", 'P'),
    (DISABLE, "隐藏任务栏显示(&H)", "隐藏任务栏显示", 'H'),
];
struct Object(HGDIOBJ);
impl Drop for Object {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}
#[derive(Clone, Copy)]
struct Colors {
    background: COLORREF,
    foreground: COLORREF,
    muted: COLORREF,
    hover: COLORREF,
    line: COLORREF,
    key: COLORREF,
}
impl Colors {
    fn silver(dark: bool) -> Self {
        if dark {
            Self {
                background: rgb(40, 40, 40),
                foreground: rgb(238, 238, 238),
                muted: rgb(170, 170, 170),
                hover: rgb(58, 58, 58),
                line: rgb(65, 65, 65),
                key: rgb(48, 48, 48),
            }
        } else {
            Self {
                background: rgb(255, 255, 255),
                foreground: rgb(41, 41, 41),
                muted: rgb(104, 104, 104),
                hover: rgb(240, 240, 240),
                line: rgb(222, 222, 222),
                key: rgb(248, 248, 248),
            }
        }
    }
}
struct Style {
    font: NativeFont,
    dpi: u32,
    colors: Colors,
    width: i32,
    row_height: i32,
    key_width: i32,
    status_width: i32,
}
impl Style {
    fn new(theme: AppTheme, dpi: u32) -> Result<Self, WireError> {
        let font = NativeFont::for_details(dpi)?;
        // Share the detail popup's application/system theme resolution.
        let background = super::details_window::palette(theme)?.background;
        let dark = (background & 255) * 299
            + ((background >> 8) & 255) * 587
            + ((background >> 16) & 255) * 114
            < 128_000;
        let px = |dip| dip * dpi as i32 / 96;
        let key_width = ['F', 'S', 'T', 'P', 'H']
            .into_iter()
            .map(|c| font.width(&c.to_string()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .unwrap_or(0)
            + px(12);
        let status_width = font.width("已开启")?.max(font.width("已关闭")?) + px(12);
        let mut width = px(264);
        for (id, _, label, _) in ITEMS {
            width = width.max(
                px(48)
                    + font.width(label)?
                    + px(24)
                    + key_width
                    + px(16)
                    + if id == PRIVACY {
                        status_width + px(8)
                    } else {
                        0
                    },
            );
        }
        let row_height = px(38).max(font.height() + px(18));
        Ok(Self {
            font,
            dpi,
            colors: Colors::silver(dark),
            width,
            row_height,
            key_width,
            status_width,
        })
    }
    fn px(&self, dip: i32) -> i32 {
        dip * self.dpi as i32 / 96
    }
    fn height(&self, index: usize) -> i32 {
        if ITEMS[index].0 == 0 {
            self.px(13)
        } else {
            self.row_height
                + if index == 0 || index == ITEMS.len() - 1 {
                    self.px(6)
                } else {
                    0
                }
        }
    }
    unsafe fn draw(
        &self,
        dc: HDC,
        rect: RECT,
        index: usize,
        selected: bool,
        privacy: bool,
    ) -> Result<(), WireError> {
        let saved = unsafe { SaveDC(dc) };
        if saved == 0 {
            return Err(WireError::InvalidState);
        }
        let result = (|| {
            if unsafe { IntersectClipRect(dc, rect.left, rect.top, rect.right, rect.bottom) } == 0 {
                return Err(WireError::InvalidState);
            }
            unsafe { fill(dc, rect, self.colors.background) }?;
            let (id, _, label, key) = ITEMS[index];
            if id == 0 {
                let top = (rect.top + rect.bottom) / 2;
                return unsafe {
                    fill(
                        dc,
                        RECT {
                            left: rect.left + self.px(16),
                            right: rect.right - self.px(16),
                            top,
                            bottom: top + self.px(1).max(1),
                        },
                        self.colors.line,
                    )
                };
            }
            let top = rect.top + if index == 0 { self.px(6) } else { 0 };
            let bottom = rect.bottom
                - if index == ITEMS.len() - 1 {
                    self.px(6)
                } else {
                    0
                };
            if selected {
                unsafe {
                    rounded(
                        dc,
                        RECT {
                            left: rect.left + self.px(6),
                            top: top + self.px(2),
                            right: rect.right - self.px(6),
                            bottom: bottom - self.px(2),
                        },
                        self.px(7),
                        self.colors.hover,
                    )
                }?;
            }
            let middle = (top + bottom) / 2;
            let y = middle - self.font.height() / 2;
            unsafe {
                self.icon(
                    dc,
                    id,
                    rect.left + self.px(17),
                    middle - self.px(8),
                    privacy,
                )
            }?;
            unsafe {
                self.font.text(
                    dc,
                    rect.left + self.px(44),
                    y,
                    label,
                    self.colors.foreground,
                )
            }?;
            let key_right = rect.right - self.px(16);
            let key_left = key_right - self.key_width;
            let key_height = self.font.height() + self.px(4);
            unsafe {
                rounded(
                    dc,
                    RECT {
                        left: key_left,
                        right: key_right,
                        top: middle - key_height / 2,
                        bottom: middle + (key_height + 1) / 2,
                    },
                    self.px(4),
                    self.colors.key,
                )
            }?;
            let key = key.to_string();
            unsafe {
                self.font.text(
                    dc,
                    key_left + (self.key_width - self.font.width(&key)?) / 2,
                    y,
                    &key,
                    self.colors.muted,
                )
            }?;
            if id == PRIVACY {
                let status = if privacy { "已开启" } else { "已关闭" };
                let left = key_left - self.px(8) - self.status_width;
                if privacy {
                    unsafe {
                        rounded(
                            dc,
                            RECT {
                                left,
                                right: key_left - self.px(8),
                                top: middle - key_height / 2,
                                bottom: middle + (key_height + 1) / 2,
                            },
                            self.px(4),
                            self.colors.hover,
                        )
                    }?;
                }
                unsafe {
                    self.font.text(
                        dc,
                        left + (self.status_width - self.font.width(status)?) / 2,
                        y,
                        status,
                        if privacy {
                            self.colors.foreground
                        } else {
                            self.colors.muted
                        },
                    )
                }?;
            }
            Ok(())
        })();
        unsafe {
            RestoreDC(dc, saved);
        }
        result
    }
    unsafe fn icon(
        &self,
        dc: HDC,
        id: u32,
        x: i32,
        y: i32,
        privacy: bool,
    ) -> Result<(), WireError> {
        let pen = Object(unsafe { CreatePen(PS_SOLID, self.px(1).max(1), self.colors.muted) });
        if pen.0.is_null() {
            return Err(WireError::InvalidState);
        }
        let previous = unsafe { SelectObject(dc, pen.0) };
        let line = |points: &[(i32, i32)]| -> Result<(), WireError> {
            let points: Vec<_> = points
                .iter()
                .map(|&(a, b)| POINT {
                    x: x + self.px(a),
                    y: y + self.px(b),
                })
                .collect();
            if unsafe { Polyline(dc, points.as_ptr(), points.len() as i32) } == 0 {
                Err(WireError::InvalidState)
            } else {
                Ok(())
            }
        };
        let result = (|| {
            match id {
                FLOAT => {
                    line(&[(2, 10), (0, 10), (0, 0), (12, 0), (12, 3)])?;
                    line(&[(4, 5), (16, 5), (16, 15), (4, 15), (4, 5)])?;
                }
                STATS => {
                    line(&[(0, 0), (0, 15), (16, 15)])?;
                    line(&[(4, 12), (4, 7)])?;
                    line(&[(9, 12), (9, 3)])?;
                    line(&[(14, 12), (14, 0)])?;
                }
                SETTINGS => {
                    for (a, b) in [(3, 4), (8, 11), (13, 6)] {
                        line(&[(a, 0), (a, b - 2)])?;
                        line(&[(a, b + 2), (a, 16)])?;
                        line(&[
                            (a - 2, b - 2),
                            (a + 2, b - 2),
                            (a + 2, b + 2),
                            (a - 2, b + 2),
                            (a - 2, b - 2),
                        ])?;
                    }
                }
                PRIVACY => {
                    line(&[
                        (8, 0),
                        (15, 3),
                        (14, 10),
                        (12, 13),
                        (8, 16),
                        (4, 13),
                        (2, 10),
                        (1, 3),
                        (8, 0),
                    ])?;
                    if privacy {
                        line(&[(4, 8), (7, 11), (12, 5)])?;
                    }
                }
                DISABLE => {
                    line(&[
                        (0, 8),
                        (4, 4),
                        (8, 3),
                        (12, 4),
                        (16, 8),
                        (12, 12),
                        (8, 13),
                        (4, 12),
                        (0, 8),
                    ])?;
                    line(&[(1, 0), (15, 16)])?;
                }
                _ => {}
            }
            Ok(())
        })();
        unsafe {
            SelectObject(dc, previous);
        }
        result
    }
}
unsafe fn fill(dc: HDC, rect: RECT, color: COLORREF) -> Result<(), WireError> {
    let brush = Object(unsafe { CreateSolidBrush(color) });
    if brush.0.is_null() || unsafe { FillRect(dc, &rect, brush.0) } == 0 {
        return Err(WireError::InvalidState);
    }
    Ok(())
}
unsafe fn rounded(dc: HDC, rect: RECT, radius: i32, color: COLORREF) -> Result<(), WireError> {
    let brush = Object(unsafe { CreateSolidBrush(color) });
    if brush.0.is_null() {
        return Err(WireError::InvalidState);
    }
    let old_brush = unsafe { SelectObject(dc, brush.0) };
    let old_pen = unsafe { SelectObject(dc, GetStockObject(NULL_PEN)) };
    let result = unsafe {
        RoundRect(
            dc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius * 2,
            radius * 2,
        )
    };
    unsafe {
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
    }
    if result == 0 {
        Err(WireError::InvalidState)
    } else {
        Ok(())
    }
}
// MSAA must be first: Windows' standard menu proxy reads this metadata.
#[repr(C)]
struct Entry {
    msaa: MSAAMENUINFO,
    title: Vec<u16>,
}
struct Menu {
    handle: HMENU,
    entries: Box<[Entry]>,
    style: Option<Style>,
    _background: Option<Object>,
    privacy: bool,
    failed: Cell<bool>,
}
impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            DestroyMenu(self.handle);
        }
    }
}
fn create(privacy: bool, theme: AppTheme, dpi: u32) -> Result<Menu, WireError> {
    let mut contrast: HIGHCONTRASTW = unsafe { mem::zeroed() };
    contrast.cbSize = mem::size_of_val(&contrast) as u32;
    if unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            (&mut contrast as *mut HIGHCONTRASTW).cast(),
            0,
        )
    } == 0
    {
        return Err(WireError::InvalidState);
    }
    let style = if contrast.dwFlags & HCF_HIGHCONTRASTON != 0 {
        None
    } else {
        Some(Style::new(theme, dpi)?)
    };
    let background = if let Some(style) = &style {
        let brush = Object(unsafe { CreateSolidBrush(style.colors.background) });
        if brush.0.is_null() {
            return Err(WireError::InvalidState);
        }
        Some(brush)
    } else {
        None
    };
    let mut menu = Menu {
        handle: unsafe { CreatePopupMenu() },
        entries: ITEMS
            .iter()
            .map(|(_, title, _, _)| Entry {
                msaa: unsafe { mem::zeroed() },
                title: wide(title),
            })
            .collect(),
        style,
        _background: background,
        privacy,
        failed: Cell::new(false),
    };
    if menu.handle.is_null() {
        return Err(WireError::InvalidState);
    }
    if let Some(brush) = &menu._background {
        let info = MENUINFO {
            cbSize: mem::size_of::<MENUINFO>() as u32,
            fMask: MIM_BACKGROUND | MIM_STYLE,
            dwStyle: MNS_NOCHECK,
            hbrBack: brush.0,
            ..unsafe { mem::zeroed() }
        };
        if unsafe { SetMenuInfo(menu.handle, &info) } == 0 {
            return Err(WireError::InvalidState);
        }
    }
    for (index, (id, _, _, _)) in ITEMS.iter().enumerate() {
        let entry = &mut menu.entries[index];
        entry.msaa = MSAAMENUINFO {
            dwMSAASignature: MSAA_MENU_SIG as u32,
            cchWText: (entry.title.len() - 1) as u32,
            pszWText: entry.title.as_mut_ptr(),
        };
        let flags = if *id == 0 {
            MF_SEPARATOR
        } else {
            MF_STRING
                | if *id == PRIVACY && privacy {
                    MF_CHECKED
                } else {
                    0
                }
        };
        if unsafe { AppendMenuW(menu.handle, flags, *id as usize, entry.title.as_ptr()) } == 0 {
            return Err(WireError::InvalidState);
        }
        if menu.style.is_some() {
            let info = MENUITEMINFOW {
                cbSize: mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE | MIIM_DATA,
                fType: MFT_OWNERDRAW | if *id == 0 { MFT_SEPARATOR } else { 0 },
                dwItemData: entry as *const Entry as usize,
                ..unsafe { mem::zeroed() }
            };
            if unsafe { SetMenuItemInfoW(menu.handle, index as u32, 1, &info) } == 0 {
                return Err(WireError::InvalidState);
            }
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
thread_local! { static ACTIVE: RefCell<Option<(HWND, Rc<Menu>)>> = const { RefCell::new(None) }; }
struct Session;
impl Session {
    fn enter(owner: HWND, menu: Rc<Menu>) -> Result<Self, WireError> {
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            if active.is_some() {
                return Err(WireError::InvalidState);
            }
            *active = Some((owner, menu));
            Ok(Self)
        })
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            active.borrow_mut().take();
        });
    }
}
fn mnemonic(key: u16) -> Option<usize> {
    let key = char::from_u32(u32::from(key))?.to_ascii_uppercase();
    ITEMS.iter().position(|(id, _, _, c)| *id != 0 && *c == key)
}
pub(super) fn handle_message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    if ![WM_MEASUREITEM, WM_DRAWITEM, WM_MENUCHAR].contains(&message) {
        return None;
    }
    let menu = ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .filter(|(owner, _)| *owner == window)
            .map(|(_, menu)| Rc::clone(menu))
    })?;
    let style = menu.style.as_ref()?;
    if lparam == 0 {
        return None;
    }
    match message {
        WM_MENUCHAR if lparam as HMENU == menu.handle => Some(
            mnemonic(wparam as u16)
                .map_or(0, |index| index as isize | ((MNC_EXECUTE as isize) << 16)),
        ),
        WM_MEASUREITEM => {
            let item = unsafe { &mut *(lparam as *mut MEASUREITEMSTRUCT) };
            if item.CtlType != ODT_MENU {
                return None;
            }
            let index = menu
                .entries
                .iter()
                .position(|entry| entry as *const Entry as usize == item.itemData)?;
            item.itemWidth = style.width as u32;
            item.itemHeight = style.height(index) as u32;
            Some(1)
        }
        WM_DRAWITEM => {
            let item = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
            if item.CtlType != ODT_MENU || item.hwndItem != menu.handle {
                return None;
            }
            let index = menu
                .entries
                .iter()
                .position(|entry| entry as *const Entry as usize == item.itemData)?;
            if unsafe {
                style.draw(
                    item.hDC,
                    item.rcItem,
                    index,
                    item.itemState & ODS_SELECTED != 0,
                    menu.privacy,
                )
            }
            .is_err()
            {
                menu.failed.set(true);
            }
            Some(1)
        }
        _ => None,
    }
}
pub(super) fn show(
    window: HWND,
    point: Option<POINT>,
    privacy: bool,
    theme: AppTheme,
    dpi: u32,
) -> Result<Option<HostAction>, WireError> {
    let _dpi = super::topology::DpiGuard::enter().map_err(|_| WireError::InvalidState)?;
    let menu = Rc::new(create(privacy, theme, dpi)?);
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
    // Strong ownership survives configuration changes/shutdown in the native modal loop.
    let _session = Session::enter(window, Rc::clone(&menu))?;
    unsafe {
        SetForegroundWindow(root);
    }
    let params = TPMPARAMS {
        cbSize: mem::size_of::<TPMPARAMS>() as u32,
        rcExclude: rect,
    };
    let selected = unsafe {
        TrackPopupMenuEx(
            menu.handle,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
            point.x,
            point.y,
            window,
            &params,
        )
    };
    if menu.failed.get() {
        return Err(WireError::InvalidState);
    }
    Ok(action(selected as u32, privacy))
}
#[cfg(test)]
mod tests;
