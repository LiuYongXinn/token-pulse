//! Owned child drawing window; it remains hidden until the taskbar adapter attaches it.
use super::{
    render::{NativeFont, Palette, rgb},
    topology::wide,
};
use crate::{
    TaskbarView, WireError,
    display::{DisplayPreferences, MeasuredPlan, accessible_text},
};
use std::{cell::UnsafeCell, mem, ptr};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, EndPaint, GdiFlush, GetDC, InvalidateRect, PAINTSTRUCT, ReleaseDC, UpdateWindow,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA,
        GetClientRect, GetWindowLongPtrW, IsWindowVisible, RegisterClassExW, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, SetWindowTextW,
        UnregisterClassW, WM_ERASEBKGND, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_PRINTCLIENT,
        WNDCLASSEXW, WS_CHILD, WS_EX_NOACTIVATE, WS_TABSTOP,
    },
};
struct State {
    font: NativeFont,
    plan: Option<MeasuredPlan>,
    palette: Palette,
    alive: bool,
    paint_failed: bool,
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
    let raw = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut State;
    if !raw.is_null() {
        let state = unsafe { &mut *raw };
        match message {
            WM_PAINT | WM_PRINTCLIENT => {
                let mut paint: PAINTSTRUCT = unsafe { mem::zeroed() };
                let dc = if message == WM_PAINT {
                    unsafe { BeginPaint(window, &mut paint) }
                } else {
                    wparam as _
                };
                let mut rect: RECT = unsafe { mem::zeroed() };
                let result = if dc.is_null() || unsafe { GetClientRect(window, &mut rect) } == 0 {
                    Err(WireError::InvalidState)
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
                if message == WM_PAINT {
                    unsafe {
                        EndPaint(window, &paint);
                    }
                }
                return 0;
            }
            WM_ERASEBKGND => return 1,
            WM_NCDESTROY => {
                state.plan = None;
                state.alive = false;
                unsafe {
                    SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                }
            }
            _ => {}
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
pub(crate) struct NativeCanvas {
    pub(crate) window: HWND,
    class: Vec<u16>,
    // Win32 may reenter the procedure from SetWindowText/SetWindowPos/DestroyWindow.
    // The stable UnsafeCell allocation permits that mutation without aliasing a Box<&mut State>.
    state: Box<UnsafeCell<State>>,
    attached: bool,
}
impl NativeCanvas {
    /// Caller owns the parent on this UI thread, before any child windows are created.
    pub(crate) unsafe fn create(parent: HWND, dpi: u32) -> Result<Self, WireError> {
        let state = Box::new(UnsafeCell::new(State {
            font: NativeFont::new(dpi)?,
            plan: None,
            palette: Palette::system(rgb(18, 18, 18))?,
            alive: true,
            paint_failed: false,
        }));
        let class = wide(&format!(
            "TokenPulse.Taskbar.Readout.{}",
            uuid::Uuid::new_v4().simple()
        ));
        let module = unsafe { GetModuleHandleW(ptr::null()) };
        let window_class = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(procedure),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..unsafe { mem::zeroed() }
        };
        if unsafe { RegisterClassExW(&window_class) } == 0 {
            return Err(WireError::InvalidState);
        }
        let window = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
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
                state.get().cast(),
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
        })
    }
    pub(crate) fn visible(&self) -> bool {
        (unsafe { (*self.state.get()).alive }) && unsafe { IsWindowVisible(self.window) } != 0
    }
    pub(crate) fn plan(&self) -> Option<&MeasuredPlan> {
        unsafe { &*self.state.get() }.plan.as_ref()
    }
    pub(crate) fn paint_failed(&self) -> bool {
        let state = unsafe { &*self.state.get() };
        state.paint_failed || !state.alive
    }
    pub(crate) fn set_attached(&mut self, attached: bool) {
        self.attached = attached;
    }
    pub(crate) fn set_palette(&mut self, palette: Palette) {
        unsafe {
            (*self.state.get()).palette = palette;
        }
    }
    pub(crate) fn clear(&mut self) -> Result<(), WireError> {
        unsafe {
            (*self.state.get()).plan = None;
        }
        if !unsafe { (*self.state.get()).alive } {
            return Err(WireError::Closed);
        }
        if unsafe { SetWindowTextW(self.window, wide("TokenPulse").as_ptr()) } == 0 {
            return Err(WireError::InvalidState);
        }
        let mut rect: RECT = unsafe { mem::zeroed() };
        if unsafe { GetClientRect(self.window, &mut rect) } == 0 {
            return Err(WireError::InvalidState);
        }
        if rect.right > 0 && rect.bottom > 0 {
            let dc = unsafe { GetDC(self.window) };
            if dc.is_null() {
                return Err(WireError::InvalidState);
            }
            let result = {
                let state = unsafe { &*self.state.get() };
                unsafe {
                    state
                        .font
                        .paint(dc, None, rect.right, rect.bottom, state.palette)
                }
            };
            let flushed = unsafe { GdiFlush() } != 0;
            unsafe {
                ReleaseDC(self.window, dc);
            }
            result?;
            if !flushed {
                return Err(WireError::InvalidState);
            }
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
        self.clear()?;
        let font = NativeFont::new(dpi)?;
        let plan = font.plan(view, prefs, now, width, height)?;
        unsafe {
            (*self.state.get()).font = font;
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
        }
        Ok(())
    }
}
impl Drop for NativeCanvas {
    fn drop(&mut self) {
        // WM_NCDESTROY marks this generation dead; never destroy a subsequently reused HWND.
        if unsafe { (*self.state.get()).alive } {
            self.clear().ok();
            unsafe {
                DestroyWindow(self.window);
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
    fn updating_attached_canvas_preserves_reserved_position_and_size() {
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
        canvas.set_attached(true);
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
        canvas.clear().unwrap();
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
