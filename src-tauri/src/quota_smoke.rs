//! Debug-only Windows WebView acceptance using an isolated synthetic native account service.
//! Injects a picker capability, never opens a real account or pretends to verify the OS picker.
use std::{
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;
use token_pulse_core::{protocol::QuotaState, quota::AccountServiceCandidate};

fn wait_ready(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<super::RuntimeState>();
    let service = state.quota.as_ref().map_err(|e| format!("{e:?}"))?;
    for _ in 0..100 {
        if matches!(
            service.snapshot().map_err(|e| format!("{e:?}"))?.state,
            QuotaState::Ready
        ) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(30));
    }
    Err("synthetic account did not become ready".into())
}
pub fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<super::RuntimeState>();
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let service = state.quota.as_ref().map_err(|e| format!("{e:?}"))?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    let home = state.data_directory.join("synthetic-account-home");
    std::fs::create_dir(&home).map_err(|e| e.to_string())?;
    std::fs::write(home.join("fixture-mode"), "service").map_err(|e| e.to_string())?;
    let fixture = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("quota-fixture.exe");
    let exe = home.join("synthetic-codex.exe");
    std::fs::copy(fixture, &exe).map_err(|e| e.to_string())?;
    let target = token_pulse_quota::NativeService::inspect(&exe, Some(&home))
        .map_err(|e| format!("{e:?}"))?;
    let (_, revision) = db
        .account_service_preferences()
        .map_err(|e| e.to_string())?;
    let handle = uuid::Uuid::new_v4().to_string();
    state
        .quota_selections
        .lock()
        .map_err(|_| "selection lock")?
        .insert(
            handle.clone(),
            AccountServiceCandidate {
                target: target.clone(),
                settings_revision: revision,
            },
            Instant::now(),
        )
        .map_err(|e| format!("{e:?}"))?;
    let handle_json = serde_json::to_string(&handle).map_err(|e| e.to_string())?;
    super::mini_smoke::evaluate(
        app,
        &main,
        &format!(
            r#"
      const before=await invoke('get_account_service_config',{{requestId:'native-account-config'}});
      if(before.data.configured || before.data.auto_connect)throw new Error('ACCOUNT_DEFAULT_FAKE');
      const saved=await invoke('save_account_service_config',{{requestId:'native-account-save',request:{{selection_handle:{handle_json},auto_connect:false,expected_settings_revision:before.data.settings_revision}}}});
      if(!saved.data.configured || saved.data.auto_connect || !saved.data.home_display_path || saved.data.executable_sha256.length!==64)throw new Error('ACCOUNT_CONFIG_SAVE_INVALID');
      const quota=await invoke('get_account_quota',{{requestId:'native-account-not-launched'}});
      if(quota.data.state!=='disconnected')throw new Error('SAVE_LAUNCHED_ACCOUNT');
    "#
        ),
    )?;
    if home.join("service.pid").exists() {
        return Err("saving launched synthetic service".into());
    }
    let (_, revision) = db
        .account_service_preferences()
        .map_err(|e| e.to_string())?;
    let handle = uuid::Uuid::new_v4().to_string();
    state
        .quota_selections
        .lock()
        .map_err(|_| "selection lock")?
        .insert(
            handle.clone(),
            AccountServiceCandidate {
                target,
                settings_revision: revision,
            },
            Instant::now(),
        )
        .map_err(|e| format!("{e:?}"))?;
    let handle_json = serde_json::to_string(&handle).map_err(|e| e.to_string())?;
    let fault =
        token_pulse_store::rusqlite::Connection::open(db.path()).map_err(|e| e.to_string())?;
    fault.execute_batch("CREATE TRIGGER reject_quota_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'synthetic quota failure'); END;").map_err(|e|e.to_string())?;
    let result = super::mini_smoke::evaluate(
        app,
        &main,
        &format!(
            r#"
      const before=await invoke('get_account_service_config',{{requestId:'native-account-fault-before'}});
      let failed=false;try{{await invoke('save_account_service_config',{{requestId:'native-account-fault',request:{{selection_handle:{handle_json},auto_connect:true,expected_settings_revision:before.data.settings_revision}}}});}}catch(e){{failed=e.code==='DB_WRITE_FAILED';}}
      const after=await invoke('get_account_service_config',{{requestId:'native-account-fault-after'}});
      if(!failed || after.data.auto_connect || after.data.settings_revision!==before.data.settings_revision || after.data.executable_sha256!==before.data.executable_sha256)throw new Error('ACCOUNT_FAILED_WRITE_LOST_CONFIG');
    "#
        ),
    );
    fault
        .execute_batch("DROP TRIGGER reject_quota_revision;")
        .map_err(|e| e.to_string())?;
    result?;
    super::mini_smoke::evaluate(
        app,
        &main,
        &format!(
            r#"
      const config=await invoke('get_account_service_config',{{requestId:'native-account-retry-read'}});
      const saved=await invoke('save_account_service_config',{{requestId:'native-account-retry',request:{{selection_handle:{handle_json},auto_connect:true,expected_settings_revision:config.data.settings_revision}}}});
      if(!saved.data.auto_connect)throw new Error('ACCOUNT_CAPABILITY_NOT_RETRYABLE');
    "#
        ),
    )?;
    // Exercise the real saved-startup hook. This is same-process acceptance, not a cold restart.
    super::quota_config::initialize(app);
    wait_ready(app)?;
    let initial_epoch = service
        .snapshot()
        .map_err(|e| format!("{e:?}"))?
        .connection_epoch;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const settings=[...document.querySelectorAll('nav button')].find(b=>b.textContent==='设置');settings.click();
      await wait(()=>[...document.querySelectorAll('[role=tab]')].some(b=>b.textContent==='数据来源'));
      [...document.querySelectorAll('[role=tab]')].find(b=>b.textContent==='数据来源').click();
      await wait(()=>document.querySelector('section[aria-label="账户额度连接"] input[type=checkbox]:checked:not(:disabled)'));
      const region=document.querySelector('section[aria-label="账户额度连接"]');
      region.querySelector('input[type=checkbox]').click();
      await wait(()=>[...region.querySelectorAll('button')].some(b=>b.textContent==='保存账户连接配置'&&!b.disabled));
      [...region.querySelectorAll('button')].find(b=>b.textContent==='保存账户连接配置').click();
      await wait(()=>region.textContent.includes('连接配置已保存；'));
      const config=await invoke('get_account_service_config',{requestId:'native-account-ui-config'});
      if(config.data.auto_connect)throw new Error('ACCOUNT_UI_AUTO_NOT_SAVED');
    "#,
    )?;
    if service
        .snapshot()
        .map_err(|e| format!("{e:?}"))?
        .connection_epoch
        != initial_epoch
    {
        return Err("saving replaced active connection".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const config=await invoke('get_account_service_config',{requestId:'native-account-guard-config'});
      const quota=await invoke('get_account_quota',{requestId:'native-account-guard-quota'});
      let stale=false;try{await invoke('manage_account_connection',{requestId:'native-account-wrong-sha',request:{kind:'connect',expected_settings_revision:config.data.settings_revision,expected_connection_epoch:quota.data.connection_epoch,acknowledged_executable_sha256:'0'.repeat(64)}});}catch(e){stale=e.code==='STALE_CONFIRMATION';}
      let conflict=false;try{await invoke('manage_account_connection',{requestId:'native-account-wrong-revision',request:{kind:'connect',expected_settings_revision:'0',expected_connection_epoch:quota.data.connection_epoch,acknowledged_executable_sha256:config.data.executable_sha256}});}catch(e){conflict=e.code==='REVISION_CONFLICT';}
      const after=await invoke('get_account_quota',{requestId:'native-account-guard-after'});
      if(!stale || !conflict || quota.data.connection_epoch!==after.data.connection_epoch)throw new Error('ACCOUNT_CONNECTION_GUARDS_FAILED');
      const region=document.querySelector('section[aria-label="账户额度连接"]');
      await wait(()=>[...region.querySelectorAll('button')].some(b=>b.textContent==='连接已保存服务'&&!b.disabled));
      [...region.querySelectorAll('button')].find(b=>b.textContent==='连接已保存服务').click();
      let connected=false;
      for(let i=0;i<100;i++) {
        const current=await invoke('get_account_quota',{requestId:'native-account-ui-wait'});
        if(current.data.connection_epoch!==quota.data.connection_epoch && current.data.state==='ready'){connected=true;break;}
        await new Promise(r=>setTimeout(r,30));
      }
      if(!connected)throw new Error('ACCOUNT_UI_CONNECTION_NOT_REPLACED');
      await wait(()=>region.textContent.includes('剩余 75%') && [...region.querySelectorAll('button')].some(b=>b.textContent==='连接已保存服务'&&!b.disabled));
    "#,
    )?;
    wait_ready(app)?;
    let pid: u32 = std::fs::read_to_string(home.join("service.pid"))
        .map_err(|e| e.to_string())?
        .parse()
        .map_err(|_| "invalid synthetic pid")?;
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if process.is_null() {
        return Err("synthetic process unavailable".into());
    }
    let result = super::mini_smoke::evaluate(app, &mini, r#"
      const quota=await invoke('get_account_quota',{requestId:'native-mini-connected-quota'});
      if(quota.data.state!=='ready' || quota.data.windows[0].remaining_percent!==75)throw new Error('MINI_ACCOUNT_READ_FAILED');
      for(const command of ['get_account_service_config','choose_account_service','save_account_service_config','cancel_account_service_selection','manage_account_connection']) {
        let denied=false;try{await invoke(command,{requestId:'native-mini-account-denied',selectionHandle:'irrelevant',request:{kind:'disconnect',expected_connection_epoch:quota.data.connection_epoch}});}catch{denied=true;}
        if(!denied)throw new Error('MINI_ACCOUNT_PERMISSION_TOO_BROAD');
      }
    "#).and_then(|_| super::mini_smoke::evaluate(app, &main, r#"
      const region=document.querySelector('section[aria-label="账户额度连接"]');
      const select=region.querySelector('select[aria-label="账户额度桶"]');
      if(!select || ![...select.options].some(o=>o.value==='other'))throw new Error('ACCOUNT_LIMIT_SELECT_MISSING');
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(select,'other');select.dispatchEvent(new Event('change',{bubbles:true}));
      await wait(()=>region.textContent.includes('剩余 20%'));
      const selected=await invoke('get_account_quota',{requestId:'native-account-selected'});
      if(selected.data.selected_limit_id!=='other' || selected.data.windows[0].duration_mins!==10080)throw new Error('ACCOUNT_BUCKET_MISMATCH');
      [...region.querySelectorAll('button')].find(b=>b.textContent==='断开本次连接').click();
      await wait(()=>[...region.querySelectorAll('[role=status]')].some(s=>s.textContent==='未连接'));
      const cleared=await invoke('get_account_quota',{requestId:'native-account-cleared'});
      if(cleared.data.windows.length || cleared.data.fetched_at_ms!==null)throw new Error('ACCOUNT_DISCONNECT_RETAINED_QUOTA');
    "#));
    let terminated = unsafe { WaitForSingleObject(process, 2000) };
    unsafe {
        CloseHandle(process);
    }
    result?;
    if terminated != 0 {
        return Err("owned account process survived disconnect".into());
    }
    println!(
        "NATIVE_ACCOUNT_CONFIG_OK: synthetic native executable, exact IPC guards, atomic Writer failure and retry, persisted startup hook (same process), real WebView configuration/save/connect/select/disconnect, mini permissions, owned process termination; OS picker and live login remain unverified"
    );
    Ok(())
}
