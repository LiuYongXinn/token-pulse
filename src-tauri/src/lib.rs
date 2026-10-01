use std::path::PathBuf;
use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use token_pulse_core::{API_VERSION, ServiceState};

struct RuntimeState {
    data_directory: PathBuf,
}

#[derive(serde::Serialize)]
struct AppStatus {
    api_version: u32,
    version: &'static str,
    development: bool,
    data_directory: String,
    collector: ServiceState,
    storage: ServiceState,
    quota: ServiceState,
    taskbar: ServiceState,
}

#[tauri::command]
fn get_app_status(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, RuntimeState>,
) -> Result<AppStatus, &'static str> {
    if window.label() != "main" {
        return Err("PERMISSION_DENIED");
    }
    Ok(AppStatus {
        api_version: API_VERSION,
        version: env!("CARGO_PKG_VERSION"),
        development: cfg!(debug_assertions),
        data_directory: state.data_directory.to_string_lossy().into_owned(),
        collector: ServiceState::NotConfigured,
        storage: ServiceState::NotImplemented,
        quota: ServiceState::NotConfigured,
        taskbar: ServiceState::NotImplemented,
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum WindowAction {
    OpenStats,
    HideMain,
    Quit,
}

#[tauri::command]
fn window_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    action: WindowAction,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("PERMISSION_DENIED".into());
    }
    match action {
        WindowAction::OpenStats => show_main(&app),
        WindowAction::HideMain => window.hide().map_err(|e| e.to_string()),
        WindowAction::Quit => {
            app.exit(0);
            Ok(())
        }
    }
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
        .setup(|app| {
            let expected_id = if cfg!(debug_assertions) { "com.tokenpulse.desktop.dev" } else { "com.tokenpulse.desktop" };
            if app.config().identifier != expected_id {
                return Err("application identifier must match build profile; use npm run tauri:dev for debug".into());
            }
            let data_directory = app.path().app_local_data_dir()?;
            token_pulse_store::prepare_data_directory(&data_directory)?;
            app.manage(RuntimeState { data_directory });
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
        .invoke_handler(tauri::generate_handler![get_app_status, window_action]);
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
    builder.run(context).expect("desktop runtime failed");
}

#[cfg(debug_assertions)]
mod smoke;
