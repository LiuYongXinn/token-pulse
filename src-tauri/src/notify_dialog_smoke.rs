//! Actual owned Windows chooser and React notification review; synthetic config only.
use super::native_dialog_driver::DialogDriver;
use std::{fs, time::Duration};
use tauri::Manager;

pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_NOTIFY_DIALOGS_OK: actual owned chooser cancel/select, React close/review/enable/review/disable, preserved original and later unrelated edit"
            ),
            Err(error) => eprintln!("NATIVE_NOTIFY_DIALOGS_FAILED: {error}"),
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}
fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<super::RuntimeState>();
    if !app.config().identifier.ends_with(".dev")
        || !state
            .data_directory
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("native-probe-"))
    {
        return Err("notify dialog scene requires isolated debug database".into());
    }
    let home = state.data_directory.join("synthetic-notify-dialog-home");
    fs::create_dir(&home).map_err(|e| e.to_string())?;
    let original_exe =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").ok_or("system directory missing")?)
            .join("System32/cmd.exe");
    let original = serde_json::to_string(&[
        original_exe.to_str().ok_or("original path encoding")?,
        "/d",
        "/s",
        "/c",
        "exit 0",
    ])
    .map_err(|e| e.to_string())?;
    let before =
        format!("# synthetic config\r\nnotify = {original}\r\nmodel='unpriced-fixture'\r\n");
    let config_path = home.join("config.toml");
    fs::write(&config_path, &before).map_err(|e| e.to_string())?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      [...document.querySelectorAll('.sidebar nav button')].find(n=>n.textContent==='设置').click();
      await wait(()=>[...document.querySelectorAll('.notify-settings button')].some(n=>n.textContent==='选择其他 Home 并预览'&&!n.disabled));
      const state=await invoke('get_notify_integrations',{requestId:'notify-dialog-before'});
      if(!Array.isArray(state.data.registrations)||state.data.registrations.length||state.data.registry_issue||state.data.service_issue)throw new Error('NOTIFY_INITIAL_STATE');
    "#,
    )?;
    for (index, action) in ["cancel", "select", "select"].into_iter().enumerate() {
        let mut driver = DialogDriver::start("notify_home", action, &home)?;
        let scene = super::mini_smoke::evaluate_with_timeout(
            app,
            &main,
            &format!(
                r#"
          const button=()=>[...document.querySelectorAll('.notify-settings button')].find(n=>n.textContent==='选择其他 Home 并预览');
          if(button()?.disabled)throw new Error('NOTIFY_PICKER_DISABLED');
          button().click();
          await wait(()=>button()?.disabled);
          if('{action}'==='cancel'){{
            await wait(()=>button()?.disabled===false,1000);
            if(document.querySelector('.notify-settings .notify-preview')||document.querySelector('.notify-settings > [role=status]')?.textContent!=='已取消目录选择，配置没有修改。')throw new Error('NOTIFY_CANCEL_STATE');
          }}else{{
            await wait(()=>document.querySelector('.notify-settings .notify-preview button.primary')?.textContent==='确认启用通知'&&!document.querySelector('.notify-settings .notify-preview button.primary').disabled,1000);
            const review=document.querySelector('.notify-settings .notify-preview');
            if(!review.textContent.includes('synthetic-notify-dialog-home')||!review.textContent.includes('继续调用原通知命令。')||!review.querySelector('pre')?.textContent.includes('cmd.exe'))throw new Error('NOTIFY_REVIEW_MISSING_ORIGINAL');
          }}
          if(document.querySelector('.notify-settings [role=alert]'))throw new Error('NOTIFY_SELECTION_REJECTED');
          const state=await invoke('get_notify_integrations',{{requestId:'notify-dialog-no-apply'}});
          if(state.data.registrations.length)throw new Error('NOTIFY_PREVIEW_REGISTERED');
        "#
            ),
            Duration::from_secs(40),
        );
        driver.finish().map_err(|e| format!("{action}: {e}"))?;
        scene?;
        if fs::read(&config_path).map_err(|e| e.to_string())? != before.as_bytes() {
            return Err("picker or review wrote config".into());
        }
        if index == 1 {
            super::mini_smoke::evaluate(
                app,
                &main,
                r#"
              [...document.querySelectorAll('.notify-settings .notify-preview button')].find(n=>n.textContent==='关闭预览').click();
              await wait(()=>!document.querySelector('.notify-settings .notify-preview'));
              await wait(()=>[...document.querySelectorAll('.notify-settings button')].some(n=>n.textContent==='选择其他 Home 并预览'&&!n.disabled));
              const state=await invoke('get_notify_integrations',{requestId:'notify-dialog-closed'});
              if(state.data.registrations.length)throw new Error('NOTIFY_CLOSE_REGISTERED');
            "#,
            )?;
        }
        println!("NATIVE_NOTIFY_DIALOG_STEP_OK: {index} {action}");
    }
    super::mini_smoke::evaluate_with_timeout(
        app,
        &main,
        r#"
      [...document.querySelectorAll('.notify-settings .notify-preview button')].find(n=>n.textContent==='确认启用通知').click();
      await wait(()=>document.querySelector('.notify-settings .notify-registration h3')?.textContent==='通知已启用'&&!document.querySelector('.notify-settings .notify-preview'),500);
      if(document.querySelector('.notify-settings [role=alert]'))throw new Error('NOTIFY_ENABLE_REJECTED');
      const state=await invoke('get_notify_integrations',{requestId:'notify-dialog-enabled'});
      if(state.data.registrations.length!==1||!state.data.registrations[0].configured||!state.data.registrations[0].chain_original||!state.data.registrations[0].current_executable)throw new Error('NOTIFY_ENABLE_INVALID');
    "#,
        Duration::from_secs(20),
    )?;
    let enabled = fs::read(&config_path).map_err(|e| e.to_string())?;
    let enabled_text = std::str::from_utf8(&enabled).map_err(|_| "config encoding")?;
    if enabled == before.as_bytes()
        || !enabled_text.contains("--tokenpulse-notify")
        || !enabled_text.contains("model='unpriced-fixture'\r\n")
        || !enabled_text.starts_with("# synthetic config\r\n")
    {
        return Err("enabled notify did not preserve unrelated config".into());
    }
    let mut changed = enabled.clone();
    changed.extend_from_slice(b"new_key='later-user-edit'\r\n");
    fs::write(&config_path, &changed).map_err(|e| e.to_string())?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const button=()=>[...document.querySelectorAll('.notify-settings .notify-registration button')].find(n=>n.textContent==='预览停用通知');
      await wait(()=>button()&&!button().disabled);button().click();
      await wait(()=>document.querySelector('.notify-settings .notify-preview button.primary')?.textContent==='确认停用通知'&&!document.querySelector('.notify-settings .notify-preview button.primary').disabled);
      const review=document.querySelector('.notify-settings .notify-preview');
      if(!review.textContent.includes('恢复后 notify')||!review.querySelectorAll('pre')[1]?.textContent.includes('cmd.exe'))throw new Error('NOTIFY_UNDO_REVIEW_MISSING');
    "#,
    )?;
    if fs::read(&config_path).map_err(|e| e.to_string())? != changed {
        return Err("undo review wrote config".into());
    }
    super::mini_smoke::evaluate_with_timeout(
        app,
        &main,
        r#"
      [...document.querySelectorAll('.notify-settings .notify-preview button')].find(n=>n.textContent==='确认停用通知').click();
      await wait(()=>!document.querySelector('.notify-settings .notify-preview')&&!document.querySelector('.notify-settings .notify-registration')&&document.querySelector('.notify-settings > [role=status]')?.textContent==='通知已停用，原通知配置已恢复。',500);
      const state=await invoke('get_notify_integrations',{requestId:'notify-dialog-disabled'});
      if(state.data.registrations.length||state.data.registry_issue||document.querySelector('.notify-settings [role=alert]'))throw new Error('NOTIFY_UNDO_RETAINED_REGISTRATION');
    "#,
        Duration::from_secs(20),
    )?;
    let expected = format!("{before}new_key='later-user-edit'\r\n");
    if fs::read(&config_path).map_err(|e| e.to_string())? != expected.as_bytes() {
        return Err("notify undo changed original or later user edit".into());
    }
    Ok(())
}
