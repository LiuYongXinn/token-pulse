//! Debug-only real Windows floating-window acceptance in an isolated probe directory.
use std::{thread, time::Duration};
use tauri::{Listener, Manager, WebviewWindow};

fn evaluate(app: &tauri::AppHandle, window: &WebviewWindow, script: &str) -> Result<(), String> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let event = format!("native-mini-probe-{}", uuid::Uuid::new_v4());
    let listener = app.listen(event.clone(), move |e| {
        let _ = sender.try_send(e.payload().to_owned());
    });
    let event_json = serde_json::to_string(&event).map_err(|e| e.to_string())?;
    let code = format!(
        r#"(async()=>{{
      const invoke=window.__TAURI_INTERNALS__.invoke;
      const wait=async check=>{{for(let i=0;i<100;i++){{if(check())return;await new Promise(r=>setTimeout(r,30));}}throw new Error('UI_WAIT_FAILED');}};
      let result=true;try{{ {script} }}catch(error){{result=false;console.error('Mini native acceptance',error);}}
      await invoke('plugin:event|emit',{{event:{event_json},payload:result}});
    }})();"#
    );
    window.eval(code).map_err(|e| e.to_string())?;
    let result = receiver.recv_timeout(Duration::from_secs(8));
    app.unlisten(listener);
    if result.as_deref() == Ok("true") {
        Ok(())
    } else {
        Err("native mini WebView check failed".into())
    }
}
fn size(window: &WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    let logical = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);
    if (logical.width - width).abs() > 1.0 || (logical.height - height).abs() > 1.0 {
        return Err(format!("mini logical size mismatch: {logical:?}"));
    }
    Ok(())
}
pub fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("main missing")?;
    evaluate(
        app,
        &main,
        r#"
      const button=[...document.querySelectorAll('.sidebar-bottom button')].find(b=>b.textContent==='显示悬浮窗');
      if(!button || button.disabled)throw new Error('MINI_ENTRY_MISSING');button.click();
    "#,
    )?;
    let mut mini = None;
    for _ in 0..100 {
        mini = app.get_webview_window("mini");
        if mini
            .as_ref()
            .is_some_and(|w| w.is_visible().unwrap_or(false))
        {
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    let mini = mini.ok_or("mini window was not created")?;
    size(&mini, 280.0, 220.0)?;
    evaluate(
        app,
        &mini,
        r#"
      await wait(()=>document.querySelector('.mini-health')?.textContent==='尚无已确认用量');
      if(document.querySelector('.mini-tokens')?.textContent!=='—' || !document.querySelector('.mini-quota')?.textContent.includes('账户未连接'))throw new Error('UNKNOWN_VALUES_REPLACED');
      if(document.querySelector('.mini-cost')?.textContent.includes('$0.00'))throw new Error('UNKNOWN_COST_ZERO');
      for(const command of ['get_sources','get_price_rules','set_display_theme','perform_window_action']) {
        let denied=false;try{await invoke(command,{requestId:'mini-denied',action:'quit',revision:null,request:{theme:'light',expected_settings_revision:'13'}});}catch{denied=true;}
        if(!denied)throw new Error('MINI_PERMISSION_TOO_BROAD');
      }
      document.querySelector('button[aria-label="展开小窗"]').click();
      await wait(()=>document.querySelector('button[aria-label="收起小窗"]'));
      if(document.querySelector('button[aria-label="小窗置顶"]')?.getAttribute('aria-pressed')!=='true')throw new Error('DEFAULT_PIN_MISSING');
      document.querySelector('button[aria-label="小窗置顶"]').click();
      await wait(()=>document.querySelector('button[aria-label="小窗置顶"]')?.getAttribute('aria-pressed')==='false');
      document.querySelector('button[aria-label="小窗置顶"]').click();
      await wait(()=>document.querySelector('button[aria-label="小窗置顶"]')?.getAttribute('aria-pressed')==='true');
    "#,
    )?;
    size(&mini, 360.0, 380.0)?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_TOPMOST,
        };
        let style = unsafe {
            GetWindowLongPtrW(mini.hwnd().map_err(|e| e.to_string())?.0 as _, GWL_EXSTYLE)
        };
        if style & WS_EX_TOPMOST as isize == 0 {
            return Err("native mini topmost flag missing".into());
        }
    }
    evaluate(
        app,
        &main,
        r#"
      const s=await invoke('get_display_settings',{requestId:'mini-shared-theme-read'});
      await invoke('set_display_theme',{requestId:'mini-shared-theme',request:{theme:'light',expected_settings_revision:s.data.settings_revision}});
      await wait(()=>document.documentElement.dataset.theme==='light');
    "#,
    )?;
    evaluate(
        app,
        &mini,
        r#"
      await wait(()=>document.documentElement.dataset.theme==='light');
      document.querySelector('button[aria-label="小窗隐私模式"]').click();
      await wait(()=>document.querySelector('button[aria-label="小窗隐私模式"]')?.getAttribute('aria-pressed')==='true');
      await wait(()=>document.querySelector('.mini-cost')?.textContent.includes('已隐藏'));
      const s=await invoke('get_display_settings',{requestId:'mini-private-read'});
      if(!s.data.preferences.privacy || !s.display_policy.privacy)throw new Error('SHARED_PRIVACY_NOT_COMMITTED');
    "#,
    )?;
    evaluate(
        app,
        &main,
        r#"
      const s=await invoke('get_display_settings',{requestId:'main-mini-policy-read'});
      const status=await invoke('get_app_status',{requestId:'main-mini-policy-status'});
      if(!s.data.preferences.privacy || !status.display_policy.privacy || status.data.data_directory.includes('native-probe-'))throw new Error('MAIN_NOT_PRIVATE');
      if(document.querySelector('.date-range-label')?.textContent!=='2024-02-28 — 2024-02-29' || document.querySelector('.price-instant-label')?.textContent!=='2024-02-29T00:00:00.123Z')throw new Error('MAIN_SCOPE_CHANGED');
    "#,
    )?;
    evaluate(
        app,
        &mini,
        r#"
      await wait(()=>!document.querySelector('button[aria-label="小窗隐私模式"]')?.disabled);
      document.querySelector('button[aria-label="小窗隐私模式"]').click();
      await wait(()=>document.querySelector('button[aria-label="小窗隐私模式"]')?.getAttribute('aria-pressed')==='false');
      document.querySelector('button[aria-label="隐藏小窗"]').click();
    "#,
    )?;
    thread::sleep(Duration::from_millis(100));
    if mini.is_visible().map_err(|e| e.to_string())? {
        return Err("mini hide did not hide".into());
    }
    // Exactly the same implementation is used by the tray recover-interaction entry.
    super::mini_window::show(app)?;
    if !mini.is_visible().map_err(|e| e.to_string())? {
        return Err("mini restore did not show existing window".into());
    }
    size(&mini, 360.0, 380.0)?;
    evaluate(
        app,
        &mini,
        r#"
      document.querySelector('button[aria-label="收起小窗"]').click();
      await wait(()=>document.querySelector('button[aria-label="展开小窗"]'));
      document.querySelector('button[aria-label="小窗置顶"]').click();
      await wait(()=>document.querySelector('button[aria-label="小窗置顶"]')?.getAttribute('aria-pressed')==='false');
    "#,
    )?;
    size(&mini, 280.0, 220.0)?;
    mini.close().map_err(|e| e.to_string())?;
    thread::sleep(Duration::from_millis(100));
    if mini.is_visible().map_err(|e| e.to_string())? || app.get_webview_window("mini").is_none() {
        return Err("mini close did not preserve hidden window".into());
    }
    evaluate(
        app,
        &main,
        r#"
      const s=await invoke('get_display_settings',{requestId:'mini-theme-restore-read'});
      await invoke('set_display_theme',{requestId:'mini-theme-restore',request:{theme:'dark',expected_settings_revision:s.data.settings_revision}});
    "#,
    )?;
    println!(
        "NATIVE_MINI_OK: two real WebViews, 280x220/360x380 DIP, native topmost, constrained IPC, shared theme/privacy, independent main filter, hide/restore/close"
    );
    Ok(())
}
