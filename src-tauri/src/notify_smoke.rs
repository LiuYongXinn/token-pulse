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
    notify_channel::NotifyCapability,
    notify_config::{ManagedNotifyCommand, prepare_enable},
    notify_registry::{NotifyRegistration, windows::NotifyRegistry},
};
use token_pulse_store::{SourceRecord, source_management::SourceMutation};

pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_NOTIFY_COLLECTOR_OK: explicitly retained synthetic original, offline marker, normal restart drain, real production headless online and duplicate hints, hidden primary remains hidden, readonly log totals 3/10/11, source pause respected, config unchanged"
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
    let cap = NotifyCapability::new();
    let command = ManagedNotifyCommand::new(
        &std::env::current_exe().map_err(|_| "own exe")?,
        cap.registration_id(),
    )
    .map_err(|_| "owned command")?;
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
    let plan = prepare_enable(before.as_bytes(), &command).map_err(|_| "notify plan")?;
    let config = plan
        .apply_to(before.as_bytes())
        .map_err(|_| "notify fixture config")?;
    fs::write(root.join("config.toml"), &config).map_err(|_| "fixture config write")?;
    let registration =
        NotifyRegistration::from_prepared(&root, cap, plan.restore_record().clone(), true)
            .map_err(|_| "fixture registration")?;
    registry
        .create(&registration)
        .map_err(|_| "fixture registration save")?;
    let id = registration.capability().registration_id();
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
    Ok(())
}
