//! Explicit real system preference transition. Normal tests never change taskbar settings.
#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use std::time::{Duration, Instant};
    use token_pulse_core::numeric::DecimalInt;
    use token_pulse_taskbar::windows::{
        topology::inspect_primary_taskbar, transport::HostConnection,
    };
    use token_pulse_taskbar::{
        HostConfiguration, HostDisplayState, HostMessage, HostReply, TaskbarView,
    };
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.first().map(String::as_str) != Some("--native-taskbar-autohide-development-check")
        || arguments.len() > 3
        || arguments.iter().skip(1).any(|argument| {
            argument != "--application-right" && argument != "--own-setting-refresh"
        })
        || arguments
            .iter()
            .skip(1)
            .any(|argument| arguments.iter().filter(|other| *other == argument).count() != 1)
    {
        std::process::exit(2);
    }
    let position = if arguments
        .iter()
        .any(|argument| argument == "--application-right")
    {
        token_pulse_core::taskbar::TaskbarPosition::ApplicationRight
    } else {
        token_pulse_core::taskbar::TaskbarPosition::NotificationLeft
    };
    println!(
        "DEVELOPMENT ONLY: actual auto-hide preference with restoration, synthetic view; no input, Explorer termination or computer restart"
    );
    let baseline = inspect_primary_taskbar().expect("supported Win10 baseline");
    let mut shell = system::Preference::capture().expect("verified primary shell");
    assert_eq!(
        shell.original(),
        0,
        "this bounded scene requires initially non-auto-hidden taskbar; no mutation"
    );
    let host = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("token-pulse-taskbar-host.exe");
    let mut connection = HostConnection::launch(&host)
        .await
        .expect("same-profile production host");
    let host_pid = connection.process_id();
    let mut view: TaskbarView =
        serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap();
    view.settings_revision = DecimalInt::parse("1").unwrap();
    let mut configuration = HostConfiguration {
        settings_revision: view.settings_revision.clone(),
        enabled: true,
        position,
        display: Default::default(),
    };
    connection
        .exchange(HostMessage::Privacy {
            settings_revision: view.settings_revision.clone(),
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
    connection
        .exchange(HostMessage::Snapshot {
            view: Box::new(view.clone()),
        })
        .await
        .unwrap();
    let initial_deadline = Instant::now() + Duration::from_secs(5);
    while status(&mut connection).await.state != HostDisplayState::Embedded {
        assert!(
            Instant::now() < initial_deadline,
            "initial measured embedding failed before any system mutation"
        );
        connection
            .exchange(HostMessage::Snapshot {
                view: Box::new(view.clone()),
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    shell
        .enable()
        .expect("normal ABM_SETSTATE, guarded restoration now armed");
    let deadline = Instant::now() + Duration::from_secs(10);
    let hidden_bounds = loop {
        assert_eq!(
            shell.state().unwrap(),
            1,
            "actual system preference readback"
        );
        let bounds = shell.bounds().unwrap();
        // A preference bit alone is not proof that Explorer actually hid its taskbar.
        if bounds.0 >= baseline.taskbar.bottom - 4 && bounds.1 > baseline.taskbar.bottom {
            break bounds;
        }
        connection
            .exchange(HostMessage::Snapshot {
                view: Box::new(view.clone()),
            })
            .await
            .expect("host remains responsive during actual transition");
        let _ = status(&mut connection).await;
        assert!(
            Instant::now() < deadline,
            "auto-hide preference enabled but actual shell motion not observed: top={} bottom={}",
            bounds.0,
            bounds.1
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let hidden_status = status(&mut connection).await;
    // Preserve the actual failure reason and observed physical motion before asserting.
    // This opt-in probe must not lose evidence merely because embedding was rejected.
    println!(
        "ACTUAL_AUTOHIDE_HIDDEN_STATUS: top={} bottom={} status={hidden_status:?} same_host={host_pid}",
        hidden_bounds.0, hidden_bounds.1
    );
    if hidden_status.state != HostDisplayState::Embedded {
        dump_owned_visibility(host_pid);
        // Inspect ordinary fresh-snapshot retries without replacing the failed first assertion.
        // This distinguishes transient animation from an unavailable host throughout hiding.
        for attempt in 1..=5 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            connection
                .exchange(HostMessage::Snapshot {
                    view: Box::new(view.clone()),
                })
                .await
                .expect("bounded hidden failure diagnostic remains responsive");
            let retry = status(&mut connection).await;
            println!(
                "ACTUAL_AUTOHIDE_FAILURE_RETRY: attempt={attempt} bounds={:?} status={retry:?} same_host={host_pid}",
                shell.bounds()
            );
        }
    }
    assert_eq!(
        hidden_status.state,
        HostDisplayState::Embedded,
        "normal hidden translation must retain embedding"
    );
    assert!(hidden_status.failure.is_none());
    println!(
        "ACTUAL_AUTOHIDE_HIDDEN: top={} bottom={} state={:?} failure={:?} same_host={host_pid}",
        hidden_bounds.0, hidden_bounds.1, hidden_status.state, hidden_status.failure
    );
    connection
        .exchange(HostMessage::Snapshot {
            view: Box::new(view.clone()),
        })
        .await
        .expect("hidden valid DC does not close host pipe");
    let hidden_refresh = status(&mut connection).await;
    println!("ACTUAL_AUTOHIDE_REFRESH: {hidden_refresh:?}");
    if hidden_refresh.state != HostDisplayState::Embedded {
        dump_owned_visibility(host_pid);
    }
    assert_eq!(
        hidden_refresh.state,
        HostDisplayState::Embedded,
        "a fresh hidden snapshot retains valid embedding"
    );
    if arguments
        .iter()
        .any(|argument| argument == "--own-setting-refresh")
    {
        let previous_revision = hidden_refresh.system_revision;
        own_setting_refresh(host_pid);
        let deadline = Instant::now() + Duration::from_secs(10);
        let refreshed = loop {
            let observed = status(&mut connection).await;
            if observed.system_revision != previous_revision {
                break observed;
            }
            assert!(
                Instant::now() < deadline,
                "own setting message was not processed"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        println!(
            "OWN_SETTING_REFRESH_WHILE_HIDDEN: status={refreshed:?} bounds={:?} same_host={host_pid} authored_message=true",
            shell.bounds()
        );
        if refreshed.state != HostDisplayState::Embedded {
            dump_owned_visibility(host_pid);
        }
        assert_eq!(refreshed.state, HostDisplayState::Embedded);
        assert!(refreshed.failure.is_none());
        connection
            .exchange(HostMessage::Snapshot {
                view: Box::new(view.clone()),
            })
            .await
            .unwrap();
        assert_eq!(
            status(&mut connection).await.state,
            HostDisplayState::Embedded
        );
    }
    shell
        .restore()
        .expect("restore original system preference before recovery assertions");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let bounds = shell.bounds().unwrap();
        connection
            .exchange(HostMessage::Snapshot {
                view: Box::new(view.clone()),
            })
            .await
            .unwrap();
        let observed = status(&mut connection).await;
        if bounds == (baseline.taskbar.top, baseline.taskbar.bottom)
            && observed.state == HostDisplayState::Embedded
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "visible taskbar failed to recover: {observed:?}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(connection.process_id(), host_pid);
    match connection
        .exchange(HostMessage::GetActions {})
        .await
        .unwrap()
    {
        HostReply::Actions { actions, .. } => assert!(actions.is_empty()),
        _ => panic!("actions reply"),
    }
    configuration.enabled = false;
    configuration.settings_revision = DecimalInt::parse("2").unwrap();
    connection
        .exchange(HostMessage::Configure { configuration })
        .await
        .unwrap();
    assert_eq!(
        status(&mut connection).await.state,
        HostDisplayState::Disabled
    );
    connection.shutdown().await.unwrap();
    assert_eq!(shell.state().unwrap(), shell.original());
    assert_eq!(
        inspect_primary_taskbar().unwrap(),
        baseline,
        "system setting and Explorer geometry both restored"
    );
    println!(
        "NATIVE_TASKBAR_AUTOHIDE_OK: position={position:?} actual_hide_motion=true same_host={host_pid} hidden_refresh_embedded=true visible_reembedded=true actions_empty=true setting_restored=true original_geometry_restored=true computer_reboot=false"
    );
}

#[cfg(windows)]
fn dump_owned_visibility(host_pid: u32) {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, RECT},
        UI::WindowsAndMessaging::{
            EnumChildWindows, FindWindowW, GWL_STYLE, GetClassNameW, GetParent, GetWindowLongPtrW,
            GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, WS_VISIBLE,
        },
    };
    struct Probe {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
        let probe = unsafe { &mut *(parameter as *mut Probe) };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut pid);
        }
        if pid == probe.pid {
            let mut class = [0u16; 128];
            let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), 128) };
            if length > 0
                && length < 127
                && String::from_utf16_lossy(&class[..length as usize])
                    .starts_with("TokenPulse.Taskbar.Readout.")
            {
                probe.windows.push(window);
            }
        }
        1
    }
    let class: Vec<_> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
    let root = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    if root.is_null() {
        println!("ACTUAL_AUTOHIDE_VISIBILITY: shell_missing=true");
        return;
    }
    let mut probe = Probe {
        pid: host_pid,
        windows: Vec::new(),
    };
    unsafe {
        EnumChildWindows(root, Some(collect), (&mut probe as *mut Probe) as LPARAM);
    }
    println!(
        "ACTUAL_AUTOHIDE_VISIBILITY: root_visible={} root_style_visible={} owned_readouts={}",
        unsafe { IsWindowVisible(root) },
        unsafe { GetWindowLongPtrW(root, GWL_STYLE) } as u32 & WS_VISIBLE != 0,
        probe.windows.len()
    );
    for window in probe.windows {
        let mut bounds: RECT = unsafe { std::mem::zeroed() };
        let rect_ok = unsafe { GetWindowRect(window, &mut bounds) } != 0;
        println!(
            "ACTUAL_AUTOHIDE_OWN_READOUT: visible={} style_visible={} parent_is_root={} rect_ok={rect_ok} bounds=({},{},{},{})",
            unsafe { IsWindowVisible(window) },
            unsafe { GetWindowLongPtrW(window, GWL_STYLE) } as u32 & WS_VISIBLE != 0,
            unsafe { GetParent(window) } == root,
            bounds.left,
            bounds.top,
            bounds.right,
            bounds.bottom
        );
    }
}

#[cfg(windows)]
fn own_setting_refresh(host_pid: u32) {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowThreadProcessId, SMTO_ABORTIFHUNG, SMTO_BLOCK,
            SendMessageTimeoutW, WM_SETTINGCHANGE,
        },
    };
    struct Probe {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
        let probe = unsafe { &mut *(parameter as *mut Probe) };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut pid);
        }
        if pid == probe.pid {
            let mut class = [0u16; 128];
            let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), 128) };
            if length > 0
                && length < 127
                && String::from_utf16_lossy(&class[..length as usize])
                    .starts_with("TokenPulse.Taskbar.Control.")
            {
                probe.windows.push(window);
            }
        }
        1
    }
    let mut probe = Probe {
        pid: host_pid,
        windows: Vec::new(),
    };
    assert_ne!(
        unsafe { EnumWindows(Some(collect), (&mut probe as *mut Probe) as LPARAM) },
        0
    );
    assert_eq!(probe.windows.len(), 1, "exactly one owned control window");
    let control = probe.windows[0];
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(control, &mut pid);
    }
    assert_eq!(pid, host_pid);
    // No broadcast, Explorer message, pointer payload or physical input.
    let mut result = 0;
    assert_ne!(
        unsafe {
            SendMessageTimeoutW(
                control,
                WM_SETTINGCHANGE,
                0,
                0,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                2000,
                &mut result,
            )
        },
        0,
        "bounded synchronous own setting message"
    );
}

#[cfg(windows)]
async fn status(
    connection: &mut token_pulse_taskbar::windows::transport::HostConnection,
) -> token_pulse_taskbar::HostStatus {
    match connection
        .exchange(token_pulse_taskbar::HostMessage::GetStatus {})
        .await
        .unwrap()
    {
        token_pulse_taskbar::HostReply::Status { status } => status,
        _ => panic!("status reply"),
    }
}

#[cfg(windows)]
mod system {
    use std::{mem, ptr};
    use windows::Win32::{
        Foundation::{HWND as Window, LPARAM},
        UI::Shell::{ABM_GETSTATE, ABM_SETSTATE, APPBARDATA, SHAppBarMessage},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, RECT, WAIT_TIMEOUT},
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            WaitForSingleObject,
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
            WindowsAndMessaging::{
                FindWindowW, GetClassNameW, GetWindowRect, GetWindowThreadProcessId,
            },
        },
    };

    pub struct Preference {
        root: HWND,
        pid: u32,
        process: HANDLE,
        dpi: HANDLE,
        original: u32,
        changed: bool,
    }
    impl Preference {
        pub fn capture() -> Result<Self, &'static str> {
            let dpi =
                unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
            if dpi.is_null() {
                return Err("physical coordinate context unavailable");
            }
            let mut preference = Self {
                root: ptr::null_mut(),
                pid: 0,
                process: ptr::null_mut(),
                dpi,
                original: 0,
                changed: false,
            };
            let class: Vec<_> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
            let root = unsafe { FindWindowW(class.as_ptr(), ptr::null()) };
            let mut pid = 0;
            if root.is_null() || unsafe { GetWindowThreadProcessId(root, &mut pid) } == 0 {
                return Err("primary shell missing");
            }
            let process = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    pid,
                )
            };
            if process.is_null() {
                return Err("shell kernel identity unavailable");
            }
            preference.root = root;
            preference.pid = pid;
            preference.process = process;
            preference.original = preference.state()?;
            if preference.original > 3 {
                return Err("unknown system preference bits");
            }
            Ok(preference)
        }
        fn verify(&self) -> Result<(), &'static str> {
            let mut pid = 0;
            let mut class = [0u16; 128];
            let n = unsafe { GetClassNameW(self.root, class.as_mut_ptr(), 128) };
            if unsafe { WaitForSingleObject(self.process, 0) } != WAIT_TIMEOUT
                || unsafe { GetWindowThreadProcessId(self.root, &mut pid) } == 0
                || pid != self.pid
                || n <= 0
                || String::from_utf16_lossy(&class[..n as usize]) != "Shell_TrayWnd"
            {
                return Err("captured primary shell generation changed");
            }
            Ok(())
        }
        pub fn original(&self) -> u32 {
            self.original
        }
        pub fn state(&self) -> Result<u32, &'static str> {
            self.verify()?;
            let mut data = APPBARDATA {
                cbSize: mem::size_of::<APPBARDATA>() as u32,
                ..Default::default()
            };
            Ok(unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) } as u32)
        }
        fn set(&self, state: u32) -> Result<(), &'static str> {
            self.verify()?;
            let mut data = APPBARDATA {
                cbSize: mem::size_of::<APPBARDATA>() as u32,
                hWnd: Window(self.root),
                lParam: LPARAM(state as isize),
                ..Default::default()
            };
            unsafe {
                SHAppBarMessage(ABM_SETSTATE, &mut data);
            }
            // ABM_SETSTATE always returns TRUE. Only real readback proves the setting.
            if self.state()? != state {
                return Err("system preference readback mismatch");
            }
            Ok(())
        }
        pub fn enable(&mut self) -> Result<(), &'static str> {
            if self.state()? != self.original || self.original != 0 {
                return Err("baseline preference changed; no mutation");
            }
            self.changed = true;
            self.set(1)
        }
        pub fn bounds(&self) -> Result<(i32, i32), &'static str> {
            self.verify()?;
            let mut bounds: RECT = unsafe { mem::zeroed() };
            if unsafe { GetWindowRect(self.root, &mut bounds) } == 0 {
                return Err("primary geometry unavailable");
            }
            Ok((bounds.top, bounds.bottom))
        }
        pub fn restore(&mut self) -> Result<(), &'static str> {
            if !self.changed {
                return Ok(());
            }
            let current = self.state()?;
            if current != 1 && current != self.original {
                return Err("external preference change; restoration refused");
            }
            if current != self.original {
                self.set(self.original)?;
            }
            self.changed = false;
            Ok(())
        }
    }
    impl Drop for Preference {
        fn drop(&mut self) {
            if self.restore().is_err() {
                eprintln!(
                    "AUTOHIDE_RESTORATION_FAILED: shell identity or system preference changed"
                );
            }
            unsafe {
                if !self.process.is_null() {
                    CloseHandle(self.process);
                }
                SetThreadDpiAwarenessContext(self.dpi);
            }
        }
    }
}

#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
