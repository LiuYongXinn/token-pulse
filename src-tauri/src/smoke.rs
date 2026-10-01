//! Debug-only integration probe against a real Windows/Tauri runtime.
//! It does not configure sources, read user logs, or modify the production directory.
use std::{
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use tauri::{Listener, Manager};

pub fn start(app: tauri::AppHandle) {
    thread::spawn(move || {
        let result = verify(&app);
        match result {
            Ok(()) => {
                println!(
                    "NATIVE_SMOKE_OK: isolated startup, real WebView IPC, native power-message routing, tray, close-to-hide, single-instance activation, explicit exit"
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
    if !path.starts_with(app.path().app_local_data_dir().map_err(|e| e.to_string())?)
        || !path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("native-probe-"))
    {
        return Err("debug storage is not isolated".into());
    }
    if app.tray_by_id("main-tray").is_none() {
        return Err("tray icon not registered".into());
    }
    let window = app
        .get_webview_window("main")
        .ok_or("main window missing")?;
    app.state::<super::RuntimeState>()
        .database
        .as_ref()
        .map_err(|e| e.to_string())?
        .ensure_session(token_pulse_store::SessionRegistration {
            session_key: "native-probe-context".into(),
            provider_session_id: None,
            parent_key: None,
            parent_provider_id: None,
            created_at_ms: None,
            ledger_id: "native-probe-ledger".into(),
            registered_at_ms: 1,
        })
        .map_err(|e| e.to_string())?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let listener = app.listen("native-smoke-ipc", move |event| {
        let _ = sender.try_send(event.payload().to_owned());
    });
    window
        .eval(
            r#"
        (async () => {
            const invoke = window.__TAURI_INTERNALS__.invoke;
            let ok = false;
            try {
                const r = await invoke('get_app_status', { requestId: 'native-smoke' });
                ok = r.api_version === 1 && r.request_id === 'native-smoke'
                    && r.data.development === true && r.data.collector === 'not_configured'
                    && r.data.storage === 'ready' && r.data.storage_error === null;
                const sources = await invoke('get_sources', { requestId: 'native-smoke-sources' });
                ok = ok && sources.api_version === 1 && sources.request_id === 'native-smoke-sources'
                    && Array.isArray(sources.data.sources) && typeof sources.data.settings_revision === 'string';
                const jobs=await invoke('list_jobs',{requestId:'native-smoke-jobs',limit:20});
                ok=ok && jobs.api_version===1 && jobs.request_id==='native-smoke-jobs' && Array.isArray(jobs.data) && jobs.data.length===0;
                const context=await invoke('get_context_snapshot',{requestId:'native-smoke-context',sessionKey:'native-probe-context'});
                ok=ok && context.api_version===1 && context.request_id==='native-smoke-context'
                    && context.data.context_tokens===null && context.data.model_context_window===null
                    && context.data.percentage===null && context.data.observed_at_ms===null && context.data.quality==='unknown';
                const prices=await invoke('get_price_rules',{requestId:'native-smoke-prices',revision:null});
                ok=ok && prices.data.price_revision==='0' && prices.data.rules.length===0;
                const draft={provider:'native-fixture',model_exact:'native-fixture',source_id:null,currency:'USD',effective_from_ms:0,effective_to_ms:null,priority:0,input_rate_atoms:'1',cached_rate_atoms:null,output_rate_atoms:'2',origin_reference:'native smoke synthetic fixture'};
                const saved=await invoke('save_price_rule',{requestId:'native-smoke-price-create',request:{kind:'create',draft},expectedPriceRevision:'0'});
                ok=ok && saved.request_id==='native-smoke-price-create' && saved.data.price_revision==='1' && saved.data.rules.length===1 && saved.data.rules[0].origin==='custom';
                let conflict=false;
                try {await invoke('save_price_rule',{requestId:'native-smoke-price-conflict',request:{kind:'create',draft},expectedPriceRevision:'1'});} catch(error) {conflict=error.code==='PRICE_RULE_CONFLICT';}
                ok=ok && conflict;
                const replaced=await invoke('save_price_rule',{requestId:'native-smoke-price-replace',request:{kind:'replace',rule_id:saved.data.rules[0].rule_id,draft:{...draft,input_rate_atoms:'3'}},expectedPriceRevision:'1'});
                const historical=await invoke('get_price_rules',{requestId:'native-smoke-price-history',revision:'1'});
                ok=ok && replaced.data.price_revision==='2' && replaced.data.rules[0].input_rate_atoms==='3' && historical.data.rules[0].input_rate_atoms==='1';
                const retired=await invoke('retire_price_rule',{requestId:'native-smoke-price-retire',ruleId:replaced.data.rules[0].rule_id,expectedPriceRevision:'2'});
                ok=ok && retired.data.price_revision==='3' && retired.data.rules.length===0;
            } catch (_) {}
            await invoke('plugin:event|emit', { event: 'native-smoke-ipc', payload: ok });
        })();
    "#,
        )
        .map_err(|e| e.to_string())?;
    let ipc = receiver.recv_timeout(Duration::from_secs(10));
    app.unlisten(listener);
    if ipc.as_deref() != Ok("true") {
        return Err("WebView get_app_status IPC failed".into());
    }
    if !window.is_visible().map_err(|e| e.to_string())? {
        return Err("cold start window is hidden".into());
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, SendMessageW, WM_POWERBROADCAST,
            },
        };
        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as HWND;
        unsafe {
            SendMessageW(hwnd, WM_POWERBROADCAST, PBT_APMSUSPEND as usize, 0);
        }
        thread::sleep(Duration::from_millis(150));
        if !app
            .state::<super::RuntimeState>()
            .collector
            .as_ref()
            .map_err(|e| e.to_string())?
            .status()
            .suspended
        {
            return Err("native power suspend not routed".into());
        }
        unsafe {
            SendMessageW(hwnd, WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize, 0);
        }
        thread::sleep(Duration::from_millis(150));
        if app
            .state::<super::RuntimeState>()
            .collector
            .as_ref()
            .map_err(|e| e.to_string())?
            .status()
            .suspended
        {
            return Err("native power resume not routed".into());
        }
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
