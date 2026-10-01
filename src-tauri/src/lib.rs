use std::path::PathBuf;
use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use token_pulse_core::{
    ServiceState,
    error::{AppError, ErrorCode},
    protocol::{AppStatus, Response, WindowAction, validate_request_id},
};

struct RuntimeState {
    selections: std::sync::Arc<std::sync::Mutex<token_pulse_core::selections::DirectorySelections>>,
    data_directory: PathBuf,
    database: token_pulse_store::StoreResult<token_pulse_store::Database>,
    collector: token_pulse_store::StoreResult<
        std::sync::Arc<token_pulse_collector::service::CollectorService>,
    >,
    jobs: token_pulse_store::StoreResult<token_pulse_collector::jobs::JobService>,
}

#[tauri::command]
fn get_app_status(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, RuntimeState>,
    request_id: String,
) -> Result<Response<AppStatus>, Box<AppError>> {
    validate_request_id(&request_id)
        .map_err(|code| AppError::new(code, "invalid-request".into()))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request_id,
        )));
    }
    Ok(Response::new(
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
            quota: ServiceState::NotConfigured,
            taskbar: ServiceState::NotImplemented,
        },
    ))
}

#[tauri::command]
fn perform_window_action(
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
        WindowAction::HideMain => window.hide().map_err(|e| e.to_string()),
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
    window.set_focus().map_err(|e| e.to_string())
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
            app.manage(RuntimeState { data_directory, database, collector, jobs, selections: Default::default() });
            #[cfg(windows)]
            power::install(app.handle()).map_err(std::io::Error::other)?;
            let open = MenuItem::with_id(app, "open", "打开统计", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 TokenPulse", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().ok_or("missing application icon")?.clone())
                .tooltip("TokenPulse · 打开统计")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => { let _ = show_main(app); },
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
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![get_app_status, perform_window_action,source_commands::get_sources,source_commands::choose_source_directory,source_commands::manage_source,job_commands::start_job,job_commands::get_job,job_commands::list_jobs,job_commands::cancel_job]);
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
#[cfg(windows)]
mod power;
#[cfg(debug_assertions)]
mod smoke;
mod source_commands;
