//! Explicit debug-only scene with own synthetic source, actual reader, public IPC and WebViews.
use std::{fs, time::Duration};
use tauri::Manager;
use token_pulse_store::{SourceRecord, source_management::SourceMutation};

pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_DIAGNOSTIC_POSITIONS_OK: own readonly rollout, exact position, missing file, real main UI/IPC, latest privacy, mini denied, resume replacement and explicit source reread preserve source bytes"
            ),
            Err(error) => eprintln!("NATIVE_DIAGNOSTIC_POSITIONS_FAILED: {error}"),
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
        return Err("diagnostic scene requires isolated debug database".into());
    }
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let root = state.data_directory.join("synthetic-diagnostic-source");
    fs::create_dir_all(root.join("sessions")).map_err(|e| e.to_string())?;
    let path = root.join("sessions").join("problem.jsonl");
    let head = b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"diagnostic-fixture\"}}\n";
    let body = b"{\"type\":\"unknown_record\",\"body\":\"private synthetic body never stored\"}\n";
    let bytes = [head.as_slice(), body.as_slice()].concat();
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    db.add_source(SourceRecord {
        source_id: "native-diagnostics".into(),
        root_path: root.to_str().ok_or("source path unavailable")?.into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .map_err(|e| e.to_string())?;
    token_pulse_collector::collect_file(db, "native-diagnostics", &path, 2)
        .map_err(|e| e.to_string())?;
    let saved = root.join("outside-rollouts.jsonl");
    fs::rename(&path, &saved).map_err(|e| e.to_string())?;
    let scan = db
        .begin_source_scan(
            "native-diagnostics".into(),
            root.to_str().unwrap().into(),
            3,
        )
        .map_err(|e| e.to_string())?;
    db.finish_source_scan(scan, None, 3)
        .map_err(|e| e.to_string())?;
    let revision = db
        .snapshot(|_, rev| Ok(rev.settings))
        .map_err(|e| e.to_string())?;
    db.mutate_sources(
        SourceMutation::Pause("native-diagnostics".into()),
        revision,
        4,
    )
    .map_err(|e| e.to_string())?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    let expected_path = serde_json::to_string(path.to_str().unwrap()).map_err(|e| e.to_string())?;
    let script = r#"
      const expected=EXPECTED_PATH;
      const settings=await invoke('get_display_settings',{requestId:'diagnostic-settings'});
      if(settings.data.preferences.display_timezone===null)await invoke('set_display_timezone',{requestId:'diagnostic-timezone',request:{timezone:'UTC',expected_settings_revision:settings.data.settings_revision}});
      const value=await invoke('query_diagnostics',{requestId:'diagnostic-locations',request:{source_id:'native-diagnostics'}});
      if(value.data.issues.length!==2 || value.data.has_more || value.data.issues.some(i=>i.path!==expected))throw new Error('DIAGNOSTIC_POSITIONS_INCORRECT');
      const record=value.data.issues.find(i=>i.kind==='log_record');
      if(record?.code!=='UNSUPPORTED_FORMAT'||record.byte_offset!=='EXPECTED_OFFSET')throw new Error('DIAGNOSTIC_OFFSET_INCORRECT');
      if(value.data.issues.find(i=>i.kind==='missing_file')?.byte_offset!==null || JSON.stringify(value.data).includes('synthetic body'))throw new Error('DIAGNOSTIC_UNKNOWN_OR_CONTENT');
      let rejected=false;try{await invoke('query_diagnostics',{requestId:'diagnostic-bad-source',request:{source_id:'../auth.json'}});}catch(e){rejected=e.code==='INVALID_QUERY';}if(!rejected)throw new Error('ARBITRARY_SOURCE_ACCEPTED');
      [...document.querySelectorAll('.sidebar nav button')].find(n=>n.textContent==='采集诊断').click();
      await wait(()=>document.querySelectorAll('.diagnostic-issue-list li').length===2);
      if(!document.querySelector('.diagnostics-issues').textContent.includes('problem.jsonl'))throw new Error('LOCATION_UI_MISSING');
      const s=await invoke('get_display_settings',{requestId:'diagnostic-private-read'});
      await invoke('set_display_privacy',{requestId:'diagnostic-private',request:{privacy:true,expected_settings_revision:s.data.settings_revision}});
      const hidden=await invoke('query_diagnostics',{requestId:'diagnostic-private-query',request:{source_id:'native-diagnostics'}});
      if(hidden.data.issues.some(i=>i.path!=='位置已隐藏'))throw new Error('DIAGNOSTIC_PRIVATE_PATH_LEAK');
      await wait(()=>document.querySelectorAll('.diagnostic-issue-list li').length===2 && !document.querySelector('.diagnostics-issues').textContent.includes('problem.jsonl'));
      const s2=await invoke('get_display_settings',{requestId:'diagnostic-visible-read'});
      await invoke('set_display_privacy',{requestId:'diagnostic-visible',request:{privacy:false,expected_settings_revision:s2.data.settings_revision}});
      await wait(()=>document.querySelector('.diagnostics-issues')?.textContent.includes('problem.jsonl'));
    "#.replace("EXPECTED_PATH",&expected_path).replace("EXPECTED_OFFSET",&head.len().to_string());
    super::mini_smoke::evaluate(app, &main, &script)?;
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      let denied=false;try{await invoke('query_diagnostics',{requestId:'mini-diagnostic-denied',request:{source_id:null}});}catch{denied=true;}if(!denied)throw new Error('MINI_DIAGNOSTICS_PERMISSION');
      denied=false;try{await invoke('start_source_reread',{requestId:'mini-source-read-denied',request:{kind:'rebuild',scope:{kind:'all'},request_key:'mini-source-read-denied'}});}catch{denied=true;}if(!denied)throw new Error('MINI_SOURCE_READ_PERMISSION');
    "#,
    )?;
    let corrected = [head.as_slice(),b"{\"timestamp\":\"1970-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"total_tokens\":9}}}}\n"].concat();
    fs::write(&path, &corrected).map_err(|e| e.to_string())?;
    super::mini_smoke::evaluate_with_timeout(
        app,
        &main,
        r#"
      const source=await invoke('get_sources',{requestId:'diagnostic-resume-read'});
      await invoke('manage_source',{requestId:'diagnostic-resume',action:{kind:'resume',source_id:'native-diagnostics'},expectedSettingsRevision:source.data.settings_revision});
      await wait(()=>document.querySelector('.source-panel')?.textContent.includes('可读取'));
    "#,
        Duration::from_secs(15),
    )?;
    super::mini_smoke::evaluate_with_timeout(
        app,
        &main,
        r#"
      const end=Date.now()+10000;
      for(;;){const d=await invoke('query_diagnostics',{requestId:'diagnostic-resolved',request:{source_id:'native-diagnostics'}});if(d.data.issues.length===0)break;if(Date.now()>end)throw new Error('DIAGNOSTICS_NOT_RESOLVED');await new Promise(r=>setTimeout(r,100));}
      await wait(()=>document.querySelector('.diagnostics-issues')?.textContent.includes('暂无已记录的问题'));
    "#,
        Duration::from_secs(15),
    )?;
    let total: String = db
        .snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
                [],
                |r| r.get(0),
            )?)
        })
        .map_err(|e| e.to_string())?;
    if total != "9"
        || fs::read(&saved).map_err(|e| e.to_string())? != bytes
        || fs::read(&path).map_err(|e| e.to_string())? != corrected
    {
        return Err("corrected total or readonly source bytes changed".into());
    }
    let before = db
        .file_checkpoint("native-diagnostics", path.to_str().unwrap(), None)
        .map_err(|e| e.to_string())?
        .ok_or("source checkpoint missing")?;
    let mut permissions = fs::metadata(&path)
        .map_err(|e| e.to_string())?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).map_err(|e| e.to_string())?;
    super::mini_smoke::evaluate_with_timeout(
        app,
        &main,
        r#"
      let rejected=false;try{await invoke('start_source_reread',{requestId:'reread-invalid-scope',request:{kind:'rebuild',scope:{kind:'sessions',session_keys:['native-fixture']},request_key:'reread-invalid-scope'}});}catch(e){rejected=e.code==='INVALID_QUERY';}if(!rejected)throw Error('REREAD_SESSION_SCOPE_ACCEPTED');
      await wait(()=>[...document.querySelectorAll('.jobs-panel button')].some(b=>b.textContent==='重读已启用来源'&&!b.disabled));
      [...document.querySelectorAll('.jobs-panel button')].find(b=>b.textContent==='重读已启用来源').click();
      await wait(()=>document.querySelector('.job-notice')?.textContent.startsWith('来源重读已提交'));
      const end=Date.now()+12000;
      for(;;){const job=await invoke('get_rebuild_status',{requestId:'native-reread-status'});if(job.data?.state==='succeeded'){window.__nativeRereadJob=job.data.job_id;break;}if(['failed','cancelled','interrupted'].includes(job.data?.state))throw Error('NATIVE_REREAD_TERMINAL_'+job.data.state);if(Date.now()>end)throw Error('NATIVE_REREAD_TIMEOUT');await new Promise(r=>setTimeout(r,100));}
      [...document.querySelectorAll('.jobs-panel button')].find(b=>b.textContent==='刷新重建状态').click();
      await wait(()=>document.querySelector('.job-card')?.textContent.includes('已完成'));
      if(!document.querySelector('.jobs-panel').textContent.includes('重读重新导入已启用来源的日志'))throw Error('REREAD_EXPLANATION_MISSING');
    "#,
        Duration::from_secs(18),
    )?;
    let job = db
        .rebuild_status()
        .map_err(|e| e.to_string())?
        .ok_or("reread job missing")?;
    let stored = db.get_job(&job.job_id).map_err(|e| e.to_string())?;
    if !stored.checkpoint.reread_sources
        || stored.job.state != token_pulse_core::protocol::JobState::Succeeded
    {
        return Err("explicit read intent or result missing".into());
    }
    let key = serde_json::to_string(&stored.request.request_key).map_err(|e| e.to_string())?;
    let id = serde_json::to_string(&job.job_id).map_err(|e| e.to_string())?;
    super::mini_smoke::evaluate(
        app,
        &main,
        &format!(
            "const retry=await invoke('start_source_reread',{{requestId:'reread-idempotent',request:{{kind:'rebuild',scope:{{kind:'all'}},request_key:{key}}}}});if(retry.data.job_id!=={id})throw Error('REREAD_NOT_IDEMPOTENT');"
        ),
    )?;
    let after = db
        .file_checkpoint("native-diagnostics", path.to_str().unwrap(), None)
        .map_err(|e| e.to_string())?
        .ok_or("new checkpoint missing")?;
    let repeated: String = db
        .snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT sum_token_decimal(total_tokens) FROM active_usage_events",
                [],
                |r| r.get(0),
            )?)
        })
        .map_err(|e| e.to_string())?;
    if before.file_generation_id == after.file_generation_id
        || repeated != "9"
        || fs::read(&path).map_err(|e| e.to_string())? != corrected
        || !fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .permissions()
            .readonly()
    {
        return Err(
            "source reread failed to replace generation or changed consumption/source".into(),
        );
    }
    println!(
        "NATIVE_SOURCE_REREAD_OK: React action, main-only IPC, durable intent, verified new generation, retry key, readonly bytes and one consumption"
    );
    verify_request_input(app)?;
    Ok(())
}

fn verify_request_input(app: &tauri::AppHandle) -> Result<(), String> {
    use serde_json::json;
    let state = app.state::<super::RuntimeState>();
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let root = state.data_directory.join("synthetic-request-input-source");
    let reference = token_pulse_core::pricing::offline::OfflinePriceCatalog::bundled()
        .map_err(|e| e.to_string())?;
    db.install_offline_price_catalog(reference.clone(), reference.verified_at_ms.value())
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("sessions")).map_err(|e| e.to_string())?;
    let path = root.join("sessions/input.jsonl");
    let usage = json!({"input_tokens":272001,"cached_input_tokens":100,"cache_write_input_tokens":50,"output_tokens":10,"reasoning_output_tokens":2,"total_tokens":272011});
    let records = [
        json!({"type":"session_meta","payload":{"id":"request-input-fixture","timestamp":"2026-10-03T00:00:00Z","model_provider":"openai"}}),
        json!({"type":"turn_context","payload":{"model":"gpt-6.1-sol","turn_id":"request-input-turn"}}),
        json!({"type":"token_usage_record","payload":{"thread_id":"request-input-fixture","turn_id":"request-input-turn","root_turn_id":"synthetic-root","session_id":"synthetic-runtime","response_id":"private-response-fixture","usage":usage,"turn_token_usage":usage,"thread_token_usage":usage}}),
        json!({"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":usage,"total_token_usage":usage,"model_context_window":1000000}}}),
    ];
    let bytes: Vec<u8> = records
        .into_iter()
        .flat_map(|mut record| {
            record["timestamp"] = "2026-10-03T00:00:01Z".into();
            let mut line = serde_json::to_vec(&record).unwrap();
            line.push(b'\n');
            line
        })
        .collect();
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    let mut permissions = fs::metadata(&path)
        .map_err(|e| e.to_string())?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).map_err(|e| e.to_string())?;
    db.add_source(SourceRecord {
        source_id: "native-request-input".into(),
        root_path: root.to_str().ok_or("fixture root unavailable")?.into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .map_err(|e| e.to_string())?;
    token_pulse_collector::collect_file(db, "native-request-input", &path, 2)
        .map_err(|e| e.to_string())?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_smoke::evaluate_with_timeout(
        app,
        &main,
        r#"
      const range={start_ms:1790985600000,end_ms:1791072000000,timezone:'UTC'};
      const filter={range,sources:{kind:'ids',ids:['native-request-input'],include_unknown:false},models:{kind:'all'},projects:{kind:'all'},sessions:{kind:'all'}};
      const result=await invoke('query_usage_events',{requestId:'native-request-input-projection',request:{query:{filter,price_basis:{mode:'event_time'},sort:'time_desc',page_size:50},cursor:null}});
      const event=result.data.events[0];
      if(result.data.events.length!==1 || event.request_input?.input_tokens!=='272001' || event.request_input.binding!=='full_request' || event.total_tokens!=='272011' || event.price.status!=='priced' || event.matched_price?.basis.kind!=='offline_assumed_reference' || event.matched_price.basis.context!=='long' || event.matched_price.basis.context_assumed || event.matched_price.basis.cache_write_assumed_zero)throw Error('NATIVE_REQUEST_INPUT_PROJECTION');
      if(JSON.stringify(result.data).includes('private-response-fixture'))throw Error('NATIVE_REQUEST_INPUT_ID_LEAK');
      [...document.querySelectorAll('.sidebar nav button')].find(b=>b.textContent==='明细').click();
      [...document.querySelectorAll('button')].find(b=>b.textContent==='改用主窗口日期')?.click();
      await wait(()=>document.querySelector('select[aria-label="日期范围"]'));
      const date=document.querySelector('select[aria-label="日期范围"]');
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(date,'last30');date.dispatchEvent(new Event('change',{bubbles:true}));
      await wait(()=>document.querySelector('.event-table')?.textContent.includes('gpt-6.1-sol'));
      [...document.querySelectorAll('.event-table button')].find(b=>b.textContent==='查看依据').click();
      await wait(()=>document.querySelector('.event-evidence')?.textContent.includes('272,001 Token'));
      const evidence=document.querySelector('.event-evidence');
      if(!evidence.textContent.includes('对应完整请求用量') || !evidence.textContent.includes('尚未采集，不能据此确认完整计费') || !evidence.textContent.includes('Standard 参考估算 · 长上下文'))throw Error('NATIVE_REQUEST_INPUT_UI');
    "#,
        Duration::from_secs(15),
    )?;
    if fs::read(&path).map_err(|e| e.to_string())? != bytes
        || !fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .permissions()
            .readonly()
    {
        return Err("request input source changed".into());
    }
    println!(
        "NATIVE_REQUEST_INPUT_OK: readonly synthetic source, exact response input, whole consumption binding, actual DTO/React, unknown conditions, no response identity exposed"
    );
    Ok(())
}
