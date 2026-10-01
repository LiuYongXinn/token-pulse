//! Debug-only integration probe against a real Windows/Tauri runtime.
//! It does not configure sources, read user logs, or modify the production directory.
use std::{
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use tauri::Manager;

pub fn start(app: tauri::AppHandle) {
    thread::spawn(move || {
        let result = verify(&app);
        match result {
            Ok(()) => {
                println!(
                    "NATIVE_SMOKE_OK: isolated startup, tray, close-to-hide, single-instance activation, explicit exit"
                );
                app.exit(0);
            }
            Err(e) => {
                eprintln!("NATIVE_SMOKE_FAILED: {e}");
                app.exit(1);
            }
        }
    });
}

fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    thread::sleep(Duration::from_secs(2));
    if !app.config().identifier.ends_with(".dev") {
        return Err("debug identifier is not isolated".into());
    }
    let path = &app.state::<super::RuntimeState>().data_directory;
    if path.file_name().and_then(|name| name.to_str()) != Some("com.tokenpulse.desktop.dev") {
        return Err("debug storage is not isolated".into());
    }
    if app.tray_by_id("main-tray").is_none() {
        return Err("tray icon not registered".into());
    }
    let window = app
        .get_webview_window("main")
        .ok_or("main window missing")?;
    if !window.is_visible().map_err(|e| e.to_string())? {
        return Err("cold start window is hidden".into());
    }
    window.close().map_err(|e| e.to_string())?;
    thread::sleep(Duration::from_millis(500));
    if window.is_visible().map_err(|e| e.to_string())? {
        return Err("close did not hide the window".into());
    }
    if app.tray_by_id("main-tray").is_none() {
        return Err("close destroyed the tray".into());
    }
    let mut second = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .arg("--native-probe")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut exited = false;
    for _ in 0..50 {
        if let Some(status) = second.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err("second instance failed".into());
            }
            exited = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if !exited {
        let _ = second.kill();
        return Err("second instance did not exit".into());
    }
    if !window.is_visible().map_err(|e| e.to_string())? {
        return Err("second instance did not reactivate main".into());
    }
    Ok(())
}
