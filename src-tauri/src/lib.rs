use std::path::PathBuf;
use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use token_pulse_core::{
    ServiceState,
    error::{AppError, ErrorCode},
    privacy::{DisplayPolicyStamp, PrivacyState, PrivateResponse},
    protocol::{AppStatus, Response, WindowAction, validate_request_id},
};

struct RuntimeState {
    quota: Result<std::sync::Arc<token_pulse_quota::service::AccountQuotaService>, ErrorCode>,
    recovery_shortcut: std::sync::Mutex<shortcuts::RecoveryRuntime>,
    #[cfg(debug_assertions)]
    native_dashboard_request: std::sync::Mutex<Option<token_pulse_core::query::DashboardRequest>>,
    mini_stats_request: std::sync::Mutex<Option<token_pulse_core::mini::MiniStatsRequest>>,
    privacy: PrivacyState,
    mini_creation: std::sync::Mutex<()>,
    mini_geometry_sequence: std::sync::atomic::AtomicU64,
    mini_geometry_worker: std::sync::atomic::AtomicBool,
    mini_window: std::sync::Mutex<token_pulse_core::mini::MiniWindowState>,
    selections: std::sync::Arc<std::sync::Mutex<token_pulse_core::selections::DirectorySelections>>,
    data_directory: PathBuf,
    database: token_pulse_store::StoreResult<token_pulse_store::Database>,
    collector: token_pulse_store::StoreResult<
        std::sync::Arc<token_pulse_collector::service::CollectorService>,
    >,
    jobs: token_pulse_store::StoreResult<token_pulse_collector::jobs::JobService>,
    rollups: token_pulse_store::StoreResult<token_pulse_store::rollup_service::RollupService>,
}

#[tauri::command]
fn get_app_status(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<AppStatus>, Box<AppError>> {
    validate_request_id(&request_id)
        .map_err(|code| AppError::new(code, "invalid-request".into()))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    Ok(PrivateResponse::new(
        request_id,
        AppStatus {
            version: env!("CARGO_PKG_VERSION").into(),
            development: cfg!(debug_assertions),
            data_directory: state.data_directory.to_string_lossy().into_owned(),
            collector: match &state.collector {
                Ok(collector) => {
                    let status = collector.status();
                    if status.error.is_some() {
                        ServiceState::Error
                    } else if status.enabled_sources == 0 {
                        ServiceState::NotConfigured
                    } else {
                        ServiceState::Ready
                    }
                }
                Err(_) => ServiceState::Error,
            },
            storage: if state.database.is_ok() {
                ServiceState::Ready
            } else {
                ServiceState::Error
            },
            storage_error: state.database.as_ref().err().map(|e| e.code),
            quota: match state
                .quota
                .as_ref()
                .and_then(|q| {
                    q.snapshot()
                        .map_err(|_| &ErrorCode::QuotaServiceUnavailable)
                })
                .map(|s| s.state)
            {
                Ok(token_pulse_core::protocol::QuotaState::Disconnected) => {
                    ServiceState::NotConfigured
                }
                Ok(token_pulse_core::protocol::QuotaState::Ready) => ServiceState::Ready,
                _ => ServiceState::Error,
            },
            taskbar: ServiceState::NotImplemented,
        },
        state.privacy.clone(),
    ))
}

#[tauri::command]
async fn perform_window_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    action: WindowAction,
    request_id: String,
) -> Result<Response<()>, Box<AppError>> {
    validate_request_id(&request_id)
        .map_err(|code| AppError::new(code, "invalid-request".into()))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    let result = match action {
        WindowAction::OpenStats => show_main(&app),
        WindowAction::ShowMini => {
            tauri::async_runtime::spawn_blocking(move || mini_window::show(&app))
                .await
                .map_err(|_| "WINDOW_CREATION_FAILED".to_string())
                .and_then(|result| result)
        }
        WindowAction::HideMain => window
            .hide()
            .map(|_| quota_commands::update_visibility(&window))
            .map_err(|e| e.to_string()),
        WindowAction::Quit => {
            app.exit(0);
            Ok(())
        }
    };
    result.map_err(|_| AppError::new(ErrorCode::WindowUnavailable, request_id.clone()))?;
    Ok(Response::new(request_id, ()))
}

fn show_main(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("WINDOW_NOT_FOUND")?;
    window.show().map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    quota_commands::update_visibility(&window);
    window.set_focus().map_err(|e| e.to_string())
}

// Unreadable/future configuration must never expose identifying fields via a default-off policy.
fn initial_privacy(
    database: &token_pulse_store::StoreResult<token_pulse_store::Database>,
) -> PrivacyState {
    let stamp = database
        .as_ref()
        .ok()
        .and_then(|db| db.display_settings().ok())
        .map(|snapshot| DisplayPolicyStamp {
            settings_revision: snapshot.settings_revision,
            privacy: snapshot.preferences.privacy,
        })
        .unwrap_or(DisplayPolicyStamp {
            settings_revision: token_pulse_core::numeric::DecimalInt::parse("0")
                .expect("literal revision"),
            privacy: true,
        });
    PrivacyState::new(stamp)
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| { let _ = show_main(app); }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let expected_id = if cfg!(debug_assertions) { "com.tokenpulse.desktop.dev" } else { "com.tokenpulse.desktop" };
            if app.config().identifier != expected_id {
                return Err("application identifier must match build profile; use npm run tauri:dev for debug".into());
            }
            let data_directory = app.path().app_local_data_dir()?;
            #[cfg(debug_assertions)]
            let data_directory=if std::env::args().any(|arg|arg=="--native-smoke") {data_directory.join(format!("native-probe-{}",uuid::Uuid::new_v4()))} else {data_directory};
            token_pulse_store::prepare_data_directory(&data_directory)?;
            let database = token_pulse_store::Database::open(&data_directory).and_then(|database| {
                let now = i64::try_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_| ErrorCode::InvalidQuery)?.as_millis()).map_err(|_|ErrorCode::NumericOverflow)?;
                database.interrupt_unfinished_jobs(now)?;
                database.interrupt_rollup_builds()?;
                Ok(database)
            });
            let collector=match &database {Ok(database)=>token_pulse_collector::service::CollectorService::start(database.clone(),Default::default()).map(std::sync::Arc::new),Err(error)=>Err(error.code.into())};
            let jobs=match &database {Ok(database)=>{
                let notify:std::sync::Arc<dyn Fn()+Send+Sync>=if let Ok(collector)=&collector {let collector=collector.clone();std::sync::Arc::new(move||collector.reconcile())} else {std::sync::Arc::new(||{})};
                token_pulse_collector::jobs::JobService::start_with_notify(database.clone(),notify)
            },Err(error)=>Err(error.code.into())};
            let rollups=match &database {Ok(database)=>token_pulse_store::rollup_service::RollupService::start(database.clone()),Err(error)=>Err(error.code.into())};
            let theme = database.as_ref().ok().and_then(|db| db.display_settings().ok()).map(|s| s.preferences.theme).unwrap_or_default();
            settings_commands::apply_native_theme(app.handle(), theme);
            let privacy = initial_privacy(&database);
            let notify_app = app.handle().clone();
            let quota = token_pulse_quota::service::AccountQuotaService::start(&uuid::Uuid::new_v4().to_string(), std::sync::Arc::new(move |event| {use tauri::Emitter; let _ = notify_app.emit("account_quota_changed", event);})).map(std::sync::Arc::new);
            app.manage(RuntimeState { quota, recovery_shortcut: Default::default(), #[cfg(debug_assertions)] native_dashboard_request: Default::default(), mini_stats_request: Default::default(), mini_creation: Default::default(), mini_geometry_sequence: Default::default(), mini_geometry_worker: Default::default(), mini_window: Default::default(), privacy, data_directory, database, collector, jobs, rollups, selections: Default::default() });
            if let Some(main) = app.get_webview_window("main") {quota_commands::update_visibility(&main);}
            #[cfg(windows)]
            power::install(app.handle()).map_err(std::io::Error::other)?;
            shortcuts::initialize(app.handle());
            let open = MenuItem::with_id(app, "open", "打开统计", true, None::<&str>)?;
            let mini = MenuItem::with_id(app, "mini", "显示悬浮窗 / 恢复交互", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 TokenPulse", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &mini, &quit])?;
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().ok_or("missing application icon")?.clone())
                .tooltip("TokenPulse · 打开统计")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => { let _ = show_main(app); },
                    "mini" => { let app = app.clone(); tauri::async_runtime::spawn_blocking(move || { let _ = mini_window::show(&app); }); },
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(event, TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }) {
                        let _ = show_main(tray.app_handle());
                    }
                })
                .build(app)?;
            #[cfg(debug_assertions)]
            if std::env::args().any(|arg| arg == "--native-smoke") {
                smoke::start(app.handle().clone());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Destroyed) {quota_commands::update_native_visibility(window);}
            if window.label()=="mini" && matches!(event,tauri::WindowEvent::Moved(_) | tauri::WindowEvent::ScaleFactorChanged {..}) { mini_window::schedule_placement(window.app_handle()); }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.label()=="mini" {if let Some(mini)=window.app_handle().get_webview_window("mini") {if mini_window::save_current_placement(&mini).is_err() {eprintln!("MINI_PLACEMENT_SAVE_FAILED");}}}
                let _ = window.hide();
                quota_commands::update_native_visibility(window);
            }
        })
        .invoke_handler(tauri::generate_handler![quota_commands::get_account_quota,quota_commands::refresh_account_quota,mini_passthrough::get_mini_passthrough,mini_passthrough::set_mini_passthrough,mini_opacity::get_mini_opacity,mini_opacity::set_mini_opacity,shortcuts::get_recovery_shortcut, shortcuts::set_recovery_shortcut, get_app_status, perform_window_action,mini_window::mini_window_action,mini_commands::open_mini_stats,mini_commands::get_mini_stats_request,mini_commands::query_mini_sessions,mini_commands::get_mini_scope,mini_commands::get_mini_usage,mini_commands::set_mini_scope,source_commands::get_sources,source_commands::choose_source_directory,source_commands::manage_source,job_commands::start_job,job_commands::get_job,job_commands::list_jobs,job_commands::cancel_job,query_commands::get_context_snapshot,query_commands::get_dashboard_bundle,query_commands::get_grouped_usage,query_commands::get_filter_options,query_commands::query_sessions,query_commands::get_session_bundle,query_commands::query_turns,query_commands::resolve_calendar_selection,settings_commands::get_display_settings,settings_commands::set_display_timezone,settings_commands::set_display_theme,settings_commands::set_display_privacy,query_commands::query_usage_events,query_commands::close_query_snapshot,price_commands::get_price_rules,price_commands::save_price_rule,price_commands::retire_price_rule]);
    let context = tauri::generate_context!();
    #[cfg(debug_assertions)]
    let context = {
        let mut context = context;
        // Cargo tests/direct debug builds must be isolated even without the CLI overlay.
        context.config_mut().identifier = "com.tokenpulse.desktop.dev".into();
        context.config_mut().product_name = Some("TokenPulse Dev".into());
        for window in &mut context.config_mut().app.windows {
            window.title = "TokenPulse · 开发版".into();
        }
        context
    };
    builder
        .build(context)
        .expect("desktop runtime failed")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                if let Some(state) = app.try_state::<RuntimeState>() {
                    if let Ok(quota) = &state.quota {
                        quota.shutdown();
                    }
                    if let Some(mini) = app.get_webview_window("mini") {
                        if mini_window::save_current_placement(&mini).is_err() {
                            eprintln!("MINI_PLACEMENT_SAVE_FAILED");
                        }
                    }
                    if let Ok(rollups) = &state.rollups {
                        rollups.shutdown();
                    }
                    if let Ok(jobs) = &state.jobs {
                        jobs.shutdown();
                    }
                    if let Ok(collector) = &state.collector {
                        collector.shutdown();
                    }
                }
            }
        });
}

mod job_commands;
mod mini_commands;
mod mini_opacity;
mod mini_passthrough;
#[cfg(debug_assertions)]
mod mini_smoke;
mod mini_window;
#[cfg(all(debug_assertions, windows))]
mod opacity_smoke;
#[cfg(all(debug_assertions, windows))]
mod passthrough_smoke;
#[cfg(windows)]
mod power;
mod price_commands;
mod query_commands;
mod quota_commands;
mod settings_commands;
mod shortcuts;
#[cfg(all(debug_assertions, windows))]
mod shortcuts_smoke;
#[cfg(debug_assertions)]
mod smoke;
mod source_commands;
