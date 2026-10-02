//! Explicit own-home acceptance through the real Windows folder dialog and actual React UI.
use std::{
    fs,
    process::{Child, Command, Stdio},
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
struct DialogDriver(Child);
impl DialogDriver {
    fn start(action: &str, folder: &std::path::Path) -> Result<Self, String> {
        use std::os::windows::process::CommandExt;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("repository missing")?
            .join("scripts/native-source-dialog-driver.ps1");
        let child = Command::new("pwsh.exe")
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(script)
            .args([
                "-ApplicationId",
                &std::process::id().to_string(),
                "-Action",
                action,
                "-Folder",
            ])
            .arg(folder)
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| "owned dialog driver could not start")?;
        Ok(Self(child))
    }
    fn finish(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(25);
        while Instant::now() < deadline {
            if let Some(status) = self.0.try_wait().map_err(|_| "dialog driver wait failed")? {
                let mut diagnostics = String::new();
                if let Some(mut error) = self.0.stderr.take() {
                    use std::io::Read;
                    let _ = error.read_to_string(&mut diagnostics);
                }
                return if status.success() {
                    Ok(())
                } else {
                    let code = [
                        "DIALOG_NOT_FOUND",
                        "DIALOG_PATH_FIELD_MISSING",
                        "DIALOG_PATH_VALUE_MISSING",
                        "DIALOG_ACTION_MISSING",
                        "DIALOG_API_FAILED",
                        "DIALOG_NOT_CLOSED",
                        "DIALOG_ACTION_TYPE_INVALID",
                        "DIALOG_AMBIGUOUS_CONTROL",
                        "DIALOG_PATH_SET_FAILED",
                        "DIALOG_ACTION_FAILED",
                    ]
                    .into_iter()
                    .find(|code| diagnostics.contains(code))
                    .unwrap_or("DIALOG_DRIVER_START_FAILED");
                    Err(format!("owned dialog driver rejected scene: {code}"))
                };
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err("owned dialog driver deadline".into())
    }
}
impl Drop for DialogDriver {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
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
    let bytes = b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"source-dialog-fixture\"}}\n{\"timestamp\":\"2026-10-03T00:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"total_tokens\":17}}}}\n";
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
        let mut driver = DialogDriver::start(action, &folder)?;
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
      button('恢复采集').click();await wait(()=>button('暂停采集')&&!button('暂停采集').disabled);
      button('移除来源并保留历史').click();await wait(()=>document.querySelector('.source-panel .source-list > .source-card .source-state')?.textContent==='已移除 · 历史保留');
      const after=await invoke('get_sources',{requestId:'dialog-retained'});
      if(after.data.sources.length!==1||!after.data.sources[0].removed||after.data.sources[0].enabled)throw new Error('SOURCE_RETAIN_INVALID');
      const calendar=await invoke('resolve_calendar_selection',{requestId:'dialog-retained-range',request:{timezone:'UTC',selection:{kind:'custom',start_date:'2026-10-03',end_date_inclusive:'2026-10-03'}}});
      const retained=await invoke('get_dashboard_bundle',{requestId:'dialog-retained-statistics',request:{filter:{range:calendar.data.range,sources:{kind:'all'},models:{kind:'all'},projects:{kind:'all'},sessions:{kind:'all'}},price_basis:{mode:'event_time'},grain:'day',heatmap_range:calendar.data.heatmap_range}});
      if(retained.data.summary.total_tokens!=='17'||retained.data.summary.input_total.value!==null||retained.data.pricing.unpriced_total_tokens!=='17')throw new Error('SOURCE_RETAIN_QUERY_LOST_USAGE_OR_UNKNOWN');
    "#,
    )?;
    if total()? != "17" || fs::read(&rollout).map_err(|e| e.to_string())? != bytes {
        return Err("source bytes or retained usage changed".into());
    }
    Ok(())
}
