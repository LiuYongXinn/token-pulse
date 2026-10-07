//! Owned child drawing window; it remains hidden until the taskbar adapter attaches it.
use super::{
    details_window::{HOVER_MS, NativeDetails},
    render::{NativeFont, Palette, rgb},
    topology::wide,
};
use crate::{
    HostAction, TaskbarView, WireError,
    click::ClickQueue,
    display::{DisplayPreferences, MeasuredPlan, accessible_text},
};
use std::{
    cell::{Cell, UnsafeCell},
    mem, ptr,
    rc::Rc,
};
use token_pulse_core::settings::AppTheme;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetDoubleClickTime, GetKeyState, TME_CANCEL, TME_HOVER, TME_LEAVE, TRACKMOUSEEVENT,
    TrackMouseEvent, VK_APPS, VK_DOWN, VK_END, VK_ESCAPE, VK_F10, VK_HOME, VK_NEXT, VK_PRIOR,
    VK_RETURN, VK_SHIFT, VK_SPACE, VK_UP,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, ClientToScreen, DrawFocusRect, EndPaint, InvalidateRect, PAINTSTRUCT,
        UpdateWindow,
    },
    System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetTickCount64},
    UI::{
        Controls::{WM_MOUSEHOVER, WM_MOUSELEAVE},
        WindowsAndMessaging::{
            CREATESTRUCTW, CS_DBLCLKS, CreateWindowExW, DLGC_WANTMESSAGE, DefWindowProcW,
            DestroyWindow, EndMenu, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW,
            IsWindowVisible, KillTimer, MA_NOACTIVATE, PostMessageW, RegisterClassExW,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetTimer, SetWindowLongPtrW,
            SetWindowPos, SetWindowTextW, UnregisterClassW, WM_CANCELMODE, WM_CONTEXTMENU,
            WM_ERASEBKGND, WM_GETDLGCODE, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDBLCLK,
            WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_MOUSEWHEEL,
            WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_PRINTCLIENT, WM_RBUTTONDOWN, WM_RBUTTONUP,
            WM_SETFOCUS, WM_TIMER, WNDCLASSEXW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE,
            WS_EX_NOPARENTNOTIFY, WS_TABSTOP,
        },
    },
};
struct State {
    font: NativeFont,
    plan: Option<MeasuredPlan>,
    palette: Palette,
    paint_failed: bool,
    interactive: bool,
    clicks: ClickQueue,
    privacy: bool,
    theme: AppTheme,
    dpi: u32,
    interaction_epoch: u64,
    menu_open: bool,
}
struct CanvasState {
    value: UnsafeCell<State>,
    painting: Cell<bool>,
    alive: Cell<bool>,
    details: NativeDetails,
    tracking: Cell<bool>,
    dismissed: Cell<bool>,
    focused: Cell<bool>,
}
impl CanvasState {
    fn get(&self) -> *mut State {
        self.value.get()
    }
}
struct PaintGuard<'a>(&'a Cell<bool>);
impl Drop for PaintGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
const CLICK_TIMER: usize = 1;
fn dialog_code(ready: bool, details_visible: bool, key: usize, shift: bool) -> LRESULT {
    if !ready {
        return 0;
    }
    // Claim only keys we process. In particular, Tab remains native focus navigation.
    // wParam is the queried virtual key; a generic query (0) claims no message.
    let command = [VK_RETURN, VK_SPACE, VK_APPS]
        .into_iter()
        .any(|k| key == k as usize)
        || (key == VK_F10 as usize && shift);
    let detail = details_visible
        && [
            VK_ESCAPE, VK_UP, VK_DOWN, VK_PRIOR, VK_NEXT, VK_HOME, VK_END,
        ]
        .into_iter()
        .any(|k| key == k as usize);
    if command || detail {
        DLGC_WANTMESSAGE as _
    } else {
        0
    }
}
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE && unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } == 0 {
        let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
    }
    let raw = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const CanvasState;
    if !raw.is_null() {
        // A modal menu can process shutdown and drop NativeCanvas before returning.
        unsafe {
            Rc::increment_strong_count(raw);
        }
        let slot = unsafe { Rc::from_raw(raw) };
        if let Some(result) = super::menu::handle_message(window, message, wparam, lparam) {
            return result;
        }
        if message == WM_NCDESTROY {
            slot.alive.set(false);
            slot.details.clear().ok();
            if !slot.painting.get() && unsafe { (*slot.get()).menu_open } {
                unsafe {
                    EndMenu();
                }
            }
            unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            }
        }
        if slot.painting.get() {
            // Do not borrow mutable render state during nested user32 dispatch.
            if message == WM_CANCELMODE || message == WM_CONTEXTMENU {
                unsafe {
                    PostMessageW(window, message, wparam, lparam);
                }
                return 0;
            }
            return if message == WM_ERASEBKGND {
                1
            } else {
                unsafe { DefWindowProcW(window, message, wparam, lparam) }
            };
        }
        if message == WM_CONTEXTMENU {
            context_menu(window, &slot, lparam);
            return 0;
        }
        if message == WM_CANCELMODE {
            clear_intentions(window, &slot);
            return 0;
        }
        let ready = unsafe {
            let state = &*slot.get();
            state.interactive && state.plan.is_some() && !state.menu_open
        } && slot.alive.get();
        match message {
            WM_GETDLGCODE => {
                return dialog_code(
                    ready,
                    slot.details.visible(),
                    wparam,
                    unsafe { GetKeyState(VK_SHIFT as i32) } < 0,
                );
            }
            WM_MOUSEMOVE if ready => {
                if !slot.tracking.get() {
                    let mut tracking = TRACKMOUSEEVENT {
                        cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | if slot.dismissed.get() { 0 } else { TME_HOVER },
                        hwndTrack: window,
                        dwHoverTime: HOVER_MS,
                    };
                    slot.tracking
                        .set(unsafe { TrackMouseEvent(&mut tracking) } != 0);
                }
                return 0;
            }
            WM_MOUSEHOVER if ready && slot.tracking.get() && !slot.dismissed.get() => {
                if slot.details.show(slot.focused.get()).is_err() {
                    unsafe {
                        (*slot.get()).paint_failed = true;
                    }
                }
                return 0;
            }
            WM_MOUSELEAVE => {
                slot.tracking.set(false);
                slot.dismissed.set(false);
                if !slot.focused.get() {
                    slot.details.leave();
                }
                return 0;
            }
            WM_SETFOCUS => {
                slot.focused.set(true);
                slot.dismissed.set(false);
                if ready && slot.details.show(true).is_err() {
                    unsafe {
                        (*slot.get()).paint_failed = true;
                    }
                }
                unsafe {
                    InvalidateRect(window, ptr::null(), 0);
                }
                return 0;
            }
            WM_KILLFOCUS => {
                slot.focused.set(false);
                slot.details.hide();
                unsafe {
                    InvalidateRect(window, ptr::null(), 0);
                }
                return 0;
            }
            WM_MOUSEWHEEL if ready => {
                slot.details.wheel(wparam, lparam);
                return 0;
            }
            WM_KEYDOWN | WM_KEYUP if wparam == VK_ESCAPE as usize => {
                if message == WM_KEYDOWN {
                    dismiss_details(window, &slot);
                }
                return 0;
            }
            WM_KEYDOWN if ready && slot.details.scroll_key(wparam as u16) => return 0,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                dismiss_details(window, &slot);
                return 0;
            }
            WM_KEYDOWN
                if ready && (wparam == VK_RETURN as usize || wparam == VK_SPACE as usize) =>
            {
                dismiss_details(window, &slot);
            }
            WM_KEYDOWN
                if ready
                    && lparam & (1 << 30) == 0
                    && (wparam == VK_APPS as usize
                        || (wparam == VK_F10 as usize
                            && unsafe { GetKeyState(VK_SHIFT as i32) } < 0)) =>
            {
                dismiss_details(window, &slot);
            }
            _ => {}
        }
        let state = unsafe { &mut *slot.get() };
        match message {
            WM_KEYDOWN | WM_KEYUP if state.interactive && state.plan.is_some() => {
                if wparam == VK_APPS as usize
                    || (wparam == VK_F10 as usize && unsafe { GetKeyState(VK_SHIFT as i32) } < 0)
                {
                    if message == WM_KEYDOWN && lparam & (1 << 30) == 0 {
                        unsafe {
                            PostMessageW(window, WM_CONTEXTMENU, window as usize, -1);
                        }
                    }
                    return 0;
                }
                if wparam == VK_RETURN as usize || wparam == VK_SPACE as usize {
                    if message == WM_KEYDOWN && lparam & (1 << 30) == 0 {
                        state.clicks.clear();
                        state
                            .clicks
                            .push(unsafe { GetTickCount64() }, HostAction::OpenFloat {});
                        unsafe {
                            KillTimer(window, CLICK_TIMER);
                        }
                    }
                    return 0;
                }
            }
            WM_MOUSEACTIVATE => return MA_NOACTIVATE as _,
            // Consume our own press; DefWindowProc would forward child notification to Explorer.
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => return 0,
            WM_RBUTTONUP if state.interactive && state.plan.is_some() => {
                let mut point = POINT {
                    x: lparam as u16 as i16 as i32,
                    y: (lparam >> 16) as u16 as i16 as i32,
                };
                if unsafe { ClientToScreen(window, &mut point) } != 0 {
                    let packed = ((point.y as u16 as u32) << 16) | point.x as u16 as u32;
                    unsafe {
                        PostMessageW(window, WM_CONTEXTMENU, window as usize, packed as LPARAM);
                    }
                }
                return 0;
            }
            WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_TIMER
                if state.interactive
                    && state.plan.is_some()
                    && unsafe { IsWindowVisible(window) } != 0 =>
            {
                let now = unsafe { GetTickCount64() };
                if message == WM_LBUTTONDBLCLK {
                    unsafe {
                        KillTimer(window, CLICK_TIMER);
                    }
                    state.clicks.double_click(now);
                } else if message == WM_LBUTTONUP {
                    let delay = unsafe { GetDoubleClickTime() }.max(1);
                    if state.clicks.release(now, delay)
                        && unsafe { SetTimer(window, CLICK_TIMER, delay, None) } == 0
                    {
                        state.clicks.clear();
                    }
                } else if wparam == CLICK_TIMER {
                    state.clicks.tick(now);
                    unsafe {
                        KillTimer(window, CLICK_TIMER);
                    }
                }
                return 0;
            }
            WM_PAINT | WM_PRINTCLIENT => {
                slot.painting.set(true);
                let _painting = PaintGuard(&slot.painting);
                let mut paint: PAINTSTRUCT = unsafe { mem::zeroed() };
                let dc = if message == WM_PAINT {
                    unsafe { BeginPaint(window, &mut paint) }
                } else {
                    wparam as _
                };
                let mut rect: RECT = unsafe { mem::zeroed() };
                let result = if dc.is_null() || unsafe { GetClientRect(window, &mut rect) } == 0 {
                    Err(WireError::InvalidState)
                } else if message == WM_PAINT && rect.right > 0 && rect.bottom > 0 {
                    state.font.present(
                        window,
                        state.plan.as_ref(),
                        rect.right,
                        rect.bottom,
                        state.palette,
                        slot.focused.get(),
                    )
                } else {
                    unsafe {
                        state.font.paint(
                            dc,
                            state.plan.as_ref(),
                            rect.right,
                            rect.bottom,
                            state.palette,
                        )
                    }
                };
                state.paint_failed |= result.is_err();
                if message == WM_PRINTCLIENT && result.is_ok() && slot.focused.get() {
                    let focus = RECT {
                        left: 1,
                        top: 1,
                        right: rect.right - 1,
                        bottom: rect.bottom - 1,
                    };
                    unsafe {
                        DrawFocusRect(dc, &focus);
                    }
                }
                if message == WM_PAINT {
                    unsafe {
                        EndPaint(window, &paint);
                    }
                }
                return 0;
            }
            WM_ERASEBKGND => return 1,
            WM_NCDESTROY => {
                state.clicks.clear();
                state.interaction_epoch = state.interaction_epoch.saturating_add(1);
                state.menu_open = false;
                state.interactive = false;
                state.plan = None;
                unsafe {
                    SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                }
            }
            _ => {}
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
fn clear_intentions(window: HWND, slot: &CanvasState) {
    dismiss_details(window, slot);
    // A new configuration requires a fresh mouse move. Ignore queued hover messages.
    slot.dismissed.set(false);
    slot.tracking.set(false);
    let mut tracking = TRACKMOUSEEVENT {
        cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_CANCEL | TME_HOVER | TME_LEAVE,
        hwndTrack: window,
        dwHoverTime: HOVER_MS,
    };
    unsafe {
        TrackMouseEvent(&mut tracking);
    }
    if slot.details.clear().is_err() {
        unsafe {
            (*slot.get()).paint_failed = true;
        }
    }
    let menu_open = unsafe {
        let state = &mut *slot.get();
        state.clicks.clear();
        state.interaction_epoch = state.interaction_epoch.saturating_add(1);
        state.menu_open
    };
    unsafe {
        KillTimer(window, CLICK_TIMER);
    }
    if menu_open {
        unsafe {
            EndMenu();
        }
    }
}
fn dismiss_details(window: HWND, slot: &CanvasState) {
    slot.details.hide();
    slot.dismissed.set(true);
    slot.tracking.set(false);
    let mut tracking = TRACKMOUSEEVENT {
        cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_CANCEL | TME_HOVER | TME_LEAVE,
        hwndTrack: window,
        dwHoverTime: HOVER_MS,
    };
    unsafe {
        TrackMouseEvent(&mut tracking);
    }
    tracking.dwFlags = TME_LEAVE;
    slot.tracking
        .set(slot.alive.get() && unsafe { TrackMouseEvent(&mut tracking) } != 0);
}
fn context_menu(window: HWND, slot: &CanvasState, lparam: LPARAM) {
    let ready = unsafe {
        let state = &*slot.get();
        slot.alive.get() && state.interactive && state.plan.is_some() && !state.menu_open
    };
    if !ready || unsafe { IsWindowVisible(window) } == 0 {
        return;
    }
    clear_intentions(window, slot);
    let (epoch, privacy, theme, dpi) = unsafe {
        let state = &mut *slot.get();
        state.menu_open = true;
        (
            state.interaction_epoch,
            state.privacy,
            state.theme,
            state.dpi,
        )
    };
    let point = (lparam != -1).then_some(POINT {
        x: lparam as u16 as i16 as i32,
        y: (lparam >> 16) as u16 as i16 as i32,
    });
    let selected = super::menu::show(window, point, privacy, theme, dpi);
    let state = unsafe { &mut *slot.get() };
    state.menu_open = false;
    if slot.alive.get()
        && state.interactive
        && state.plan.is_some()
        && state.interaction_epoch == epoch
    {
        match selected {
            Ok(Some(action)) => state.clicks.push(unsafe { GetTickCount64() }, action),
            Err(_) => state.paint_failed = true,
            _ => {}
        }
    }
}
pub(crate) struct NativeCanvas {
    pub(crate) window: HWND,
    class: Vec<u16>,
    // Strong procedure references keep resources alive through nested native menu dispatch.
    state: Rc<CanvasState>,
    attached: bool,
    palette_confirmed: bool,
}
impl NativeCanvas {
    /// Caller owns the parent on this UI thread, before any child windows are created.
    pub(crate) unsafe fn create(parent: HWND, dpi: u32) -> Result<Self, WireError> {
        let state = Rc::new(CanvasState {
            value: UnsafeCell::new(State {
                font: NativeFont::new(dpi)?,
                plan: None,
                palette: Palette::system(rgb(18, 18, 18))?,
                paint_failed: false,
                interactive: false,
                clicks: ClickQueue::default(),
                privacy: true,
                theme: AppTheme::Light,
                dpi,
                interaction_epoch: 0,
                menu_open: false,
            }),
            painting: Cell::new(false),
            alive: Cell::new(true),
            details: NativeDetails::create(parent)?,
            tracking: Cell::new(false),
            dismissed: Cell::new(false),
            focused: Cell::new(false),
        });
        let class = wide(&format!(
            "TokenPulse.Taskbar.Readout.{}",
            uuid::Uuid::new_v4().simple()
        ));
        let module = unsafe { GetModuleHandleW(ptr::null()) };
        let window_class = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(procedure),
            style: CS_DBLCLKS,
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..unsafe { mem::zeroed() }
        };
        if unsafe { RegisterClassExW(&window_class) } == 0 {
            return Err(WireError::InvalidState);
        }
        let window = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_NOPARENTNOTIFY,
                class.as_ptr(),
                wide("TokenPulse").as_ptr(),
                WS_CHILD | WS_TABSTOP,
                0,
                0,
                0,
                0,
                parent,
                ptr::null_mut(),
                module,
                Rc::as_ptr(&state).cast(),
            )
        };
        if window.is_null() {
            unsafe {
                UnregisterClassW(class.as_ptr(), module);
            }
            return Err(WireError::InvalidState);
        }
        Ok(Self {
            window,
            class,
            state,
            attached: false,
            palette_confirmed: false,
        })
    }
    pub(crate) fn visible(&self) -> bool {
        self.alive() && unsafe { IsWindowVisible(self.window) } != 0
    }
    pub(crate) fn own_visible_style(&self) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GWL_STYLE, WS_VISIBLE};
        self.alive()
            && unsafe { GetWindowLongPtrW(self.window, GWL_STYLE) } as u32 & WS_VISIBLE != 0
    }
    pub(crate) fn alive(&self) -> bool {
        // Explorer can invalidate a cross-process child without our cached NCDESTROY flag
        // having caught up. Verify the kernel owner and full generation class as well;
        // a reused numeric HWND must never be cleared or destroyed by this canvas owner.
        if !self.state.alive.get() {
            return false;
        }
        use windows_sys::Win32::{
            System::Threading::{GetCurrentProcessId, GetCurrentThreadId},
            UI::WindowsAndMessaging::{GetClassNameW, GetWindowThreadProcessId},
        };
        let mut pid = 0;
        let thread = unsafe { GetWindowThreadProcessId(self.window, &mut pid) };
        let mut class = [0u16; 128];
        let length = unsafe { GetClassNameW(self.window, class.as_mut_ptr(), 128) };
        let valid = thread == unsafe { GetCurrentThreadId() }
            && pid == unsafe { GetCurrentProcessId() }
            && length > 0
            && class[..length as usize] == self.class[..self.class.len() - 1];
        if !valid {
            self.state.alive.set(false);
        }
        valid
    }
    pub(crate) fn plan(&self) -> Option<&MeasuredPlan> {
        unsafe { &*self.state.get() }.plan.as_ref()
    }
    pub(crate) fn paint_failed(&self) -> bool {
        let state = unsafe { &*self.state.get() };
        state.paint_failed || self.state.details.failed() || !self.state.alive.get()
    }
    pub(crate) fn set_attached(&mut self, attached: bool) {
        self.attached = attached;
        unsafe {
            (*self.state.get()).interactive = attached;
        }
        if !attached {
            self.clear_interactions();
        } else if self.alive() {
            // SetParent / showing the reserved child can invalidate its layered surface.
            // Present the prepared frame immediately, without waiting for the next snapshot.
            unsafe {
                InvalidateRect(self.window, ptr::null(), 0);
                UpdateWindow(self.window);
            }
        }
    }
    pub(crate) fn set_palette(&mut self, palette: Palette) {
        self.palette_confirmed = true;
        unsafe {
            (*self.state.get()).palette = palette;
        }
    }
    pub(crate) fn palette_confirmed(&self) -> bool {
        self.palette_confirmed
    }
    pub(crate) fn clear(&mut self) -> Result<(), WireError> {
        self.clear_interactions();
        self.state.details.clear()?;
        self.clear_render()
    }
    pub(crate) fn clear_interactions(&mut self) {
        if self.alive() {
            clear_intentions(self.window, &self.state);
        } else {
            let state = unsafe { &mut *self.state.get() };
            state.clicks.clear();
            state.interaction_epoch = state.interaction_epoch.saturating_add(1);
            state.plan = None;
            self.state.details.clear().ok();
        }
    }
    pub(crate) fn take_actions(&mut self) -> Vec<HostAction> {
        unsafe { (*self.state.get()).clicks.take(GetTickCount64()) }
    }
    pub(crate) fn clear_render(&mut self) -> Result<(), WireError> {
        unsafe {
            (*self.state.get()).plan = None;
        }
        if !self.alive() {
            return Err(WireError::Closed);
        }
        if unsafe { SetWindowTextW(self.window, wide("TokenPulse").as_ptr()) } == 0 {
            acceptance_trace("clear_render_set_text");
            return Err(WireError::InvalidState);
        }
        let mut rect: RECT = unsafe { mem::zeroed() };
        if unsafe { GetClientRect(self.window, &mut rect) } == 0 {
            acceptance_trace("clear_render_client_rect");
            return Err(WireError::InvalidState);
        }
        if rect.right > 0 && rect.bottom > 0 {
            let result = {
                let state = unsafe { &*self.state.get() };
                state.font.present(
                    self.window,
                    None,
                    rect.right,
                    rect.bottom,
                    state.palette,
                    false,
                )
            };
            if result.is_err() {
                acceptance_trace("clear_render_font_paint");
            }
            result?;
        }
        Ok(())
    }
    pub(crate) fn prepare(
        &mut self,
        view: &TaskbarView,
        prefs: DisplayPreferences,
        dpi: u32,
        width: i32,
        height: i32,
        now: i64,
    ) -> Result<(), WireError> {
        let result = self.prepare_frame(view, prefs, dpi, width, height, now);
        if result.is_err() {
            // Failed frames and privacy barriers must still discard old private pixels.
            self.clear().ok();
        }
        result
    }
    fn prepare_frame(
        &mut self,
        view: &TaskbarView,
        prefs: DisplayPreferences,
        dpi: u32,
        width: i32,
        height: i32,
        now: i64,
    ) -> Result<(), WireError> {
        // Build a replacement while the old layered surface remains visible. The
        // WM_PAINT path publishes the complete bitmap in one UpdateLayeredWindow.
        let font = NativeFont::for_taskbar(dpi, prefs, height)?;
        let plan = font.plan(view, prefs, now, width, height)?;
        self.state.details.prepare(self.window, view, dpi, now)?;
        unsafe {
            (*self.state.get()).font = font;
            (*self.state.get()).privacy = view.privacy;
            (*self.state.get()).theme = view.details.as_ref().map_or(AppTheme::System, |d| d.theme);
            (*self.state.get()).dpi = dpi;
        }
        if let Some(plan) = plan {
            let accessible = wide(&accessible_text(view)?);
            if unsafe { SetWindowTextW(self.window, accessible.as_ptr()) } == 0 {
                return Err(WireError::InvalidState);
            }
            let (width, height) = (plan.width, plan.height);
            unsafe {
                (*self.state.get()).plan = Some(plan);
            }
            if unsafe {
                SetWindowPos(
                    self.window,
                    ptr::null_mut(),
                    0,
                    0,
                    width,
                    height,
                    SWP_NOACTIVATE
                        | SWP_NOZORDER
                        | if self.attached {
                            SWP_NOMOVE | SWP_NOSIZE
                        } else {
                            0
                        },
                )
            } == 0
            {
                self.clear()?;
                return Err(WireError::InvalidState);
            }
            unsafe {
                InvalidateRect(self.window, ptr::null(), 0);
                UpdateWindow(self.window);
            }
        } else {
            self.clear()?;
        }
        Ok(())
    }
}
fn acceptance_trace(stage: &str) {
    #[cfg(debug_assertions)]
    if std::env::var_os("TOKENPULSE_ACCEPTANCE_HOST_DIAGNOSTICS").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        eprintln!("NATIVE_CANVAS_FAILED: {stage} last_error={}", unsafe {
            windows_sys::Win32::Foundation::GetLastError()
        });
    }
    #[cfg(not(debug_assertions))]
    let _ = stage;
}
impl Drop for NativeCanvas {
    fn drop(&mut self) {
        // WM_NCDESTROY marks this generation dead; never destroy a subsequently reused HWND.
        if self.alive() {
            self.clear().ok();
            if self.alive() {
                unsafe {
                    DestroyWindow(self.window);
                }
            }
        }
        unsafe {
            UnregisterClassW(self.class.as_ptr(), GetModuleHandleW(ptr::null()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::topology::{DpiGuard, rect};
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextW, WS_POPUP};
    #[test]
    fn own_visibility_distinguishes_a_hidden_ancestor_from_a_hidden_readout() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow, WS_EX_NOACTIVATE,
        };
        let _dpi = DpiGuard::enter().unwrap();
        let parent = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                wide("STATIC").as_ptr(),
                wide("TokenPulse synthetic visibility test").as_ptr(),
                WS_POPUP,
                0,
                0,
                320,
                80,
                ptr::null_mut(),
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            )
        };
        assert!(!parent.is_null());
        let canvas = unsafe { NativeCanvas::create(parent, 96) }.unwrap();
        // Verify the kernel layer rather than inferring presentation from WS_VISIBLE
        // or a screenshot. This also fails if the test/host loses its OS-aware manifest.
        use windows_sys::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE;
        assert_ne!(
            unsafe { GetWindowLongPtrW(canvas.window, GWL_EXSTYLE) } as u32 & WS_EX_LAYERED,
            0
        );
        assert!(!canvas.own_visible_style());
        assert!(!canvas.visible());
        unsafe {
            ShowWindow(canvas.window, SW_SHOWNOACTIVATE);
        }
        assert!(canvas.own_visible_style());
        assert!(
            !canvas.visible(),
            "hidden ancestor suppresses effective visibility"
        );
        unsafe {
            ShowWindow(parent, SW_SHOWNOACTIVATE);
        }
        assert!(canvas.own_visible_style());
        assert!(canvas.visible());
        unsafe {
            ShowWindow(canvas.window, SW_HIDE);
        }
        assert!(
            !canvas.own_visible_style(),
            "an explicitly hidden readout remains rejected"
        );
        assert!(!canvas.visible());
        drop(canvas);
        assert_ne!(unsafe { DestroyWindow(parent) }, 0);
    }
    #[test]
    fn dialog_requests_only_active_commands_and_visible_detail_keys() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_TAB;
        for key in [0, VK_TAB as usize, b'A' as usize, VK_F10 as usize] {
            assert_eq!(dialog_code(true, true, key, false), 0);
        }
        for key in [VK_RETURN, VK_SPACE, VK_APPS] {
            assert_eq!(
                dialog_code(true, false, key as usize, false),
                DLGC_WANTMESSAGE as isize
            );
            assert_eq!(dialog_code(false, true, key as usize, false), 0);
        }
        assert_eq!(
            dialog_code(true, false, VK_F10 as usize, true),
            DLGC_WANTMESSAGE as isize
        );
        for key in [
            VK_ESCAPE, VK_UP, VK_DOWN, VK_PRIOR, VK_NEXT, VK_HOME, VK_END,
        ] {
            assert_eq!(dialog_code(true, false, key as usize, false), 0);
            assert_eq!(
                dialog_code(true, true, key as usize, false),
                DLGC_WANTMESSAGE as isize
            );
        }
    }
    #[test]
    fn native_dialog_tab_leaves_canvas_and_dismisses_details_without_an_action() {
        use windows_sys::Win32::UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{GetFocus, SetFocus, VK_TAB},
            WindowsAndMessaging::{
                IsDialogMessageW, MSG, SW_SHOWNOACTIVATE, ShowWindow, WS_OVERLAPPEDWINDOW,
                WS_VISIBLE,
            },
        };
        let _dpi = DpiGuard::enter().unwrap();
        let parent = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                wide("STATIC").as_ptr(),
                wide("TokenPulse SYNTHETIC dialog navigation test").as_ptr(),
                WS_OVERLAPPEDWINDOW,
                200,
                200,
                1000,
                100,
                ptr::null_mut(),
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            )
        };
        assert!(!parent.is_null());
        unsafe {
            ShowWindow(parent, SW_SHOWNOACTIVATE);
        }
        let dpi = unsafe { GetDpiForWindow(parent) };
        let mut canvas = unsafe { NativeCanvas::create(parent, dpi) }.unwrap();
        let next = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                wide("BUTTON").as_ptr(),
                wide("Synthetic next control").as_ptr(),
                WS_CHILD | WS_TABSTOP | WS_VISIBLE,
                750,
                0,
                200,
                40,
                parent,
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            )
        };
        assert!(!next.is_null());
        let view = super::super::details_window::fixture();
        canvas
            .prepare(
                &view,
                DisplayPreferences::default(),
                dpi,
                700,
                60,
                view.generated_at_ms.value(),
            )
            .unwrap();
        canvas.set_attached(true);
        unsafe {
            ShowWindow(canvas.window, SW_SHOWNOACTIVATE);
            SetFocus(canvas.window);
        }
        assert_eq!(unsafe { GetFocus() }, canvas.window);
        assert!(canvas.state.details.visible());
        let message = MSG {
            hwnd: canvas.window,
            message: WM_KEYDOWN,
            wParam: VK_TAB as usize,
            ..unsafe { mem::zeroed() }
        };
        assert_ne!(unsafe { IsDialogMessageW(parent, &message) }, 0);
        assert_eq!(unsafe { GetFocus() }, next);
        assert!(!canvas.state.details.visible());
        assert!(canvas.take_actions().is_empty());
        // Native dialog tab order can also re-enter this ordinary WS_TABSTOP child.
        let message = MSG {
            hwnd: next,
            ..message
        };
        assert_ne!(unsafe { IsDialogMessageW(parent, &message) }, 0);
        assert_eq!(unsafe { GetFocus() }, canvas.window);
        assert!(canvas.state.details.visible());
        let enter = MSG {
            hwnd: canvas.window,
            message: WM_KEYDOWN,
            wParam: VK_RETURN as usize,
            ..unsafe { mem::zeroed() }
        };
        // IsDialogMessage dispatches our requested message; do not dispatch it twice.
        assert_ne!(unsafe { IsDialogMessageW(parent, &enter) }, 0);
        assert_eq!(canvas.take_actions(), [HostAction::OpenFloat {}]);
        assert!(!canvas.state.details.visible());
        canvas.clear().unwrap();
        drop(canvas);
        unsafe {
            DestroyWindow(parent);
        }
    }
    #[test]
    fn authored_own_canvas_events_hover_focus_escape_rearm_and_clear_without_actions() {
        use windows_sys::Win32::UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::VK_END,
            WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindow},
        };
        let _dpi = DpiGuard::enter().unwrap();
        let parent = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                wide("STATIC").as_ptr(),
                wide("TokenPulse SYNTHETIC canvas event test").as_ptr(),
                WS_POPUP,
                200,
                200,
                1000,
                80,
                ptr::null_mut(),
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            )
        };
        assert!(!parent.is_null());
        unsafe {
            ShowWindow(parent, SW_SHOWNOACTIVATE);
        }
        let dpi = unsafe { GetDpiForWindow(parent) };
        let mut canvas = unsafe { NativeCanvas::create(parent, dpi) }.unwrap();
        let view = super::super::details_window::fixture();
        canvas
            .prepare(
                &view,
                DisplayPreferences::default(),
                dpi,
                900,
                80,
                view.generated_at_ms.value(),
            )
            .unwrap();
        canvas.set_attached(true);
        unsafe {
            ShowWindow(canvas.window, SW_SHOWNOACTIVATE);
        }
        let window = canvas.window;
        let event = |message, wparam| unsafe { procedure(window, message, wparam, 0) };
        // An old queued hover without a newly armed move cannot reopen after clear.
        event(WM_MOUSEHOVER, 0);
        assert!(!canvas.state.details.visible());
        event(WM_MOUSEMOVE, 0);
        assert!(canvas.state.tracking.get());
        event(WM_MOUSEHOVER, 0);
        assert!(canvas.state.details.visible());
        event(WM_SETFOCUS, 0);
        assert!(canvas.state.focused.get());
        event(WM_MOUSELEAVE, 0);
        assert!(canvas.state.details.visible());
        event(WM_KEYDOWN, VK_END as usize);
        assert!(canvas.take_actions().is_empty());
        event(WM_KEYDOWN, VK_ESCAPE as usize);
        assert!(!canvas.state.details.visible());
        event(WM_MOUSEMOVE, 0);
        event(WM_MOUSEHOVER, 0);
        assert!(!canvas.state.details.visible());
        event(WM_MOUSELEAVE, 0);
        event(WM_MOUSEMOVE, 0);
        event(WM_MOUSEHOVER, 0);
        assert!(canvas.state.details.visible());
        event(WM_KEYDOWN, VK_RETURN as usize);
        assert!(!canvas.state.details.visible());
        assert_eq!(canvas.take_actions(), [HostAction::OpenFloat {}]);
        canvas.clear_interactions();
        assert!(!canvas.state.details.visible());
        event(WM_MOUSEHOVER, 0);
        assert!(!canvas.state.details.visible());
        canvas
            .prepare(
                &view,
                DisplayPreferences::default(),
                dpi,
                900,
                80,
                view.generated_at_ms.value(),
            )
            .unwrap();
        event(WM_MOUSEMOVE, 0);
        event(WM_MOUSEHOVER, 0);
        assert!(canvas.state.details.visible());
        event(WM_KILLFOCUS, 0);
        assert!(!canvas.state.details.visible());
        assert!(!canvas.paint_failed());
        canvas.clear().unwrap();
        assert!(canvas.take_actions().is_empty());
        drop(canvas);
        unsafe {
            DestroyWindow(parent);
        }
    }
    #[test]
    fn updating_attached_canvas_preserves_reserved_position_and_size() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindow};
        let _dpi = DpiGuard::enter().unwrap();
        let parent = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                wide("STATIC").as_ptr(),
                wide("TokenPulse OWN TEST WINDOW").as_ptr(),
                WS_POPUP,
                0,
                0,
                500,
                100,
                ptr::null_mut(),
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            )
        };
        assert!(!parent.is_null());
        let mut canvas = unsafe { NativeCanvas::create(parent, 96) }.unwrap();
        let mut view: TaskbarView =
            serde_json::from_str(include_str!("../../../../fixtures/taskbar-display.json"))
                .unwrap();
        canvas
            .prepare(
                &view,
                DisplayPreferences::default(),
                96,
                400,
                60,
                1790899200000,
            )
            .unwrap();
        assert_ne!(
            unsafe {
                SetWindowPos(
                    canvas.window,
                    ptr::null_mut(),
                    17,
                    23,
                    400,
                    60,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            },
            0
        );
        let before = rect(canvas.window).unwrap();
        unsafe {
            ShowWindow(parent, SW_SHOWNOACTIVATE);
            ShowWindow(canvas.window, SW_SHOWNOACTIVATE);
        }
        super::super::render::PRESENTED_FRAMES.with(|frames| frames.borrow_mut().clear());
        canvas.set_attached(true);
        super::super::render::PRESENTED_FRAMES.with(|frames| {
            assert_eq!(
                *frames.borrow(),
                [true],
                "attachment immediately presents its prepared frame"
            );
            frames.borrow_mut().clear();
        });
        view.privacy = true;
        view.scope_label = None;
        view.costs.clear();
        view.quota = None;
        canvas
            .prepare(
                &view,
                DisplayPreferences::default(),
                96,
                400,
                60,
                1790899200000,
            )
            .unwrap();
        assert_eq!(rect(canvas.window).unwrap(), before);
        assert!(canvas.plan().is_some());
        super::super::render::PRESENTED_FRAMES.with(|frames| {
            assert_eq!(
                *frames.borrow(),
                [true],
                "refresh must never submit an empty layered frame"
            );
            frames.borrow_mut().clear();
        });
        canvas.clear().unwrap();
        super::super::render::PRESENTED_FRAMES.with(|frames| {
            assert_eq!(
                *frames.borrow(),
                [false],
                "explicit clear still overwrites private pixels"
            );
        });
        assert!(canvas.plan().is_none());
        let mut caption = [0; 64];
        let length = unsafe { GetWindowTextW(canvas.window, caption.as_mut_ptr(), 64) };
        assert_eq!(
            String::from_utf16(&caption[..length as usize]).unwrap(),
            "TokenPulse"
        );
        assert_eq!(rect(canvas.window).unwrap(), before);
        drop(canvas);
        unsafe {
            DestroyWindow(parent);
        }
    }
}
