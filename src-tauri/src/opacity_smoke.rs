//! Debug-only actual Windows alpha inspection, UI interaction and Writer fault injection.
use tauri::Manager;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
fn inspect(window: &tauri::WebviewWindow, expected: u8) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as _;
    let mut alpha = 0;
    let mut key = 0;
    let mut flags = 0;
    unsafe {
        if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_LAYERED as isize == 0
            || GetLayeredWindowAttributes(hwnd, &mut key, &mut alpha, &mut flags) == 0
            || flags != LWA_ALPHA
            || alpha != expected
        {
            return Err(format!("native opacity mismatch: {alpha}, {flags}"));
        }
    }
    Ok(())
}
pub fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    inspect(&mini, 255)?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      await wait(()=>document.querySelector('section[aria-label="小窗透明度设置"] input[type=range]:not(:disabled)'));
      const region=document.querySelector('section[aria-label="小窗透明度设置"]');
      const slider=region.querySelector('input[type=range]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(slider,'80');slider.dispatchEvent(new Event('input',{bubbles:true}));slider.dispatchEvent(new Event('change',{bubbles:true}));
      await wait(()=>[...region.querySelectorAll('button')].some(b=>b.textContent==='保存小窗透明度'&&!b.disabled));
      [...region.querySelectorAll('button')].find(b=>b.textContent==='保存小窗透明度').click();
      await wait(()=>region.querySelector('[aria-label="小窗透明度状态"]')?.textContent==='已保存 80%');
      const actual=await invoke('get_mini_opacity',{requestId:'native-opacity-ui'});
      if(!actual.data.supported || actual.data.opacity_percent!==80)throw new Error('OPACITY_UI_DTO_MISMATCH');
    "#,
    )?;
    inspect(&mini, 204)?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      let state=await invoke('mini_window_action',{requestId:'native-opacity-pin-read',request:{kind:'read'}});
      await invoke('mini_window_action',{requestId:'native-opacity-pin-toggle',request:{kind:'set_pinned',pinned:!state.data.pinned}});
      await invoke('mini_window_action',{requestId:'native-opacity-pin-restore',request:{kind:'set_pinned',pinned:state.data.pinned}});
    "#,
    )?;
    inspect(&mini, 204)?;
    let state = app.state::<super::RuntimeState>();
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let fault =
        token_pulse_store::rusqlite::Connection::open(db.path()).map_err(|e| e.to_string())?;
    fault.execute_batch("CREATE TRIGGER reject_opacity_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'native opacity failure'); END;").map_err(|e|e.to_string())?;
    let result = super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const before=await invoke('get_mini_opacity',{requestId:'native-opacity-before'});
      let failed=false;try{await invoke('set_mini_opacity',{requestId:'native-opacity-fault',request:{opacity_percent:70,expected_settings_revision:before.data.settings_revision}});}catch(e){failed=e.code==='DB_WRITE_FAILED';}
      const after=await invoke('get_mini_opacity',{requestId:'native-opacity-after'});
      if(!failed || after.data.opacity_percent!==80 || after.data.settings_revision!==before.data.settings_revision)throw new Error('OPACITY_FAULT_LOST_SETTING');
    "#,
    );
    fault
        .execute_batch("DROP TRIGGER reject_opacity_revision;")
        .map_err(|e| e.to_string())?;
    result?;
    inspect(&mini, 204)?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      const value=await invoke('get_mini_opacity',{requestId:'native-mini-opacity-read'});
      let denied=false;try{await invoke('set_mini_opacity',{requestId:'native-mini-opacity-write',request:{opacity_percent:100,expected_settings_revision:value.data.settings_revision}});}catch{denied=true;}
      if(!denied || value.data.opacity_percent!==80)throw new Error('MINI_OPACITY_PERMISSIONS');
    "#,
    )?;
    // Explicit show uses the recovery/tray entry and must reapply alpha after Tauri
    // resets cursor-ignore. Keyboard input is accepted separately on an interactive desktop.
    mini.set_ignore_cursor_events(true)
        .map_err(|e| e.to_string())?;
    mini.hide().map_err(|e| e.to_string())?;
    super::mini_window::show(app)?;
    inspect(&mini, 204)?;
    unsafe {
        if GetWindowLongPtrW(mini.hwnd().map_err(|e| e.to_string())?.0 as _, GWL_EXSTYLE)
            & WS_EX_TRANSPARENT as isize
            != 0
        {
            return Err("recovery left cursor pass-through enabled".into());
        }
    }
    mini.destroy().map_err(|e| e.to_string())?;
    for _ in 0..100 {
        if app.get_webview_window("mini").is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    super::mini_window::show(app)?;
    let recreated = app
        .get_webview_window("mini")
        .ok_or("recreated mini missing")?;
    inspect(&recreated, 204)?;
    recreated.hide().map_err(|e| e.to_string())?;
    println!(
        "NATIVE_MINI_OPACITY_OK: actual settings UI, HWND alpha 255->204, Writer failure restores 204, mini read-only capability, cursor recovery retains alpha, destroyed WebView recreated from persisted setting"
    );
    Ok(())
}
