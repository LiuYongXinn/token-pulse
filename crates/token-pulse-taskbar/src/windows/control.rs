//! Dedicated Win32 UI thread. The hidden top-level window receives shell/system broadcasts.
//! It never claims embedding and does not modify Explorer in this module.
use super::{
    TransportError,
    topology::{ProbeError, TaskbarTopology, inspect_primary_taskbar, wide},
    transport::IO_TIMEOUT,
};
use crate::TaskbarView;
use std::{
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
enum Operation {
    Replace(Option<Box<TaskbarView>>),
    Inspect,
}
struct Request {
    operation: Operation,
    reply: oneshot::Sender<NativeReceipt>,
}
#[derive(Debug, Clone)]
pub struct NativeReceipt {
    pub cached_view_present: bool,
    pub private_fields_present: bool,
    pub control_visible: bool,
    pub system_revision: u64,
    pub topology: Result<TaskbarTopology, ProbeError>,
}
struct State {
    receiver: Receiver<Request>,
    view: Option<Box<TaskbarView>>,
    topology: Result<TaskbarTopology, ProbeError>,
    system_revision: u64,
    taskbar_created: u32,
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
        }
    }
    fn system_changed(&mut self) {
        self.system_revision = self.system_revision.saturating_add(1);
        self.topology = inspect_primary_taskbar();
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
    let raw = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut State;
    if !raw.is_null() {
        let state = unsafe { &mut *raw };
        match message {
            APPLY => {
                // A posted message carries no pointers or commands; only this private bounded
                // Rust queue owns the request. External WM_APP messages cannot create an action.
                while let Ok(request) = state.receiver.try_recv() {
                    if let Operation::Replace(view) = request.operation {
                        state.view = view;
                    }
                    let _ = request.reply.send(state.receipt(window));
                }
                return 0;
            }
            WM_CLOSE => {
                state.view = None;
                unsafe {
                    DestroyWindow(window);
                }
                return 0;
            }
            WM_DESTROY => {
                state.view = None;
                unsafe {
                    PostQuitMessage(0);
                }
                return 0;
            }
            WM_NCDESTROY => unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            },
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE | WM_THEMECHANGED
            | WM_POWERBROADCAST => state.system_changed(),
            _ if message == state.taskbar_created => state.system_changed(),
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
        let (sender, receiver) = mpsc::sync_channel(4);
        let (ready_sender, ready) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("taskbar-native-ui".into())
            .spawn(move || {
                let result = unsafe { native_thread(receiver, &ready_sender) };
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
            .map_err(|_| TransportError::Native)
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
    let mut state = Box::new(State {
        receiver,
        view: None,
        topology: inspect_primary_taskbar(),
        system_revision: 0,
        taskbar_created,
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
            (&mut *state as *mut State).cast(),
        )
    };
    if window.is_null() {
        unsafe {
            UnregisterClassW(class.as_ptr(), module);
        }
        return Err(TransportError::Native);
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
    state.view = None;
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
        GetClassNameW, GetParent, GetWindowThreadProcessId, WM_APP,
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
        let clear = controller.replace(None).await.unwrap();
        assert!(!clear.cached_view_present && !clear.private_fields_present);
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
