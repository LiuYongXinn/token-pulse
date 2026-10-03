//! Debug-only taskbar companion to the exclusive power acceptance scenes.
use std::{
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;
use token_pulse_core::{
    numeric::DecimalInt,
    taskbar::{TaskbarPosition, TaskbarRuntimeState},
};
use token_pulse_taskbar::windows::topology::{TaskbarTopology, inspect_primary_taskbar};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    },
    UI::WindowsAndMessaging::{
        FindWindowW, GetWindowTextW, GetWindowThreadProcessId, WM_KILLFOCUS, WM_SETFOCUS,
    },
};
pub struct Probe {
    baseline: TaskbarTopology,
    old_host: u32,
    old_process: HeldProcess,
    revision: Option<DecimalInt>,
    navigation: DecimalInt,
    shell_pid: u32,
    shell_root: usize,
    shell_process: HeldProcess,
    position: TaskbarPosition,
}
pub fn prepare(app: &tauri::AppHandle, position: TaskbarPosition) -> Result<Probe, String> {
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("power mini missing")?;
    mini.hide().map_err(|error| error.to_string())?;
    let baseline = settled_topology()?;
    let (shell_root, shell_pid) = shell_identity()?;
    let shell_process = HeldProcess::open(shell_pid)?;
    println!("NATIVE_POWER_TASKBAR_BASELINE: {baseline:?}");
    let main = app.get_webview_window("main").ok_or("power main missing")?;
    super::mini_smoke::evaluate(
        app,
        &main,
        &(format!(
            r#"
        const position={};
        "#,
            serde_json::to_string(&position).map_err(|error| error.to_string())?
        ) + r#"
        const sessions=await invoke('query_mini_sessions',{requestId:'power-taskbar-sessions',request:{query:{search:'',page_size:10},cursor:null}});
        if(sessions.data.options.length!==1||sessions.data.next_cursor!==null)throw new Error('POWER_FIXTURE_SESSION_NOT_UNIQUE');
        const scope=await invoke('get_mini_scope',{requestId:'power-taskbar-scope-read'});
        await invoke('set_mini_scope',{requestId:'power-taskbar-scope-save',request:{mini_scope:{kind:'session',session_key:sessions.data.options[0].session_key,start:{kind:'fixed',start_ms:0}},expected_settings_revision:scope.data.settings_revision}});
        const display=await invoke('get_display_settings',{requestId:'power-taskbar-privacy-read'});
        if(display.data.preferences.privacy)await invoke('set_display_privacy',{requestId:'power-taskbar-privacy-save',request:{privacy:false,expected_settings_revision:display.data.settings_revision}});
        const current=await invoke('get_taskbar_preferences',{requestId:'power-taskbar-enable-read'});
        await invoke('set_taskbar_preferences',{requestId:'power-taskbar-enable',request:{preferences:{...current.data.preferences,enabled:true,fallback_to_mini:false,position},expected_settings_revision:current.data.settings_revision}});
        const saved=await invoke('get_taskbar_preferences',{requestId:'power-taskbar-position-before'});
        if(saved.data.preferences.position!==position||!saved.data.preferences.enabled||saved.data.preferences.fallback_to_mini)throw new Error('POWER_TASKBAR_POSITION_MISMATCH');
        const usage=await invoke('get_mini_usage',{requestId:'power-taskbar-before-usage'});
        if(usage.data.usage.total_tokens!=='3'||usage.data.mini_scope.kind!=='session')throw new Error('POWER_BEFORE_USAGE_MISMATCH:'+JSON.stringify(usage.data));
    "#),
    )?;
    wait_fresh(app, "3", 0)?;
    let service = super::taskbar_commands::service(app).ok_or("power taskbar service missing")?;
    let old_host = service.owned_host_pid().ok_or("power old host missing")?;
    let old_process = HeldProcess::open(old_host)?;
    let revision = service
        .snapshot()
        .map_err(|error| error.to_string())?
        .applied_settings_revision;
    super::taskbar_smoke::own_readout_message(app, WM_SETFOCUS, 0, 0)?;
    let (_, visible, text) = super::taskbar_smoke::own_details(app)?;
    if !visible || !text.contains("可信 Token：3；") {
        return Err("pre-suspend details missing baseline tokens".into());
    }
    let navigation = app
        .state::<super::RuntimeState>()
        .main_navigation
        .lock()
        .map_err(|_| "power navigation lock")?
        .revision
        .clone();
    println!(
        "NATIVE_POWER_TASKBAR_PREPARED: host={old_host} tokens=3 details_visible=true fixed_session=true production_actor=true"
    );
    Ok(Probe {
        baseline,
        old_host,
        old_process,
        revision,
        navigation,
        shell_pid,
        shell_root,
        shell_process,
        position,
    })
}
impl Probe {
    pub fn wait_suspended(&self, app: &tauri::AppHandle) -> Result<(), String> {
        wait(
            || {
                super::taskbar_commands::service(app).is_some_and(|service| {
                    service
                        .snapshot()
                        .is_ok_and(|state| state.state == TaskbarRuntimeState::Suspended)
                }) && self.old_process.exited()
                    && inspect_primary_taskbar().is_ok_and(|topology| topology == self.baseline)
            },
            "authored suspend did not retire host and restore geometry",
        )
    }
    pub fn verify_resumed(self, app: &tauri::AppHandle) -> Result<(), String> {
        let fresh_after =
            token_pulse_collector::jobs::now_ms().map_err(|error| error.to_string())?;
        wait_fresh(app, "10", fresh_after)?;
        let service =
            super::taskbar_commands::service(app).ok_or("resumed taskbar service missing")?;
        let after = service.snapshot().map_err(|error| error.to_string())?;
        let host = service.owned_host_pid().ok_or("resumed host missing")?;
        let held = HeldProcess::open(host)?;
        if after.applied_settings_revision != self.revision
            || after.issue.is_some()
            || after.error.is_some()
            || after.action_error.is_some()
            || after.fallback_error.is_some()
        {
            return Err(format!("resumed taskbar status mismatch: {after:?}"));
        }
        let same_host = host == self.old_host && self.old_process.live();
        if !same_host && !self.old_process.exited() {
            return Err("replacement left original native host alive".into());
        }
        if super::taskbar_smoke::own_details(app)?.1 {
            return Err("old details remained visible after resume".into());
        }
        let main = app
            .get_webview_window("main")
            .ok_or("resumed main missing")?;
        let mini = app
            .get_webview_window("mini")
            .ok_or("resumed mini missing")?;
        if main.is_visible().map_err(|error| error.to_string())?
            || mini.is_visible().map_err(|error| error.to_string())?
            || app
                .state::<super::RuntimeState>()
                .main_navigation
                .lock()
                .map_err(|_| "resumed navigation lock")?
                .revision
                != self.navigation
        {
            return Err("power resume caused unexpected window or navigation intention".into());
        }
        super::mini_smoke::evaluate(
            app,
            &main,
            &(format!(
                r#"const position={};"#,
                serde_json::to_string(&self.position).map_err(|error| error.to_string())?
            ) + r#"
            const saved=await invoke('get_taskbar_preferences',{requestId:'power-taskbar-position-after'});
            if(saved.data.preferences.position!==position||!saved.data.preferences.enabled||saved.data.preferences.fallback_to_mini)throw new Error('POWER_TASKBAR_RESUMED_POSITION_MISMATCH');
            const status=await invoke('get_taskbar_status',{requestId:'power-taskbar-resumed-status'});
            if(status.data.state!=='embedded'||status.data.issue!==null||status.data.error!==null||status.data.action_error!==null)throw new Error('POWER_TASKBAR_STATUS_MISMATCH');
            const usage=await invoke('get_mini_usage',{requestId:'power-taskbar-resumed-usage'});
            if(usage.data.usage.total_tokens!=='10'||usage.data.mini_scope.kind!=='session')throw new Error('POWER_RESUMED_USAGE_MISMATCH');
        "#),
        )?;
        super::taskbar_smoke::own_readout_message(app, WM_SETFOCUS, 0, 0)?;
        let (_, visible, text) = super::taskbar_smoke::own_details(app)?;
        if !visible || !text.contains("可信 Token：10；") {
            return Err("resumed details kept stale tokens".into());
        }
        super::taskbar_smoke::own_readout_message(app, WM_KILLFOCUS, 0, 0)?;
        service.shutdown();
        wait(
            || held.exited(),
            "normal taskbar shutdown did not exit owned host",
        )?;
        let restored = settled_topology()?;
        let cleanup = service
            .snapshot()
            .map_err(|error| error.to_string())?
            .last_cleanup;
        println!(
            "NATIVE_POWER_TASKBAR_CLEANUP: baseline={:?} restored={restored:?} cleanup={cleanup:?}",
            self.baseline
        );
        let same_shell =
            self.shell_process.live() && shell_identity()? == (self.shell_root, self.shell_pid);
        let geometry = restored_geometry(&self.baseline, &restored);
        if !same_shell || geometry.is_none() {
            return Err(format!(
                "post-power taskbar geometry differs: baseline={:?} restored={restored:?} cleanup={cleanup:?}",
                self.baseline
            ));
        }
        println!(
            "NATIVE_POWER_TASKBAR_GEOMETRY_OK: same_shell=true verification={geometry:?} current_full_width=true"
        );
        println!(
            "NATIVE_POWER_TASKBAR_POSITION_OK: position={} preferences_preserved=true",
            serde_json::to_string(&self.position).map_err(|error| error.to_string())?
        );
        println!(
            "NATIVE_POWER_TASKBAR_OK: old_host={} resumed_host={host} same_host={same_host} old_host_retired={} tokens=3/10 fresh_snapshot=true old_details_hidden=true no_unexpected_actions=true geometry_restored=true physical_input=false computer_restart=false",
            self.old_host,
            self.old_process.exited()
        );
        Ok(())
    }
}
fn shell_identity() -> Result<(usize, u32), String> {
    let class: Vec<u16> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
    let root = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    let mut pid = 0;
    if root.is_null() || unsafe { GetWindowThreadProcessId(root, &mut pid) } == 0 || pid == 0 {
        return Err("primary shell identity missing".into());
    }
    Ok((root as usize, pid))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Geometry {
    Exact,
    CurrentNotificationBoundary,
}
fn restored_geometry(before: &TaskbarTopology, after: &TaskbarTopology) -> Option<Geometry> {
    // Native inspection separately verifies the current kernel/window topology. These bounds
    // must describe full width, not a remaining partial reservation or a shortened task list.
    if before.validate().is_err()
        || after.validate().is_err()
        || before.task_switch.right != before.rebar.right
        || before.task_list.right != before.rebar.right
        || after.task_switch.right != after.rebar.right
        || after.task_list.right != after.rebar.right
    {
        return None;
    }
    if before == after {
        return Some(Geometry::Exact);
    }
    let same_left_and_vertical =
        |left: token_pulse_taskbar::windows::topology::ScreenRect,
         right: token_pulse_taskbar::windows::topology::ScreenRect| {
            left.left == right.left && left.top == right.top && left.bottom == right.bottom
        };
    (before.build == after.build
        && before.dpi == after.dpi
        && before.taskbar == after.taskbar
        && same_left_and_vertical(before.rebar, after.rebar)
        && same_left_and_vertical(before.task_switch, after.task_switch)
        && same_left_and_vertical(before.task_list, after.task_list)
        && before.notification.top == after.notification.top
        && before.notification.bottom == after.notification.bottom
        && before.notification.right == after.notification.right
        && before.rebar.right == before.notification.left
        && after.rebar.right == after.notification.left)
        .then_some(Geometry::CurrentNotificationBoundary)
}
fn wait_fresh(app: &tauri::AppHandle, tokens: &str, fresh_after: i64) -> Result<(), String> {
    wait(
        || {
            super::taskbar_commands::service(app).is_some_and(|service| {
                service.snapshot().is_ok_and(|state| {
                    state.state == TaskbarRuntimeState::Embedded
                        && state
                            .last_snapshot_at_ms
                            .is_some_and(|at| at.value() >= fresh_after)
                })
            }) && super::taskbar_smoke::own_readout_window(app).is_ok_and(|window| {
                let mut caption = [0; 4096];
                let length =
                    unsafe { GetWindowTextW(window, caption.as_mut_ptr(), caption.len() as i32) };
                length > 0
                    && String::from_utf16_lossy(&caption[..length as usize])
                        .split('；')
                        .next()
                        == Some(format!("Token {tokens}").as_str())
            })
        },
        "native caption and fresh actor snapshot did not reach expected tokens",
    )
}
fn wait(condition: impl Fn() -> bool, label: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !condition() {
        if Instant::now() >= deadline {
            return Err(label.into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
fn settled_topology() -> Result<TaskbarTopology, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut previous = None;
    let mut since = Instant::now();
    loop {
        let current = inspect_primary_taskbar().ok();
        if current != previous {
            previous = current;
            since = Instant::now();
        } else if let Some(topology) = &previous {
            if since.elapsed() >= Duration::from_millis(750) {
                return Ok(topology.clone());
            }
        }
        if Instant::now() >= deadline {
            return Err("full taskbar geometry did not settle".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}
struct HeldProcess(HANDLE);
impl HeldProcess {
    fn open(pid: u32) -> Result<Self, String> {
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if handle.is_null() {
            return Err("owned host handle missing".into());
        }
        let process = Self(handle);
        if !process.live() {
            return Err("owned host not alive".into());
        }
        Ok(process)
    }
    fn live(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_TIMEOUT }
    }
    fn exited(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }
}
impl Drop for HeldProcess {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use token_pulse_taskbar::windows::topology::ScreenRect;
    fn original() -> TaskbarTopology {
        TaskbarTopology {
            build: 19045,
            dpi: 144,
            taskbar: ScreenRect {
                left: 0,
                top: 1380,
                right: 2560,
                bottom: 1440,
            },
            rebar: ScreenRect {
                left: 502,
                top: 1380,
                right: 2044,
                bottom: 1440,
            },
            task_switch: ScreenRect {
                left: 504,
                top: 1380,
                right: 2044,
                bottom: 1440,
            },
            task_list: ScreenRect {
                left: 504,
                top: 1380,
                right: 2044,
                bottom: 1440,
            },
            notification: ScreenRect {
                left: 2044,
                top: 1380,
                right: 2560,
                bottom: 1440,
            },
        }
    }
    fn changed() -> TaskbarTopology {
        TaskbarTopology {
            build: 19045,
            dpi: 144,
            taskbar: ScreenRect {
                left: 0,
                top: 1380,
                right: 2560,
                bottom: 1440,
            },
            rebar: ScreenRect {
                left: 502,
                top: 1380,
                right: 2080,
                bottom: 1440,
            },
            task_switch: ScreenRect {
                left: 504,
                top: 1380,
                right: 2080,
                bottom: 1440,
            },
            task_list: ScreenRect {
                left: 504,
                top: 1380,
                right: 2080,
                bottom: 1440,
            },
            notification: ScreenRect {
                left: 2080,
                top: 1380,
                right: 2560,
                bottom: 1440,
            },
        }
    }
    #[test]
    fn full_same_frame_and_shared_boundary_are_distinct_valid_outcomes() {
        assert_eq!(
            restored_geometry(&original(), &original()),
            Some(Geometry::Exact)
        );
        assert_eq!(
            restored_geometry(&original(), &changed()),
            Some(Geometry::CurrentNotificationBoundary)
        );
        assert_eq!(
            restored_geometry(&changed(), &original()),
            Some(Geometry::CurrentNotificationBoundary)
        );
    }
    #[test]
    fn boundary_check_rejects_unreturned_space_and_other_system_changes() {
        let rejected: Vec<TaskbarTopology> = (0..10)
            .map(|case| {
                let mut value = changed();
                match case {
                    0 => value.task_switch.right = 2000,
                    1 => value.task_list.right = 2079,
                    2 => value.dpi = 96,
                    3 => value.taskbar.top = 1379,
                    4 => value.rebar.left = 501,
                    5 => value.task_switch.left = 503,
                    6 => value.task_list.top = 1381,
                    7 => value.notification.right = 2559,
                    8 => value.notification.left = 2081,
                    _ => value.build = 22621,
                }
                value
            })
            .collect();
        for value in rejected {
            assert_eq!(restored_geometry(&original(), &value), None, "{value:?}");
        }
        let mut shortened = original();
        shortened.task_list.right = 2000;
        assert_eq!(restored_geometry(&shortened, &shortened), None);
    }
}
