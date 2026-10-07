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
    updates: std::sync::Arc<update_service::UpdateService>,
    notify_operations: notify_commands::SharedOperations,
    #[cfg(windows)]
    notify: std::sync::Mutex<
        Result<
            token_pulse_integration::notify_service::NotifyService,
            token_pulse_integration::notify_registry::RegistryError,
        >,
    >,
    taskbar: std::sync::Mutex<Option<std::sync::Arc<taskbar_service::TaskbarService>>>,
    quota_selections:
        std::sync::Arc<std::sync::Mutex<token_pulse_core::quota::AccountServiceSelections>>,
    quota_config_actions: std::sync::Arc<std::sync::Mutex<()>>,
    quota: Result<std::sync::Arc<token_pulse_quota::service::AccountQuotaService>, ErrorCode>,
    recovery_shortcut: std::sync::Mutex<shortcuts::RecoveryRuntime>,
    #[cfg(debug_assertions)]
    native_dashboard_request: std::sync::Mutex<Option<token_pulse_core::query::DashboardRequest>>,
    main_geometry: main_window::PlacementRuntime,
    main_navigation: std::sync::Mutex<token_pulse_core::navigation::MainNavigationSnapshot>,
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
    revaluations: token_pulse_store::StoreResult<
        std::sync::Arc<token_pulse_store::revalue_service::RevalueService>,
    >,
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
            storage: if state
                .database
                .as_ref()
                .is_ok_and(|db| db.integrity_error().is_none())
            {
                ServiceState::Ready
            } else {
                ServiceState::Error
            },
            storage_error: match &state.database {
                Ok(db) => db.integrity_error(),
                Err(e) => Some(e.code),
            },
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
            taskbar: match taskbar_commands::service(window.app_handle())
                .and_then(|s| s.snapshot().ok())
                .map(|s| s.state)
            {
                Some(token_pulse_core::taskbar::TaskbarRuntimeState::Embedded) => {
                    ServiceState::Ready
                }
                Some(token_pulse_core::taskbar::TaskbarRuntimeState::Disabled) => {
                    ServiceState::NotConfigured
                }
                _ => ServiceState::Error,
            },
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
        WindowAction::HideMini => {
            tauri::async_runtime::spawn_blocking(move || mini_window::hide(&app))
                .await
                .map_err(|_| "WINDOW_HIDE_FAILED".to_string())
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
    window.unminimize().map_err(|e| e.to_string())?;
    main_window::fit_current(&window).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let expected_id = if cfg!(debug_assertions) { "com.tokenpulse.desktop.dev" } else { "com.tokenpulse.desktop" };
            if app.config().identifier != expected_id {
                return Err("application identifier must match build profile; use npm run tauri:dev for debug".into());
            }
            let data_directory = app.path().app_local_data_dir()?;
            #[cfg(all(debug_assertions, windows))]
            let power_scene = power_resume_smoke::scene()?;
            #[cfg(all(debug_assertions, windows))]
            let quota_startup_scene = quota_startup_smoke::scene()?;
            #[cfg(all(debug_assertions, windows))]
            let main_window_scene = main_window_smoke::scene()?;
            #[cfg(all(debug_assertions, windows))]
            let navigation_directory = navigation_smoke::directory()?;
            #[cfg(all(debug_assertions, windows))]
            if quota_startup_scene.is_some() && main_window_scene.is_some() { return Err("native startup scenes cannot be combined".into()); }
            #[cfg(all(debug_assertions, windows))]
            let data_directory = if let Some(scene) = &main_window_scene { data_directory.join(&scene.directory) } else { data_directory };
            #[cfg(all(debug_assertions, windows))]
            let data_directory = if let Some(scene) = &quota_startup_scene { data_directory.join(&scene.directory) } else { data_directory };
            #[cfg(all(debug_assertions, windows))]
            let data_directory = if let Some(directory) = &navigation_directory { let target = data_directory.join(directory); if !target.join("token-pulse.db").is_file() { return Err("isolated real-data copy missing".into()); } target } else { data_directory };
            #[cfg(debug_assertions)]
            let data_directory=if std::env::args().any(|arg|arg=="--native-smoke") {
                #[cfg(windows)]
                if quota_startup_scene.is_some() || main_window_scene.is_some() || navigation_directory.is_some() {data_directory}
                else {
                if std::env::args().any(|arg|arg=="--native-notify-smoke") {data_directory.join(format!("native-notify-{}",uuid::Uuid::new_v4().simple()))}
                else {data_directory.join(format!("native-probe-{}",uuid::Uuid::new_v4()))}
                }
                #[cfg(not(windows))]
                {data_directory.join(format!("native-probe-{}",uuid::Uuid::new_v4()))}
            } else {data_directory};
            token_pulse_store::prepare_data_directory(&data_directory)?;
            let database = token_pulse_store::Database::open_desktop(&data_directory).and_then(|database| {
                let now = i64::try_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_| ErrorCode::InvalidQuery)?.as_millis()).map_err(|_|ErrorCode::NumericOverflow)?;
                database.interrupt_unfinished_jobs(now)?;
                database.interrupt_rollup_builds()?;
                // Native fixtures keep their explicit empty price state, except the catalog scene.
                let install_catalog = !cfg!(debug_assertions)
                    || !std::env::args().any(|arg| arg == "--native-smoke")
                    || std::env::args().any(|arg| arg == "--native-offline-prices-smoke");
                if install_catalog {
                    database.install_offline_price_catalog(token_pulse_core::pricing::offline::OfflinePriceCatalog::bundled()?, now)?;
                }
                #[cfg(debug_assertions)]
                if std::env::args().any(|arg|arg=="--native-smoke") && std::env::args().any(|arg|arg=="--native-price-revalue-smoke") {price_revalue_smoke::seed(&database)?;}
                Ok(database)
            });
            if let Ok(database) = &database {
                let usage_app = app.handle().clone();
                database.on_usage_changed(std::sync::Arc::new(move |revision| {
                    use tauri::Emitter;
                    let _ = usage_app.emit_to("main", "usage_changed", revision);
                }));
            }
            let options=token_pulse_collector::service::CollectorOptions::default();
            #[cfg(debug_assertions)]
            let options=if std::env::args().any(|arg|arg=="--native-smoke") && (std::env::args().any(|arg|arg=="--native-notify-smoke") || std::env::args().any(|arg|arg.starts_with("--native-navigation-id=")) || std::env::args().any(|arg|arg.starts_with("--native-power-"))) {
                token_pulse_collector::service::CollectorOptions {watcher:false,active_poll:std::time::Duration::from_secs(3600),manifest_poll:std::time::Duration::from_secs(3600)}
            } else {options};
            let collector=match &database {Ok(database)=>token_pulse_collector::service::CollectorService::start(database.clone(),options).map(std::sync::Arc::new),Err(error)=>Err(error.code.into())};
            #[cfg(windows)]
            let notify=std::sync::Mutex::new(notify_runtime::start(&data_directory,&collector));
            let jobs=match &database {Ok(database)=>{
                let notify:std::sync::Arc<dyn Fn()+Send+Sync>=if let Ok(collector)=&collector {let collector=collector.clone();std::sync::Arc::new(move||collector.reconcile())} else {std::sync::Arc::new(||{})};
                token_pulse_collector::jobs::JobService::start_with_notify(database.clone(),notify)
            },Err(error)=>Err(error.code.into())};
            let rollups=match &database {Ok(database)=>token_pulse_store::rollup_service::RollupService::start(database.clone()),Err(error)=>Err(error.code.into())};
            let price_app=app.handle().clone();
            let revaluations=match &database {Ok(database)=>token_pulse_store::revalue_service::RevalueService::start(database.clone(),std::sync::Arc::new(move||{use tauri::Emitter;let _=price_app.emit("price_revalue_changed",());})).map(std::sync::Arc::new),Err(error)=>Err(error.code.into())};
            let theme = database.as_ref().ok().and_then(|db| db.display_settings().ok()).map(|s| s.preferences.theme).unwrap_or_default();
            settings_commands::apply_native_theme(app.handle(), theme);
            let privacy = initial_privacy(&database);
            let notify_app = app.handle().clone();
            let quota = token_pulse_quota::service::AccountQuotaService::start(&uuid::Uuid::new_v4().to_string(), std::sync::Arc::new(move |event| {use tauri::Emitter; let _ = notify_app.emit("account_quota_changed", event);})).map(std::sync::Arc::new);
            let update_app = app.handle().clone(); let updates = update_service::UpdateService::production(app.package_info().version.to_string(), std::sync::Arc::new(move || { use tauri::Emitter; let _ = update_app.emit("updates_changed", ()); })); app.manage(RuntimeState { updates, notify_operations: notify_commands::initialize(&data_directory), #[cfg(windows)] notify, taskbar: Default::default(), quota_selections: Default::default(), quota_config_actions: Default::default(), quota, recovery_shortcut: Default::default(), #[cfg(debug_assertions)] native_dashboard_request: Default::default(), main_geometry: Default::default(), main_navigation: Default::default(), mini_creation: Default::default(), mini_geometry_sequence: Default::default(), mini_geometry_worker: Default::default(), mini_window: Default::default(), privacy, data_directory, database, collector, jobs, rollups, revaluations, selections: Default::default() });
            #[cfg(all(debug_assertions, windows))]
            let main_window_before_restore = if main_window_scene.is_some() {app.state::<RuntimeState>().database.as_ref().map_err(|e| e.to_string())?.main_window_preferences()?.placement} else {None};
            #[cfg(all(debug_assertions, windows))]
            if navigation_directory.is_some() || std::env::args().any(|arg| arg == "--native-secondary-screen") { navigation_smoke::place_on_secondary(app.handle())?; }
            main_window::initialize(app.handle());
            taskbar_commands::initialize(app.handle());
            quota_config::initialize(app.handle());
            if let Some(main) = app.get_webview_window("main") {quota_commands::update_visibility(&main);}
            #[cfg(windows)]
            #[cfg(debug_assertions)]
            if power_scene.is_some() {app.manage(power_resume_smoke::PowerEvents::default());}
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
                #[cfg(windows)]
                if navigation_directory.is_some() { navigation_smoke::start(app.handle().clone()); return Ok(()); }
                #[cfg(windows)]
                if let Some(scene) = power_scene {power_resume_smoke::start(app.handle().clone(),scene);return Ok(());}
                #[cfg(windows)]
                if let Some(scene) = main_window_scene {main_window_smoke::start(app.handle().clone(),scene.phase,main_window_before_restore);return Ok(());}
                #[cfg(windows)]
                if let Some(scene) = quota_startup_scene {quota_startup_smoke::start(app.handle().clone(),scene.phase);return Ok(());}
                #[cfg(windows)]
                if std::env::args().any(|arg|arg=="--native-notify-smoke") {notify_smoke::start(app.handle().clone());return Ok(());}
                if std::env::args().any(|arg|arg=="--native-updates-smoke") {update_smoke::start(app.handle().clone());return Ok(());}
                #[cfg(windows)]
                if std::env::args().any(|arg|arg=="--native-notify-dialogs-smoke") {notify_dialog_smoke::start(app.handle().clone());return Ok(());}
                #[cfg(windows)]
                if std::env::args().any(|arg|arg=="--native-account-dialogs-smoke") {account_dialog_smoke::start(app.handle().clone());return Ok(());}
                #[cfg(windows)]
                if std::env::args().any(|arg|arg=="--native-source-dialogs-smoke") {source_dialog_smoke::start(app.handle().clone());return Ok(());}
                #[cfg(windows)]
                if std::env::args().any(|arg| arg == "--native-recovery-routes-smoke") {shortcuts_smoke::start_routes(app.handle().clone());return Ok(());}
                if std::env::args().any(|arg|arg=="--native-diagnostics-smoke") {diagnostics_smoke::start(app.handle().clone());return Ok(());}
                if std::env::args().any(|arg|arg=="--native-price-revalue-smoke") {price_revalue_smoke::start(app.handle().clone());return Ok(());}
                if std::env::args().any(|arg| arg == "--native-offline-prices-smoke") {
                    offline_prices_smoke::start(app.handle().clone());
                    return Ok(());
                }
                if std::env::args().any(|arg| arg == "--native-price-alias-smoke") {
                    price_alias_smoke::start(app.handle().clone());
                    return Ok(());
                }
                #[cfg(windows)]
                if std::env::args().any(|arg| arg == "--native-taskbar-explorer-restart-smoke") {
                    taskbar_explorer_smoke::start(app.handle().clone());
                    return Ok(());
                }
                #[cfg(windows)]
                if std::env::args().any(|arg| arg == "--native-taskbar-actions-smoke") {
                    taskbar_smoke::start_actions(app.handle().clone());
                    return Ok(());
                }
                #[cfg(windows)]
                if std::env::args().any(|arg| arg == "--native-taskbar-smoke") {
                    taskbar_smoke::start(app.handle().clone());
                    return Ok(());
                }
                #[cfg(windows)]
                if std::env::args().any(|arg| arg == "--native-mini-placement-smoke") {
                    mini_smoke::start_placement(app.handle().clone());
                    return Ok(());
                }
                if std::env::args().any(|arg| arg == "--native-mini-visibility-smoke") {
                    mini_smoke::start_visibility(app.handle().clone());
                    return Ok(());
                }
                smoke::start(app.handle().clone());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Destroyed) {quota_commands::update_native_visibility(window);}
            if window.label()=="main" && matches!(event,tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged {..}) { main_window::schedule(window.app_handle(),matches!(event,tauri::WindowEvent::ScaleFactorChanged {..})); }
            if window.label()=="mini" && matches!(event,tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged {..}) { mini_window::schedule_placement(window.app_handle()); }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.label()=="main" {if let Some(main)=window.app_handle().get_webview_window("main") {if main_window::save_current(&main).is_err() {eprintln!("MAIN_PLACEMENT_SAVE_FAILED");}}}
                if window.label()=="mini" {if let Some(mini)=window.app_handle().get_webview_window("mini") {if mini_window::save_current_placement(&mini).is_err() {eprintln!("MINI_PLACEMENT_SAVE_FAILED");}}}
                let _ = window.hide();
                quota_commands::update_native_visibility(window);
                if window.label()=="mini" {mini_window::notify_visibility(window.app_handle());}
            }
        })
        .invoke_handler(tauri::generate_handler![update_commands::get_update_status,update_commands::check_for_updates,update_commands::download_update,update_commands::install_update,notify_commands::get_notify_integrations,notify_commands::prepare_notify_integration,notify_commands::apply_notify_integration,notify_commands::release_notify_preview,notify_commands::retire_notify_integration,navigation::get_main_navigation,taskbar_commands::get_taskbar_preferences,taskbar_commands::set_taskbar_preferences,taskbar_commands::get_taskbar_status,taskbar_commands::retry_taskbar_embed,quota_config::get_account_service_config,quota_config::choose_account_service,quota_config::cancel_account_service_selection,quota_config::save_account_service_config,quota_config::manage_account_connection,quota_commands::get_account_quota,quota_commands::refresh_account_quota,mini_passthrough::get_mini_passthrough,mini_passthrough::set_mini_passthrough,mini_opacity::get_mini_opacity,mini_opacity::set_mini_opacity,shortcuts::get_recovery_shortcut, shortcuts::set_recovery_shortcut, get_app_status, perform_window_action,mini_window::get_mini_visibility,mini_window::mini_window_action,mini_commands::open_mini_stats,mini_commands::get_mini_stats_request,mini_commands::query_mini_sessions,mini_commands::get_mini_scope,mini_commands::get_mini_usage,mini_commands::set_mini_scope,source_commands::get_sources,source_commands::query_diagnostics,source_commands::choose_source_directory,source_commands::manage_source,job_commands::start_job,job_commands::start_source_reread,job_commands::get_job,job_commands::list_jobs,job_commands::get_rebuild_status,job_commands::cancel_job,query_commands::get_context_snapshot,query_commands::get_dashboard_bundle,query_commands::get_usage_revision,
            query_commands::restore_usage_display,query_commands::get_grouped_usage,query_commands::get_filter_options,query_commands::query_sessions,query_commands::get_session_bundle,query_commands::query_turns,query_commands::resolve_calendar_selection,settings_commands::get_display_settings,settings_commands::set_display_timezone,settings_commands::set_display_theme,settings_commands::set_display_privacy,query_commands::query_usage_events,query_commands::close_query_snapshot,revalue_commands::get_price_revalue_status,revalue_commands::start_price_revalue,revalue_commands::cancel_price_revalue,price_commands::get_price_rules,price_commands::get_offline_price_catalog,price_commands::mutate_model_alias,price_commands::save_price_rule,price_commands::retire_price_rule]);
    let mut context = tauri::generate_context!();
    local_paths::configure(&mut context).expect("project-local storage unavailable");
    #[cfg(debug_assertions)]
    let context = {
        let mut context = context;
        // Cargo tests/direct debug builds must be isolated even without the CLI overlay.
        context.config_mut().identifier = "com.tokenpulse.desktop.dev".into();
        context.config_mut().product_name = Some("TokenPulse Dev".into());
        for window in &mut context.config_mut().app.windows {
            window.title = "TokenPulse · 开发版".into();
            #[cfg(windows)]
            if navigation_smoke::secondary_probe() { window.focus = false; }
        }
        context
    };
    builder
        .build(context)
        .expect("desktop runtime failed")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { code, api, .. } = &event {
                if let Some(owner) = taskbar_commands::service(app) {
                    if !owner.exit_ready.load(std::sync::atomic::Ordering::Acquire) {
                        api.prevent_exit();
                        if owner.begin_exit() {
                            let app = app.clone();
                            let code = code.unwrap_or(0);
                            std::thread::spawn(move || {
                                owner.shutdown();
                                app.exit(code);
                            });
                        }
                        return;
                    }
                }
            }
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                main_window::stop_and_save(app);
                if let Some(state) = app.try_state::<RuntimeState>() {
                    #[cfg(windows)]
                    if let Ok(notify) = state.notify.lock() {
                        if let Ok(service) = notify.as_ref() {
                            service.shutdown();
                        }
                    }
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
                    if let Ok(service) = &state.revaluations {
                        service.shutdown();
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

#[cfg(debug_assertions)]
mod diagnostics_smoke;
#[cfg(all(debug_assertions, windows))]
mod input_smoke;
mod job_commands;
mod mini_commands;
mod mini_opacity;
mod mini_passthrough;
mod mini_shape;
#[cfg(debug_assertions)]
mod mini_smoke;
mod mini_window;
mod notify_commands;
#[cfg(windows)]
mod notify_runtime;
#[cfg(all(debug_assertions, windows))]
mod notify_smoke;
#[cfg(debug_assertions)]
mod offline_prices_smoke;
#[cfg(all(debug_assertions, windows))]
mod opacity_smoke;
#[cfg(all(debug_assertions, windows))]
mod passthrough_smoke;
#[cfg(windows)]
mod power;
#[cfg(all(debug_assertions, windows))]
mod power_resume_smoke;
#[cfg(all(debug_assertions, windows))]
mod power_taskbar_smoke;
#[cfg(debug_assertions)]
mod price_alias_smoke;
mod price_commands;
#[cfg(debug_assertions)]
mod price_revalue_smoke;
mod query_commands;
mod quota_commands;
mod quota_config;
#[cfg(all(debug_assertions, windows))]
mod quota_smoke;
#[cfg(all(debug_assertions, windows))]
mod quota_startup_smoke;
#[cfg(all(debug_assertions, windows))]
mod quota_taskbar_smoke;
#[cfg(windows)]
#[doc(hidden)]
pub mod release_verifier;
mod revalue_commands;
mod settings_commands;
mod shortcuts;
#[cfg(all(debug_assertions, windows))]
mod shortcuts_smoke;
#[cfg(debug_assertions)]
mod smoke;
mod source_commands;
#[cfg(all(debug_assertions, windows))]
mod source_dialog_smoke;
mod taskbar_commands;
#[cfg(all(debug_assertions, windows))]
mod taskbar_explorer_smoke;
mod taskbar_service;
#[cfg(all(debug_assertions, windows))]
mod taskbar_smoke;
mod update_commands;
mod update_installer;
mod update_service;
mod update_signing;
#[cfg(debug_assertions)]
mod update_smoke;
mod update_transport;

mod navigation;

pub mod local_paths;

#[cfg(all(debug_assertions, windows))]
mod account_dialog_smoke;
#[cfg(all(debug_assertions, windows))]
mod native_dialog_driver;

#[cfg(all(debug_assertions, windows))]
mod notify_dialog_smoke;

mod main_window;

#[cfg(all(debug_assertions, windows))]
mod main_window_smoke;

#[cfg(all(debug_assertions, windows))]
mod navigation_smoke;
