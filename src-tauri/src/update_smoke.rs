//! Explicit actual WebView permission checks with isolated empty data, no network or install.
use tauri::{Listener, Manager};
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_UPDATES_IPC_OK: actual main/mini WebViews, configured idle or unavailable/null state, strict requests, main-only commands, direct plugin denied, shared privacy; no network, download or installation"
            ),
            Err(error) => eprintln!("NATIVE_UPDATES_IPC_FAILED: {error}"),
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}
fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let runtime = app.state::<super::RuntimeState>();
    if !runtime
        .data_directory
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("native-probe-"))
    {
        return Err("update smoke requires isolated database".into());
    }
    let initial_phase = runtime
        .updates
        .snapshot()
        .map_err(|_| "update state")?
        .phase;
    if option_env!("TOKENPULSE_UPDATER_PUBLIC_KEY").is_none()
        && initial_phase != token_pulse_core::updates::UpdatePhase::Idle
    {
        return Err("default project public key did not enable the update owner".into());
    }
    println!("NATIVE_UPDATES_INITIAL_STATE: {initial_phase:?}");
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(8);
    let listener = app.listen("updates_changed", move |_| {
        let _ = sender.try_send(());
    });
    let main_checks = r#"
      const expectedVersion=__COMPILED_VERSION__;
      window.__tokenPulseUpdatesExpectedVersion=expectedVersion;
      const sources=await invoke('get_sources',{requestId:'updates-isolated-sources'});
      const config=await invoke('get_account_service_config',{requestId:'updates-isolated-account'});
      if(sources.data.sources.length||config.data.auto_connect)throw new Error('UPDATES_NOT_ISOLATED');
      const before=await invoke('get_price_rules',{requestId:'updates-prices-before',revision:null});
      const read=()=>invoke('get_update_status',{requestId:'updates-status'});
      const initial=await read();
      const configured=initial.data.phase==='idle'&&initial.data.issue===null;
      const unavailable=initial.data.phase==='unavailable'&&initial.data.issue==='publication_not_configured';
      if((!configured&&!unavailable)||initial.data.update_revision!=='0'||initial.data.release!==null||initial.data.last_checked_at_ms!==null||initial.data.downloaded_bytes!==null||initial.data.total_bytes!==null||initial.data.current_version!==expectedVersion)throw new Error('UPDATES_UNKNOWN_STATE');
      window.__tokenPulseUpdatesConfigured=configured;
      const checks=[
        ['download_update',{requestId:'updates-download',request:{expected_update_revision:'0'}},configured?'INVALID_QUERY':'UPDATE_UNAVAILABLE'],
        ['install_update',{requestId:'updates-install',request:{expected_update_revision:'0'}},'UPDATE_UNAVAILABLE'],
        ['download_update',{requestId:'updates-stale',request:{expected_update_revision:'1'}},'REVISION_CONFLICT'],
        ['get_update_status',{requestId:''},'INVALID_QUERY']
      ];
      if(unavailable)checks.push(['check_for_updates',{requestId:'updates-check'},'UPDATE_UNAVAILABLE']);
      for(const [command,args,code] of checks){let denied=false;try{await invoke(command,args);}catch(e){denied=e.code===code;}if(!denied)throw new Error('UPDATES_CODE:'+command+':'+code);}
      for(const field of ['url','pubkey','verified','installer_path']){
        let denied=false;try{await invoke('download_update',{requestId:'updates-injected',request:{expected_update_revision:'0',[field]:'injected'}});}catch{denied=true;}if(!denied)throw new Error('UPDATES_REQUEST_INJECTION');
      }
      let denied=false;try{await invoke('plugin:updater|check',{});}catch{denied=true;}if(!denied)throw new Error('UPDATES_DIRECT_PLUGIN_ALLOWED');
      const settings=await invoke('get_display_settings',{requestId:'updates-policy'});
      await invoke('set_display_privacy',{requestId:'updates-hide',request:{privacy:true,expected_settings_revision:settings.data.settings_revision}});
      const hidden=await read();
      if(!hidden.display_policy.privacy||JSON.stringify(hidden.data)!==JSON.stringify(initial.data))throw new Error('UPDATES_PRIVATE_PROJECTION');
      const current=await invoke('get_display_settings',{requestId:'updates-hidden-policy'});
      await invoke('set_display_privacy',{requestId:'updates-show',request:{privacy:false,expected_settings_revision:current.data.settings_revision}});
      const after=await invoke('get_price_rules',{requestId:'updates-prices-after',revision:null});
      if(after.data.price_revision!==before.data.price_revision||JSON.stringify((await read()).data)!==JSON.stringify(initial.data))throw new Error('UPDATES_REJECTED_MUTATION');
    "#.replace("__COMPILED_VERSION__", &serde_json::to_string(env!("CARGO_PKG_VERSION")).map_err(|_| "compiled version")?);
    let result = super::mini_smoke::evaluate(app, &main, &main_checks).and_then(|_| super::mini_smoke::evaluate(app, &mini, r#"
      for(const command of ['get_update_status','check_for_updates','download_update','install_update','plugin:updater|check']){
        let denied=false;try{await invoke(command,{requestId:'updates-mini',request:{expected_update_revision:'0'}});}catch{denied=true;}if(!denied)throw new Error('UPDATES_MINI_ALLOWED:'+command);
      }
    "#));
    let result = result.and_then(|_| super::mini_smoke::evaluate(app, &main, r#"
      [...document.querySelectorAll('.sidebar nav button')].find(n=>n.textContent==='设置').click();
      await wait(()=>[...document.querySelectorAll('[role=tab]')].some(n=>n.textContent==='软件更新'));
      [...document.querySelectorAll('[role=tab]')].find(n=>n.textContent==='软件更新').click();
      const configured=window.__tokenPulseUpdatesConfigured;
      const expectedVersion=window.__tokenPulseUpdatesExpectedVersion;
      if(typeof expectedVersion!=='string')throw new Error('UPDATES_EXPECTED_VERSION');
      const expectedPhase=configured?'尚未检查更新':'更新不可用';
      await wait(()=>document.querySelector('.update-panel [role=status]')?.textContent===expectedPhase);
      const panel=document.querySelector('.update-panel');
      const button=(name)=>[...panel.querySelectorAll('button')].find(n=>n.textContent===name);
      if(!button('检查更新')||button('检查更新').disabled===configured||button('安装更新')||panel.textContent.includes('未配置有效的更新签名公钥')===configured||panel.textContent.includes('当前已是最新版本')||panel.querySelector('progress'))throw new Error('UPDATES_UI_AVAILABILITY');
      if(!panel.textContent.includes(expectedVersion)||!panel.textContent.includes('尚未提供'))throw new Error('UPDATES_UI_NULL');
      button('刷新更新状态').click();
      await wait(()=>document.querySelector('.update-panel [role=status]')?.textContent===expectedPhase);
      const settings=await invoke('get_display_settings',{requestId:'updates-ui-privacy'});
      await invoke('set_display_privacy',{requestId:'updates-ui-hide',request:{privacy:true,expected_settings_revision:settings.data.settings_revision}});
      await wait(()=>document.querySelector('.update-panel [role=status]')?.textContent===expectedPhase);
      if(!document.querySelector('.update-panel').textContent.includes(expectedVersion))throw new Error('UPDATES_UI_PRIVATE_PUBLIC_VERSION');
      const hidden=await invoke('get_display_settings',{requestId:'updates-ui-hidden'});
      await invoke('set_display_privacy',{requestId:'updates-ui-show',request:{privacy:false,expected_settings_revision:hidden.data.settings_revision}});
    "#));
    app.unlisten(listener);
    result?;
    if receiver.try_iter().next().is_some() {
        return Err("rejected updates emitted a mutation".into());
    }
    Ok(())
}
