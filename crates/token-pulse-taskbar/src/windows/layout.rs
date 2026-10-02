//! A thread-owned, conditional Explorer reservation. Only the verified Win10 adapter mutates
//! Explorer. A destroyed/replaced window, lost owner property or changed parent geometry prevents
//! restoration. The child remains hidden until all postconditions hold.
use super::topology::{
    DpiGuard, ProbeError, ReservationPlan, ScreenRect, TaskbarTopology, TaskbarWindows, class_name,
    client_rect, discover_primary_taskbar, process_id, rect, wide,
};
use std::{marker::PhantomData, ptr, rc::Rc};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, HANDLE, HWND, SetLastError, WAIT_ABANDONED, WAIT_OBJECT_0,
    },
    Graphics::Gdi::{DCX_CACHE, DCX_WINDOW, GetDCEx, GetPixel, ReleaseDC},
    System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetParent, GetPropW, GetWindowLongPtrW, HWND_TOP, RemovePropW, SW_HIDE,
        SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SetParent, SetPropW, SetWindowPos,
        ShowWindow, WS_EX_LAYOUTRTL,
    },
};

const OWNER: &str = "TokenPulse.Taskbar.Layout.v1.owner";
pub(crate) fn background() -> Result<u32, ProbeError> {
    let _dpi = DpiGuard::enter()?;
    let windows = discover_primary_taskbar()?;
    let bounds = windows.topology()?;
    if bounds.task_switch.left <= bounds.rebar.left {
        return Err(ProbeError::UnsafeGeometry);
    }
    let dc = unsafe { GetDCEx(windows.rebar, ptr::null_mut(), DCX_CACHE | DCX_WINDOW) };
    if dc.is_null() {
        return Err(ProbeError::BackgroundUnavailable);
    }
    // One pixel of the verified taskbar container, never a desktop/app screenshot.
    let color = unsafe { GetPixel(dc, 0, 1) };
    unsafe {
        ReleaseDC(windows.rebar, dc);
    }
    if color == u32::MAX {
        Err(ProbeError::BackgroundUnavailable)
    } else {
        Ok(color)
    }
}
struct LayoutMutex(HANDLE, PhantomData<Rc<()>>);
impl LayoutMutex {
    fn acquire(windows: &TaskbarWindows) -> Result<Self, ProbeError> {
        Self::acquire_named(&format!(
            "Local\\TokenPulse.Taskbar.Layout.{}.{:08x}{:08x}",
            windows.pid, windows.birth[1], windows.birth[0]
        ))
    }
    fn acquire_named(name: &str) -> Result<Self, ProbeError> {
        let name = wide(name);
        let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(ProbeError::Os);
        }
        if !matches!(
            unsafe { WaitForSingleObject(handle, 0) },
            WAIT_OBJECT_0 | WAIT_ABANDONED
        ) {
            unsafe {
                CloseHandle(handle);
            }
            return Err(ProbeError::UnexpectedStructure);
        }
        Ok(Self(handle, PhantomData))
    }
}
impl Drop for LayoutMutex {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreDisposition {
    Restored,
    AlreadyRestored,
    ExternalChange,
    IdentityLost,
    Failed,
}
#[derive(Debug, Clone, Copy)]
struct Geometry {
    original: ScreenRect,
    expected: ScreenRect,
    parent_size: (i32, i32),
    dpi: u32,
}
impl Geometry {
    fn restoration(self, current: ScreenRect, parent: ScreenRect, dpi: u32) -> RestoreDisposition {
        if (parent.width(), parent.height()) != self.parent_size || dpi != self.dpi {
            RestoreDisposition::ExternalChange
        } else if current == self.expected {
            RestoreDisposition::Restored
        } else if current == self.original {
            RestoreDisposition::AlreadyRestored
        } else {
            RestoreDisposition::ExternalChange
        }
    }
}
fn relative(screen: ScreenRect, parent: HWND) -> Result<ScreenRect, ProbeError> {
    // This adapter accepts only borderless, non-mirrored taskbar containers. All coordinates are
    // physical under DpiGuard; reject a nontrivial client origin rather than guessing its offset.
    let bounds = rect(parent)?;
    let client = client_rect(parent)?;
    if client.left != 0
        || client.top != 0
        || client.width() != bounds.width()
        || client.height() != bounds.height()
        || unsafe { GetWindowLongPtrW(parent, GWL_EXSTYLE) } & WS_EX_LAYOUTRTL as isize != 0
    {
        return Err(ProbeError::UnsafeGeometry);
    }
    Ok(ScreenRect {
        left: screen.left - bounds.left,
        top: screen.top - bounds.top,
        right: screen.right - bounds.left,
        bottom: screen.bottom - bounds.top,
    })
}
fn position(window: HWND, bounds: ScreenRect, flags: u32) -> Result<(), ProbeError> {
    if !bounds.valid()
        || unsafe {
            SetWindowPos(
                window,
                ptr::null_mut(),
                bounds.left,
                bounds.top,
                bounds.width(),
                bounds.height(),
                SWP_NOACTIVATE | SWP_NOZORDER | flags,
            )
        } == 0
    {
        Err(ProbeError::Os)
    } else {
        Ok(())
    }
}
struct OwnedWindow {
    window: HWND,
    class: String,
    pid: u32,
}
impl OwnedWindow {
    fn capture(window: HWND) -> Result<Self, ProbeError> {
        if process_id(window) != std::process::id() {
            return Err(ProbeError::UnexpectedStructure);
        }
        Ok(Self {
            window,
            class: class_name(window)?,
            pid: process_id(window),
        })
    }
    fn valid(&self) -> bool {
        process_id(self.window) == self.pid
            && class_name(self.window).is_ok_and(|c| c == self.class)
    }
}

pub(crate) struct LayoutLease {
    windows: TaskbarWindows,
    _mutex: LayoutMutex,
    child: OwnedWindow,
    previous_parent: OwnedWindow,
    owner_key: Vec<u16>,
    marker: usize,
    geometry: Geometry,
    baseline: TaskbarTopology,
    pub(crate) slot: ScreenRect,
    active: bool,
}
impl LayoutLease {
    pub(crate) fn attach(child: HWND, width: i32) -> Result<Self, ProbeError> {
        let _dpi = DpiGuard::enter()?;
        let windows = discover_primary_taskbar()?;
        let mutex = LayoutMutex::acquire(&windows)?;
        let baseline = windows.topology()?;
        let plan = baseline.plan(width, (320 * baseline.dpi / 96) as i32)?;
        let original = relative(baseline.task_switch, windows.rebar)?;
        let expected = relative(plan.remaining_task_switch, windows.rebar)?;
        let parent = client_rect(windows.rebar)?;
        let owner_key = wide(OWNER);
        if !unsafe { GetPropW(windows.switch, owner_key.as_ptr()) }.is_null() {
            return Err(ProbeError::UnexpectedStructure);
        }
        let child = OwnedWindow::capture(child)?;
        let previous_parent = OwnedWindow::capture(unsafe { GetParent(child.window) })?;
        if !child.class.starts_with("TokenPulse.Taskbar.Readout.")
            || !previous_parent
                .class
                .starts_with("TokenPulse.Taskbar.Control.")
        {
            return Err(ProbeError::UnexpectedStructure);
        }
        // A window property belongs to this window generation and vanishes on destruction.
        // The marker is a nonzero random value, never a cross-process pointer to dereference.
        let marker = (uuid::Uuid::new_v4().as_u128() as u32).max(1) as usize;
        if unsafe { SetPropW(windows.switch, owner_key.as_ptr(), marker as HANDLE) } == 0 {
            return Err(ProbeError::Os);
        }
        let mut lease = Self {
            windows,
            _mutex: mutex,
            child,
            previous_parent,
            owner_key,
            marker,
            geometry: Geometry {
                original,
                expected,
                parent_size: (parent.width(), parent.height()),
                dpi: baseline.dpi,
            },
            baseline,
            slot: plan.host,
            active: true,
        };
        // From here all failures run the same conditional cleanup through Drop.
        position(lease.windows.switch, expected, 0)?;
        lease.verify_reserved(plan)?;
        unsafe {
            SetLastError(0);
        }
        let previous = unsafe { SetParent(lease.child.window, lease.windows.root) };
        if previous != lease.previous_parent.window
            || (previous.is_null() && unsafe { GetLastError() } != 0)
        {
            return Err(ProbeError::Os);
        }
        // SetParent can change the child process DPI context. Reenter physical coordinates and
        // verify the actual resulting rectangles before exposing the child.
        let _dpi = DpiGuard::enter()?;
        lease.verify_reserved(plan)?;
        let child_bounds = relative(plan.host, lease.windows.root)?;
        position(lease.child.window, child_bounds, 0)?;
        if rect(lease.child.window)? != plan.host
            || unsafe { GetParent(lease.child.window) } != lease.windows.root
        {
            return Err(ProbeError::UnsafeGeometry);
        }
        // SetParent places this window among root children. Explicitly raise only our already
        // verified disjoint slot above the ReBar background; no activation or popup overlay.
        if unsafe {
            SetWindowPos(
                lease.child.window,
                HWND_TOP,
                child_bounds.left,
                child_bounds.top,
                child_bounds.width(),
                child_bounds.height(),
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        } == 0
        {
            return Err(ProbeError::Os);
        }
        lease.verify_reserved(plan)?;
        // Mark used mutably so construction and cleanup never share a mutable state borrow.
        lease.active = true;
        Ok(lease)
    }
    fn owns(&self) -> bool {
        self.windows.verify().is_ok()
            && unsafe { GetPropW(self.windows.switch, self.owner_key.as_ptr()) } as usize
                == self.marker
    }
    fn verify_reserved(&self, plan: ReservationPlan) -> Result<(), ProbeError> {
        if !self.owns() {
            return Err(ProbeError::UnexpectedStructure);
        }
        self.windows.safe_slot(plan.host, self.child.window)?;
        let actual = self.windows.topology()?;
        if actual.taskbar != self.baseline.taskbar
            || actual.rebar != self.baseline.rebar
            || actual.notification != self.baseline.notification
            || actual.dpi != self.baseline.dpi
            || actual.task_switch != plan.remaining_task_switch
            || actual.task_list.left < actual.task_switch.left
            || actual.task_list.top < actual.task_switch.top
            || actual.task_list.right > actual.task_switch.right
            || actual.task_list.bottom > actual.task_switch.bottom
        {
            return Err(ProbeError::UnsafeGeometry);
        }
        Ok(())
    }
    pub(crate) fn valid(&self) -> bool {
        let Ok(_dpi) = DpiGuard::enter() else {
            return false;
        };
        self.child.valid()
            && unsafe { GetParent(self.child.window) } == self.windows.root
            && rect(self.child.window).is_ok_and(|r| r == self.slot)
            && self
                .verify_reserved(ReservationPlan {
                    host: self.slot,
                    remaining_task_switch: ScreenRect {
                        right: self.slot.left,
                        ..self.baseline.task_switch
                    },
                })
                .is_ok()
    }
    pub(crate) fn release(&mut self) -> RestoreDisposition {
        if !self.active {
            return RestoreDisposition::AlreadyRestored;
        }
        self.active = false;
        let Ok(_dpi) = DpiGuard::enter() else {
            return RestoreDisposition::Failed;
        };
        if self.child.valid() {
            unsafe {
                ShowWindow(self.child.window, SW_HIDE);
            }
            if unsafe { GetParent(self.child.window) } == self.windows.root
                && self.previous_parent.valid()
            {
                unsafe {
                    SetParent(self.child.window, self.previous_parent.window);
                }
            }
        }
        if !self.owns() {
            return RestoreDisposition::IdentityLost;
        }
        let result = match (
            rect(self.windows.switch).and_then(|r| relative(r, self.windows.rebar)),
            client_rect(self.windows.rebar),
            self.windows.topology(),
        ) {
            (Ok(current), Ok(parent), Ok(topology)) => {
                let disposition = self.geometry.restoration(current, parent, topology.dpi);
                if disposition == RestoreDisposition::Restored {
                    let original_screen = ScreenRect {
                        left: self.geometry.original.left + topology.rebar.left,
                        top: self.geometry.original.top + topology.rebar.top,
                        right: self.geometry.original.right + topology.rebar.left,
                        bottom: self.geometry.original.bottom + topology.rebar.top,
                    };
                    if self
                        .windows
                        .safe_slot(original_screen, self.child.window)
                        .is_err()
                    {
                        RestoreDisposition::ExternalChange
                    } else if position(self.windows.switch, self.geometry.original, 0).is_err()
                        || rect(self.windows.switch)
                            .and_then(|r| relative(r, self.windows.rebar))
                            .ok()
                            != Some(self.geometry.original)
                    {
                        RestoreDisposition::Failed
                    } else {
                        RestoreDisposition::Restored
                    }
                } else {
                    disposition
                }
            }
            _ => RestoreDisposition::Failed,
        };
        // Remove only our own marker; another owner/window generation must be left alone.
        if self.owns() && result != RestoreDisposition::Failed {
            unsafe {
                RemovePropW(self.windows.switch, self.owner_key.as_ptr());
            }
        }
        result
    }
}
impl Drop for LayoutLease {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_mutex_prevents_another_ui_thread_from_owning_layout_until_release() {
        let name = format!(
            "Local\\TokenPulse.Taskbar.Layout.TEST.{}",
            uuid::Uuid::new_v4().simple()
        );
        let owner = LayoutMutex::acquire_named(&name).unwrap();
        let other_name = name.clone();
        assert!(
            std::thread::spawn(move || LayoutMutex::acquire_named(&other_name).is_err())
                .join()
                .unwrap()
        );
        drop(owner);
        assert!(
            std::thread::spawn(move || LayoutMutex::acquire_named(&name).is_ok())
                .join()
                .unwrap()
        );
    }
    #[test]
    fn restoration_preserves_external_geometry_and_accepts_parent_origin_moves() {
        let original = ScreenRect {
            left: 2,
            top: 0,
            right: 1614,
            bottom: 60,
        };
        let expected = ScreenRect {
            right: 1214,
            ..original
        };
        let geometry = Geometry {
            original,
            expected,
            parent_size: (1614, 60),
            dpi: 144,
        };
        let parent = ScreenRect {
            left: 0,
            top: 0,
            right: 1614,
            bottom: 60,
        };
        assert_eq!(
            geometry.restoration(expected, parent, 144),
            RestoreDisposition::Restored
        );
        assert_eq!(
            geometry.restoration(original, parent, 144),
            RestoreDisposition::AlreadyRestored
        );
        assert_eq!(
            geometry.restoration(
                ScreenRect {
                    right: 1100,
                    ..expected
                },
                parent,
                144
            ),
            RestoreDisposition::ExternalChange
        );
        assert_eq!(
            geometry.restoration(
                expected,
                ScreenRect {
                    right: 1800,
                    ..parent
                },
                144
            ),
            RestoreDisposition::ExternalChange
        );
        assert_eq!(
            geometry.restoration(expected, parent, 192),
            RestoreDisposition::ExternalChange
        );
        assert_eq!(
            geometry.restoration(
                expected,
                ScreenRect {
                    left: -2500,
                    right: -886,
                    ..parent
                },
                144
            ),
            RestoreDisposition::Restored
        );
    }
}
