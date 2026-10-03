//! Explicit debug-only whole-app acceptance across a real normal Explorer restart.
//! Uses the production taskbar actor and WebView IPC, with an isolated empty database.
use std::{
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;
use token_pulse_core::{
    numeric::DecimalInt,
    taskbar::{TaskbarCleanupOutcome, TaskbarRuntimeSnapshot, TaskbarRuntimeState},
};
use token_pulse_taskbar::windows::topology::inspect_primary_taskbar;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    },
    UI::WindowsAndMessaging::{
        FindWindowW, GetClassNameW, GetWindowThreadProcessId, WM_KILLFOCUS, WM_SETFOCUS,
    },
};

pub(super) fn start(app: tauri::AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        let result = verify(&app);
        if let Err(error) = &result {
            eprintln!("NATIVE_TASKBAR_EXPLORER_APP_FAILED: {error}");
        }
        // Always stop our actor before exit, including any driver/verification failure.
        // The normal restart driver itself restores Explorer in its finally block.
        if let Some(service) = super::taskbar_commands::service(&app) {
            service.shutdown();
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}

fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments != ["--native-smoke", "--native-taskbar-explorer-restart-smoke"] {
        return Err("actual Explorer acceptance requires its exclusive explicit scene".into());
    }
    let runtime = app.state::<super::RuntimeState>();
    if !runtime
        .data_directory
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("native-probe-"))
    {
        return Err("actual Explorer acceptance requires an isolated database".into());
    }
    let scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("repository directory missing")?
        .join("scripts");
    let powershell =
        std::env::var_os("TOKENPULSE_ACCEPTANCE_PWSH").unwrap_or_else(|| "pwsh.exe".into());
    let old_shell = shell_pid()?;
    let mut driver = Command::new(powershell);
    driver
        .args(["-NoProfile", "-File"])
        .arg(scripts.join("restart-explorer-for-acceptance.ps1"))
        .args(["-BaselineShellPid", &old_shell.to_string()])
        .stdin(Stdio::null())
        .creation_flags(0x08000000);
    let inspection = driver.output().map_err(|error| error.to_string())?;
    if !inspection.status.success() {
        return Err("readonly Explorer qualification failed".into());
    }
    let qualification: serde_json::Value =
        serde_json::from_slice(&inspection.stdout).map_err(|error| error.to_string())?;
    if qualification["ShellPid"] != old_shell || qualification["Eligible"] != true {
        return Err(format!(
            "normal Explorer restart not eligible: {}",
            qualification["Reason"]
        ));
    }
    println!(
        "NATIVE_TASKBAR_EXPLORER_APP_START: isolated SQLite, real WebView/actor/host; normal registered Explorer restart, no input/force/computer reboot"
    );
    inspect_primary_taskbar().map_err(|error| format!("initial topology: {error:?}"))?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    mini.hide().map_err(|error| error.to_string())?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
        const current=await invoke('get_taskbar_preferences',{requestId:'explorer-app-enable-read'});
        await invoke('set_taskbar_preferences',{requestId:'explorer-app-enable',request:{preferences:{...current.data.preferences,enabled:true,fallback_to_mini:false,position:'notification_left'},expected_settings_revision:current.data.settings_revision}});
        "#,
    )?;
    super::taskbar_smoke::wait_actions_ready(app)?;
    let service = super::taskbar_commands::service(app).ok_or("taskbar service missing")?;
    let host_pid = service.owned_host_pid().ok_or("host missing")?;
    let host_process = HeldHost::open(host_pid)?;
    let before = service.snapshot().map_err(|error| error.to_string())?;
    let old_class = readout_class(app)?;
    let navigation_before = runtime
        .main_navigation
        .lock()
        .map_err(|_| "navigation lock")?
        .revision
        .clone();
    super::taskbar_smoke::own_readout_message(app, WM_SETFOCUS, 0, 0)?;
    let (old_details, visible, _) = super::taskbar_smoke::own_details(app)?;
    if !visible {
        return Err("old details not visible before restart".into());
    }
    let old_details_class = class(old_details)?;

    // The normal production actor continues its own heartbeat/status/snapshot loop.
    // Do not touch its pipe, manufacture TaskbarCreated or retry/relaunch its host.
    driver.arg("-RestartShell");
    let restarted = driver.output().map_err(|error| error.to_string())?;
    println!(
        "EXPLORER_APP_RESTART_DRIVER: {}",
        String::from_utf8_lossy(&restarted.stdout).trim()
    );
    if !restarted.status.success() {
        return Err(format!(
            "normal Explorer driver failed: {}",
            String::from_utf8_lossy(&restarted.stderr)
        ));
    }
    let evidence: serde_json::Value =
        serde_json::from_slice(&restarted.stdout).map_err(|error| error.to_string())?;
    let new_shell = shell_pid()?;
    if new_shell == old_shell
        || evidence["OldPid"] != old_shell
        || evidence["NewPid"] != new_shell
        || evidence["OldProcessExited"] != true
        || evidence["ShutdownStatus"] != 0
        || evidence["RestartStatus"] != 0
        || evidence["ForceShutdown"] != false
        || evidence["ComputerRestart"] != false
    {
        return Err("actual normal restart evidence incomplete".into());
    }
    let fresh_after_ms =
        token_pulse_collector::jobs::now_ms().map_err(|error| error.to_string())?;
    let after = wait_recovered(
        app,
        host_pid,
        &old_class,
        &old_details_class,
        &before.applied_settings_revision,
        fresh_after_ms,
    )?;
    host_process.require_live()?;
    if service.owned_host_pid() != Some(host_pid)
        || readout_class(app)? == old_class
        || after.applied_settings_revision != before.applied_settings_revision
        || after.error.is_some()
        || after.action_error.is_some()
    {
        return Err(format!("whole-app host recovery mismatch: {after:?}"));
    }
    let (new_details, visible, _) = super::taskbar_smoke::own_details(app)?;
    if visible || class(new_details)? == old_details_class {
        return Err("old details generation survived actual Explorer restart".into());
    }
    if mini.is_visible().map_err(|error| error.to_string())?
        || runtime
            .main_navigation
            .lock()
            .map_err(|_| "navigation lock")?
            .revision
            != navigation_before
    {
        return Err("unexpected window intention after actual Explorer restart".into());
    }
    let expected_revision = serde_json::to_string(&before.applied_settings_revision)
        .map_err(|error| error.to_string())?;
    super::mini_smoke::evaluate(
        app,
        &main,
        &format!(
            r#"
            const status=await invoke('get_taskbar_status',{{requestId:'explorer-app-recovered-status'}});
            if(status.data.state!=='embedded'||status.data.applied_settings_revision!=={expected_revision}||status.data.issue!==null||status.data.error!==null||status.data.action_error!==null)throw new Error('WHOLE_APP_RECOVERED_STATUS_MISMATCH');
            const display=await invoke('get_display_settings',{{requestId:'explorer-app-privacy-read'}});
            await invoke('set_display_privacy',{{requestId:'explorer-app-privacy-save',request:{{privacy:true,expected_settings_revision:display.data.settings_revision}}}});
            "#
        ),
    )?;
    super::taskbar_smoke::wait_actions_ready(app)?;
    host_process.require_live()?;
    if service.owned_host_pid() != Some(host_pid) {
        return Err("privacy after restart replaced the healthy host".into());
    }
    if super::taskbar_smoke::own_details(app)?.1 {
        return Err("post-restart privacy barrier did not hide old details".into());
    }
    super::taskbar_smoke::own_readout_message(app, WM_SETFOCUS, 0, 0)?;
    let (_, visible, text) = super::taskbar_smoke::own_details(app)?;
    if !visible
        || !text.contains("隐私模式")
        || text.contains("已计价部分估算")
        || text.contains("额度桶")
        || text.contains("剩余")
    {
        return Err("new shell did not apply post-restart private details".into());
    }
    super::taskbar_smoke::own_readout_message(app, WM_KILLFOCUS, 0, 0)?;
    change_enabled(app, &main, false)?;
    wait_disabled(app)?;
    host_process.require_exited()?;
    let new_baseline = settled_topology(new_shell)?;
    change_enabled(app, &main, true)?;
    super::taskbar_smoke::wait_actions_ready(app)?;
    // Disabling intentionally shuts down the actor's host; enabling launches a new one.
    // Same-host continuity is required across Explorer restart, not across this disable.
    let reenabled_host = service.owned_host_pid().ok_or("reenabled host missing")?;
    service.shutdown();
    let restored = settled_topology(new_shell)?;
    if restored != new_baseline {
        return Err(format!(
            "whole-app shutdown geometry mismatch: baseline={new_baseline:?} actual={restored:?}"
        ));
    }
    println!(
        "NATIVE_TASKBAR_EXPLORER_APP_OK: old_shell={old_shell} new_shell={new_shell} same_host={host_pid} reenabled_host={reenabled_host} new_canvas=true new_details=true production_actor_recovered=true webview_status=true post_restart_privacy=true no_unexpected_actions=true new_geometry_restored=true computer_reboot=false"
    );
    Ok(())
}

fn change_enabled(
    app: &tauri::AppHandle,
    main: &tauri::WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    super::mini_smoke::evaluate(
        app,
        main,
        &format!(
            r#"
            const current=await invoke('get_taskbar_preferences',{{requestId:'explorer-app-enabled-read'}});
            await invoke('set_taskbar_preferences',{{requestId:'explorer-app-enabled-save',request:{{preferences:{{...current.data.preferences,enabled:{enabled}}},expected_settings_revision:current.data.settings_revision}}}});
            "#
        ),
    )
}

fn wait_recovered(
    app: &tauri::AppHandle,
    host_pid: u32,
    old_class: &str,
    old_details_class: &str,
    revision: &Option<DecimalInt>,
    fresh_after_ms: i64,
) -> Result<TaskbarRuntimeSnapshot, String> {
    let service = super::taskbar_commands::service(app).ok_or("service missing")?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let state = service.snapshot().map_err(|error| error.to_string())?;
        let observed_pid = service.owned_host_pid();
        if observed_pid.is_some_and(|pid| pid != host_pid) {
            return Err("actor replaced host during actual Explorer recovery".into());
        }
        let readout = readout_class(app);
        let details = super::taskbar_smoke::own_details(app)
            .and_then(|(window, visible, _)| class(window).map(|name| (name, visible)));
        // A cached Embedded state can precede the next actor poll. Require the
        // independently observed new native generations as well as the status.
        if state.state == TaskbarRuntimeState::Embedded
            && state.applied_settings_revision.as_ref() == revision.as_ref()
            && observed_pid == Some(host_pid)
            && state
                .last_snapshot_at_ms
                .is_some_and(|at| at.value() >= fresh_after_ms)
            && readout.as_ref().is_ok_and(|name| name != old_class)
            && details
                .as_ref()
                .is_ok_and(|(name, visible)| name != old_details_class && !visible)
        {
            return Ok(state);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "whole-app recovery deadline: state={state:?} host={observed_pid:?} readout={readout:?} details={details:?}"
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_disabled(app: &tauri::AppHandle) -> Result<(), String> {
    let service = super::taskbar_commands::service(app).ok_or("service missing")?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = service.snapshot().map_err(|error| error.to_string())?;
        if state.state == TaskbarRuntimeState::Disabled
            && matches!(
                state.last_cleanup,
                Some(
                    TaskbarCleanupOutcome::Restored
                        | TaskbarCleanupOutcome::AlreadyRestored
                        | TaskbarCleanupOutcome::NoRecord
                )
            )
            && service.owned_host_pid().is_none()
            // Normal native shutdown removes the lease before the guardian observes
            // it, so NoRecord is expected. Independently require a full-width,
            // validated new shell geometry instead of calling NoRecord restored.
            && inspect_primary_taskbar().is_ok()
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("disable restoration deadline: {state:?}"));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn settled_topology(
    expected_shell: u32,
) -> Result<token_pulse_taskbar::windows::topology::TaskbarTopology, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut previous = None;
    let mut stable_since = Instant::now();
    loop {
        if shell_pid()? != expected_shell {
            return Err("shell changed while observing restored geometry".into());
        }
        let topology = inspect_primary_taskbar();
        if topology.as_ref().ok() != previous.as_ref() {
            previous = topology.as_ref().ok().cloned();
            stable_since = Instant::now();
        } else if let Some(value) = &previous {
            // Explorer lays out MSTaskListWClass asynchronously after its parent's
            // synchronous reservation restore. Freeze independently settled geometry.
            if stable_since.elapsed() >= Duration::from_millis(750) {
                return Ok(value.clone());
            }
        }
        if Instant::now() >= deadline {
            return Err(format!("restored geometry did not settle: {topology:?}"));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn class(window: HWND) -> Result<String, String> {
    let mut buffer = [0; 128];
    let length = unsafe { GetClassNameW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    if length <= 0 {
        return Err("owned window class unavailable".into());
    }
    Ok(String::from_utf16_lossy(&buffer[..length as usize]))
}

fn readout_class(app: &tauri::AppHandle) -> Result<String, String> {
    class(super::taskbar_smoke::own_readout_window(app)?)
}

fn shell_pid() -> Result<u32, String> {
    let name: Vec<_> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
    let root = unsafe { FindWindowW(name.as_ptr(), std::ptr::null()) };
    if root.is_null() {
        return Err("primary shell missing".into());
    }
    let mut pid = 0;
    if unsafe { GetWindowThreadProcessId(root, &mut pid) } == 0 || pid == 0 {
        return Err("primary shell PID unavailable".into());
    }
    Ok(pid)
}

// Holding the exact kernel object prevents a reused numeric PID from satisfying
// same-host recovery, and independently verifies normal disable waited for exit.
struct HeldHost(HANDLE);
impl HeldHost {
    fn open(pid: u32) -> Result<Self, String> {
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if handle.is_null() {
            return Err("owned host kernel handle unavailable".into());
        }
        let process = Self(handle);
        process.require_live()?;
        Ok(process)
    }
    fn require_live(&self) -> Result<(), String> {
        if unsafe { WaitForSingleObject(self.0, 0) } != WAIT_TIMEOUT {
            return Err("original host kernel process is no longer alive".into());
        }
        Ok(())
    }
    fn require_exited(&self) -> Result<(), String> {
        if unsafe { WaitForSingleObject(self.0, 0) } != WAIT_OBJECT_0 {
            return Err("normal disable did not wait for original host exit".into());
        }
        Ok(())
    }
}
impl Drop for HeldHost {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}
