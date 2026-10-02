//! Explicit debug-only real Tauri/host/layout check, using the isolated native-smoke database.
use std::{
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;
use token_pulse_core::taskbar::TaskbarRuntimeState;
pub fn start(app: tauri::AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        let result = super::mini_window::show(&app).and_then(|_| verify(&app));
        if let Err(error) = &result {
            eprintln!("NATIVE_TASKBAR_MANAGER_FAILED: {error}");
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}
fn wait(app: &tauri::AppHandle, state: TaskbarRuntimeState) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = super::taskbar_commands::service(app)
            .ok_or("taskbar service missing")?
            .snapshot()
            .map_err(|e| e.to_string())?;
        if snapshot.state == state {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(format!("taskbar functional state timeout: {snapshot:?}"));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
fn wait_fallback(app: &tauri::AppHandle, visible: bool) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let state = super::taskbar_commands::service(app)
            .ok_or("taskbar missing")?
            .snapshot()
            .map_err(|e| e.to_string())?;
        let actual = app
            .get_webview_window("mini")
            .is_some_and(|w| w.is_visible().unwrap_or(false));
        if state.fallback_visible == Some(visible) && actual == visible {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(format!(
                "fallback state timeout: expected={visible}, {state:?}, actual={actual}"
            ));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
pub fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    use token_pulse_taskbar::windows::topology::inspect_primary_taskbar;
    let path = &app.state::<super::RuntimeState>().data_directory;
    if !path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("native-probe-"))
    {
        return Err("taskbar check requires isolated smoke database".into());
    }
    let main = app.get_webview_window("main").ok_or("main missing")?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    let before = inspect_primary_taskbar().map_err(|e| format!("taskbar baseline: {e:?}"))?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      let current=await invoke('get_taskbar_preferences',{requestId:'taskbar-default'});
      if(current.data.preferences.enabled)throw new Error('TASKBAR_DEFAULT_ENABLED');
      const saved=await invoke('set_taskbar_preferences',{requestId:'taskbar-enable',request:{preferences:{...current.data.preferences,enabled:true,fallback_to_mini:false},expected_settings_revision:current.data.settings_revision}});
      if(!saved.data.preferences.enabled)throw new Error('TASKBAR_INTENT_NOT_SAVED');
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Embedded)?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const waitFor=async(read)=>{for(let i=0;i<100;i++){if(read())return;await new Promise(r=>setTimeout(r,50));}throw new Error('TASKBAR_UI_TIMEOUT');};
      document.querySelector('nav button:last-child').click();
      await waitFor(()=>[...document.querySelectorAll('[role=tab]')].some(n=>n.textContent==='任务栏显示'));
      [...document.querySelectorAll('[role=tab]')].find(n=>n.textContent==='任务栏显示').click();
      await waitFor(()=>document.querySelector('.taskbar-enable input')?.checked===true);
      await waitFor(()=>document.querySelector('.taskbar-runtime [role=status]')?.textContent==='已嵌入任务栏');
      if(document.querySelector('.taskbar-settings-panel').textContent.includes('显示隐私策略已变化'))throw new Error('TASKBAR_UI_PROTOCOL_STAMP');
    "#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      let denied=false;try{await invoke('get_taskbar_status',{requestId:'taskbar-mini-denied'});}catch{denied=true;}
      if(!denied)throw new Error('MINI_TASKBAR_COMMAND_ALLOWED');
    "#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const configured=await invoke('get_taskbar_preferences',{requestId:'taskbar-current'});
      const status=await invoke('get_taskbar_status',{requestId:'taskbar-actual'});
      if(status.data.state!=='embedded'||status.data.applied_settings_revision!==configured.data.settings_revision)throw new Error('TASKBAR_FAKE_STATUS');
      const display=await invoke('get_display_settings',{requestId:'taskbar-privacy-before'});
      await invoke('set_display_privacy',{requestId:'taskbar-privacy-change',request:{privacy:!display.data.preferences.privacy,expected_settings_revision:display.data.settings_revision}});
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Embedded)?;
    // A synthetic system message to our main HWND only, not a real machine suspend.
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, SendMessageW, WM_POWERBROADCAST,
    };
    let hwnd = main.hwnd().map_err(|e| e.to_string())?.0.cast();
    unsafe {
        SendMessageW(hwnd, WM_POWERBROADCAST, PBT_APMSUSPEND as usize, 0);
    }
    wait(app, TaskbarRuntimeState::Suspended)?;
    if inspect_primary_taskbar().map_err(|e| format!("suspended topology: {e:?}"))? != before {
        return Err("suspend did not restore original geometry".into());
    }
    unsafe {
        SendMessageW(hwnd, WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize, 0);
    }
    wait(app, TaskbarRuntimeState::Embedded)?;
    main.hide().map_err(|e| e.to_string())?;
    let last_update = super::taskbar_commands::service(app)
        .ok_or("service missing")?
        .snapshot()
        .map_err(|e| e.to_string())?
        .last_snapshot_at_ms;
    super::quota_commands::update_visibility(&main);
    thread::sleep(Duration::from_millis(1200));
    wait(app, TaskbarRuntimeState::Embedded)?;
    if super::taskbar_commands::service(app)
        .ok_or("service missing")?
        .snapshot()
        .map_err(|e| e.to_string())?
        .last_snapshot_at_ms
        == last_update
    {
        return Err("hidden main did not publish another actual snapshot".into());
    }
    super::show_main(app)?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const waitFor=async(read)=>{for(let i=0;i<100;i++){if(read())return;await new Promise(r=>setTimeout(r,50));}throw new Error('TASKBAR_UI_SAVE_TIMEOUT');};
      await waitFor(()=>document.querySelector('.taskbar-enable input')?.checked===true);
      document.querySelector('.taskbar-enable input').click();
      await waitFor(()=>document.querySelector('.taskbar-preferences button.primary')?.disabled===false);
      document.querySelector('.taskbar-preferences button.primary').click();
      await waitFor(()=>document.querySelector('.taskbar-runtime [role=status]')?.textContent==='已关闭');
      const current=await invoke('get_taskbar_preferences',{requestId:'taskbar-disable-ui-verify'});
      if(current.data.preferences.enabled)throw new Error('TASKBAR_UI_SAVE_NOT_PERSISTED');
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Disabled)?;
    if inspect_primary_taskbar().map_err(|e| format!("disabled topology: {e:?}"))? != before {
        return Err("manager disable did not restore original geometry".into());
    }
    // Destroy this test's own mini to cover automatic creation as well as hidden-window reuse.
    mini.destroy().map_err(|e| e.to_string())?;
    let until = Instant::now() + Duration::from_secs(3);
    while app.get_webview_window("mini").is_some() {
        if Instant::now() >= until {
            return Err("old isolated mini did not close".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    // A real unsupported position is a controlled fallback trigger; no machine setting changes.
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return Err("no foreground HWND to verify nonactivating fallback".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const current=await invoke('get_taskbar_preferences',{requestId:'taskbar-fallback-enable'});
      await invoke('set_taskbar_preferences',{requestId:'taskbar-fallback-save',request:{preferences:{...current.data.preferences,enabled:true,position:'application_right',fallback_to_mini:true},expected_settings_revision:current.data.settings_revision}});
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Unavailable)?;
    wait_fallback(app, true)?;
    let mini = app
        .get_webview_window("mini")
        .ok_or("fallback did not create mini")?;
    if unsafe { GetForegroundWindow() } != foreground {
        return Err("automatic fallback stole foreground focus".into());
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_NOACTIVATE,
    };
    if unsafe {
        GetWindowLongPtrW(
            mini.hwnd().map_err(|e| e.to_string())?.0.cast(),
            GWL_EXSTYLE,
        )
    } & WS_EX_NOACTIVATE as isize
        != 0
    {
        return Err("fallback left mini permanently nonactivating".into());
    }
    if inspect_primary_taskbar().map_err(|e| format!("fallback topology: {e:?}"))? != before {
        return Err("unsupported-position fallback changed taskbar geometry".into());
    }
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      await invoke('mini_window_action',{requestId:'taskbar-fallback-user-hide',request:{kind:'hide'}});
    "#,
    )?;
    wait_fallback(app, false)?;
    thread::sleep(Duration::from_millis(1500));
    if mini.is_visible().map_err(|e| e.to_string())? {
        return Err("fallback reopened user-hidden mini in same failure episode".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      await invoke('retry_taskbar_embed',{requestId:'taskbar-fallback-explicit-retry'});
    "#,
    )?;
    wait_fallback(app, true)?;
    if unsafe { GetForegroundWindow() } != foreground {
        return Err("reused fallback mini stole foreground focus".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const waitFor=async(read)=>{for(let i=0;i<100;i++){if(read())return;await new Promise(r=>setTimeout(r,50));}throw new Error('TASKBAR_FALLBACK_UI_TIMEOUT');};
      const read=()=>[...document.querySelectorAll('.taskbar-preferences label')].find(n=>n.textContent==='任务栏不可用时显示悬浮窗')?.querySelector('input');
      await waitFor(()=>read()?.checked===true); read().click();
      await waitFor(()=>document.querySelector('.taskbar-preferences button.primary')?.disabled===false);document.querySelector('.taskbar-preferences button.primary').click();
      await waitFor(()=>document.querySelector('.taskbar-preferences button.primary')?.disabled===true && read()?.checked===false);
      const current=await invoke('get_taskbar_preferences',{requestId:'taskbar-fallback-ui-verify'});
      if(current.data.preferences.fallback_to_mini)throw new Error('TASKBAR_FALLBACK_UI_NOT_SAVED');
    "#,
    )?;
    thread::sleep(Duration::from_millis(1200));
    if !mini.is_visible().map_err(|e| e.to_string())? {
        return Err("disabling fallback closed existing mini".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const current=await invoke('get_taskbar_preferences',{requestId:'taskbar-exit-enable'});
      await invoke('set_taskbar_preferences',{requestId:'taskbar-exit-enable-save',request:{preferences:{...current.data.preferences,enabled:true,position:'notification_left'},expected_settings_revision:current.data.settings_revision}});
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Embedded)?;
    if !mini.is_visible().map_err(|e| e.to_string())? {
        return Err("successful re-embedding closed existing mini".into());
    }
    super::taskbar_commands::service(app)
        .ok_or("service missing")?
        .shutdown();
    if inspect_primary_taskbar().map_err(|e| format!("shutdown topology: {e:?}"))? != before {
        return Err("shutdown while embedded did not restore original geometry".into());
    }
    println!(
        "NATIVE_TASKBAR_MANAGER_OK: isolated SQLite DTO, real settings UI/status and save, native host embedding, shared privacy barrier, hidden-main refresh, synthetic power routing, nonactivating fallback/user-hide/explicit-retry, retained mini after recovery, disable and embedded shutdown geometry restore"
    );
    Ok(())
}
