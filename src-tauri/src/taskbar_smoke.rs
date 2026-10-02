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
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const current=await invoke('get_taskbar_preferences',{requestId:'taskbar-exit-enable'});
      await invoke('set_taskbar_preferences',{requestId:'taskbar-exit-enable-save',request:{preferences:{...current.data.preferences,enabled:true},expected_settings_revision:current.data.settings_revision}});
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Embedded)?;
    super::taskbar_commands::service(app)
        .ok_or("service missing")?
        .shutdown();
    if inspect_primary_taskbar().map_err(|e| format!("shutdown topology: {e:?}"))? != before {
        return Err("shutdown while embedded did not restore original geometry".into());
    }
    println!(
        "NATIVE_TASKBAR_MANAGER_OK: isolated SQLite DTO, real settings UI/status and save, native host embedding, shared privacy barrier, hidden-main snapshot refresh, synthetic power routing, disable and embedded shutdown restore original geometry"
    );
    Ok(())
}
