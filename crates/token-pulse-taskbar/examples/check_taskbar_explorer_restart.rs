//! Explicit normal Explorer process restart. Never invoked by normal tests or the product.
#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use std::{os::windows::process::CommandExt, path::PathBuf, process::Stdio, time::Duration};
    use token_pulse_core::numeric::DecimalInt;
    use token_pulse_taskbar::{
        HostConfiguration, HostDisplayState, HostMessage, HostReply, HostRestore, TaskbarView,
        windows::{topology::inspect_primary_taskbar, transport::HostConnection},
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args != ["--native-taskbar-explorer-restart-development-check"] {
        std::process::exit(2);
    }
    println!(
        "DEVELOPMENT ONLY: normal registered Explorer shutdown/restart; synthetic host, no computer reboot or forced termination"
    );
    let scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts");
    let powershell =
        std::env::var_os("TOKENPULSE_ACCEPTANCE_PWSH").unwrap_or_else(|| "pwsh.exe".into());
    let old_shell = shell_pid();
    inspect_primary_taskbar().expect("supported baseline");
    let inspection = std::process::Command::new(&powershell)
        .args(["-NoProfile", "-File"])
        .arg(scripts.join("restart-explorer-for-acceptance.ps1"))
        .args(["-BaselineShellPid", &old_shell.to_string()])
        .stdin(Stdio::null())
        .creation_flags(0x08000000)
        .output()
        .expect("read-only eligibility process");
    assert!(inspection.status.success(), "read-only inspection failed");
    let eligibility: serde_json::Value =
        serde_json::from_slice(&inspection.stdout).expect("inspection JSON");
    assert_eq!(
        eligibility["Eligible"], true,
        "Explorer eligibility: {eligibility}"
    );
    let host_file = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("token-pulse-taskbar-host.exe");
    let mut connection = HostConnection::launch(&host_file)
        .await
        .expect("own production host");
    let host_pid = connection.process_id();
    let mut fixture: TaskbarView =
        serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap();
    fixture.settings_revision = DecimalInt::parse("1").unwrap();
    let mut configuration = HostConfiguration {
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
        .exchange(HostMessage::Configure {
            configuration: configuration.clone(),
        })
        .await
        .unwrap();
    connection
        .exchange(HostMessage::Snapshot {
            view: Box::new(fixture.clone()),
        })
        .await
        .unwrap();
    let initial = status(&mut connection).await;
    assert_eq!(initial.state, HostDisplayState::Embedded);
    let old_readout = readout_class(host_pid);
    let driver = tokio::task::spawn_blocking(move || {
        std::process::Command::new(powershell)
            .args(["-NoProfile", "-File"])
            .arg(scripts.join("restart-explorer-for-acceptance.ps1"))
            .args(["-BaselineShellPid", &old_shell.to_string(), "-RestartShell"])
            .stdin(Stdio::null())
            .creation_flags(0x08000000)
            .output()
    });
    // Keep the same host alive across the real shell absence, without manufacturing
    // TaskbarCreated or killing/relaunching it to make the recovery assertion pass.
    while !driver.is_finished() {
        connection
            .exchange(HostMessage::Heartbeat {})
            .await
            .expect("same host heartbeat during real restart");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let restarted = driver.await.unwrap().expect("normal restart driver");
    println!(
        "EXPLORER_RESTART_DRIVER: {}",
        String::from_utf8_lossy(&restarted.stdout).trim()
    );
    assert!(
        restarted.status.success(),
        "normal restart failed: {}",
        String::from_utf8_lossy(&restarted.stderr)
    );
    let evidence: serde_json::Value =
        serde_json::from_slice(&restarted.stdout).expect("restart JSON");
    assert_eq!(evidence["OldPid"], old_shell);
    assert_eq!(evidence["OldProcessExited"], true);
    assert_eq!(evidence["ShutdownStatus"], 0);
    assert_eq!(evidence["RestartStatus"], 0);
    assert_eq!(evidence["ForceShutdown"], false);
    assert_eq!(evidence["ComputerRestart"], false);
    let new_shell = shell_pid();
    assert_ne!(new_shell, old_shell);
    assert_eq!(evidence["NewPid"], new_shell);
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let restored = loop {
        let observed = status(&mut connection).await;
        if observed.state == HostDisplayState::Embedded {
            break observed;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "new shell embedding deadline: {observed:?}"
        );
        connection
            .exchange(HostMessage::Snapshot {
                view: Box::new(fixture.clone()),
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert_eq!(connection.process_id(), host_pid);
    assert!(restored.system_revision.value() > initial.system_revision.value());
    assert_eq!(restored.settings_revision, initial.settings_revision);
    assert_ne!(
        readout_class(host_pid),
        old_readout,
        "new canvas generation required"
    );
    match connection
        .exchange(HostMessage::GetActions {})
        .await
        .unwrap()
    {
        HostReply::Actions { actions, .. } => assert!(actions.is_empty(), "no stale shell action"),
        _ => panic!("actions reply"),
    }
    configuration.enabled = false;
    configuration.settings_revision = DecimalInt::parse("2").unwrap();
    connection
        .exchange(HostMessage::Configure {
            configuration: configuration.clone(),
        })
        .await
        .unwrap();
    let disabled = status(&mut connection).await;
    assert_eq!(disabled.state, HostDisplayState::Disabled);
    assert_eq!(disabled.last_restore, Some(HostRestore::Restored));
    let new_baseline = inspect_primary_taskbar().expect("independent new shell geometry");
    configuration.enabled = true;
    configuration.settings_revision = DecimalInt::parse("3").unwrap();
    fixture.settings_revision = configuration.settings_revision.clone();
    connection
        .exchange(HostMessage::Configure { configuration })
        .await
        .unwrap();
    connection
        .exchange(HostMessage::Snapshot {
            view: Box::new(fixture),
        })
        .await
        .unwrap();
    assert_eq!(
        status(&mut connection).await.state,
        HostDisplayState::Embedded
    );
    connection.shutdown().await.unwrap();
    assert_eq!(
        inspect_primary_taskbar().unwrap(),
        new_baseline,
        "normal cleanup restores new shell, not stale old geometry"
    );
    println!(
        "NATIVE_EXPLORER_RESTART_OK: old_shell={old_shell} new_shell={new_shell} same_host={host_pid} new_canvas=true revision_advanced=true stale_actions_empty=true new_geometry_restored=true computer_reboot=false cleanup={:?}",
        connection.last_cleanup()
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
fn shell_pid() -> u32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};
    let name: Vec<_> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
    let window = unsafe { FindWindowW(name.as_ptr(), std::ptr::null()) };
    assert!(!window.is_null(), "primary shell required");
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    assert_ne!(pid, 0);
    pid
}

#[cfg(windows)]
fn readout_class(pid: u32) -> String {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumChildWindows, FindWindowW, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
        },
    };
    struct Search {
        pid: u32,
        found: Vec<String>,
    }
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
        let state = unsafe { &mut *(parameter as *mut Search) };
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(window, &mut owner) };
        if owner == state.pid && unsafe { IsWindowVisible(window) } != 0 {
            let mut buffer = [0u16; 128];
            let length = unsafe { GetClassNameW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
            if length > 0 {
                let class = String::from_utf16_lossy(&buffer[..length as usize]);
                if class.starts_with("TokenPulse.Taskbar.Readout.") {
                    state.found.push(class);
                }
            }
        }
        1
    }
    let name: Vec<_> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
    let root = unsafe { FindWindowW(name.as_ptr(), std::ptr::null()) };
    assert!(!root.is_null());
    let mut search = Search {
        pid,
        found: Vec::new(),
    };
    unsafe { EnumChildWindows(root, Some(collect), &mut search as *mut Search as LPARAM) };
    assert_eq!(search.found.len(), 1, "unique owned embedded readout");
    search.found.remove(0)
}

#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
