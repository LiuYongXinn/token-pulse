//! Read-only Explorer topology probe and pure reservation plan. No layout mutations.
use std::{mem, ptr};
use windows_sys::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT},
        System::{
            SystemInformation::{GetWindowsDirectoryW, OSVERSIONINFOW},
            Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
        },
        UI::{
            HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
                SetThreadDpiAwarenessContext,
            },
            WindowsAndMessaging::{
                EnumChildWindows, FindWindowW, GetClassNameW, GetParent, GetWindowRect,
                GetWindowThreadProcessId, IsWindowVisible,
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
    fn valid(self) -> bool {
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
struct DpiGuard(HANDLE);
impl DpiGuard {
    fn enter() -> Result<Self, ProbeError> {
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
fn rect(window: HWND) -> Result<ScreenRect, ProbeError> {
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
fn process_id(window: HWND) -> u32 {
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
fn verify_explorer(pid: u32) -> Result<(), ProbeError> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
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
    Ok(())
}
/// Inspect only the primary taskbar on the currently supported Win10 build.
/// Unsupported versions are an explicit capability failure, never embedded success.
pub fn inspect_primary_taskbar() -> Result<TaskbarTopology, ProbeError> {
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
    verify_explorer(pid)?;
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
    let topology = TaskbarTopology {
        build: version.dwBuildNumber,
        dpi: unsafe { GetDpiForWindow(root) },
        taskbar: rect(root)?,
        rebar: rect(rebar)?,
        task_switch: rect(switch)?,
        task_list: rect(list)?,
        notification: rect(tray)?,
    };
    topology.validate()?;
    // A probe is ephemeral; a future mutating adapter must recheck HWND/PID/generation.
    if process_id(root) != pid || unsafe { GetParent(rebar) } != root {
        return Err(ProbeError::UnexpectedStructure);
    }
    Ok(topology)
}
