//! Read-only Explorer topology probe and pure reservation plan. No layout mutations.
use std::{mem, ptr};
use windows_sys::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM, RECT, WAIT_TIMEOUT},
        System::{
            SystemInformation::{GetWindowsDirectoryW, OSVERSIONINFOW},
            Threading::{
                GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, WaitForSingleObject,
            },
        },
        UI::{
            HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
                SetThreadDpiAwarenessContext,
            },
            WindowsAndMessaging::{
                EnumChildWindows, FindWindowW, GetClassNameW, GetClientRect, GetParent,
                GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
            },
        },
    },
};

pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeError {
    Os,
    UnsupportedVersion,
    MissingTaskbar,
    UnexpectedStructure,
    UnsafeGeometry,
    InsufficientSpace,
    BackgroundUnavailable,
    CleanupTimeout,
    GuardianUnavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl ScreenRect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
    pub(crate) fn valid(self) -> bool {
        [self.left, self.top, self.right, self.bottom]
            .iter()
            .all(|v| (-1_000_000..=1_000_000).contains(v))
            && self.left < self.right
            && self.top < self.bottom
    }
    fn contains(self, other: Self) -> bool {
        self.left <= other.left
            && self.top <= other.top
            && self.right >= other.right
            && self.bottom >= other.bottom
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskbarTopology {
    pub build: u32,
    pub dpi: u32,
    pub taskbar: ScreenRect,
    pub rebar: ScreenRect,
    pub task_switch: ScreenRect,
    pub task_list: ScreenRect,
    pub notification: ScreenRect,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservationPlan {
    pub host: ScreenRect,
    pub remaining_task_switch: ScreenRect,
}
impl TaskbarTopology {
    pub(crate) fn application_split(
        &self,
        coverage: &super::buttons::ButtonCoverage,
        minimum_task_width: i32,
    ) -> Result<i32, ProbeError> {
        self.validate()?;
        coverage.validate()?;
        if minimum_task_width <= 0
            || coverage.list.left != self.task_list.left
            || coverage.list.top != self.task_list.top
            || coverage.list.bottom != self.task_list.bottom
            || coverage.list.right > self.task_list.right
        {
            return Err(ProbeError::UnsafeGeometry);
        }
        let split = i64::from(self.task_switch.left) + i64::from(minimum_task_width);
        let split = split.max(i64::from(coverage.rightmost()) + i64::from(8 * self.dpi / 96));
        if split > i64::from(self.task_switch.right) {
            return Err(ProbeError::InsufficientSpace);
        }
        Ok(split as i32)
    }
    /// Reserve after actual task buttons, preserving minimum application area and an 8 DIP gap.
    pub fn plan_application_right(
        &self,
        host_width: i32,
        minimum_task_width: i32,
        coverage: &super::buttons::ButtonCoverage,
    ) -> Result<ReservationPlan, ProbeError> {
        if host_width <= 0 {
            return Err(ProbeError::UnsafeGeometry);
        }
        let split = self.application_split(coverage, minimum_task_width)?;
        if i64::from(split) + i64::from(host_width) > i64::from(self.task_switch.right) {
            return Err(ProbeError::InsufficientSpace);
        }
        Ok(ReservationPlan {
            host: ScreenRect {
                left: split,
                right: split + host_width,
                ..self.task_switch
            },
            remaining_task_switch: ScreenRect {
                right: split,
                ..self.task_switch
            },
        })
    }
    pub fn validate(&self) -> Result<(), ProbeError> {
        if self.build != 19045 {
            return Err(ProbeError::UnsupportedVersion);
        }
        if !(96..=768).contains(&self.dpi)
            || ![
                self.taskbar,
                self.rebar,
                self.task_switch,
                self.task_list,
                self.notification,
            ]
            .iter()
            .all(|r| r.valid())
            || self.taskbar.width() <= self.taskbar.height()
            || self.taskbar.height() < 24 * self.dpi as i32 / 96
            || !self.taskbar.contains(self.rebar)
            || !self.taskbar.contains(self.notification)
            || !self.rebar.contains(self.task_switch)
            || !self.task_switch.contains(self.task_list)
            || self.rebar.right != self.task_switch.right
            || self.task_switch.top != self.rebar.top
            || self.task_switch.bottom != self.rebar.bottom
            || self.rebar.right > self.notification.left
        {
            return Err(ProbeError::UnsafeGeometry);
        }
        Ok(())
    }
    /// Input widths are measured physical pixels. This plan does not reserve Explorer space.
    pub fn plan(
        &self,
        host_width: i32,
        minimum_task_width: i32,
    ) -> Result<ReservationPlan, ProbeError> {
        self.validate()?;
        if host_width <= 0 || minimum_task_width <= 0 {
            return Err(ProbeError::UnsafeGeometry);
        }
        if i64::from(host_width) + i64::from(minimum_task_width)
            > i64::from(self.task_switch.width())
        {
            return Err(ProbeError::InsufficientSpace);
        }
        let split = self.task_switch.right - host_width;
        Ok(ReservationPlan {
            host: ScreenRect {
                left: split,
                ..self.task_switch
            },
            remaining_task_switch: ScreenRect {
                right: split,
                ..self.task_switch
            },
        })
    }
}
pub(crate) struct DpiGuard(HANDLE);
impl DpiGuard {
    pub(crate) fn enter() -> Result<Self, ProbeError> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.is_null() {
            Err(ProbeError::Os)
        } else {
            Ok(Self(previous))
        }
    }
}
impl Drop for DpiGuard {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}
struct ProcessHandle(HANDLE);
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub(crate) fn rect(window: HWND) -> Result<ScreenRect, ProbeError> {
    let mut rect: RECT = unsafe { mem::zeroed() };
    if unsafe { GetWindowRect(window, &mut rect) } == 0 {
        return Err(ProbeError::Os);
    }
    Ok(ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}
pub(crate) fn process_id(window: HWND) -> u32 {
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(window, &mut pid);
    }
    pid
}
struct Child {
    handle: HWND,
    parent: HWND,
    class: String,
}
struct Children {
    windows: Vec<Child>,
    overflow: bool,
    examined: usize,
}
unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
    let children = unsafe { &mut *(parameter as *mut Children) };
    children.examined += 1;
    if children.examined > 256 {
        children.overflow = true;
        return 0;
    }
    if unsafe { IsWindowVisible(window) } == 0 {
        return 1;
    }
    let mut class = [0; 128];
    let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
    if length <= 0 || length == 127 {
        children.overflow = true;
        return 0;
    }
    children.windows.push(Child {
        handle: window,
        parent: unsafe { GetParent(window) },
        class: String::from_utf16_lossy(&class[..length as usize]),
    });
    1
}
fn unique(children: &[Child], name: &str, parent: HWND, pid: u32) -> Result<HWND, ProbeError> {
    let matches: Vec<_> = children.iter().filter(|c| c.class == name).collect();
    if matches.len() != 1 || matches[0].parent != parent || process_id(matches[0].handle) != pid {
        return Err(ProbeError::UnexpectedStructure);
    }
    Ok(matches[0].handle)
}
fn verify_explorer(pid: u32) -> Result<ProcessHandle, ProbeError> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return Err(ProbeError::Os);
    }
    let handle = ProcessHandle(handle);
    let mut image = vec![0; 32768];
    let mut length = image.len() as u32;
    if unsafe { QueryFullProcessImageNameW(handle.0, 0, image.as_mut_ptr(), &mut length) } == 0 {
        return Err(ProbeError::Os);
    }
    let image = String::from_utf16(&image[..length as usize]).map_err(|_| ProbeError::Os)?;
    let mut directory = vec![0; 32768];
    let count = unsafe { GetWindowsDirectoryW(directory.as_mut_ptr(), directory.len() as u32) };
    if count == 0 || count as usize >= directory.len() {
        return Err(ProbeError::Os);
    }
    let directory = String::from_utf16(&directory[..count as usize]).map_err(|_| ProbeError::Os)?;
    if !image.eq_ignore_ascii_case(&format!(
        "{}\\explorer.exe",
        directory.trim_end_matches('\\')
    )) {
        return Err(ProbeError::UnexpectedStructure);
    }
    Ok(handle)
}
pub(crate) fn birth(handle: HANDLE) -> Result<[u32; 2], ProbeError> {
    let mut created: FILETIME = unsafe { mem::zeroed() };
    let mut exited: FILETIME = unsafe { mem::zeroed() };
    let mut kernel: FILETIME = unsafe { mem::zeroed() };
    let mut user: FILETIME = unsafe { mem::zeroed() };
    if unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) } == 0 {
        return Err(ProbeError::Os);
    }
    Ok([created.dwLowDateTime, created.dwHighDateTime])
}
pub(crate) fn class_name(window: HWND) -> Result<String, ProbeError> {
    let mut text = [0; 128];
    let length = unsafe { GetClassNameW(window, text.as_mut_ptr(), 128) };
    if length <= 0 || length == 127 {
        return Err(ProbeError::UnexpectedStructure);
    }
    String::from_utf16(&text[..length as usize]).map_err(|_| ProbeError::UnexpectedStructure)
}
pub(crate) fn client_rect(window: HWND) -> Result<ScreenRect, ProbeError> {
    let mut rect: RECT = unsafe { mem::zeroed() };
    if unsafe { GetClientRect(window, &mut rect) } == 0 {
        return Err(ProbeError::Os);
    }
    Ok(ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}
pub(crate) struct TaskbarWindows {
    pub(crate) root: HWND,
    pub(crate) rebar: HWND,
    pub(crate) switch: HWND,
    pub(crate) list: HWND,
    pub(crate) tray: HWND,
    pub(crate) pid: u32,
    pub(crate) birth: [u32; 2],
    process: ProcessHandle,
}
impl TaskbarWindows {
    pub(crate) fn safe_slot(&self, slot: ScreenRect, child: HWND) -> Result<(), ProbeError> {
        self.verify()?;
        let mut children = Children {
            windows: vec![],
            overflow: false,
            examined: 0,
        };
        unsafe {
            EnumChildWindows(
                self.root,
                Some(collect),
                (&mut children as *mut Children) as LPARAM,
            );
        }
        if children.overflow {
            return Err(ProbeError::UnexpectedStructure);
        }
        if children
            .windows
            .iter()
            .any(|c| c.parent == self.rebar && c.handle != self.switch)
        {
            return Err(ProbeError::UnexpectedStructure);
        }
        for window in children
            .windows
            .iter()
            .filter(|c| c.parent == self.root && c.handle != self.rebar && c.handle != child)
        {
            let bounds = rect(window.handle)?;
            if bounds.left < slot.right
                && bounds.right > slot.left
                && bounds.top < slot.bottom
                && bounds.bottom > slot.top
            {
                return Err(ProbeError::UnsafeGeometry);
            }
        }
        Ok(())
    }
    pub(crate) fn verify(&self) -> Result<(), ProbeError> {
        if unsafe { WaitForSingleObject(self.process.0, 0) } != WAIT_TIMEOUT
            || birth(self.process.0)? != self.birth
        {
            return Err(ProbeError::UnexpectedStructure);
        }
        for (window, class, parent) in [
            (self.root, "Shell_TrayWnd", std::ptr::null_mut()),
            (self.rebar, "ReBarWindow32", self.root),
            (self.switch, "MSTaskSwWClass", self.rebar),
            (self.list, "MSTaskListWClass", self.switch),
            (self.tray, "TrayNotifyWnd", self.root),
        ] {
            if process_id(window) != self.pid
                || class_name(window)? != class
                || unsafe { GetParent(window) } != parent
            {
                return Err(ProbeError::UnexpectedStructure);
            }
        }
        Ok(())
    }
    pub(crate) fn topology(&self) -> Result<TaskbarTopology, ProbeError> {
        self.verify()?;
        Ok(TaskbarTopology {
            build: 19045,
            dpi: unsafe { GetDpiForWindow(self.root) },
            taskbar: rect(self.root)?,
            rebar: rect(self.rebar)?,
            task_switch: rect(self.switch)?,
            task_list: rect(self.list)?,
            notification: rect(self.tray)?,
        })
    }
}
/// Inspect only the primary taskbar on the currently supported Win10 build.
/// Unsupported versions are an explicit capability failure, never embedded success.
pub fn inspect_primary_taskbar() -> Result<TaskbarTopology, ProbeError> {
    let _dpi = DpiGuard::enter()?;
    let windows = discover_primary_taskbar()?;
    let topology = windows.topology()?;
    topology.validate()?;
    Ok(topology)
}
pub(crate) fn discover_primary_taskbar() -> Result<TaskbarWindows, ProbeError> {
    let _dpi = DpiGuard::enter()?;
    let mut version: OSVERSIONINFOW = unsafe { mem::zeroed() };
    version.dwOSVersionInfoSize = mem::size_of_val(&version) as u32;
    if unsafe { RtlGetVersion(&mut version) } != 0 {
        return Err(ProbeError::Os);
    }
    if version.dwMajorVersion != 10 || version.dwMinorVersion != 0 || version.dwBuildNumber != 19045
    {
        return Err(ProbeError::UnsupportedVersion);
    }
    let root = unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), ptr::null()) };
    if root.is_null() {
        return Err(ProbeError::MissingTaskbar);
    }
    let pid = process_id(root);
    if pid == 0 {
        return Err(ProbeError::Os);
    }
    let process = verify_explorer(pid)?;
    let mut children = Children {
        windows: vec![],
        overflow: false,
        examined: 0,
    };
    unsafe {
        EnumChildWindows(
            root,
            Some(collect),
            (&mut children as *mut Children) as LPARAM,
        );
    }
    if children.overflow {
        return Err(ProbeError::UnexpectedStructure);
    }
    let rebar = unique(&children.windows, "ReBarWindow32", root, pid)?;
    let switch = unique(&children.windows, "MSTaskSwWClass", rebar, pid)?;
    let list = unique(&children.windows, "MSTaskListWClass", switch, pid)?;
    let tray = unique(&children.windows, "TrayNotifyWnd", root, pid)?;
    // Extra toolbars or another application in the proposed parent invalidate this adapter.
    if children
        .windows
        .iter()
        .any(|child| child.parent == rebar && child.handle != switch)
    {
        return Err(ProbeError::UnexpectedStructure);
    }
    let windows = TaskbarWindows {
        root,
        rebar,
        switch,
        list,
        tray,
        pid,
        birth: birth(process.0)?,
        process,
    };
    windows.verify()?;
    Ok(windows)
}
