//! Dedicated Win32 UI thread. The hidden top-level window receives shell/system broadcasts.
//! Embedding is opt-in and delegated to the guarded, thread-owned layout lease.
use super::{
    TransportError,
    canvas::NativeCanvas,
    layout::{LayoutLease, RestoreDisposition, background},
    render::Palette,
    topology::{ProbeError, TaskbarTopology, inspect_primary_taskbar, wide},
    transport::IO_TIMEOUT,
};
use crate::{
    TaskbarView,
    display::{Density, DisplayPreferences},
};
use std::{
    cell::{Cell, UnsafeCell},
    mem, ptr,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
};
use tokio::{sync::oneshot, time::timeout};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
        WindowsAndMessaging::{
            CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
            GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, IsWindow, IsWindowVisible, MSG,
            PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW,
            SetWindowLongPtrW, TranslateMessage, UnregisterClassW, WM_APP, WM_CLOSE, WM_DESTROY,
            WM_DISPLAYCHANGE, WM_DPICHANGED, WM_NCCREATE, WM_NCDESTROY, WM_POWERBROADCAST,
            WM_SETTINGCHANGE, WM_THEMECHANGED, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_POPUP,
        },
    },
};
const APPLY: u32 = WM_APP + 1;
const REFRESH: u32 = WM_APP + 2;
struct ThreadState {
    busy: Cell<bool>,
    taskbar_created: u32,
    state: UnsafeCell<State>,
}
struct BusyGuard<'a>(&'a Cell<bool>);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
enum Operation {
    Replace(Option<Box<TaskbarView>>),
    Inspect,
    Enable(bool),
    Configure(bool, DisplayPreferences),
}
struct Request {
    operation: Operation,
    reply: oneshot::Sender<Result<NativeReceipt, TransportError>>,
}
#[derive(Debug, Clone)]
pub struct NativeReceipt {
    pub cached_view_present: bool,
    pub private_fields_present: bool,
    pub control_visible: bool,
    pub system_revision: u64,
    pub topology: Result<TaskbarTopology, ProbeError>,
    pub readout_visible: bool,
    pub measured_width: Option<i32>,
    pub measured_density: Option<Density>,
    pub paint_failed: bool,
    pub embedded: bool,
    pub embedding_failure: Option<ProbeError>,
    pub last_restore: Option<RestoreDisposition>,
}
impl NativeReceipt {
    pub fn host_status(
        &self,
        configuration: Option<&crate::HostConfiguration>,
    ) -> Result<crate::HostStatus, TransportError> {
        use crate::{HostDisplayState, HostFailure, HostRestore};
        let enabled = configuration.is_some_and(|c| c.enabled);
        if self.paint_failed {
            return Err(TransportError::Native);
        }
        let failure = if enabled && !self.embedded {
            self.embedding_failure
                .or(self.topology.as_ref().err().copied())
                .or(self.cached_view_present.then_some(ProbeError::Os))
        } else {
            None
        };
        let state = if !enabled {
            HostDisplayState::Disabled
        } else if self.embedded {
            HostDisplayState::Embedded
        } else if failure.is_some() {
            HostDisplayState::Unavailable
        } else {
            HostDisplayState::WaitingSnapshot
        };
        let status = crate::HostStatus {
            settings_revision: configuration.map(|c| c.settings_revision.clone()),
            system_revision: token_pulse_core::numeric::DecimalInt::from_nonnegative(
                self.system_revision.into(),
            )
            .map_err(|_| TransportError::Native)?,
            state,
            failure: failure.map(|e| match e {
                ProbeError::Os => HostFailure::Os,
                ProbeError::UnsupportedVersion => HostFailure::UnsupportedVersion,
                ProbeError::MissingTaskbar => HostFailure::MissingTaskbar,
                ProbeError::UnexpectedStructure => HostFailure::UnexpectedStructure,
                ProbeError::UnsafeGeometry => HostFailure::UnsafeGeometry,
                ProbeError::InsufficientSpace => HostFailure::InsufficientSpace,
                ProbeError::BackgroundUnavailable => HostFailure::BackgroundUnavailable,
                ProbeError::CleanupTimeout => HostFailure::CleanupTimeout,
                ProbeError::GuardianUnavailable => HostFailure::GuardianUnavailable,
            }),
            density: (state == HostDisplayState::Embedded)
                .then_some(self.measured_density)
                .flatten(),
            last_restore: self.last_restore.map(|r| match r {
                RestoreDisposition::NoRecord => HostRestore::NoRecord,
                RestoreDisposition::Restored => HostRestore::Restored,
                RestoreDisposition::AlreadyRestored => HostRestore::AlreadyRestored,
                RestoreDisposition::ExternalChange => HostRestore::ExternalChange,
                RestoreDisposition::IdentityLost => HostRestore::IdentityLost,
                RestoreDisposition::Failed => HostRestore::Failed,
                RestoreDisposition::Uncertain => HostRestore::Uncertain,
            }),
        };
        status.validate()?;
        Ok(status)
    }
}
struct State {
    receiver: Receiver<Request>,
    view: Option<Box<TaskbarView>>,
    topology: Result<TaskbarTopology, ProbeError>,
    system_revision: u64,
    canvas: Option<NativeCanvas>,
    native_failed: bool,
    enabled: bool,
    preferences: DisplayPreferences,
    layout: Option<LayoutLease>,
    embedding_failure: Option<ProbeError>,
    last_restore: Option<RestoreDisposition>,
    instance: String,
}
impl State {
    fn receipt(&self, window: HWND) -> NativeReceipt {
        NativeReceipt {
            cached_view_present: self.view.is_some(),
            private_fields_present: self.view.as_ref().is_some_and(|v| {
                v.scope_label.is_some() || !v.costs.is_empty() || v.quota.is_some()
            }),
            control_visible: unsafe { IsWindowVisible(window) != 0 },
            system_revision: self.system_revision,
            topology: self.topology.clone(),
            readout_visible: self.canvas.as_ref().is_some_and(|c| c.visible()),
            measured_width: self.canvas.as_ref().and_then(|c| c.plan().map(|p| p.width)),
            measured_density: self
                .canvas
                .as_ref()
                .and_then(|c| c.plan().map(|p| p.density)),
            paint_failed: self.native_failed
                || self.canvas.as_ref().is_some_and(|c| c.paint_failed()),
            embedded: self.layout.as_ref().is_some_and(|l| l.valid())
                && self.canvas.as_ref().is_some_and(|c| c.visible()),
            embedding_failure: self.embedding_failure,
            last_restore: self.last_restore,
        }
    }
    fn system_changed(&mut self) {
        self.system_revision = self.system_revision.saturating_add(1);
        self.detach();
        self.topology = inspect_primary_taskbar();
        if self.prepare().is_err() {
            self.native_failed = true;
        }
    }
    fn detach(&mut self) {
        if let Some(mut layout) = self.layout.take() {
            self.last_restore = Some(layout.release());
        }
        if let Some(canvas) = self.canvas.as_mut() {
            canvas.set_attached(false);
        }
    }
    fn prepare(&mut self) -> Result<(), TransportError> {
        if !self.enabled || self.view.is_none() || self.layout.as_ref().is_some_and(|l| !l.valid())
        {
            self.detach();
            self.topology = inspect_primary_taskbar();
        }
        self.embedding_failure = None;
        if let Some(canvas) = self.canvas.as_mut() {
            canvas.clear().map_err(|_| TransportError::Native)?;
            if let (Some(view), Ok(topology)) = (&self.view, &self.topology) {
                let available = self
                    .layout
                    .as_ref()
                    .map(|l| l.slot.width())
                    .unwrap_or_else(|| {
                        topology.task_switch.width() - (320 * topology.dpi / 96) as i32
                    });
                if self.enabled {
                    match background() {
                        Ok(color) => canvas.set_palette(
                            Palette::system(color).map_err(|_| TransportError::Native)?,
                        ),
                        Err(error) => self.embedding_failure = Some(error),
                    }
                }
                let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                    Ok(duration) => {
                        i64::try_from(duration.as_millis()).map_err(|_| TransportError::Native)?
                    }
                    Err(before) => -i64::try_from(before.duration().as_millis())
                        .map_err(|_| TransportError::Native)?,
                };
                canvas
                    .prepare(
                        view,
                        self.preferences,
                        topology.dpi,
                        available,
                        topology.rebar.height(),
                        now,
                    )
                    .map_err(|_| TransportError::Native)?;
                if self.enabled && self.layout.is_none() && self.embedding_failure.is_none() {
                    if let Some(plan) = canvas.plan() {
                        match LayoutLease::attach(canvas.window, plan.width, &self.instance) {
                            Ok(layout) => {
                                self.layout = Some(layout);
                                canvas.set_attached(true);
                            }
                            Err(error) => self.embedding_failure = Some(error),
                        }
                    } else {
                        self.embedding_failure = Some(ProbeError::InsufficientSpace);
                    }
                }
            } else if self.enabled {
                self.embedding_failure = self.topology.as_ref().err().copied();
            }
        }
        if self.native_failed || self.canvas.as_ref().is_none_or(|c| c.paint_failed()) {
            Err(TransportError::Native)
        } else {
            Ok(())
        }
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
    let raw = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut ThreadState;
    if !raw.is_null() {
        let slot = unsafe { &*raw };
        // Cross-process window operations can dispatch incoming SendMessage calls on this same
        // thread. Defer stateful work rather than forming another &mut State during that call.
        if slot.busy.get() {
            if message == slot.taskbar_created {
                unsafe {
                    PostMessageW(window, REFRESH, 0, 0);
                }
            }
            match message {
                WM_DESTROY => unsafe {
                    PostQuitMessage(0);
                },
                WM_NCDESTROY => unsafe {
                    SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                },
                APPLY | WM_CLOSE => unsafe {
                    PostMessageW(window, message, 0, 0);
                },
                WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE | WM_THEMECHANGED
                | WM_POWERBROADCAST | REFRESH => unsafe {
                    PostMessageW(window, REFRESH, 0, 0);
                },
                _ => {}
            }
            return unsafe { DefWindowProcW(window, message, wparam, lparam) };
        }
        let taskbar_created = slot.taskbar_created;
        let handled = matches!(
            message,
            APPLY
                | REFRESH
                | WM_CLOSE
                | WM_DESTROY
                | WM_NCDESTROY
                | WM_DISPLAYCHANGE
                | WM_DPICHANGED
                | WM_SETTINGCHANGE
                | WM_THEMECHANGED
                | WM_POWERBROADCAST
        ) || message == taskbar_created;
        if !handled {
            return unsafe { DefWindowProcW(window, message, wparam, lparam) };
        }
        slot.busy.set(true);
        let _busy = BusyGuard(&slot.busy);
        let state = unsafe { &mut *slot.state.get() };
        match message {
            APPLY => {
                // A posted message carries no pointers or commands; only this private bounded
                // Rust queue owns the request. External WM_APP messages cannot create an action.
                while let Ok(request) = state.receiver.try_recv() {
                    let result = match request.operation {
                        Operation::Replace(view) => {
                            state.view = view;
                            state.prepare()
                        }
                        Operation::Enable(enabled) => {
                            state.enabled = enabled;
                            state.prepare()
                        }
                        Operation::Configure(enabled, preferences) => {
                            if state.preferences != preferences {
                                state.detach();
                            }
                            state.enabled = enabled;
                            state.preferences = preferences;
                            state.prepare()
                        }
                        Operation::Inspect => {
                            if state.layout.as_ref().is_some_and(|l| !l.valid()) {
                                state.detach();
                                state.topology = inspect_primary_taskbar();
                                state.embedding_failure = Some(ProbeError::UnsafeGeometry);
                            }
                            Ok(())
                        }
                    };
                    if result.is_err() {
                        state.native_failed = true;
                    }
                    let _ = request.reply.send(result.map(|_| state.receipt(window)));
                }
                return 0;
            }
            WM_CLOSE => {
                state.detach();
                state.view = None;
                if let Some(canvas) = state.canvas.as_mut() {
                    canvas.clear().ok();
                }
                unsafe {
                    DestroyWindow(window);
                }
                return 0;
            }
            WM_DESTROY => {
                state.detach();
                state.view = None;
                if let Some(canvas) = state.canvas.as_mut() {
                    canvas.clear().ok();
                }
                unsafe {
                    PostQuitMessage(0);
                }
                return 0;
            }
            WM_NCDESTROY => unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            },
            REFRESH | WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE | WM_THEMECHANGED
            | WM_POWERBROADCAST => state.system_changed(),
            _ if message == taskbar_created => state.system_changed(),
            _ => {}
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

pub struct NativeController {
    window: usize,
    sender: SyncSender<Request>,
    thread: Option<JoinHandle<()>>,
}
impl NativeController {
    pub fn start() -> Result<Self, TransportError> {
        Self::start_for_instance(&uuid::Uuid::new_v4().simple().to_string())
    }
    pub fn start_for_instance(instance: &str) -> Result<Self, TransportError> {
        if instance.len() != 32 || !instance.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(TransportError::InvalidStartup);
        }
        let instance = instance.to_ascii_lowercase();
        let (sender, receiver) = mpsc::sync_channel(4);
        let (ready_sender, ready) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("taskbar-native-ui".into())
            .spawn(move || {
                let result = unsafe { native_thread(receiver, &ready_sender, instance) };
                if result.is_err() {
                    let _ = ready_sender.send(Err(TransportError::Native));
                }
            })
            .map_err(|_| TransportError::Native)?;
        match ready.recv_timeout(IO_TIMEOUT) {
            Ok(Ok(window)) => Ok(Self {
                window,
                sender,
                thread: Some(thread),
            }),
            _ => {
                drop(ready);
                drop(sender);
                let _ = thread.join();
                Err(TransportError::Native)
            }
        }
    }
    async fn request(&self, operation: Operation) -> Result<NativeReceipt, TransportError> {
        let (reply, response) = oneshot::channel();
        self.sender
            .try_send(Request { operation, reply })
            .map_err(|_| TransportError::Native)?;
        if unsafe { PostMessageW(self.window as HWND, APPLY, 0, 0) } == 0 {
            return Err(TransportError::Native);
        }
        timeout(IO_TIMEOUT, response)
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|_| TransportError::Native)?
    }
    pub async fn replace(
        &self,
        view: Option<TaskbarView>,
    ) -> Result<NativeReceipt, TransportError> {
        if let Some(view) = &view {
            view.validate()?;
        }
        self.request(Operation::Replace(view.map(Box::new))).await
    }
    pub async fn inspect(&self) -> Result<NativeReceipt, TransportError> {
        self.request(Operation::Inspect).await
    }
    /// Only this private Rust queue can enable native layout mutation. Default is disabled.
    pub async fn enable_taskbar(&self, enabled: bool) -> Result<NativeReceipt, TransportError> {
        self.request(Operation::Enable(enabled)).await
    }
    pub async fn configure(
        &self,
        enabled: bool,
        preferences: DisplayPreferences,
    ) -> Result<NativeReceipt, TransportError> {
        preferences
            .validate()
            .map_err(|_| crate::WireError::InvalidFrame)?;
        self.request(Operation::Configure(enabled, preferences))
            .await
    }
}
impl Drop for NativeController {
    fn drop(&mut self) {
        unsafe {
            PostMessageW(self.window as HWND, WM_CLOSE, 0, 0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
unsafe fn native_thread(
    receiver: Receiver<Request>,
    ready: &SyncSender<Result<usize, TransportError>>,
    instance: String,
) -> Result<(), TransportError> {
    let dpi = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if dpi.is_null() {
        return Err(TransportError::Native);
    }
    let module = unsafe { GetModuleHandleW(ptr::null()) };
    let class = wide(&format!(
        "TokenPulse.Taskbar.Control.{}",
        uuid::Uuid::new_v4().simple()
    ));
    let taskbar_created = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
    if module.is_null() || taskbar_created == 0 {
        return Err(TransportError::Native);
    }
    let state = Box::new(ThreadState {
        busy: Cell::new(false),
        taskbar_created,
        state: UnsafeCell::new(State {
            receiver,
            view: None,
            topology: inspect_primary_taskbar(),
            system_revision: 0,
            canvas: None,
            native_failed: false,
            enabled: false,
            preferences: DisplayPreferences::default(),
            layout: None,
            embedding_failure: None,
            last_restore: None,
            instance,
        }),
    });
    let window_class = WNDCLASSEXW {
        cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(procedure),
        hInstance: module,
        lpszClassName: class.as_ptr(),
        ..unsafe { mem::zeroed() }
    };
    if unsafe { RegisterClassExW(&window_class) } == 0 {
        return Err(TransportError::Native);
    }
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            wide("TokenPulse native controller").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            module,
            (&*state as *const ThreadState).cast(),
        )
    };
    if window.is_null() {
        unsafe {
            UnregisterClassW(class.as_ptr(), module);
        }
        return Err(TransportError::Native);
    }
    let canvas_dpi = unsafe { &*state.state.get() }
        .topology
        .as_ref()
        .map(|t| t.dpi)
        .unwrap_or(96);
    match unsafe { NativeCanvas::create(window, canvas_dpi) } {
        Ok(canvas) => unsafe { (*state.state.get()).canvas = Some(canvas) },
        Err(_) => {
            unsafe {
                DestroyWindow(window);
                UnregisterClassW(class.as_ptr(), module);
            }
            return Err(TransportError::Native);
        }
    }
    if ready.send(Ok(window as usize)).is_ok() {
        let mut message: MSG = unsafe { mem::zeroed() };
        loop {
            let status = unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) };
            if status <= 0 {
                break;
            }
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    unsafe {
        state.busy.set(true);
        (*state.state.get()).view = None;
        (*state.state.get()).detach();
    }
    // Move the child owner out before dropping it; WM_PARENTNOTIFY can reenter the controller.
    let canvas = unsafe { (*state.state.get()).canvas.take() };
    drop(canvas);
    // State outlives all WM_NCDESTROY access even when initialization or pumping failed.
    if unsafe { IsWindow(window) } != 0 {
        unsafe {
            DestroyWindow(window);
        }
    }
    unsafe {
        UnregisterClassW(class.as_ptr(), module);
        SetThreadDpiAwarenessContext(dpi);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use token_pulse_core::{
        numeric::{DecimalInt, EpochMs},
        protocol::CoverageState,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GW_CHILD, GetClassNameW, GetParent, GetWindow, GetWindowTextW, GetWindowThreadProcessId,
        WM_APP,
    };
    fn view() -> TaskbarView {
        TaskbarView {
            settings_revision: DecimalInt::parse("9007199254740993").unwrap(),
            usage_revision: DecimalInt::parse("9007199254740993").unwrap(),
            price_revision: DecimalInt::parse("3").unwrap(),
            generated_at_ms: EpochMs::new(1000).unwrap(),
            privacy: false,
            scope_label: Some("SYNTHETIC PRIVATE SCOPE".into()),
            timezone: "UTC".into(),
            total_tokens: None,
            input_tokens: None,
            cached_tokens: Some(DecimalInt::parse("0").unwrap()),
            output_tokens: None,
            usage_status: CoverageState::Unknown,
            costs: vec![],
            priced_tokens: DecimalInt::parse("0").unwrap(),
            unpriced_tokens: DecimalInt::parse("0").unwrap(),
            quota: None,
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn actual_hidden_window_owns_cache_and_clears_it_before_native_receipt() {
        let controller = NativeController::start().unwrap();
        let window = controller.window as HWND;
        let mut pid = 0;
        assert_ne!(unsafe { GetWindowThreadProcessId(window, &mut pid) }, 0);
        assert_eq!(pid, std::process::id());
        assert!(unsafe { GetParent(window) }.is_null()); // Real top-level broadcast recipient.
        let initial = controller.inspect().await.unwrap();
        assert!(!initial.cached_view_present);
        assert!(!initial.control_visible);
        let visible = controller.replace(Some(view())).await.unwrap();
        assert!(visible.cached_view_present && visible.private_fields_present);
        assert!(!visible.control_visible);
        assert!(!visible.readout_visible && !visible.paint_failed);
        let canvas = unsafe { GetWindow(window, GW_CHILD) };
        assert!(!canvas.is_null());
        let caption = |window| {
            let mut text = [0; 4096];
            let length = unsafe { GetWindowTextW(window, text.as_mut_ptr(), 4096) };
            String::from_utf16(&text[..length as usize]).unwrap()
        };
        if initial.topology.is_ok() {
            assert!(visible.measured_width.is_some());
            assert!(caption(canvas).contains("SYNTHETIC PRIVATE SCOPE"));
            assert!(caption(canvas).contains("缓存 0"));
        }
        let clear = controller.replace(None).await.unwrap();
        assert!(!clear.cached_view_present && !clear.private_fields_present);
        assert!(clear.measured_width.is_none());
        assert_eq!(caption(canvas), "TokenPulse");
        let mut invalid = view();
        invalid.privacy = true;
        assert!(controller.replace(Some(invalid)).await.is_err());
        assert!(!controller.inspect().await.unwrap().cached_view_present);
        // A foreign-looking wake message cannot smuggle a pointer/operation into the queue.
        assert_ne!(
            unsafe { PostMessageW(window, WM_APP + 1, usize::MAX, -1) },
            0
        );
        assert!(!controller.inspect().await.unwrap().cached_view_present);
        let mut name = [0; 128];
        let length = unsafe { GetClassNameW(window, name.as_mut_ptr(), 128) };
        assert!(length > 0);
        drop(controller);
        assert_eq!(unsafe { IsWindow(window) }, 0);
        assert_eq!(unsafe { IsWindow(canvas) }, 0);
        let mut info: WNDCLASSEXW = unsafe { mem::zeroed() };
        let class = wide(&String::from_utf16(&name[..length as usize]).unwrap());
        assert_eq!(
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::GetClassInfoExW(
                    GetModuleHandleW(ptr::null()),
                    class.as_ptr(),
                    &mut info,
                )
            },
            0
        );
    }
    #[tokio::test(flavor = "current_thread")]
    async fn actual_controller_routes_system_and_taskbar_created_notifications_without_showing() {
        let controller = NativeController::start().unwrap();
        let initial = controller.inspect().await.unwrap();
        for message in [
            WM_DISPLAYCHANGE,
            WM_DPICHANGED,
            WM_SETTINGCHANGE,
            WM_THEMECHANGED,
            WM_POWERBROADCAST,
            unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) },
        ] {
            let suggested = windows_sys::Win32::Foundation::RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            let parameter = if message == WM_DPICHANGED {
                (&suggested as *const _) as LPARAM
            } else {
                0
            };
            let mut result = 0;
            assert_ne!(
                unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::SendMessageTimeoutW(
                        controller.window as HWND,
                        message,
                        0,
                        parameter,
                        windows_sys::Win32::UI::WindowsAndMessaging::SMTO_ABORTIFHUNG
                            | windows_sys::Win32::UI::WindowsAndMessaging::SMTO_BLOCK,
                        2000,
                        &mut result,
                    )
                },
                0,
                "controller notification {message}"
            );
        }
        let after = controller.inspect().await.unwrap();
        assert!(after.system_revision >= initial.system_revision + 6);
        assert!(!after.control_visible && !after.cached_view_present);
        // These are messages sent only to our controller, not actual Explorer/DPI changes.
        assert_eq!(after.topology, inspect_primary_taskbar());
    }
}
