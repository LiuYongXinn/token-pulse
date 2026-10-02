//! Explicit own-data native scene: production headless -> main owner -> actual collector / SQLite.
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tauri::Manager;
use token_pulse_core::{
    numeric::EpochMs,
    protocol::{CoverageState, DateRange, DimensionSelection, UsageFilter},
};
use token_pulse_integration::{
    notify_config::windows::read_config,
    notify_manager::NotifyManager,
    notify_registry::{RegistryError, windows::NotifyRegistry},
};
use token_pulse_store::{SourceRecord, source_management::SourceMutation};

pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_NOTIFY_COLLECTOR_OK: reviewed manager install and undo preserving user settings, registration retirement without orphan markers, explicitly retained synthetic original, offline marker, normal restart drain, real production headless online and duplicate hints, hidden primary remains hidden, readonly log totals 3/10/11, source pause respected"
            ),
            Err(error) => eprintln!("NATIVE_NOTIFY_COLLECTOR_FAILED: {error}"),
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}
fn filter() -> UsageFilter {
    UsageFilter {
        range: DateRange {
            start_ms: EpochMs::new(0).unwrap(),
            end_ms: EpochMs::new(5000).unwrap(),
            timezone: "UTC".into(),
        },
        sources: DimensionSelection::All {},
        models: DimensionSelection::All {},
        projects: DimensionSelection::All {},
        sessions: DimensionSelection::All {},
    }
}
fn wait(condition: impl Fn() -> bool, label: &str) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(15);
    while !condition() {
        if Instant::now() >= until {
            return Err(format!("notify condition timed out: {label}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
fn readonly(path: &Path, value: bool) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|_| "fixture metadata")?
        .permissions();
    permissions.set_readonly(value);
    fs::set_permissions(path, permissions).map_err(|_| "fixture permissions".into())
}
fn append(path: &Path, tokens: u8) -> Result<Vec<u8>, String> {
    readonly(path, false)?;
    let mut bytes = serde_json::to_vec(
        &serde_json::json!({"timestamp":"1970-01-01T00:00:01Z","type":"event_msg",
        "payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":tokens}}}}),
    )
    .map_err(|_| "fixture encoding")?;
    bytes.push(b'\n');
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(&bytes))
        .map_err(|_| "fixture append")?;
    readonly(path, true)?;
    fs::read(path).map_err(|_| "fixture readback".into())
}
fn invoke(probe: &str, id: &str, turn: &str) -> Result<(), String> {
    let payload=serde_json::to_string(&serde_json::json!({"type":"agent-turn-complete","thread-id":"native-notify-thread","turn-id":turn,
        "cwd":"ignored synthetic path","input-messages":["ignored synthetic private body"],"total_tokens":999999})).map_err(|_|"notification encoding")?;
    let mut child = Command::new(std::env::current_exe().map_err(|_| "own executable")?)
        .env("TOKENPULSE_NATIVE_NOTIFY_PROBE", probe)
        .args(["--tokenpulse-notify", "--integration", id, &payload])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "headless spawn")?;
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().map_err(|_| "headless status")?.is_some() {
            break;
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            return Err("headless did not exit".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child
        .wait_with_output()
        .map_err(|_| "headless completion")?;
    if !output.status.success() || !output.stdout.is_empty() || !output.stderr.is_empty() {
        return Err("headless wake rejected or unexpected output".into());
    }
    Ok(())
}
fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<super::RuntimeState>();
    let probe = state
        .data_directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.starts_with("native-notify-"))
        .ok_or("isolated notify directory required")?;
    if !app.config().identifier.ends_with(".dev") {
        return Err("debug identifier required".into());
    }
    let db = state
        .database
        .as_ref()
        .map_err(|_| "database unavailable")?;
    let collector = state
        .collector
        .as_ref()
        .map_err(|_| "collector unavailable")?;
    let registry =
        NotifyRegistry::open(&state.data_directory).map_err(|_| "notify registry unavailable")?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    main.hide().map_err(|_| "hide main")?;
    let root = state.data_directory.join("synthetic-notify-home");
    fs::create_dir_all(root.join("sessions")).map_err(|_| "fixture directories")?;
    let path = root.join("sessions").join("notify.jsonl");
    fs::write(
        &path,
        b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"native-notify-session\"}}\n",
    )
    .map_err(|_| "fixture metadata")?;
    readonly(&path, true)?;
    // Exact prior command is explicit in this synthetic fixture, never silently inserted.
    let original =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").ok_or("system directory")?)
            .join("System32")
            .join("cmd.exe");
    let original = [
        original.to_str().ok_or("original encoding")?,
        "/d",
        "/s",
        "/c",
        "exit 0",
    ];
    let before = format!(
        "# synthetic fixture\r\nnotify={}\r\nmodel='unpriced-fixture'\r\n",
        serde_json::to_string(&original).map_err(|_| "original encoding")?
    );
    fs::write(root.join("config.toml"), before.as_bytes()).map_err(|_| "fixture base config")?;
    let mut manager = NotifyManager::new(
        registry.clone(),
        std::env::current_exe().map_err(|_| "own exe")?,
    );
    let preview = manager
        .prepare_enable(&root, None)
        .map_err(|_| "notify enable review")?;
    if !preview.chain_original || !preview.can_chain_original {
        return Err("existing original was not retained by default".into());
    }
    let enabled = manager
        .apply(&preview.plan_id)
        .map_err(|_| "notify manager installation")?;
    if !enabled.configured || enabled.cleanup_error.is_some() {
        return Err("notify manager installation state".into());
    }
    let config = read_config(&root).map_err(|_| "installed config readback")?;
    let id = preview.registration_id.as_str();
    {
        let owner = state.notify.lock().map_err(|_| "notify owner lock")?;
        owner
            .as_ref()
            .map_err(|_| "notify owner unavailable")?
            .shutdown();
    }
    db.add_source(SourceRecord {
        source_id: "native-notify".into(),
        root_path: root.to_str().ok_or("root encoding")?.into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .map_err(|_| "source fixture")?;
    collector.reconcile();
    wait(
        || {
            db.usage_totals(&filter())
                .is_ok_and(|value| value.total_tokens.as_str() == "0")
                && db
                    .usage_coverage(&filter())
                    .is_ok_and(|coverage| matches!(coverage.state, CoverageState::Complete))
                && collector.status().queue_length == 0
        },
        "empty source scanned",
    )?;
    let bytes = append(&path, 3)?;
    invoke(probe, id, "offline-1")?;
    if !registry
        .has_pending(id)
        .map_err(|_| "offline marker lookup")?
    {
        return Err("offline marker not saved".into());
    }
    if db
        .usage_totals(&filter())
        .map_err(|_| "baseline totals")?
        .total_tokens
        .as_str()
        != "0"
    {
        return Err("unsignalled collector changed totals".into());
    }
    {
        let mut owner = state.notify.lock().map_err(|_| "notify owner lock")?;
        *owner = super::notify_runtime::start(&state.data_directory, &state.collector);
    }
    wait(
        || {
            db.usage_totals(&filter())
                .is_ok_and(|value| value.total_tokens.as_str() == "3")
                && !registry.has_pending(id).unwrap_or(true)
        },
        "startup dirty bit consumed",
    )?;
    if fs::read(&path).map_err(|_| "readonly source read")? != bytes {
        return Err("collector altered source bytes".into());
    }
    wait(
        || {
            state.notify.lock().is_ok_and(|owner| {
                owner
                    .as_ref()
                    .is_ok_and(|service| service.status().listener_count == Some(1))
            })
        },
        "real listener ready",
    )?;
    let bytes = append(&path, 7)?;
    invoke(probe, id, "online-2")?;
    wait(
        || {
            db.usage_totals(&filter())
                .is_ok_and(|value| value.total_tokens.as_str() == "10")
        },
        "online headless schedules logs",
    )?;
    invoke(probe, id, "online-2")?;
    std::thread::sleep(Duration::from_millis(200));
    if db
        .usage_totals(&filter())
        .map_err(|_| "duplicate totals")?
        .total_tokens
        .as_str()
        != "10"
        || fs::read(&path).map_err(|_| "source readback")? != bytes
    {
        return Err("duplicate hint changed ledger or source".into());
    }
    let revision = db
        .snapshot(|_, revision| Ok(revision.settings))
        .map_err(|_| "settings revision")?;
    db.mutate_sources(SourceMutation::Pause("native-notify".into()), revision, 2)
        .map_err(|_| "pause fixture")?;
    collector.reconcile();
    let bytes = append(&path, 1)?;
    invoke(probe, id, "paused-3")?;
    std::thread::sleep(Duration::from_millis(300));
    if db
        .usage_totals(&filter())
        .map_err(|_| "paused totals")?
        .total_tokens
        .as_str()
        != "10"
    {
        return Err("notify bypassed paused source".into());
    }
    let revision = db
        .snapshot(|_, revision| Ok(revision.settings))
        .map_err(|_| "resume revision")?;
    db.mutate_sources(SourceMutation::Resume("native-notify".into()), revision, 3)
        .map_err(|_| "resume fixture")?;
    collector.reconcile();
    wait(
        || {
            db.usage_totals(&filter())
                .is_ok_and(|value| value.total_tokens.as_str() == "11")
        },
        "resume includes appended log",
    )?;
    if main.is_visible().map_err(|_| "main visibility")? {
        return Err("notification activated hidden primary".into());
    }
    if fs::read(root.join("config.toml")).map_err(|_| "config readback")? != config
        || fs::read(&path).map_err(|_| "source final readback")? != bytes
        || !fs::metadata(&path)
            .map_err(|_| "source metadata")?
            .permissions()
            .readonly()
    {
        return Err("source or config changed by app".into());
    }
    // A later unrelated user setting must survive controlled undo through the same file writer.
    let mut changed = config;
    changed.extend_from_slice(b"new_key='user-kept'\r\n");
    fs::write(root.join("config.toml"), &changed).map_err(|_| "fixture unrelated user edit")?;
    let undo = manager
        .prepare_disable(id)
        .map_err(|_| "notify undo review")?;
    let disabled = manager
        .apply(&undo.plan_id)
        .map_err(|_| "notify manager undo")?;
    if disabled.configured
        || disabled.cleanup_error.is_some()
        || !matches!(registry.get(id), Err(RegistryError::NotFound))
        || !manager
            .registrations()
            .map_err(|_| "notify remaining registrations")?
            .is_empty()
    {
        return Err("notify manager retirement state".into());
    }
    let expected = format!("{before}new_key='user-kept'\r\n");
    if read_config(&root).map_err(|_| "restored config readback")? != expected.as_bytes() {
        return Err("notify undo overwrote user setting".into());
    }
    state
        .notify
        .lock()
        .map_err(|_| "notify owner reload lock")?
        .as_ref()
        .map_err(|_| "notify owner reload")?
        .reload();
    wait(
        || {
            state.notify.lock().is_ok_and(|owner| {
                owner
                    .as_ref()
                    .is_ok_and(|service| service.status().listener_count == Some(0))
            })
        },
        "undo removes current listener",
    )?;
    invoke(probe, id, "after-undo")?;
    if registry.has_pending(id).map_err(|_| "after undo marker")? {
        return Err("inactive config still queued notification".into());
    }
    // Actual WebView IPC / app capability, not a direct call to manager or command functions.
    super::mini_smoke::evaluate_with_timeout(
        app,
        &main,
        r#"
      const request={kind:'enable_source',source_id:'native-notify',chain_original:null};
      let status=await invoke('get_notify_integrations',{requestId:'notify-ipc-initial'});
      if(status.data.registrations?.length!==0 || status.data.listener_count!==0)throw new Error('NOTIFY_INITIAL_STATUS');
      let rejected=false;try{await invoke('prepare_notify_integration',{requestId:'notify-bad-source',request:{...request,source_id:'../auth.json'}});}catch(e){rejected=e.code==='INVALID_QUERY';}if(!rejected)throw new Error('NOTIFY_ARBITRARY_SOURCE');
      rejected=false;try{await invoke('prepare_notify_integration',{requestId:'notify-bad-path',request:{...request,path:'private-synthetic'}});}catch{rejected=true;}if(!rejected)throw new Error('NOTIFY_ARBITRARY_PATH');
      const cancelled=await invoke('prepare_notify_integration',{requestId:'notify-release-prepare',request});
      if(!cancelled.data.can_chain_original||!cancelled.data.chain_original||cancelled.data.redacted||!cancelled.data.before_notify||!cancelled.data.after_notify)throw new Error('NOTIFY_REVIEW_FIELDS');
      await invoke('release_notify_preview',{requestId:'notify-release',planId:cancelled.data.plan_id});
      rejected=false;try{await invoke('apply_notify_integration',{requestId:'notify-released-apply',planId:cancelled.data.plan_id});}catch(e){rejected=e.details?.notify_issue==='plan_not_found';}if(!rejected)throw new Error('NOTIFY_RELEASED_PLAN_ACCEPTED');
      const stale=await invoke('prepare_notify_integration',{requestId:'notify-stale-prepare',request});
      const s=await invoke('get_display_settings',{requestId:'notify-privacy-read'});
      await invoke('set_display_privacy',{requestId:'notify-private',request:{privacy:true,expected_settings_revision:s.data.settings_revision}});
      const hidden=await invoke('get_notify_integrations',{requestId:'notify-hidden-read'});
      if(!hidden.data.redacted || hidden.data.registrations.some(r=>r.home_path!==null))throw new Error('NOTIFY_PRIVATE_PATH');
      rejected=false;try{await invoke('apply_notify_integration',{requestId:'notify-hidden-apply',planId:stale.data.plan_id});}catch(e){rejected=e.code==='PERMISSION_DENIED';}if(!rejected)throw new Error('NOTIFY_PRIVATE_APPLY');
      rejected=false;try{await invoke('prepare_notify_integration',{requestId:'notify-hidden-prepare',request});}catch(e){rejected=e.code==='PERMISSION_DENIED';}if(!rejected)throw new Error('NOTIFY_PRIVATE_PREPARE');
      const p=await invoke('get_display_settings',{requestId:'notify-visible-read'});
      await invoke('set_display_privacy',{requestId:'notify-visible',request:{privacy:false,expected_settings_revision:p.data.settings_revision}});
      rejected=false;try{await invoke('apply_notify_integration',{requestId:'notify-stale-apply',planId:stale.data.plan_id});}catch(e){rejected=e.code==='STALE_CONFIRMATION';}if(!rejected)throw new Error('NOTIFY_PRIVACY_EPOCH_PLAN_ACCEPTED');
      const fresh=await invoke('prepare_notify_integration',{requestId:'notify-fresh-prepare',request});
      const enabled=await invoke('apply_notify_integration',{requestId:'notify-enable',planId:fresh.data.plan_id});
      if(enabled.data.configured!==true||enabled.data.retired||enabled.data.cleanup_issue!==null)throw new Error('NOTIFY_IPC_ENABLE');
      const active=await invoke('get_notify_integrations',{requestId:'notify-active-read'});
      if(active.data.registrations.length!==1||active.data.registrations[0].configured!==true||active.data.registrations[0].home_path===null)throw new Error('NOTIFY_ACTIVE_STATUS');
      rejected=false;try{await invoke('retire_notify_integration',{requestId:'notify-active-retire',registrationId:fresh.data.registration_id});}catch(e){rejected=e.details?.notify_issue==='active_configuration';}if(!rejected)throw new Error('NOTIFY_ACTIVE_RETIRE');
      const s2=await invoke('get_display_settings',{requestId:'notify-path-hide-read'});
      await invoke('set_display_privacy',{requestId:'notify-path-hide',request:{privacy:true,expected_settings_revision:s2.data.settings_revision}});
      const privateActive=await invoke('get_notify_integrations',{requestId:'notify-private-active'});
      if(!privateActive.data.redacted||privateActive.data.registrations[0].home_path!==null||privateActive.data.registrations[0].configured!==true)throw new Error('NOTIFY_ACTIVE_PATH_LEAK');
      const s3=await invoke('get_display_settings',{requestId:'notify-path-show-read'});
      await invoke('set_display_privacy',{requestId:'notify-path-show',request:{privacy:false,expected_settings_revision:s3.data.settings_revision}});
      const undo=await invoke('prepare_notify_integration',{requestId:'notify-undo-prepare',request:{kind:'disable',registration_id:fresh.data.registration_id}});
      const disabled=await invoke('apply_notify_integration',{requestId:'notify-undo',planId:undo.data.plan_id});
      if(disabled.data.configured!==false||!disabled.data.retired||disabled.data.cleanup_issue!==null)throw new Error('NOTIFY_IPC_UNDO');
      const gone=await invoke('get_notify_integrations',{requestId:'notify-retired-read'});
      if(gone.data.registrations.length!==0)throw new Error('NOTIFY_IPC_RETIREMENT');
      const retired=await invoke('retire_notify_integration',{requestId:'notify-idempotent-retire',registrationId:fresh.data.registration_id});
      if(!retired.data.retired||retired.data.configured!==null)throw new Error('NOTIFY_IPC_ABSENT_CONFIG_UNKNOWN');
    "#,
        Duration::from_secs(20),
    )?;
    if read_config(&root).map_err(|_| "IPC restored config")? != expected.as_bytes()
        || fs::read(&path).map_err(|_| "IPC source readonly")? != bytes
    {
        return Err("IPC changed unrelated config or source".into());
    }
    super::mini_window::show(app)?;
    let mini = app
        .get_webview_window("mini")
        .ok_or("notify mini missing")?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      const calls=[['get_notify_integrations',{}],['prepare_notify_integration',{request:{kind:'choose_home',chain_original:null}}],['apply_notify_integration',{planId:'a'.repeat(32)}],['release_notify_preview',{planId:'a'.repeat(32)}],['retire_notify_integration',{registrationId:'a'.repeat(32)}]];
      for(const [command,args] of calls){let denied=false;try{await invoke(command,{...args,requestId:'notify-mini-denied'});}catch{denied=true;}if(!denied)throw new Error('NOTIFY_MINI_CAPABILITY:'+command);}
    "#,
    )?;
    mini.hide().map_err(|_| "notify mini hide")?;
    println!(
        "NATIVE_NOTIFY_IPC_OK: actual main WebView five commands, readonly review/release, default original, privacy redaction/write gate/stale preview, active retirement refused, conditional enable/undo, null absent configuration, mini capability denied, source bytes unchanged"
    );
    Ok(())
}
