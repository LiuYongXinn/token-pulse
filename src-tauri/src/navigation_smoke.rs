//! Real Windows WebView, isolated real-data copy. Reports only anonymous measurements.
use tauri::{Listener, Manager};
static BLOCKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static BOOT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
pub(super) fn wait_if_blocked() {
    let started = std::time::Instant::now();
    while BLOCKED.load(std::sync::atomic::Ordering::Acquire)
        && started.elapsed() < std::time::Duration::from_secs(20)
    {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
pub(super) fn directory() -> Result<Option<String>, String> {
    let Some(argument) = std::env::args().find(|arg| arg.starts_with("--native-navigation-id="))
    else {
        return Ok(None);
    };
    if !std::env::args().any(|arg| arg == "--native-smoke") {
        return Err("navigation probe requires native-smoke".into());
    }
    BOOT.get_or_init(std::time::Instant::now);
    if std::env::args().any(|arg| arg == "--native-navigation-restart") {
        BLOCKED.store(true, std::sync::atomic::Ordering::Release);
    }
    let id = uuid::Uuid::parse_str(argument.trim_start_matches("--native-navigation-id="))
        .map_err(|_| "invalid navigation probe identity")?;
    Ok(Some(format!("native-navigation-{id}")))
}
pub(super) fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let result = run(&app);
        match result {
            Ok(()) => {
                println!("NATIVE_NAVIGATION_OK");
                app.exit(0);
            }
            Err(error) => {
                eprintln!("NATIVE_NAVIGATION_FAILED {error}");
                app.exit(1);
            }
        }
    });
}
fn run(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<super::RuntimeState>();
    if !state
        .data_directory
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("native-navigation-"))
    {
        return Err("probe isolation failed".into());
    }
    let database = state
        .database
        .as_ref()
        .map_err(|_| "probe database unavailable")?;
    let database_startup_ms = BOOT
        .get()
        .map(|started| started.elapsed().as_secs_f64() * 1000.0);
    // Freeze background services in this disposable copy, after normal startup. Queries,
    // committed version notifications and IPC stay real. The installed process is untouched.
    if let Ok(service) = &state.jobs {
        service.shutdown();
    }
    if let Ok(service) = &state.rollups {
        service.shutdown();
    }
    if let Ok(service) = &state.revaluations {
        service.shutdown();
    }
    if let Ok(service) = &state.collector {
        service.shutdown();
    }
    let counts = database.snapshot(|tx, _| Ok(serde_json::json!({
        "events": tx.query_row("SELECT COUNT(*) FROM usage_events", [], |r| r.get::<_, i64>(0))?,
        "observations": tx.query_row("SELECT COUNT(*) FROM observations", [], |r| r.get::<_, i64>(0))?,
        "sessions": tx.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get::<_, i64>(0))?,
    }))).map_err(|_| "probe counts failed")?;
    let report = std::sync::Arc::new(std::sync::Mutex::new(None));
    let captured = report.clone();
    let event = format!("native-navigation-report-{}", uuid::Uuid::new_v4());
    let listener = app.listen(event.clone(), move |message| {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(message.payload()) {
            if let Ok(mut slot) = captured.lock() {
                *slot = Some(value);
            }
        }
    });
    let window = app
        .get_webview_window("main")
        .ok_or("main window missing")?;
    place_on_secondary(app)?;
    super::main_window::fit_current(&window).map_err(|_| "secondary window fit")?;
    let monitor = window
        .current_monitor()
        .map_err(|_| "current monitor unavailable")?
        .ok_or("current monitor missing")?;
    let primary = window
        .primary_monitor()
        .map_err(|_| "primary monitor unavailable")?
        .ok_or("primary monitor missing")?;
    if monitor.position() == primary.position() {
        return Err("probe remained on primary monitor".into());
    }
    let secondary_screen = serde_json::json!({ "secondary": true, "scale": monitor.scale_factor(), "width": monitor.size().width, "height": monitor.size().height });
    if !window.is_visible().unwrap_or(false) || window.is_minimized().unwrap_or(true) {
        return Err("probe window must be visible".into());
    }
    let update_only = std::env::args().any(|arg| arg == "--native-navigation-update");
    let restart = std::env::args().any(|arg| arg == "--native-navigation-restart");
    let script = include_str!("navigation_smoke.js")
        .replace(
            "REPORT_EVENT",
            &serde_json::to_string(&event).map_err(|_| "probe event encoding")?,
        )
        .replace("RESTART_PHASE", if restart { "true" } else { "false" });
    let script = script.replace("UPDATE_PHASE", if update_only { "true" } else { "false" });
    println!("NATIVE_NAVIGATION_UI_START");
    let result = super::mini_smoke::evaluate_with_timeout(
        app,
        &window,
        &script,
        std::time::Duration::from_secs(120),
    );
    result?;
    println!("NATIVE_NAVIGATION_UI_OK");
    let ui = report
        .lock()
        .map_err(|_| "probe report lock")?
        .take()
        .ok_or("probe report missing")?;
    BLOCKED.store(true, std::sync::atomic::Ordering::Release);
    let blocked = super::mini_smoke::evaluate_with_timeout(
        app,
        &window,
        include_str!("navigation_block_smoke.js"),
        std::time::Duration::from_secs(15),
    );
    BLOCKED.store(false, std::sync::atomic::Ordering::Release);
    blocked?;
    println!("NATIVE_NAVIGATION_BLOCKED_OK");
    let timings = token_pulse_store::query_timing::take();
    let request = state
        .native_dashboard_request
        .lock()
        .map_err(|_| "probe request lock")?
        .clone()
        .ok_or("actual dashboard request missing")?;
    // Let successful real IPC reads persist every current range before the next process.
    let drain = include_str!("navigation_drain_smoke.js").replace(
        "DASHBOARD_REQUEST",
        &serde_json::to_string(&request).map_err(|_| "probe request encoding")?,
    );
    let drain = drain.replace(
        "MEASUREMENT_EVENT",
        &serde_json::to_string(&event).map_err(|_| "probe event encoding")?,
    );
    super::mini_smoke::evaluate_with_timeout(
        app,
        &window,
        &drain,
        std::time::Duration::from_secs(60),
    )?;
    println!("NATIVE_NAVIGATION_AUX_OK");
    let ancillary = report.lock().map_err(|_| "probe report lock")?.take();
    super::mini_window::show(app)?;
    let mini = app
        .get_webview_window("mini")
        .ok_or("mini window missing")?;
    let mini_script = include_str!("navigation_mini_smoke.js").replace(
        "MEASUREMENT_EVENT",
        &serde_json::to_string(&event).map_err(|_| "probe event encoding")?,
    );
    super::mini_smoke::evaluate_with_timeout(
        app,
        &mini,
        &mini_script,
        std::time::Duration::from_secs(60),
    )?;
    println!("NATIVE_NAVIGATION_MINI_OK");
    let mini_queries = report.lock().map_err(|_| "probe report lock")?.take();
    mini.hide().map_err(|_| "mini probe hide")?;
    app.unlisten(listener);
    let baseline = if update_only {
        serde_json::json!([])
    } else {
        benchmark(database, &request)?
    };
    let report = serde_json::json!({ "format": 1, "input": "programmatic DOM click in visible Windows WebView2", "database_startup_ms": database_startup_ms, "screen": secondary_screen, "counts": counts, "ui": ui, "blocked_refresh_passed": true, "backend": timings, "baseline": baseline, "ancillary": ancillary, "mini": mini_queries, "summary_cache": database.summary_cache_stats(), "build": "debug custom-protocol", "backend_debug_assertions": token_pulse_store::query_timing::build_debug_assertions_enabled() });
    let phase = if update_only {
        "update"
    } else if std::env::args().any(|arg| arg == "--native-navigation-restart") {
        "restart"
    } else {
        "first"
    };
    let target = state
        .data_directory
        .join(format!("navigation-{phase}.json"));
    std::fs::write(
        target,
        serde_json::to_vec_pretty(&report).map_err(|_| "probe report encoding")?,
    )
    .map_err(|_| "probe report write")?;
    Ok(())
}
fn benchmark(
    db: &token_pulse_store::Database,
    request: &token_pulse_core::query::DashboardRequest,
) -> Result<serde_json::Value, String> {
    use token_pulse_core::{
        numeric::EpochMs,
        query::{GroupDimension, GroupSort, GroupedUsageRequest},
    };
    let at = EpochMs::new(token_pulse_collector::jobs::now_ms().map_err(|_| "probe clock")?)
        .map_err(|_| "probe clock range")?;
    let mut rows = Vec::new();
    for dimension in [GroupDimension::Models, GroupDimension::Projects] {
        let query = GroupedUsageRequest {
            filter: request.filter.clone(),
            price_basis: request.price_basis.clone(),
            dimension,
            sort: GroupSort::TotalDesc,
            limit: 200,
        };
        let started = std::time::Instant::now();
        let raw = db
            .snapshot(|tx, rev| {
                token_pulse_store::query::groups::bundle(tx, rev, &query, at, "native-benchmark")
            })
            .map_err(|_| "uncached group baseline")?;
        let uncached_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = std::time::Instant::now();
        let cached = db
            .grouped_usage_bundle(&query, at, "native-benchmark")
            .map_err(|_| "cached group baseline")?;
        let cached_ms = started.elapsed().as_secs_f64() * 1000.0;
        let equal = serde_json::to_value(raw).map_err(|_| "baseline encode")?
            == serde_json::to_value(cached).map_err(|_| "baseline encode")?;
        if !equal {
            return Err("cache baseline result mismatch".into());
        }
        rows.push(serde_json::json!({ "kind": dimension, "uncached_ms": uncached_ms, "cached_ms": cached_ms, "equal": equal }));
    }
    let started = std::time::Instant::now();
    let raw = db
        .snapshot(|tx, rev| {
            token_pulse_store::query::dashboard::bundle(tx, rev, request, at, "native-benchmark")
        })
        .map_err(|_| "uncached dashboard baseline")?;
    let uncached_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = std::time::Instant::now();
    let cached = db
        .dashboard_bundle(request, at, "native-benchmark")
        .map_err(|_| "cached dashboard baseline")?;
    let cached_ms = started.elapsed().as_secs_f64() * 1000.0;
    let equal = serde_json::to_value(raw).map_err(|_| "baseline encode")?
        == serde_json::to_value(cached).map_err(|_| "baseline encode")?;
    if !equal {
        return Err("cache dashboard baseline result mismatch".into());
    }
    rows.push(serde_json::json!({ "kind": "overview", "uncached_ms": uncached_ms, "cached_ms": cached_ms, "equal": equal }));
    Ok(serde_json::json!(rows))
}

pub(super) fn place_on_secondary(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("main window missing")?;
    place_window_on_secondary(&window)
}
pub(super) fn place_window_on_secondary(window: &tauri::WebviewWindow) -> Result<(), String> {
    let primary = window
        .primary_monitor()
        .map_err(|_| "primary monitor unavailable")?
        .ok_or("primary monitor missing")?;
    let secondary = window
        .available_monitors()
        .map_err(|_| "monitor enumeration unavailable")?
        .into_iter()
        .find(|monitor| monitor.position() != primary.position())
        .ok_or("secondary monitor required")?;
    let area = secondary.work_area();
    window
        .set_position(tauri::PhysicalPosition::new(
            area.position.x + 32,
            area.position.y + 32,
        ))
        .map_err(|_| "secondary placement failed")?;
    Ok(())
}

pub(super) fn secondary_probe() -> bool {
    std::env::args()
        .any(|arg| arg == "--native-secondary-screen" || arg.starts_with("--native-navigation-id="))
}
