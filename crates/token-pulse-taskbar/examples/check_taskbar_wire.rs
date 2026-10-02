//! Explicit production-wire/native-host check. Synthetic fixture only; never run by default CI.
#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use token_pulse_core::numeric::DecimalInt;
    use token_pulse_taskbar::{
        HostConfiguration, HostDisplayState, HostMessage, HostReply, HostRestore, TaskbarView,
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
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
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
    connection
        .exchange(HostMessage::Privacy {
            settings_revision: DecimalInt::parse("3").unwrap(),
            enabled: true,
        })
        .await
        .unwrap();
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
    connection.shutdown().await.unwrap();
    println!(
        "waiting={:?} shown={:?} token_only={:?} disabled={:?} geometry_restored=true focus_preserved=true cleanup={:?}",
        waiting.state,
        shown.state,
        single.state,
        disabled.state,
        connection.last_cleanup()
    );
}
#[cfg(windows)]
fn own_readout_width(pid: u32) -> i32 {
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
        widths: Vec<i32>,
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
                probe.widths.push(rect.right - rect.left);
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
        widths: vec![],
    };
    unsafe {
        EnumChildWindows(root, Some(collect), (&mut probe as *mut Probe) as LPARAM);
        SetThreadDpiAwarenessContext(previous);
    }
    assert_eq!(probe.widths.len(), 1, "only own embedded readout geometry");
    probe.widths[0]
}
#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
