//! Explicit production-wire/native-host check. Synthetic fixture only; never run by default CI.
#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use token_pulse_core::numeric::DecimalInt;
    use token_pulse_taskbar::{
        HostAction, HostConfiguration, HostDisplayState, HostMessage, HostReply, HostRestore,
        TaskbarView,
        display::{DisplayLayout, DisplayPreferences},
        windows::{topology::inspect_primary_taskbar, transport::HostConnection},
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--native-taskbar-wire-development-check"] {
        std::process::exit(2);
    }
    println!(
        "DEVELOPMENT ONLY: synthetic fixture over production native host pipe; actual taskbar layout"
    );
    let before = inspect_primary_taskbar().expect("supported baseline");
    let focus = unsafe { GetForegroundWindow() };
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
    let mut configuration = HostConfiguration {
        settings_revision: fixture.settings_revision.clone(),
        enabled: true,
        display: DisplayPreferences::default(),
    };
    connection
        .exchange(HostMessage::Privacy {
            settings_revision: fixture.settings_revision.clone(),
            enabled: false,
        })
        .await
        .unwrap();
    connection
        .exchange(HostMessage::Configure {
            configuration: configuration.clone(),
        })
        .await
        .unwrap();
    let HostReply::Status { status: waiting } = connection
        .exchange(HostMessage::GetStatus {})
        .await
        .unwrap()
    else {
        panic!("status")
    };
    assert_eq!(waiting.state, HostDisplayState::WaitingSnapshot);
    connection
        .exchange(HostMessage::Snapshot {
            view: Box::new(fixture.clone()),
        })
        .await
        .unwrap();
    let HostReply::Status { status: shown } = connection
        .exchange(HostMessage::GetStatus {})
        .await
        .unwrap()
    else {
        panic!("status")
    };
    assert_eq!(shown.state, HostDisplayState::Embedded);
    assert!(shown.density.is_some() && shown.failure.is_none());
    let full_width = own_readout_width(connection.process_id());
    let pointer = PointerRestore::save();
    actual_click(connection.process_id(), false);
    connection
        .exchange(HostMessage::Snapshot {
            view: Box::new(fixture.clone()),
        })
        .await
        .unwrap();
    tokio::time::sleep(click_deadline()).await;
    assert_eq!(
        actions(&mut connection, "1").await,
        [HostAction::OpenFloat {}],
        "single click survives ordinary snapshot refresh"
    );
    assert!(
        actions(&mut connection, "1").await.is_empty(),
        "single click consumed once"
    );
    println!("foreground baseline={focus:?} after_single={:?}", unsafe {
        GetForegroundWindow()
    });
    actual_click(connection.process_id(), true);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        actions(&mut connection, "1").await,
        [HostAction::OpenStats {}],
        "actual OS double click emits only statistics"
    );
    tokio::time::sleep(click_deadline()).await;
    assert!(
        actions(&mut connection, "1").await.is_empty(),
        "double click has no delayed mini"
    );
    println!("foreground after_double={:?}", unsafe {
        GetForegroundWindow()
    });
    // Author a pending release on the verified own child, then cross the configuration barrier.
    pending_release(connection.process_id());
    configuration.settings_revision = DecimalInt::parse("2").unwrap();
    configuration.display.layout = DisplayLayout::SingleRow;
    configuration.display.show_costs = false;
    configuration.display.show_quota = false;
    configuration.display.show_weekly_reset = false;
    connection
        .exchange(HostMessage::Configure {
            configuration: configuration.clone(),
        })
        .await
        .unwrap();
    let HostReply::Status { status: changed } = connection
        .exchange(HostMessage::GetStatus {})
        .await
        .unwrap()
    else {
        panic!("status")
    };
    assert_eq!(changed.state, HostDisplayState::WaitingSnapshot);
    assert_eq!(
        inspect_primary_taskbar().unwrap(),
        before,
        "old revision detached before configuration ACK"
    );
    fixture.settings_revision = configuration.settings_revision.clone();
    connection
        .exchange(HostMessage::Snapshot {
            view: Box::new(fixture.clone()),
        })
        .await
        .unwrap();
    tokio::time::sleep(click_deadline()).await;
    assert!(
        actions(&mut connection, "2").await.is_empty(),
        "configuration clears old pending click"
    );
    let HostReply::Status { status: single } = connection
        .exchange(HostMessage::GetStatus {})
        .await
        .unwrap()
    else {
        panic!("status")
    };
    assert_eq!(single.state, HostDisplayState::Embedded);
    let token_only_width = own_readout_width(connection.process_id());
    assert!(
        token_only_width < full_width,
        "token-only preferences use measured shorter slot"
    );
    pending_release(connection.process_id());
    connection
        .exchange(HostMessage::Privacy {
            settings_revision: DecimalInt::parse("3").unwrap(),
            enabled: true,
        })
        .await
        .unwrap();
    tokio::time::sleep(click_deadline()).await;
    assert!(
        actions(&mut connection, "2").await.is_empty(),
        "privacy clears pending intentions before ACK"
    );
    assert_eq!(
        inspect_primary_taskbar().unwrap(),
        before,
        "privacy clears and detaches before ACK"
    );
    fixture.settings_revision = DecimalInt::parse("3").unwrap();
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
    configuration.settings_revision = DecimalInt::parse("4").unwrap();
    configuration.enabled = false;
    connection
        .exchange(HostMessage::Configure { configuration })
        .await
        .unwrap();
    let HostReply::Status { status: disabled } = connection
        .exchange(HostMessage::GetStatus {})
        .await
        .unwrap()
    else {
        panic!("status")
    };
    assert_eq!(disabled.state, HostDisplayState::Disabled);
    assert_eq!(disabled.last_restore, Some(HostRestore::Restored));
    assert_eq!(inspect_primary_taskbar().unwrap(), before);
    assert_eq!(unsafe { GetForegroundWindow() }, focus);
    drop(pointer);
    connection.shutdown().await.unwrap();
    println!(
        "waiting={:?} shown={:?} token_only={:?} disabled={:?} actual_single_double=true revision_privacy_clear=true geometry_restored=true focus_preserved=true cleanup={:?}",
        waiting.state,
        shown.state,
        single.state,
        disabled.state,
        connection.last_cleanup()
    );
}
#[cfg(windows)]
fn own_readout_width(pid: u32) -> i32 {
    own_readout(pid).1
}
#[cfg(windows)]
fn own_readout(pid: u32) -> (windows_sys::Win32::Foundation::HWND, i32) {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, RECT},
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
            WindowsAndMessaging::{
                EnumChildWindows, FindWindowW, GetClassNameW, GetWindowRect,
                GetWindowThreadProcessId,
            },
        },
    };
    struct Probe {
        pid: u32,
        windows: Vec<(HWND, i32)>,
    }
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
        let probe = unsafe { &mut *(parameter as *mut Probe) };
        let mut actual = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut actual);
        }
        if actual == probe.pid {
            let mut class = [0; 128];
            let n = unsafe { GetClassNameW(window, class.as_mut_ptr(), 128) };
            if n > 0
                && String::from_utf16_lossy(&class[..n as usize])
                    .starts_with("TokenPulse.Taskbar.Readout.")
            {
                let mut rect: RECT = unsafe { std::mem::zeroed() };
                assert_ne!(unsafe { GetWindowRect(window, &mut rect) }, 0);
                probe.windows.push((window, rect.right - rect.left));
            }
        }
        1
    }
    let class: Vec<u16> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
    let root = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    assert!(!root.is_null());
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    assert!(!previous.is_null());
    let mut probe = Probe {
        pid,
        windows: vec![],
    };
    unsafe {
        EnumChildWindows(root, Some(collect), (&mut probe as *mut Probe) as LPARAM);
        SetThreadDpiAwarenessContext(previous);
    }
    assert_eq!(probe.windows.len(), 1, "only own embedded readout geometry");
    probe.windows[0]
}
#[cfg(windows)]
fn click_deadline() -> std::time::Duration {
    std::time::Duration::from_millis(
        u64::from(unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() })
            + 100,
    )
}
#[cfg(windows)]
async fn actions(
    connection: &mut token_pulse_taskbar::windows::transport::HostConnection,
    revision: &str,
) -> Vec<token_pulse_taskbar::HostAction> {
    let token_pulse_taskbar::HostReply::Actions {
        settings_revision,
        actions,
    } = connection
        .exchange(token_pulse_taskbar::HostMessage::GetActions {})
        .await
        .unwrap()
    else {
        panic!("actual action reply")
    };
    assert_eq!(settings_revision.unwrap().as_str(), revision);
    actions
}
#[cfg(windows)]
fn pending_release(pid: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_LBUTTONUP,
    };
    let window = own_readout(pid).0;
    assert_ne!(
        unsafe {
            SendMessageTimeoutW(
                window,
                WM_LBUTTONUP,
                0,
                0,
                SMTO_ABORTIFHUNG,
                1000,
                std::ptr::null_mut(),
            )
        },
        0
    );
}
#[cfg(windows)]
struct PointerRestore(windows_sys::Win32::Foundation::POINT);
#[cfg(windows)]
impl PointerRestore {
    fn save() -> Self {
        let mut point = Default::default();
        assert_ne!(
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point) },
            0
        );
        Self(point)
    }
}
#[cfg(windows)]
impl Drop for PointerRestore {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos(self.0.x, self.0.y);
        }
    }
}
#[cfg(windows)]
fn actual_click(pid: u32, double: bool) {
    use windows_sys::Win32::{
        Foundation::{POINT, RECT},
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
                SendInput,
            },
            WindowsAndMessaging::{
                GetClassNameW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
                SetCursorPos, WindowFromPoint,
            },
        },
    };
    let window = own_readout(pid).0;
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    assert!(!previous.is_null());
    let mut rect: RECT = Default::default();
    assert_ne!(unsafe { GetWindowRect(window, &mut rect) }, 0);
    let center = POINT {
        x: rect.left + (rect.right - rect.left) / 2,
        y: rect.top + (rect.bottom - rect.top) / 2,
    };
    assert_ne!(unsafe { SetCursorPos(center.x, center.y) }, 0);
    // Newly reparented cross-process children may precede desktop hit-test publication.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while unsafe { WindowFromPoint(center) } != window && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let hit = unsafe { WindowFromPoint(center) };
    if hit != window {
        let mut class = [0; 128];
        let n = unsafe { GetClassNameW(hit, class.as_mut_ptr(), 128) };
        eprintln!(
            "refused own rect=({}, {}, {}, {}) center=({}, {}) hit_class={}",
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            center.x,
            center.y,
            String::from_utf16_lossy(&class[..n.max(0) as usize])
        );
        let mut title = [0; 256];
        let n = unsafe { GetWindowTextW(hit, title.as_mut_ptr(), 256) };
        let mut process = 0;
        let mut actual: RECT = Default::default();
        unsafe {
            GetWindowThreadProcessId(hit, &mut process);
            GetWindowRect(hit, &mut actual);
        }
        eprintln!(
            "hit title={} pid={} rect=({}, {}, {}, {})",
            String::from_utf16_lossy(&title[..n.max(0) as usize]),
            process,
            actual.left,
            actual.top,
            actual.right,
            actual.bottom
        );
    }
    assert_eq!(
        unsafe { WindowFromPoint(center) },
        window,
        "refuse mouse input outside own readout"
    );
    let input: Vec<INPUT> = (0..if double { 4 } else { 2 })
        .map(|n| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: if n % 2 == 0 {
                        MOUSEEVENTF_LEFTDOWN
                    } else {
                        MOUSEEVENTF_LEFTUP
                    },
                    ..Default::default()
                },
            },
        })
        .collect();
    assert_eq!(
        unsafe {
            SendInput(
                input.len() as u32,
                input.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        },
        input.len() as u32
    );
    unsafe {
        SetThreadDpiAwarenessContext(previous);
    }
}
#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
