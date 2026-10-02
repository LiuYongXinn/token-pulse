//! A thread-owned, conditional Explorer reservation. Only the verified Win10 adapter mutates
//! Explorer. A destroyed/replaced window, lost owner property or changed parent geometry prevents
//! restoration. The child remains hidden until all postconditions hold.
use super::ownership::{LayoutRecord, Ownership, Phase, ProcessIdentity};
use super::topology::{
    DpiGuard, ProbeError, ReservationPlan, ScreenRect, TaskbarTopology, TaskbarWindows, class_name,
    client_rect, discover_primary_taskbar, process_id, rect, wide,
};
use std::{marker::PhantomData, os::windows::io::AsRawHandle, ptr, rc::Rc};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, HANDLE, HWND, SetLastError, WAIT_ABANDONED, WAIT_OBJECT_0,
    },
    Graphics::Gdi::{DCX_CACHE, DCX_WINDOW, GetDCEx, GetPixel, ReleaseDC},
    System::Threading::{CreateMutexW, GetProcessId, ReleaseMutex, WaitForSingleObject},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetParent, GetWindowLongPtrW, HWND_TOP, SW_HIDE, SWP_NOACTIVATE, SWP_NOZORDER,
        SWP_SHOWWINDOW, SetParent, SetWindowPos, ShowWindow, WS_EX_LAYOUTRTL,
    },
};

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
    NoRecord,
    Uncertain,
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
    ownership: Ownership,
    baseline: TaskbarTopology,
    pub(crate) slot: ScreenRect,
    active: bool,
}
impl LayoutLease {
    pub(crate) fn attach(child: HWND, width: i32, instance: &str) -> Result<Self, ProbeError> {
        let _dpi = DpiGuard::enter()?;
        let windows = discover_primary_taskbar()?;
        let mutex = LayoutMutex::acquire(&windows)?;
        let baseline = windows.topology()?;
        let plan = baseline.plan(width, (320 * baseline.dpi / 96) as i32)?;
        let original = relative(baseline.task_switch, windows.rebar)?;
        let expected = relative(plan.remaining_task_switch, windows.rebar)?;
        let parent = client_rect(windows.rebar)?;
        let child = OwnedWindow::capture(child)?;
        let previous_parent = OwnedWindow::capture(unsafe { GetParent(child.window) })?;
        if !child.class.starts_with("TokenPulse.Taskbar.Readout.")
            || !previous_parent
                .class
                .starts_with("TokenPulse.Taskbar.Control.")
        {
            return Err(ProbeError::UnexpectedStructure);
        }
        let ownership = Ownership::publish(
            windows.switch,
            instance,
            LayoutRecord {
                original,
                expected,
                parent_size: (parent.width(), parent.height()),
                dpi: baseline.dpi,
                host: Ownership::current_host()?,
                shell: ProcessIdentity {
                    pid: windows.pid,
                    birth: windows.birth,
                },
            },
        )?;
        let mut lease = Self {
            windows,
            _mutex: mutex,
            child,
            previous_parent,
            ownership,
            baseline,
            slot: plan.host,
            active: true,
        };
        // From here all failures run the same conditional cleanup through Drop.
        position(lease.windows.switch, expected, 0)?;
        lease.ownership.reserved(lease.windows.switch)?;
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
        self.windows.verify().is_ok() && self.ownership.owns(self.windows.switch)
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
        restore_owned(&self.windows, &self.ownership, false)
    }
}
impl Drop for LayoutLease {
    fn drop(&mut self) {
        self.release();
    }
}

fn restore_owned(
    windows: &TaskbarWindows,
    ownership: &Ownership,
    terminated: bool,
) -> RestoreDisposition {
    if windows.verify().is_err()
        || !ownership.owns(windows.switch)
        || ownership.record.shell
            != (ProcessIdentity {
                pid: windows.pid,
                birth: windows.birth,
            })
    {
        return RestoreDisposition::IdentityLost;
    }
    let record = ownership.record;
    let geometry = Geometry {
        original: record.original,
        expected: record.expected,
        parent_size: record.parent_size,
        dpi: record.dpi,
    };
    let result = match (
        rect(windows.switch).and_then(|r| relative(r, windows.rebar)),
        client_rect(windows.rebar),
        windows.topology(),
    ) {
        (Ok(current), Ok(parent), Ok(topology)) => {
            let disposition = geometry.restoration(current, parent, topology.dpi);
            if terminated
                && ownership.phase == Phase::Prepared
                && disposition == RestoreDisposition::AlreadyRestored
            {
                // The host died before confirming its synchronous SetWindowPos. Do not release
                // ownership while a pending shrink could still be applied by Explorer.
                RestoreDisposition::Uncertain
            } else if disposition == RestoreDisposition::Restored {
                let original_screen = ScreenRect {
                    left: record.original.left + topology.rebar.left,
                    top: record.original.top + topology.rebar.top,
                    right: record.original.right + topology.rebar.left,
                    bottom: record.original.bottom + topology.rebar.top,
                };
                if windows.safe_slot(original_screen, ptr::null_mut()).is_err() {
                    RestoreDisposition::ExternalChange
                } else if !ownership.owns(windows.switch) || windows.verify().is_err() {
                    RestoreDisposition::IdentityLost
                } else if position(windows.switch, record.original, 0).is_err()
                    || rect(windows.switch)
                        .and_then(|r| relative(r, windows.rebar))
                        .ok()
                        != Some(record.original)
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
    if !matches!(
        result,
        RestoreDisposition::Failed
            | RestoreDisposition::Uncertain
            | RestoreDisposition::IdentityLost
    ) {
        if windows.verify().is_err() {
            return RestoreDisposition::IdentityLost;
        }
        if ownership.remove(windows.switch).is_err() {
            return RestoreDisposition::Failed;
        }
    }
    result
}

/// Caller supplies the kernel handle of its own child and the instance used to launch that child.
/// A live process, wrong instance, wrong birth, replaced shell or changed layout cannot authorize
/// a write. This is a native Rust API, not a frontend command or an arbitrary HWND endpoint.
pub fn recover_terminated_host<H: AsRawHandle>(
    instance: &str,
    process: &H,
) -> Result<RestoreDisposition, ProbeError> {
    let handle = process.as_raw_handle().cast();
    if unsafe { WaitForSingleObject(handle, 0) } != WAIT_OBJECT_0 {
        return Err(ProbeError::UnexpectedStructure);
    }
    let identity = ProcessIdentity {
        pid: unsafe { GetProcessId(handle) },
        birth: super::topology::birth(handle)?,
    };
    if identity.pid == 0 {
        return Err(ProbeError::UnexpectedStructure);
    }
    let _dpi = DpiGuard::enter()?;
    let windows = discover_primary_taskbar()?;
    let _mutex = LayoutMutex::acquire(&windows)?;
    let Some(ownership) = Ownership::load(windows.switch, instance)? else {
        return Ok(RestoreDisposition::NoRecord);
    };
    if ownership.record.host != identity {
        return Err(ProbeError::UnexpectedStructure);
    }
    Ok(restore_owned(&windows, &ownership, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_refuses_a_live_kernel_process_handle_before_any_window_operation() {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        };
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                std::process::id(),
            )
        };
        assert!(!handle.is_null());
        let process = unsafe { OwnedHandle::from_raw_handle(handle.cast()) };
        assert_eq!(
            recover_terminated_host("00112233445566778899aabbccddeeff", &process),
            Err(ProbeError::UnexpectedStructure)
        );
    }
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
