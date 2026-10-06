//! Explicit own-home acceptance through the real Windows folder dialog and actual React UI.
use super::native_dialog_driver::DialogDriver;
use std::{
    fs,
    time::{Duration, Instant},
};
use tauri::Manager;

pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_SOURCE_DIALOGS_OK: actual owned Windows folder chooser cancel/select, React add/pause/resume/retain-history, readonly synthetic rollout"
            ),
            Err(error) => eprintln!("NATIVE_SOURCE_DIALOGS_FAILED: {error}"),
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
        return Err("source dialog scene requires isolated debug database".into());
    }
    let database = state.database.as_ref().map_err(|e| e.to_string())?;
    if !database
        .sources_snapshot()
        .map_err(|e| e.to_string())?
        .sources
        .is_empty()
    {
        return Err("source scene must begin empty".into());
    }
    let folder = state.data_directory.join("synthetic-dialog-home");
    fs::create_dir_all(folder.join("sessions")).map_err(|e| e.to_string())?;
    let rollout = folder.join("sessions/synthetic.jsonl");
    let bytes = b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"source-dialog-fixture\"}}\n{\"timestamp\":\"2026-10-03T00:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"total_tokens\":17,\"cache_write_input_tokens\":2}}}}\n";
    fs::write(&rollout, bytes).map_err(|e| e.to_string())?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      [...document.querySelectorAll('.sidebar nav button')].find(n=>n.textContent==='设置').click();
      await wait(()=>document.querySelector('.source-panel .source-empty'));
      const source=await invoke('get_sources',{requestId:'dialog-before'});
      window.__sourceDialogRevision=source.data.settings_revision;
    "#,
    )?;
    for action in ["cancel", "select"] {
        let mut driver = DialogDriver::start("source", action, &folder)?;
        let scene = super::mini_smoke::evaluate_with_timeout(
            app,
            &main,
            if action == "cancel" {
                r#"
          const button=[...document.querySelectorAll('.source-panel button')].find(n=>n.textContent==='添加自定义目录');
          if(!button||button.disabled)throw new Error('SOURCE_DIALOG_ENTRY');button.click();
          await wait(()=>[...document.querySelectorAll('.source-panel button')].find(n=>n.textContent==='添加自定义目录')?.disabled);
          await wait(()=>[...document.querySelectorAll('.source-panel button')].find(n=>n.textContent==='添加自定义目录')?.disabled===false,1000);
          const after=await invoke('get_sources',{requestId:'dialog-cancelled'});
          if(after.data.sources.length||after.data.settings_revision!==window.__sourceDialogRevision||document.querySelector('.source-panel [role=alert]'))throw new Error('SOURCE_CANCEL_CHANGED_STATE');
        "#
            } else {
                r#"
          const button=[...document.querySelectorAll('.source-panel button')].find(n=>n.textContent==='添加自定义目录');button.click();
          await wait(()=>[...document.querySelectorAll('.source-panel button')].find(n=>n.textContent==='添加自定义目录')?.disabled);
          await wait(()=>[...document.querySelectorAll('.source-panel button')].find(n=>n.textContent==='添加自定义目录')?.disabled===false,1000);
          if(document.querySelector('.source-panel [role=alert]'))throw new Error('SOURCE_DIALOG_ADD_REJECTED:'+document.querySelector('.source-panel [role=alert]').textContent);
          await wait(()=>document.querySelectorAll('.source-panel .source-list > .source-card').length===1);
          const after=await invoke('get_sources',{requestId:'dialog-selected'});
          if(after.data.sources.length!==1||after.data.sources[0].origin!=='custom'||!after.data.sources[0].enabled||after.data.sources[0].removed)throw new Error('SOURCE_SELECTION_NOT_ADDED');
        "#
            },
            Duration::from_secs(40),
        );
        driver.finish().map_err(|e| format!("{action}: {e}"))?;
        if scene.is_err() {
            let current = database.sources_snapshot().map_err(|e| e.to_string())?;
            println!(
                "NATIVE_SOURCE_DIALOG_FAILURE_EVIDENCE: source_count={}; revision={}",
                current.sources.len(),
                current.settings_revision.as_str()
            );
        }
        scene?;
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    let total = || {
        database
            .snapshot(|tx, _| {
                Ok(tx.query_row(
                    "SELECT coalesce(sum_token_decimal(total_tokens),'0') FROM active_usage_events",
                    [],
                    |r| r.get::<_, String>(0),
                )?)
            })
            .map_err(|e| e.to_string())
    };
    while total()? != "17" && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    if total()? != "17" {
        return Err("selected source not imported exactly once".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const button=name=>[...document.querySelectorAll('.source-panel .source-list > .source-card button')].find(n=>n.textContent===name);
      button('暂停采集').click();await wait(()=>button('恢复采集')&&!button('恢复采集').disabled);
      await wait(()=>document.querySelector('.sidebar-bottom')?.textContent.includes('采集已暂停 · 历史保留'));
      button('恢复采集').click();await wait(()=>button('暂停采集')&&!button('暂停采集').disabled);
      await wait(()=>document.querySelector('.sidebar-bottom')?.textContent.includes('正在采集本地来源'));
      button('移除来源并保留历史').click();await wait(()=>document.querySelector('.source-panel .source-list > .source-card .source-state')?.textContent==='已移除 · 历史保留');
      await wait(()=>document.querySelector('.sidebar-bottom')?.textContent.includes('采集已停止 · 历史保留'));
      const after=await invoke('get_sources',{requestId:'dialog-retained'});
      if(after.data.sources.length!==1||!after.data.sources[0].removed||after.data.sources[0].enabled)throw new Error('SOURCE_RETAIN_INVALID');
      const calendar=await invoke('resolve_calendar_selection',{requestId:'dialog-retained-range',request:{timezone:'UTC',selection:{kind:'custom',start_date:'2026-10-03',end_date_inclusive:'2026-10-03'}}});
      const retained=await invoke('get_dashboard_bundle',{requestId:'dialog-retained-statistics',request:{filter:{range:calendar.data.range,sources:{kind:'all'},models:{kind:'all'},projects:{kind:'all'},sessions:{kind:'all'}},price_basis:{mode:'event_time'},grain:'day',heatmap_range:calendar.data.heatmap_range}});
      if(retained.data.summary.total_tokens!=='17'||retained.data.summary.input_total.value!==null||retained.data.summary.cache_write_input.value!=='2'||!retained.data.summary.cache_write_input.complete||retained.data.pricing.unpriced_total_tokens!=='17')throw new Error('SOURCE_RETAIN_QUERY_LOST_USAGE_OR_UNKNOWN');
      const events=await invoke('query_usage_events',{requestId:'dialog-write-evidence',request:{query:{filter:retained.data.filter??{range:calendar.data.range,sources:{kind:'all'},models:{kind:'all'},projects:{kind:'all'},sessions:{kind:'all'}},price_basis:{mode:'event_time'},sort:'time_desc',page_size:50},cursor:null}});
      if(events.data.events.length!==1||events.data.events[0].usage.cache_write_input!=='2'||events.data.events[0].raw_last.cache_write_input!=='2'||events.data.events[0].total_tokens!=='17')throw new Error('SOURCE_CACHE_WRITE_EVIDENCE');
      [...document.querySelectorAll('.sidebar nav button')].find(n=>n.textContent==='总览').click();
      // The fixture has a fixed date. Select it explicitly instead of depending on today's date.
      await wait(()=>document.querySelector('select[aria-label="日期范围"]')&&!document.querySelector('select[aria-label="日期范围"]').disabled);
      const date=document.querySelector('select[aria-label="日期范围"]');
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(date,'custom');date.dispatchEvent(new Event('change',{bubbles:true}));
      await wait(()=>document.querySelector('form[aria-label="自定义日期"]'));
      for(const label of ['开始日期','结束日期（包含当天）']){
        const input=document.querySelector(`input[aria-label="${label}"]`);
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'2026-10-03');
        input.dispatchEvent(new Event('input',{bubbles:true}));input.dispatchEvent(new Event('change',{bubbles:true}));
      }
      [...document.querySelectorAll('form[aria-label="自定义日期"] button')].find(n=>n.textContent==='应用日期').click();
      await wait(()=>[...document.querySelectorAll('.breakdown-measures .measure')].find(n=>n.querySelector('dt')?.textContent==='缓存写入（输入包含项）')?.querySelector('dd')?.textContent?.startsWith('2'));
    "#,
    )?;
    if total()? != "17" || fs::read(&rollout).map_err(|e| e.to_string())? != bytes {
        return Err("source bytes or retained usage changed".into());
    }
    Ok(())
}
