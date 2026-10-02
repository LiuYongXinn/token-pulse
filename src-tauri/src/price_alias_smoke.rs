//! Explicit isolated real WebView/SQLite contract check; no logs/accounts/market rates.
use tauri::Manager;
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_PRICE_ALIAS_OK: real main WebView versioned create/replace/retire/history/CAS/conflicts, committed notifications, mini denied, shared privacy projection; UI editing acceptance separate"
            ),
            Err(error) => eprintln!("NATIVE_PRICE_ALIAS_FAILED: {error}"),
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
        return Err("alias smoke requires isolated database".into());
    }
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    use tauri::Listener;
    let (sender, receiver) = std::sync::mpsc::sync_channel(8);
    let subscription = app.listen("price_rules_changed", move |event| {
        let _ = sender.try_send(event.payload().to_owned());
    });
    let result = super::mini_smoke::evaluate(app, &main, r#"
      const read=await invoke('get_price_rules',{requestId:'alias-initial',revision:null});
      if(read.data.price_revision!=='0'||read.data.aliases.length)throw new Error('ALIAS_NOT_ISOLATED');
      const draft={provider:'synthetic-provider',alias:'synthetic-snapshot',canonical_model:'synthetic-canonical'};
      const write=(request,revision,id)=>invoke('mutate_model_alias',{requestId:id,request,expectedPriceRevision:revision});
      const created=await write({kind:'create',draft},'0','alias-create');
      if(created.data.price_revision!=='1'||created.data.aliases[0].canonical_model!==draft.canonical_model)throw new Error('ALIAS_CREATE');
      for(const [request,revision,expected,id] of [
        [{kind:'create',draft},'0','REVISION_CONFLICT','alias-stale'],
        [{kind:'create',draft},'1','PRICE_RULE_CONFLICT','alias-duplicate'],
        [{kind:'create',draft:{...draft,alias:'incoming',canonical_model:draft.alias}},'1','PRICE_RULE_CONFLICT','alias-chain']
      ]){let rejected=false;try{await write(request,revision,id);}catch(error){rejected=error.code===expected;}if(!rejected)throw new Error(id);}
      const changed=await write({kind:'replace',alias_id:created.data.aliases[0].alias_id,draft:{...draft,canonical_model:'synthetic-other'}},'1','alias-replace');
      if(changed.data.price_revision!=='2'||changed.data.aliases[0].alias_id===created.data.aliases[0].alias_id)throw new Error('ALIAS_REPLACE');
      const historical=await invoke('get_price_rules',{requestId:'alias-history',revision:'1'});
      if(historical.data.aliases[0].canonical_model!==draft.canonical_model)throw new Error('ALIAS_HISTORY');
      const settings=await invoke('get_display_settings',{requestId:'alias-policy'});
      await invoke('set_display_privacy',{requestId:'alias-private',request:{privacy:true,expected_settings_revision:settings.data.settings_revision}});
      const privateResult=await write({kind:'replace',alias_id:changed.data.aliases[0].alias_id,draft},'2','alias-private-replace');
      if(!privateResult.display_policy.privacy||privateResult.data.aliases.length||privateResult.data.rules.length||privateResult.data.price_revision!=='3')throw new Error('ALIAS_PRIVACY');
      const privateSettings=await invoke('get_display_settings',{requestId:'alias-private-settings'});
      await invoke('set_display_privacy',{requestId:'alias-public',request:{privacy:false,expected_settings_revision:privateSettings.data.settings_revision}});
      const current=await invoke('get_price_rules',{requestId:'alias-current',revision:null});
      if(current.data.aliases[0].canonical_model!==draft.canonical_model)throw new Error('ALIAS_PRIVATE_COMMIT_LOST');
      const retired=await write({kind:'retire',alias_id:current.data.aliases[0].alias_id},'3','alias-retire');
      if(retired.data.price_revision!=='4'||retired.data.aliases.length)throw new Error('ALIAS_RETIRE');
    "#).and_then(|_| super::mini_smoke::evaluate(app, &mini, r#"
      let denied=false;try{await invoke('mutate_model_alias',{requestId:'alias-mini-denied',request:{kind:'create',draft:{provider:'synthetic',alias:'a',canonical_model:'b'}},expectedPriceRevision:'4'});}catch{denied=true;}if(!denied)throw new Error('ALIAS_MINI_ALLOWED');
    "#));
    app.unlisten(subscription);
    result?;
    let revisions: Vec<String> = receiver
        .try_iter()
        .map(|payload| {
            serde_json::from_str::<token_pulse_core::pricing::PriceChanged>(&payload)
                .map(|value| value.price_revision.as_str().to_owned())
                .map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    if revisions != ["1", "2", "3", "4"] {
        return Err(format!(
            "alias committed notifications mismatch: {revisions:?}"
        ));
    }
    Ok(())
}
