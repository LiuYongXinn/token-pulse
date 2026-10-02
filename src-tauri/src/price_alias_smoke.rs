//! Explicit isolated real WebView/SQLite contract check; no logs/accounts/market rates.
use tauri::Manager;
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_PRICE_ALIAS_OK: real main WebView versioned create/replace/retire/history/CAS/conflicts, committed notifications, mini denied, shared privacy projection, actual React alias form create/replace/history/retire and captured stale draft"
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
    let result = result.and_then(|_| verify_editor(app, &main));
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
    if revisions != ["1", "2", "3", "4", "5", "6", "7", "8"] {
        return Err(format!(
            "alias committed notifications mismatch: {revisions:?}"
        ));
    }
    Ok(())
}
fn verify_editor(app: &tauri::AppHandle, main: &tauri::WebviewWindow) -> Result<(), String> {
    super::mini_smoke::evaluate(
        app,
        main,
        r#"
      [...document.querySelectorAll('.sidebar nav button')].find(n=>n.textContent==='设置').click();
      await wait(()=>[...document.querySelectorAll('[role=tab]')].some(n=>n.textContent==='价格规则'));
      [...document.querySelectorAll('[role=tab]')].find(n=>n.textContent==='价格规则').click();
      const find=(text,root=document)=>[...root.querySelectorAll('button')].find(n=>n.textContent===text);
      const input=(label,root=document)=>[...root.querySelectorAll('label')].find(n=>n.firstChild?.textContent===label)?.querySelector('input');
      const fill=(node,value)=>{if(!node)throw new Error('ALIAS_UI_INPUT_MISSING');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(node,value);node.dispatchEvent(new Event('input',{bubbles:true}));};
      const version=(n)=>document.querySelector('.price-version')?.textContent.startsWith('当前价格版本 '+n);
      await wait(()=>find('新增别名')?.disabled===false&&version(4));
      find('新增别名').click();
      await wait(()=>document.querySelector('form[aria-label="新增模型别名"]'));
      let form=document.querySelector('form[aria-label="新增模型别名"]');
      fill(input('提供方',form),'synthetic-ui');fill(input('日志模型标识',form),'synthetic-ui-snapshot');fill(input('标准模型标识',form),'synthetic-ui-canonical');
      await new Promise(resolve=>setTimeout(resolve,50));
      find('保存别名并发布版本',form).click();await wait(()=>version(5)&&find('编辑别名'));
      find('编辑别名').click();await wait(()=>document.querySelector('form[aria-label="替换模型别名"]'));
      form=document.querySelector('form[aria-label="替换模型别名"]');fill(input('标准模型标识',form),'synthetic-ui-changed');
      await new Promise(resolve=>setTimeout(resolve,50));find('保存别名并发布版本',form).click();
      await wait(()=>version(6)&&document.querySelector('.model-alias-table')?.textContent.includes('synthetic-ui-changed'));
      fill(document.querySelector('[aria-label="历史价格版本"]'),'5');await new Promise(resolve=>setTimeout(resolve,50));find('查看').click();
      await wait(()=>document.querySelector('.price-version')?.textContent.startsWith('历史价格版本 5'));
      if(!document.querySelector('.model-alias-table')?.textContent.includes('synthetic-ui-canonical')||find('新增别名').disabled!==true||find('编辑别名'))throw new Error('ALIAS_UI_HISTORY_NOT_READONLY');
      find('刷新当前版本').click();await wait(()=>version(6)&&find('退休别名'));
      find('退休别名').click();await wait(()=>version(7)&&document.querySelector('.model-alias-empty'));
      find('新增别名').click();await wait(()=>document.querySelector('form[aria-label="新增模型别名"]'));
      form=document.querySelector('form[aria-label="新增模型别名"]');fill(input('提供方',form),'synthetic-ui');fill(input('日志模型标识',form),'unsaved-ui');fill(input('标准模型标识',form),'unsaved-canonical');
      await invoke('mutate_model_alias',{requestId:'alias-ui-external',request:{kind:'create',draft:{provider:'synthetic-external',alias:'external',canonical_model:'external-canonical'}},expectedPriceRevision:'7'});
      find('刷新当前版本').click();await wait(()=>version(8));find('保存别名并发布版本',form).click();
      await wait(()=>document.querySelector('.price-panel [role=alert]')?.textContent.includes('配置或作业状态已发生变化'));
      if(!form.isConnected||input('日志模型标识',form).value!=='unsaved-ui'||!form.textContent.includes('基于版本 7 发布'))throw new Error('ALIAS_UI_STALE_DRAFT_LOST');
      const settings=await invoke('get_display_settings',{requestId:'alias-ui-policy'});
      await invoke('set_display_privacy',{requestId:'alias-ui-policy-hide',request:{privacy:true,expected_settings_revision:settings.data.settings_revision}});
      await wait(()=>!document.querySelector('.price-panel')&&document.body.textContent.includes('价格规则已隐藏'));
      if(document.body.textContent.includes('unsaved-ui')||document.querySelector('input[value="unsaved-ui"]'))throw new Error('ALIAS_UI_PRIVATE_DRAFT_NOT_CLEARED');
      const hiddenSettings=await invoke('get_display_settings',{requestId:'alias-ui-hidden-policy'});
      await invoke('set_display_privacy',{requestId:'alias-ui-policy-show',request:{privacy:false,expected_settings_revision:hiddenSettings.data.settings_revision}});
      await wait(()=>version(8)&&find('新增别名')?.disabled===false);
      if(document.querySelector('form[aria-label="新增模型别名"]')||document.body.textContent.includes('unsaved-ui'))throw new Error('ALIAS_UI_PRIVATE_DRAFT_RESTORED');
    "#,
    )
}
