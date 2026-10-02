//! Explicit native development fixture: a real foreground window owned by this probe.
//! No external window is activated or queried for titles. It is never linked into the host.
use std::{
    ptr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, StationsAndDesktops::*},
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
pub struct PhysicalCoordinates(
    DPI_AWARENESS_CONTEXT,
    std::marker::PhantomData<std::rc::Rc<()>>,
);
impl PhysicalCoordinates {
    pub fn enter() -> Self {
        let prior =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        assert!(!prior.is_null());
        Self(prior, Default::default())
    }
}
impl Drop for PhysicalCoordinates {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}
unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let creation = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, creation.lpCreateParams as isize);
        }
    }
    if message == WM_ACTIVATE && wparam as u16 == WA_INACTIVE as u16 {
        let counter = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const AtomicUsize;
        if !counter.is_null() {
            unsafe { &*counter }.fetch_add(1, Ordering::SeqCst);
        }
    }
    if message == WM_DESTROY {
        unsafe {
            PostQuitMessage(0);
        }
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
pub struct ForegroundFixture {
    window: usize,
    deactivations: Arc<AtomicUsize>,
    worker: Option<thread::JoinHandle<()>>,
}
impl ForegroundFixture {
    pub fn create() -> Self {
        let input = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
        assert!(
            !input.is_null(),
            "INPUT_DESKTOP_UNAVAILABLE:{}; unlock interactive desktop",
            unsafe { GetLastError() }
        );
        unsafe {
            CloseDesktop(input);
        }
        for key in [
            VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN, VK_LBUTTON, VK_RBUTTON, VK_MBUTTON,
        ] {
            assert!(
                unsafe { GetAsyncKeyState(i32::from(key)) } >= 0,
                "INPUT_KEY_ALREADY_HELD; release keys before native acceptance"
            );
        }
        let deactivations = Arc::new(AtomicUsize::new(0));
        let observed = deactivations.clone();
        let (ready, receive) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let _dpi = PhysicalCoordinates::enter();
            let class: Vec<u16> = format!("TokenPulse.NativeForeground.{}", uuid::Uuid::new_v4())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let instance = unsafe { GetModuleHandleW(ptr::null()) };
            let definition = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..unsafe { std::mem::zeroed() }
            };
            assert_ne!(unsafe { RegisterClassW(&definition) }, 0);
            let title: Vec<u16> = "TokenPulse · 原生焦点测试窗口"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let window = unsafe {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_POPUP | WS_CAPTION,
                    180,
                    100,
                    360,
                    140,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    instance,
                    Arc::as_ptr(&observed).cast(),
                )
            };
            assert!(!window.is_null());
            unsafe {
                ShowWindow(window, SW_SHOWNOACTIVATE);
            }
            ready.send(window as usize).unwrap();
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } > 0 {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            unsafe {
                UnregisterClassW(class.as_ptr(), instance);
            }
        });
        let mut fixture = Self {
            window: receive.recv_timeout(Duration::from_secs(3)).unwrap(),
            deactivations,
            worker: Some(worker),
        };
        fixture.activate();
        fixture
    }
    pub fn activate(&mut self) {
        let _dpi = PhysicalCoordinates::enter();
        let window = self.window as HWND;
        let mut rect = RECT::default();
        assert_ne!(unsafe { GetWindowRect(window, &mut rect) }, 0);
        let point = POINT {
            x: rect.left + (rect.right - rect.left) / 2,
            y: rect.top + (rect.bottom - rect.top) / 2,
        };
        let until = std::time::Instant::now() + Duration::from_secs(1);
        while unsafe { WindowFromPoint(point) } != window && std::time::Instant::now() < until {
            thread::sleep(Duration::from_millis(10));
        }
        if unsafe { WindowFromPoint(point) } != window {
            let hit = unsafe { WindowFromPoint(point) };
            let mut class = [0; 128];
            let mut hit_rect = RECT::default();
            let n = unsafe { GetClassNameW(hit, class.as_mut_ptr(), 128) };
            unsafe {
                GetWindowRect(hit, &mut hit_rect);
            }
            eprintln!(
                "FOREGROUND_FIXTURE_REFUSED rect=({}, {}, {}, {}) visible={} hit_class={} hit_rect=({}, {}, {}, {})",
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                unsafe { IsWindowVisible(window) },
                String::from_utf16_lossy(&class[..n.max(0) as usize]),
                hit_rect.left,
                hit_rect.top,
                hit_rect.right,
                hit_rect.bottom
            );
        }
        assert_eq!(
            unsafe { WindowFromPoint(point) },
            window,
            "refuse input outside own foreground fixture"
        );
        assert_ne!(unsafe { SetPhysicalCursorPos(point.x, point.y) }, 0);
        let mut inputs: [INPUT; 2] = unsafe { std::mem::zeroed() };
        for (input, flags) in inputs
            .iter_mut()
            .zip([MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP])
        {
            input.r#type = INPUT_MOUSE;
            input.Anonymous.mi.dwFlags = flags;
        }
        assert_eq!(
            unsafe { SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) },
            2
        );
        for _ in 0..100 {
            if unsafe { GetForegroundWindow() } == window {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            unsafe { GetForegroundWindow() },
            window,
            "own fixture must become actual foreground; unlock desktop"
        );
        self.deactivations.store(0, Ordering::SeqCst);
    }
    pub fn assert_preserved(&self, stage: &str) {
        assert_eq!(
            unsafe { GetForegroundWindow() },
            self.window as HWND,
            "foreground fixture changed during {stage}"
        );
        assert_eq!(
            self.deactivations.load(Ordering::SeqCst),
            0,
            "foreground fixture deactivated during {stage}"
        );
    }
}
impl Drop for ForegroundFixture {
    fn drop(&mut self) {
        unsafe {
            PostMessageW(self.window as HWND, WM_CLOSE, 0, 0);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
