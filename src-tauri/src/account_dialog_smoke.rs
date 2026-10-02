//! Explicit actual Windows picker acceptance; never injects a selection capability.
use super::native_dialog_driver::DialogDriver;
use std::{fs, time::Duration};
use tauri::Manager;

pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_ACCOUNT_DIALOGS_OK: owned Windows executable/Home cancel/select, React draft/save/connect/refresh/disconnect, synthetic service only"
            ),
            Err(error) => eprintln!("NATIVE_ACCOUNT_DIALOGS_FAILED: {error}"),
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
        return Err("account dialog scene requires isolated debug database".into());
    }
    let database = state.database.as_ref().map_err(|e| e.to_string())?;
    if database
        .account_service_preferences()
        .map_err(|e| e.to_string())?
        .0
        .target
        .is_some()
    {
        return Err("account scene must begin unconfigured".into());
    }
    let home = state.data_directory.join("synthetic-account-dialog-home");
    fs::create_dir(&home).map_err(|e| e.to_string())?;
    fs::write(home.join("fixture-mode"), "service").map_err(|e| e.to_string())?;
    let executable = home.join("synthetic-codex.exe");
    fs::copy(
        std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("quota-fixture.exe"),
        &executable,
    )
    .map_err(|e| e.to_string())?;
    let original = fs::read(&executable).map_err(|e| e.to_string())?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      [...document.querySelectorAll('.sidebar nav button')].find(n=>n.textContent==='设置').click();
      await wait(()=>document.querySelector('.account-service'));
      await wait(()=>[...document.querySelectorAll('.account-service button')].some(n=>n.textContent==='选择账户服务程序'&&!n.disabled));
      const config=await invoke('get_account_service_config',{requestId:'account-dialog-before'});
      if(config.data.configured||config.data.auto_connect)throw new Error('ACCOUNT_DEFAULT_INVALID');
      window.__accountDialogRevision=config.data.settings_revision;
    "#,
    )?;
    for (kind, action, label, target) in [
        (
            "account_executable",
            "cancel",
            "选择账户服务程序",
            &executable,
        ),
        (
            "account_executable",
            "select",
            "选择账户服务程序",
            &executable,
        ),
        ("account_home", "cancel", "选择账户服务 Home", &home),
        ("account_home", "select", "选择账户服务 Home", &home),
    ] {
        let mut driver = DialogDriver::start(kind, action, target)?;
        let scene = super::mini_smoke::evaluate_with_timeout(
            app,
            &main,
            &format!(
                r#"
          const button=()=>[...document.querySelectorAll('.account-service button')].find(n=>n.textContent==='{label}');
          if(!button()||button().disabled)throw new Error('ACCOUNT_DIALOG_ENTRY');
          const beforeDraft=document.querySelector('.account-service article:first-of-type').textContent;
          button().click();
          await wait(()=>button()?.disabled);
          await wait(()=>button()?.disabled===false,1000);
          if(document.querySelector('.account-service [role=alert]'))throw new Error('ACCOUNT_DIALOG_REJECTED');
          const after=await invoke('get_account_service_config',{{requestId:'account-dialog-unchanged'}});
          const quota=await invoke('get_account_quota',{{requestId:'account-dialog-not-started'}});
          if(after.data.configured||after.data.auto_connect||after.data.settings_revision!==window.__accountDialogRevision||quota.data.state!=='disconnected')throw new Error('ACCOUNT_SELECTION_CHANGED_PERSISTED_STATE');
          const card=document.querySelector('.account-service article:first-of-type');
          if('{action}'==='cancel'&&card.textContent!==beforeDraft)throw new Error('ACCOUNT_CANCEL_CHANGED_DRAFT');
          if('{action}'==='select'){{
            if(card.querySelector('h3')?.textContent!=='待保存的连接配置')throw new Error('ACCOUNT_SELECTION_NO_DRAFT');
            const values=[...card.querySelectorAll('dd')].map(n=>n.textContent);
            if(values.length!==3||values[2].length!==64)throw new Error('ACCOUNT_PREVIEW_MISSING_FINGERPRINT');
            if('{kind}'==='account_executable'){{
              if(!values[0].endsWith('synthetic-codex.exe')||values[1]!=='账户服务默认目录')throw new Error('ACCOUNT_EXECUTABLE_PREVIEW');
              window.__accountDialogHash=values[2];
            }}else if(!values[1].endsWith('synthetic-account-dialog-home')||values[2]!==window.__accountDialogHash)throw new Error('ACCOUNT_HOME_LOST_PROGRAM');
          }}
        "#
            ),
            Duration::from_secs(40),
        );
        driver
            .finish()
            .map_err(|e| format!("{kind} {action}: {e}"))?;
        scene?;
        if home.join("service.pid").exists() {
            return Err("picker launched account service".into());
        }
        println!("NATIVE_ACCOUNT_DIALOG_STEP_OK: {kind} {action}");
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const button=name=>[...document.querySelectorAll('.account-service button')].find(n=>n.textContent===name);
      if(button('保存账户连接配置')?.disabled)throw new Error('ACCOUNT_SAVE_DISABLED');
      button('保存账户连接配置').click();
      await wait(()=>document.querySelector('.account-service article:first-of-type h3')?.textContent==='已保存的连接配置');
      await wait(()=>button('连接已保存服务')?.disabled===false);
      const config=await invoke('get_account_service_config',{requestId:'account-dialog-saved'});
      const quota=await invoke('get_account_quota',{requestId:'account-dialog-saved-disconnected'});
      if(!config.data.configured||config.data.auto_connect||config.data.settings_revision===window.__accountDialogRevision||config.data.executable_sha256!==window.__accountDialogHash||!config.data.home_display_path.endsWith('synthetic-account-dialog-home')||quota.data.state!=='disconnected')throw new Error('ACCOUNT_SAVE_INVALID');
      window.__accountSavedRevision=config.data.settings_revision;
    "#,
    )?;
    if home.join("service.pid").exists() {
        return Err("save launched account service".into());
    }
    let (preferences, _) = database
        .account_service_preferences()
        .map_err(|e| e.to_string())?;
    let target = preferences.target.ok_or("saved target missing")?;
    if std::path::Path::new(&target.executable_path)
        != executable.canonicalize().map_err(|e| e.to_string())?
        || target.home_path.as_ref().map(std::path::Path::new)
            != Some(home.canonicalize().map_err(|e| e.to_string())?.as_path())
    {
        return Err("persisted target not owned fixture".into());
    }
    super::mini_smoke::evaluate_with_timeout(
        app,
        &main,
        r#"
      const button=name=>[...document.querySelectorAll('.account-service button')].find(n=>n.textContent===name);
      button('连接已保存服务').click();
      await wait(()=>document.querySelector('.account-service .account-windows'),500);
      await wait(()=>button('刷新账户额度')?.disabled===false);
      const before=await invoke('get_account_quota',{requestId:'account-dialog-connected'});
      if(before.data.state!=='ready'||before.data.fetched_at_ms===null||before.data.windows.length!==1||!before.data.windows.some(w=>w.remaining_percent===75))throw new Error('ACCOUNT_CONNECT_NO_REAL_SERVICE_DTO');
      const rendered=[...document.querySelectorAll('.account-service .account-windows span')].map(n=>n.textContent);
      if(!rendered.includes('剩余 75%'))throw new Error('ACCOUNT_CONNECTED_UI_MISSING');
      button('刷新账户额度').click();
      await wait(()=>[...document.querySelectorAll('.account-service > p[role=status]')].some(n=>/已提交额度读取|额度读取正在进行|刷新额度|额度尚未到刷新时间/.test(n.textContent))&&button('刷新账户额度')?.disabled===false);
      if(document.querySelector('.account-service [role=alert]'))throw new Error('ACCOUNT_REFRESH_REJECTED');
      const after=await invoke('get_account_quota',{requestId:'account-dialog-refreshed'});
      if(after.data.state!=='ready'||after.data.connection_epoch!==before.data.connection_epoch||after.data.fetched_at_ms<before.data.fetched_at_ms)throw new Error('ACCOUNT_REFRESH_LOST_CONNECTION');
      button('断开本次连接').click();
      await wait(()=>document.querySelectorAll('.account-service .account-windows > div').length===0&&button('断开本次连接')?.disabled);
      const quota=await invoke('get_account_quota',{requestId:'account-dialog-disconnected'});
      const config=await invoke('get_account_service_config',{requestId:'account-dialog-retained-config'});
      if(quota.data.state!=='disconnected'||quota.data.windows.length||quota.data.fetched_at_ms!==null||config.data.settings_revision!==window.__accountSavedRevision||!config.data.configured)throw new Error('ACCOUNT_DISCONNECT_LOST_CONFIG_OR_RETAINED_VALUES');
    "#,
        Duration::from_secs(25),
    )?;
    if fs::read(&executable).map_err(|e| e.to_string())? != original
        || fs::read_to_string(home.join("fixture-mode")).map_err(|e| e.to_string())? != "service"
    {
        return Err("fixture source changed".into());
    }
    Ok(())
}
