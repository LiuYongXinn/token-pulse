//! Explicit real-account native-host acceptance. Only UUID-isolated debug scenes call this.
use std::{
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;
use token_pulse_core::{protocol::QuotaState, taskbar::TaskbarRuntimeState};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextW, WM_SETFOCUS};

fn until(timeout: Duration, mut check: impl FnMut() -> Result<bool, String>) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if check()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("native account display acceptance deadline".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}
fn caption(app: &tauri::AppHandle) -> Result<String, String> {
    let window = super::taskbar_smoke::own_readout_window(app)?;
    let mut text = vec![0; 32768];
    let n = unsafe { GetWindowTextW(window, text.as_mut_ptr(), text.len() as i32) };
    if n <= 0 || n as usize == text.len() - 1 {
        return Err("own taskbar caption missing or truncated".into());
    }
    Ok(String::from_utf16_lossy(&text[..n as usize]))
}
/// Matching native text against the production DTO proves delivery, not numeric formatting;
/// separate fixed-fixture display tests cover formatting and actual GDI bar layout.
fn expected(app: &tauri::AppHandle) -> Result<token_pulse_taskbar::TaskbarView, String> {
    let runtime = app.state::<super::RuntimeState>();
    let now = token_pulse_core::numeric::EpochMs::new(
        token_pulse_collector::jobs::now_ms().map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let input = runtime
        .database
        .as_ref()
        .map_err(|e| e.to_string())?
        .taskbar_input(now, &uuid::Uuid::new_v4().to_string())
        .map_err(|e| e.to_string())?;
    let quota = runtime
        .quota
        .as_ref()
        .map_err(|e| e.to_string())?
        .snapshot()
        .map_err(|e| e.to_string())?;
    let usage = input.usage.ok_or("taskbar usage envelope missing")?;
    Ok(token_pulse_taskbar::TaskbarView::from_optional_snapshots(
        &usage,
        Some(&quota),
        input.privacy,
        input.theme,
    ))
}
fn verify_native_quota(app: &tauri::AppHandle) -> Result<(), String> {
    until(Duration::from_secs(10), || {
        let view = expected(app)?;
        let quota = view
            .quota
            .as_ref()
            .ok_or("visible account projection missing")?;
        if !matches!(quota.state, QuotaState::Ready) || quota.fetched_at_ms.is_none() {
            return Err("taskbar account proof missing".into());
        }
        if caption(app)?
            != token_pulse_taskbar::display::accessible_text(&view)
                .map_err(|_| "invalid account readout")?
        {
            return Ok(false);
        }
        let (_, visible, text) = super::taskbar_smoke::own_details(app)?;
        if !visible {
            return Ok(false);
        }
        let content = token_pulse_taskbar::details::content(&view, view.generated_at_ms.value())
            .map_err(|_| "invalid account details")?;
        // Do not compare the relative countdown, which can legitimately cross a minute.
        Ok(content
            .rows
            .iter()
            .filter(|row| {
                row.label.starts_with("账户")
                    || row.label == "额度桶"
                    || row.label.starts_with("窗口 ")
                    || row.label == "重置时间"
            })
            .all(|row| {
                text.contains(&format!(
                    "{}：{}",
                    row.label,
                    row.value.split(" · 距重置 ").next().unwrap_or(&row.value)
                ))
            }))
    })
}
fn privacy(
    app: &tauri::AppHandle,
    main: &tauri::WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    super::mini_smoke::evaluate(
        app,
        main,
        &format!(
            r#"
        const settings=await invoke('get_display_settings',{{requestId:'native-account-privacy-before'}});
        await invoke('set_display_privacy',{{requestId:'native-account-privacy-save',request:{{privacy:{enabled},expected_settings_revision:settings.data.settings_revision}}}});
    "#
        ),
    )?;
    super::taskbar_smoke::wait_actions_ready(app)?;
    if super::taskbar_smoke::own_details(app)?.1 {
        return Err("privacy barrier left old native details visible".into());
    }
    Ok(())
}
pub fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let runtime = app.state::<super::RuntimeState>();
    if !std::env::args().any(|a| a == "--native-existing-account")
        || !runtime
            .data_directory
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("native-account-startup-"))
    {
        return Err("native real-account taskbar probe requires explicit isolated scene".into());
    }
    let baseline = token_pulse_taskbar::windows::topology::inspect_primary_taskbar()
        .map_err(|e| format!("{e:?}"))?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
        const prefs=await invoke('get_taskbar_preferences',{requestId:'native-account-taskbar-before'});
        if(prefs.data.preferences.enabled)throw new Error('ACCOUNT_TASKBAR_NOT_ISOLATED');
        await invoke('set_taskbar_preferences',{requestId:'native-account-taskbar-enable',request:{preferences:{...prefs.data.preferences,enabled:true,fallback_to_mini:false,display:{...prefs.data.preferences.display,show_tokens:false,show_costs:false,show_quota:true,show_weekly_reset:true}},expected_settings_revision:prefs.data.settings_revision}});
    "#,
    )?;
    super::taskbar_smoke::wait_actions_ready(app)?;
    let owner = super::taskbar_commands::service(app).ok_or("taskbar owner missing")?;
    eprintln!(
        "NATIVE_TASKBAR_ACCOUNT_OWNED_PID: {}",
        owner.owned_host_pid().ok_or("owned host missing")?
    );
    super::taskbar_smoke::own_readout_message(app, WM_SETFOCUS, 0, 0)?;
    verify_native_quota(app)?;
    privacy(app, &main, true)?;
    for window in [&main, &mini] {
        super::mini_smoke::evaluate(
            app,
            window,
            r#"
            const q=await invoke('get_account_quota',{requestId:'native-account-hidden'});
            if(q.display_policy?.privacy!==true||q.data.windows.length||q.data.fetched_at_ms!==null)throw new Error('ACCOUNT_PRIVACY_DTO');
            await wait(()=>document.querySelectorAll('progress').length===0);
        "#,
        )?;
    }
    if !caption(app)?.contains("费用和账户已隐藏") || caption(app)?.contains("剩余") {
        return Err("readout retained private account fields".into());
    }
    super::taskbar_smoke::own_readout_message(app, WM_SETFOCUS, 0, 0)?;
    let (_, visible, text) = super::taskbar_smoke::own_details(app)?;
    if !visible || !text.contains("隐私模式") || text.contains("剩余") || text.contains("额度桶")
    {
        return Err("hidden details retained account fields".into());
    }
    privacy(app, &main, false)?;
    super::taskbar_smoke::own_readout_message(app, WM_SETFOCUS, 0, 0)?;
    verify_native_quota(app)?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"await invoke('mini_window_action',{requestId:'native-account-mini-hide',request:{kind:'hide'}});"#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"await invoke('perform_window_action',{requestId:'native-account-main-hide',action:'hide_main'});"#,
    )?;
    let quota = runtime.quota.as_ref().map_err(|e| e.to_string())?;
    let first = quota.snapshot().map_err(|e| e.to_string())?;
    until(Duration::from_secs(90), || {
        if main.is_visible().map_err(|e| e.to_string())?
            || mini.is_visible().map_err(|e| e.to_string())?
        {
            return Err("hidden application surface reopened during account refresh".into());
        }
        let next = quota.snapshot().map_err(|e| e.to_string())?;
        if next.connection_epoch != first.connection_epoch {
            return Err("account changed during taskbar refresh; previous proof discarded".into());
        }
        Ok(matches!(next.state, QuotaState::Ready)
            && next.last_attempt_at_ms > first.last_attempt_at_ms
            && next.fetched_at_ms > first.fetched_at_ms)
    })?;
    super::taskbar_smoke::wait_actions_ready(app)?;
    super::taskbar_smoke::own_readout_message(app, WM_SETFOCUS, 0, 0)?;
    verify_native_quota(app)?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
        const prefs=await invoke('get_taskbar_preferences',{requestId:'native-account-taskbar-disable-before'});
        await invoke('set_taskbar_preferences',{requestId:'native-account-taskbar-disable',request:{preferences:{...prefs.data.preferences,enabled:false},expected_settings_revision:prefs.data.settings_revision}});
    "#,
    )?;
    until(Duration::from_secs(10), || {
        Ok(owner.snapshot().map_err(|e| e.to_string())?.state == TaskbarRuntimeState::Disabled)
    })?;
    if token_pulse_taskbar::windows::topology::inspect_primary_taskbar()
        .map_err(|e| format!("{e:?}"))?
        != baseline
    {
        return Err("account probe did not restore actual taskbar geometry".into());
    }
    eprintln!(
        "NATIVE_TASKBAR_EXISTING_ACCOUNT_OK: production real-account pipe/readout/details, shared privacy, taskbar-only background read, disabled geometry restored; authored own-window focus, physical input separate"
    );
    Ok(())
}
