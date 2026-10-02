//! Explicit own-data scene. Runs the production boot publication and actual WebView commands.
use tauri::Manager;
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_OFFLINE_PRICES_OK: production startup publication, immutable main IPC/history, 172 factual prices, React search/tier/history, mini denied, latest shared privacy, idempotent writer and unchanged consumption"
            ),
            Err(error) => eprintln!("NATIVE_OFFLINE_PRICES_FAILED: {error}"),
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}
fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<super::RuntimeState>();
    if !state
        .data_directory
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("native-probe-"))
    {
        return Err("offline scene requires isolated database".into());
    }
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let before = db
        .snapshot(|_, revision| Ok((revision.data, revision.settings)))
        .map_err(|e| e.to_string())?;
    let catalog = token_pulse_core::pricing::offline::OfflinePriceCatalog::bundled()
        .map_err(|e| e.to_string())?;
    if db
        .install_offline_price_catalog(
            catalog,
            token_pulse_collector::jobs::now_ms().map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?
        != 1
    {
        return Err("duplicate boot advanced price revision".into());
    }
    let main = app.get_webview_window("main").ok_or("missing main")?;
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("missing mini")?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const catalog=await invoke('get_offline_price_catalog',{requestId:'offline-current',revision:null});
      const rules=await invoke('get_price_rules',{requestId:'offline-rules',revision:'1'});
      if(catalog.data.price_revision!=='1'||catalog.data.catalog.entries.length!==172||rules.data.rules.length!==37)throw new Error('OFFLINE_BOOT_CONTENT');
      if(rules.data.rules.some(r=>r.model_exact==='gpt-6.1-sol'||r.model_exact==='gpt-5.5'||r.origin!=='offline'||r.effective_from_ms!==1790899200000))throw new Error('OFFLINE_UNSAFE_RULE');
      const empty=await invoke('get_offline_price_catalog',{requestId:'offline-history-zero',revision:'0'});
      if(empty.data.catalog!==null||empty.data.price_revision!=='0')throw new Error('OFFLINE_HISTORY_ZERO');
      for(const revision of ['2','-1','01','9223372036854775808']) {let rejected=false;try{await invoke('get_offline_price_catalog',{requestId:'offline-bad-version',revision});}catch{rejected=true;}if(!rejected)throw new Error('OFFLINE_BAD_VERSION');}
      [...document.querySelectorAll('.sidebar nav button')].find(n=>n.textContent==='设置').click();
      await wait(()=>[...document.querySelectorAll('[role=tab]')].some(n=>n.textContent==='价格规则'));
      [...document.querySelectorAll('[role=tab]')].find(n=>n.textContent==='价格规则').click();
      await wait(()=>document.querySelector('.offline-prices')?.textContent.includes('51 个模型 / 172 条单价'));
      const fill=(node,value)=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(node,value);node.dispatchEvent(new Event('input',{bubbles:true}));};
      const root=()=>document.querySelector('.offline-prices');
      fill(root().querySelector('input'),'gpt-6.1-sol');
      await wait(()=>root().querySelectorAll('tbody tr').length===2);
      if(!root().textContent.includes('需请求档位与缓存写入')||!root().textContent.includes('2.50'))throw new Error('OFFLINE_UI_CONDITIONS');
      const select=root().querySelector('select');Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(select,'fast');select.dispatchEvent(new Event('change',{bubbles:true}));
      await wait(()=>root().querySelector('tbody')?.textContent.includes('Fast'));
      if(!root().textContent.includes('非默认参考模式'))throw new Error('OFFLINE_UI_TIER');
      fill(document.querySelector('[aria-label="历史价格版本"]'),'0');
      const history=document.querySelector('.price-version form');history.querySelector('button').click();
      await wait(()=>document.querySelector('.price-version')?.textContent.startsWith('历史价格版本 0')&&root()?.textContent.includes('此价格版本尚无内置目录'));
      [...document.querySelectorAll('button')].find(n=>n.textContent==='刷新当前版本').click();
      await wait(()=>root()?.textContent.includes('51 个模型 / 172 条单价'));
    "#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      let denied=false;try{await invoke('get_offline_price_catalog',{requestId:'offline-mini-denied',revision:null});}catch{denied=true;}if(!denied)throw new Error('OFFLINE_MINI_ALLOWED');
    "#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const settings=await invoke('get_display_settings',{requestId:'offline-policy'});
      await invoke('set_display_privacy',{requestId:'offline-private',request:{privacy:true,expected_settings_revision:settings.data.settings_revision}});
      const hidden=await invoke('get_offline_price_catalog',{requestId:'offline-hidden',revision:'1'});
      if(!hidden.display_policy.privacy||hidden.data.catalog!==null||hidden.data.price_revision!=='1')throw new Error('OFFLINE_PRIVACY');
      await wait(()=>!document.querySelector('.offline-prices'));
      const next=await invoke('get_display_settings',{requestId:'offline-private-settings'});
      await invoke('set_display_privacy',{requestId:'offline-public',request:{privacy:false,expected_settings_revision:next.data.settings_revision}});
      await wait(()=>document.querySelector('.offline-prices')?.textContent.includes('51 个模型 / 172 条单价'));
      if(document.querySelector('.offline-prices input').value!=='')throw new Error('OFFLINE_PRIVACY_RESTORED_SEARCH');
    "#,
    )?;
    if db
        .snapshot(|_, revision| Ok(revision.data))
        .map_err(|e| e.to_string())?
        != before.0
    {
        return Err("catalog changed consumption revision".into());
    }
    Ok(())
}
