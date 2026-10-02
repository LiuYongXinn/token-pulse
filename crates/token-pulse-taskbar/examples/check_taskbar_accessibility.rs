//! Opt-in native UI Automation check of an owned production host. Synthetic fixture only.
#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use token_pulse_core::numeric::DecimalInt;
    use token_pulse_taskbar::{
        HostConfiguration, HostDisplayState, HostMessage, HostReply, TaskbarView,
        windows::{topology::inspect_primary_taskbar, transport::HostConnection},
    };
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let validate_actions = if args == ["--native-taskbar-accessibility-development-check"] {
        false
    } else if args == ["--native-taskbar-accessibility-actions-development-check"] {
        true
    } else {
        std::process::exit(2);
    };
    println!(
        "DEVELOPMENT ONLY: owned synthetic host, actual UI Automation properties; no direct SendInput or external window names"
    );
    let before = inspect_primary_taskbar().expect("supported baseline");
    let executable = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("token-pulse-taskbar-host.exe");
    let mut connection = HostConnection::launch(&executable).await.unwrap();
    let mut fixture: TaskbarView =
        serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap();
    fixture.settings_revision = DecimalInt::parse("1").unwrap();
    let configuration = HostConfiguration {
        position: Default::default(),
        settings_revision: fixture.settings_revision.clone(),
        enabled: true,
        display: Default::default(),
    };
    connection
        .exchange(HostMessage::Privacy {
            settings_revision: fixture.settings_revision.clone(),
            enabled: false,
        })
        .await
        .unwrap();
    connection
        .exchange(HostMessage::Configure { configuration })
        .await
        .unwrap();
    connection
        .exchange(HostMessage::Snapshot {
            view: Box::new(fixture.clone()),
        })
        .await
        .unwrap();
    let HostReply::Status { status } = connection
        .exchange(HostMessage::GetStatus {})
        .await
        .unwrap()
    else {
        panic!("status");
    };
    assert_eq!(status.state, HostDisplayState::Embedded);
    let _com = AutomationApartment::enter();
    let window = own_readout(connection.process_id());
    let full = read_properties(window, connection.process_id());
    assert!(full.name.contains("Token 683100"));
    assert!(full.name.contains("输入 —") && full.name.contains("缓存 0"));
    assert!(full.name.contains("0.565000000000001") && full.name.contains("USD"));
    assert!(full.name.contains("SYNTHETIC DEVELOPMENT FIXTURE"));
    assert!(full.name.contains("SYNTHETIC DEVELOPMENT BUCKET"));
    assert!(
        full.focusable,
        "visible native taskbar entry must expose keyboard focusability"
    );
    assert!(!full.offscreen);
    println!(
        "NATIVE_TASKBAR_UIA_PUBLIC_OK: exact synthetic integers/amounts/null, scope/account name, focusable={}, control_type={}",
        full.focusable, full.control_type
    );
    if validate_actions {
        use token_pulse_taskbar::HostAction;
        println!(
            "DEVELOPMENT ACTION MODE: invoke only five owned synthetic host menu items; intents consumed by this probe, no production application receives them"
        );
        for (name, expected) in [
            ("显示悬浮窗", HostAction::OpenFloat {}),
            ("打开当前范围统计", HostAction::OpenStats {}),
            ("任务栏设置", HostAction::OpenTaskbarSettings {}),
            ("隐私模式", HostAction::SetPrivacy { enabled: true }),
            ("隐藏任务栏显示", HostAction::DisableTaskbar {}),
        ] {
            let menu = open_menu(window, connection.process_id()).await;
            invoke_menu_action(menu.entry, menu.popup, connection.process_id(), name);
            let mut received = false;
            for _ in 0..40 {
                let HostReply::Actions {
                    settings_revision,
                    actions,
                } = connection
                    .exchange(HostMessage::GetActions {})
                    .await
                    .unwrap()
                else {
                    panic!("action reply");
                };
                assert_eq!(settings_revision.unwrap().as_str(), "1");
                if !actions.is_empty() {
                    assert_eq!(actions.as_slice(), std::slice::from_ref(&expected));
                    received = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            assert!(
                received,
                "native UIA Invoke did not produce the expected action"
            );
            let HostReply::Actions { actions, .. } = connection
                .exchange(HostMessage::GetActions {})
                .await
                .unwrap()
            else {
                panic!("action reply");
            };
            assert!(actions.is_empty(), "UIA action consumed exactly once");
            assert!(
                own_popup(connection.process_id()).is_none(),
                "invoked menu must end"
            );
            drop(menu);
        }
        println!(
            "NATIVE_TASKBAR_UIA_ACTIONS_OK: five actual owned UIA Invoke calls produced precisely the whitelisted single-consumption intents over the production host pipe"
        );
    }
    let menu = open_menu(window, connection.process_id()).await;
    read_menu(menu.popup, connection.process_id());
    connection
        .exchange(HostMessage::Privacy {
            settings_revision: DecimalInt::parse("2").unwrap(),
            enabled: true,
        })
        .await
        .unwrap();
    assert!(
        own_popup(connection.process_id()).is_none(),
        "privacy ACK must close the owned menu"
    );
    drop(menu);
    let cleared = read_properties(window, connection.process_id());
    assert_eq!(
        cleared.name, "TokenPulse",
        "ACK must clear the actual native provider name"
    );
    assert_eq!(inspect_primary_taskbar().unwrap(), before);
    fixture.settings_revision = DecimalInt::parse("2").unwrap();
    fixture.privacy = true;
    fixture.scope_label = None;
    fixture.costs.clear();
    fixture.quota = None;
    connection
        .exchange(HostMessage::Snapshot {
            view: Box::new(fixture),
        })
        .await
        .unwrap();
    let private = read_properties(
        own_readout(connection.process_id()),
        connection.process_id(),
    );
    assert!(private.name.contains("Token 683100") && private.name.contains("费用和账户已隐藏"));
    for secret in [
        "0.565000000000001",
        "SYNTHETIC DEVELOPMENT FIXTURE",
        "SYNTHETIC DEVELOPMENT BUCKET",
    ] {
        assert!(
            !private.name.contains(secret),
            "fresh UIA name leaked synthetic private field"
        );
    }
    connection.exchange(HostMessage::Shutdown {}).await.unwrap();
    assert_eq!(inspect_primary_taskbar().unwrap(), before);
    println!(
        "NATIVE_TASKBAR_UIA_OK: actual owned provider/menu names and invocation patterns, full precision, null/zero, privacy ACK and fresh private name, native geometry restored; physical Narrator/keyboard acceptance separate"
    );
}

#[cfg(windows)]
fn invoke_menu_action(
    entry: windows_sys::Win32::Foundation::HWND,
    popup: windows_sys::Win32::Foundation::HWND,
    pid: u32,
    name: &str,
) {
    use windows::{
        Win32::{
            Foundation::HWND,
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            UI::Accessibility::{
                CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationInvokePattern,
                TreeScope_Children, UIA_InvokePatternId, UIA_MenuItemControlTypeId,
            },
        },
        core::Interface,
    };
    assert_eq!(own_popup(pid), Some(popup));
    unsafe {
        let automation: IUIAutomation2 =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).unwrap();
        automation.SetConnectionTimeout(1000).unwrap();
        automation.SetTransactionTimeout(1000).unwrap();
        automation.SetAutoSetFocus(false).unwrap();
        let automation: IUIAutomation = automation.cast().unwrap();
        let root = automation.ElementFromHandle(HWND(popup)).unwrap();
        assert_eq!(root.CurrentProcessId().unwrap(), pid as i32);
        let elements = root
            .FindAll(
                TreeScope_Children,
                &automation.CreateTrueCondition().unwrap(),
            )
            .unwrap();
        let count = elements.Length().unwrap();
        assert!((5..=8).contains(&count));
        let mut selected = None;
        for index in 0..count {
            let item = elements.GetElement(index).unwrap();
            if item.CurrentControlType().unwrap() == UIA_MenuItemControlTypeId
                && item.CurrentName().unwrap().to_string().starts_with(name)
            {
                assert!(selected.is_none(), "unique owned menu action");
                assert_eq!(item.CurrentProcessId().unwrap(), pid as i32);
                selected = Some(item);
            }
        }
        let item = selected.expect("owned named menu item");
        assert!(item.CurrentIsEnabled().unwrap().as_bool());
        assert!(!item.CurrentIsOffscreen().unwrap().as_bool());
        let bounds = item.CurrentBoundingRectangle().unwrap();
        // Windows menu proxies may synthesize selection/Enter internally. Owner identity
        // alone is insufficient: never invoke through a covered or inactive input surface.
        ensure_menu_input(
            entry,
            popup,
            pid,
            bounds.left,
            bounds.top,
            bounds.right,
            bounds.bottom,
        );
        let invoke: IUIAutomationInvokePattern = item
            .GetCurrentPattern(UIA_InvokePatternId)
            .unwrap()
            .cast()
            .unwrap();
        if let Err(error) = invoke.Invoke() {
            panic!(
                "NATIVE_TASKBAR_UIA_INVOKE_FAILED: HRESULT={:08x}",
                error.code().0 as u32
            );
        }
    }
    verify_owned(entry, pid);
}

#[cfg(windows)]
fn ensure_menu_input(
    entry: windows_sys::Win32::Foundation::HWND,
    popup: windows_sys::Win32::Foundation::HWND,
    pid: u32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) {
    use windows_sys::Win32::{
        Foundation::POINT,
        System::StationsAndDesktops::{CloseDesktop, DESKTOP_READOBJECTS, OpenInputDesktop},
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
            WindowsAndMessaging::{
                GA_ROOT, GUITHREADINFO, GetAncestor, GetClassNameW, GetForegroundWindow,
                GetGUIThreadInfo, GetWindowThreadProcessId, WindowFromPoint,
            },
        },
    };
    let desktop = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
    assert!(
        !desktop.is_null(),
        "NATIVE_TASKBAR_UIA_INPUT_REFUSED: input desktop unavailable"
    );
    assert_ne!(unsafe { CloseDesktop(desktop) }, 0);
    struct DpiContext(windows_sys::Win32::Foundation::HANDLE);
    impl Drop for DpiContext {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    assert!(!previous.is_null());
    let _dpi = DpiContext(previous);
    assert!(right > left && bottom > top);
    let point = POINT {
        x: left + (right - left) / 2,
        y: top + (bottom - top) / 2,
    };
    let hit = unsafe { WindowFromPoint(point) };
    if hit != popup {
        let mut class = [0; 128];
        let length = unsafe { GetClassNameW(hit, class.as_mut_ptr(), class.len() as i32) };
        eprintln!(
            "NATIVE_TASKBAR_UIA_INPUT_REFUSED: owned menu item covered; hit_class={}",
            String::from_utf16_lossy(&class[..length.max(0) as usize])
        );
        panic!("refuse native Invoke outside owned menu hit surface");
    }
    let mut actual = 0;
    let thread = unsafe { GetWindowThreadProcessId(popup, &mut actual) };
    assert_eq!(actual, pid);
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    assert_ne!(unsafe { GetGUIThreadInfo(thread, &mut info) }, 0);
    assert_eq!(
        info.hwndMenuOwner, entry,
        "NATIVE_TASKBAR_UIA_INPUT_REFUSED: menu owner changed"
    );
    let foreground = unsafe { GetForegroundWindow() };
    assert!(
        !foreground.is_null()
            && (foreground == popup || foreground == unsafe { GetAncestor(entry, GA_ROOT) }),
        "NATIVE_TASKBAR_UIA_INPUT_REFUSED: menu is not foreground"
    );
    verify_owned(entry, pid);
    assert_eq!(own_popup(pid), Some(popup));
}

#[cfg(windows)]
struct OwnedMenu {
    entry: windows_sys::Win32::Foundation::HWND,
    popup: windows_sys::Win32::Foundation::HWND,
    pid: u32,
}
#[cfg(windows)]
impl Drop for OwnedMenu {
    fn drop(&mut self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowThreadProcessId, PostMessageW, WM_CANCELMODE,
        };
        let mut actual = 0;
        unsafe {
            GetWindowThreadProcessId(self.entry, &mut actual);
        }
        if actual == self.pid {
            unsafe {
                PostMessageW(self.entry, WM_CANCELMODE, 0, 0);
            }
        }
    }
}
#[cfg(windows)]
async fn open_menu(entry: windows_sys::Win32::Foundation::HWND, pid: u32) -> OwnedMenu {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CONTEXTMENU};
    verify_owned(entry, pid);
    let mut menu = OwnedMenu {
        entry,
        popup: std::ptr::null_mut(),
        pid,
    };
    assert_ne!(
        unsafe { PostMessageW(entry, WM_CONTEXTMENU, entry as usize, -1) },
        0
    );
    for _ in 0..60 {
        if let Some(popup) = own_popup(pid) {
            menu.popup = popup;
            return menu;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("owned native menu did not open within deadline");
}
#[cfg(windows)]
fn own_popup(pid: u32) -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
        },
    };
    struct Probe {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
        let probe = unsafe { &mut *(parameter as *mut Probe) };
        let mut actual = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut actual);
        }
        if actual == probe.pid && unsafe { IsWindowVisible(window) } != 0 {
            let mut class = [0; 32];
            let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
            if length > 0 && String::from_utf16_lossy(&class[..length as usize]) == "#32768" {
                probe.windows.push(window);
            }
        }
        1
    }
    let mut probe = Probe {
        pid,
        windows: vec![],
    };
    assert_ne!(
        unsafe { EnumWindows(Some(collect), (&mut probe as *mut Probe) as LPARAM) },
        0
    );
    assert!(probe.windows.len() <= 1, "unique owned popup");
    probe.windows.first().copied()
}
#[cfg(windows)]
fn read_menu(popup: windows_sys::Win32::Foundation::HWND, pid: u32) {
    use windows::{
        Win32::{
            Foundation::HWND,
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            UI::Accessibility::{
                CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationInvokePattern,
                TreeScope_Children, UIA_InvokePatternId, UIA_MenuControlTypeId,
                UIA_MenuItemControlTypeId,
            },
        },
        core::Interface,
    };
    unsafe {
        let automation: IUIAutomation2 =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).unwrap();
        automation.SetConnectionTimeout(1000).unwrap();
        automation.SetTransactionTimeout(1000).unwrap();
        automation.SetAutoSetFocus(false).unwrap();
        let automation: IUIAutomation = automation.cast().unwrap();
        let root = automation.ElementFromHandle(HWND(popup)).unwrap();
        assert_eq!(root.CurrentProcessId().unwrap(), pid as i32);
        assert_eq!(root.CurrentControlType().unwrap(), UIA_MenuControlTypeId);
        let elements = root
            .FindAll(
                TreeScope_Children,
                &automation.CreateTrueCondition().unwrap(),
            )
            .unwrap();
        let count = elements.Length().unwrap();
        assert!((5..=8).contains(&count));
        let mut names = vec![];
        for index in 0..count {
            let item = elements.GetElement(index).unwrap();
            if item.CurrentControlType().unwrap() != UIA_MenuItemControlTypeId {
                continue;
            }
            assert_eq!(item.CurrentProcessId().unwrap(), pid as i32);
            assert!(item.CurrentIsEnabled().unwrap().as_bool());
            let _: IUIAutomationInvokePattern = item
                .GetCurrentPattern(UIA_InvokePatternId)
                .unwrap()
                .cast()
                .unwrap();
            // Pattern availability only; never invoke a menu item from this reader.
            names.push(item.CurrentName().unwrap().to_string());
        }
        assert_eq!(names.len(), 5);
        for (name, expected) in names.iter().zip([
            "显示悬浮窗",
            "打开当前范围统计",
            "任务栏设置",
            "隐私模式",
            "隐藏任务栏显示",
        ]) {
            assert!(name.starts_with(expected), "native menu UIA name missing");
        }
    }
    println!(
        "NATIVE_TASKBAR_UIA_MENU_OK: five owned named enabled menu items and native invoke patterns; read-only query, no pattern invoked"
    );
}

#[cfg(windows)]
struct AutomationApartment;
#[cfg(windows)]
impl AutomationApartment {
    fn enter() -> Self {
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
        // This example's current-thread runtime owns no windows. Host UI lives in its process.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .unwrap();
        Self
    }
}
#[cfg(windows)]
impl Drop for AutomationApartment {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}
#[cfg(windows)]
struct Properties {
    name: String,
    focusable: bool,
    offscreen: bool,
    control_type: i32,
}
#[cfg(windows)]
fn read_properties(window: windows_sys::Win32::Foundation::HWND, pid: u32) -> Properties {
    use windows::{
        Win32::{
            Foundation::HWND,
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            UI::Accessibility::{CUIAutomation8, IUIAutomation, IUIAutomation2},
        },
        core::Interface,
    };
    verify_owned(window, pid);
    let properties = unsafe {
        let automation: IUIAutomation2 =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).unwrap();
        automation.SetConnectionTimeout(1000).unwrap();
        automation.SetTransactionTimeout(1000).unwrap();
        automation.SetAutoSetFocus(false).unwrap();
        let automation: IUIAutomation = automation.cast().unwrap();
        let element = automation.ElementFromHandle(HWND(window)).unwrap();
        assert_eq!(element.CurrentProcessId().unwrap(), pid as i32);
        Properties {
            name: element.CurrentName().unwrap().to_string(),
            focusable: element.CurrentIsKeyboardFocusable().unwrap().as_bool(),
            offscreen: element.CurrentIsOffscreen().unwrap().as_bool(),
            control_type: element.CurrentControlType().unwrap().0,
        }
    };
    verify_owned(window, pid);
    properties
}
#[cfg(windows)]
fn verify_owned(window: windows_sys::Win32::Foundation::HWND, pid: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetWindowThreadProcessId, IsWindow,
    };
    let mut actual = 0;
    let mut class = [0; 128];
    unsafe {
        assert_ne!(IsWindow(window), 0);
        GetWindowThreadProcessId(window, &mut actual);
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        assert!(
            length > 0
                && String::from_utf16_lossy(&class[..length as usize])
                    .starts_with("TokenPulse.Taskbar.Readout.")
        );
    }
    assert_eq!(actual, pid);
}
#[cfg(windows)]
fn own_readout(pid: u32) -> windows_sys::Win32::Foundation::HWND {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumChildWindows, FindWindowW, GetClassNameW, GetWindowThreadProcessId,
        },
    };
    struct Probe {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
        let probe = unsafe { &mut *(parameter as *mut Probe) };
        let mut actual = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut actual);
        }
        if actual == probe.pid {
            let mut class = [0; 128];
            let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
            if length > 0
                && String::from_utf16_lossy(&class[..length as usize])
                    .starts_with("TokenPulse.Taskbar.Readout.")
            {
                probe.windows.push(window);
            }
        }
        1
    }
    let class: Vec<u16> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
    let root = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    assert!(!root.is_null());
    let mut probe = Probe {
        pid,
        windows: vec![],
    };
    unsafe {
        EnumChildWindows(root, Some(collect), (&mut probe as *mut Probe) as LPARAM);
    }
    assert_eq!(probe.windows.len(), 1, "unique owned embedded readout");
    verify_owned(probe.windows[0], pid);
    probe.windows[0]
}
#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
