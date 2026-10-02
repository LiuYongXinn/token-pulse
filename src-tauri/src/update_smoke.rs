//! Explicit actual WebView permission checks with isolated empty data and no publication key.
use tauri::{Listener, Manager};
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_UPDATES_IPC_OK: actual main/mini WebViews, unavailable/null state, strict requests, main-only custom commands, direct plugin denied, shared privacy; no download or installation"
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
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(8);
    let listener = app.listen("updates_changed", move |_| {
        let _ = sender.try_send(());
    });
    let result = super::mini_smoke::evaluate(app, &main, r#"
      const sources=await invoke('get_sources',{requestId:'updates-isolated-sources'});
      const config=await invoke('get_account_service_config',{requestId:'updates-isolated-account'});
      if(sources.data.sources.length||config.data.auto_connect)throw new Error('UPDATES_NOT_ISOLATED');
      const before=await invoke('get_price_rules',{requestId:'updates-prices-before',revision:null});
      const read=()=>invoke('get_update_status',{requestId:'updates-status'});
      const initial=await read();
      if(initial.data.phase!=='unavailable'||initial.data.issue!=='publication_not_configured'||initial.data.update_revision!=='0'||initial.data.release!==null||initial.data.last_checked_at_ms!==null||initial.data.downloaded_bytes!==null||initial.data.total_bytes!==null||initial.data.current_version!=='0.1.0')throw new Error('UPDATES_UNKNOWN_STATE');
      for(const [command,args,code] of [
        ['check_for_updates',{requestId:'updates-check'},'UPDATE_UNAVAILABLE'],
        ['download_update',{requestId:'updates-download',request:{expected_update_revision:'0'}},'UPDATE_UNAVAILABLE'],
        ['download_update',{requestId:'updates-stale',request:{expected_update_revision:'1'}},'REVISION_CONFLICT'],
        ['get_update_status',{requestId:''},'INVALID_QUERY']
      ]){let denied=false;try{await invoke(command,args);}catch(e){denied=e.code===code;}if(!denied)throw new Error('UPDATES_CODE:'+command+':'+code);}
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
    "#).and_then(|_| super::mini_smoke::evaluate(app, &mini, r#"
      for(const command of ['get_update_status','check_for_updates','download_update','plugin:updater|check']){
        let denied=false;try{await invoke(command,{requestId:'updates-mini',request:{expected_update_revision:'0'}});}catch{denied=true;}if(!denied)throw new Error('UPDATES_MINI_ALLOWED:'+command);
      }
    "#));
    app.unlisten(listener);
    result?;
    if receiver.try_iter().next().is_some() {
        return Err("rejected updates emitted a mutation".into());
    }
    Ok(())
}
