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
                "NATIVE_DIAGNOSTIC_POSITIONS_OK: own readonly rollout, exact position, missing file, real main UI/IPC, latest privacy, mini denied, resume replacement resolves issues without losing source bytes"
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
      await wait(()=>document.querySelector('.diagnostics-issues')?.textContent.includes('暂无已保存的问题'));
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
    Ok(())
}
