//! Explicit debug-only real Tauri/host/layout check, using the isolated native-smoke database.
use std::{
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;
use token_pulse_core::taskbar::TaskbarRuntimeState;
pub fn start(app: tauri::AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        let result = super::mini_window::show(&app).and_then(|_| verify(&app));
        if let Err(error) = &result {
            eprintln!("NATIVE_TASKBAR_MANAGER_FAILED: {error}");
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}
pub fn start_actions(app: tauri::AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        let result = super::mini_window::show(&app).and_then(|_| verify_actions(&app));
        if let Err(error) = &result {
            eprintln!("NATIVE_TASKBAR_ACTIONS_FAILED: {error}");
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}
fn verify_actions(app: &tauri::AppHandle) -> Result<(), String> {
    use token_pulse_taskbar::windows::topology::inspect_primary_taskbar;
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_LBUTTONDBLCLK, WM_LBUTTONUP};
    if !app
        .state::<super::RuntimeState>()
        .data_directory
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("native-probe-"))
    {
        return Err("actions check requires isolated smoke database".into());
    }
    let before = inspect_primary_taskbar().map_err(|e| format!("actions baseline: {e:?}"))?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      await invoke('mini_window_action',{requestId:'taskbar-action-compact',request:{kind:'set_expanded',expanded:false}});
      await invoke('mini_window_action',{requestId:'taskbar-action-hide',request:{kind:'hide'}});
    "#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const current=await invoke('get_taskbar_preferences',{requestId:'taskbar-actions-default'});
      await invoke('set_taskbar_preferences',{requestId:'taskbar-actions-enable',request:{preferences:{...current.data.preferences,enabled:true,fallback_to_mini:false},expected_settings_revision:current.data.settings_revision}});
    "#,
    )?;
    wait_actions_ready(app)?;
    main.hide().map_err(|e| e.to_string())?;
    // These are authored messages to our verified own readout, explicitly not real mouse input.
    own_click_message(app, WM_LBUTTONUP)?;
    until_action(app, || {
        mini.is_visible().unwrap_or(false)
            && app
                .state::<super::RuntimeState>()
                .mini_window
                .lock()
                .is_ok_and(|state| state.expanded)
    })?;
    let size = mini
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(mini.scale_factor().map_err(|e| e.to_string())?);
    if (size.width - 360.0).abs() > 1.0 || (size.height - 380.0).abs() > 1.0 {
        return Err(format!("taskbar mini size mismatch: {size:?}"));
    }
    if main.is_visible().map_err(|e| e.to_string())? {
        return Err("single intention opened statistics".into());
    }
    if !app
        .state::<super::RuntimeState>()
        .database
        .as_ref()
        .map_err(|e| e.code.to_string())?
        .mini_window_preferences()
        .map_err(|e| e.code.to_string())?
        .interaction
        .expanded
    {
        return Err("taskbar expansion was not persisted".into());
    }
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      await wait(()=>document.querySelector('.mini-window.expanded'));
      const current=await invoke('mini_window_action',{requestId:'taskbar-native-expanded-state',request:{kind:'read'}});
      if(!current.data.expanded)throw new Error('TASKBAR_MINI_STATE_NOT_EXPANDED');
      await invoke('mini_window_action',{requestId:'taskbar-double-hide',request:{kind:'hide'}});
    "#,
    )?;
    wait_actions_ready(app)?;
    own_click_message(app, WM_LBUTTONUP)?;
    own_click_message(app, WM_LBUTTONDBLCLK)?;
    own_click_message(app, WM_LBUTTONUP)?;
    until_action(app, || {
        main.is_visible().unwrap_or(false)
            && app
                .state::<super::RuntimeState>()
                .main_navigation
                .lock()
                .is_ok_and(|value| value.mini_stats().is_some())
    })?;
    thread::sleep(Duration::from_millis(
        u64::from(unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() })
            + 200,
    ));
    if mini.is_visible().map_err(|e| e.to_string())? {
        return Err("double intention also opened mini".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const current=await invoke('get_mini_scope',{requestId:'taskbar-double-scope'});
      const intent=await invoke('get_mini_stats_request',{requestId:'taskbar-double-intent'});
      const usage=await invoke('get_mini_usage',{requestId:'taskbar-double-usage'});
      if(JSON.stringify(current.data.mini_scope)!==JSON.stringify(intent.data?.mini_scope)||intent.data.calendar.range.timezone!==usage.data.range.timezone||intent.data.calendar.range.start_ms!==usage.data.range.start_ms)throw new Error('TASKBAR_SHARED_STATS_RANGE_LOST');
      await wait(()=>document.querySelector('.mini-stat-scope'));
      const status=await invoke('get_taskbar_status',{requestId:'taskbar-action-error'});
      if(status.data.action_error!==null)throw new Error('TASKBAR_ACTION_ERROR:'+status.data.action_error);
    "#,
    )?;
    verify_menu_actions(app, &main, &mini)?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const current=await invoke('get_taskbar_preferences',{requestId:'menu-reenable'});
      await invoke('set_taskbar_preferences',{requestId:'menu-reenable-save',request:{preferences:{...current.data.preferences,enabled:true},expected_settings_revision:current.data.settings_revision}});
    "#,
    )?;
    wait_actions_ready(app)?;
    let (pid, _) = open_own_menu(app)?;
    let navigation_before = app
        .state::<super::RuntimeState>()
        .main_navigation
        .lock()
        .map_err(|_| "navigation lock")?
        .revision
        .clone();
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const current=await invoke('get_display_settings',{requestId:'menu-open-privacy'});
      await invoke('set_display_privacy',{requestId:'menu-open-privacy-save',request:{privacy:!current.data.preferences.privacy,expected_settings_revision:current.data.settings_revision}});
    "#,
    )?;
    wait_actions_ready(app)?;
    if own_menu(pid).is_some()
        || app
            .state::<super::RuntimeState>()
            .main_navigation
            .lock()
            .map_err(|_| "navigation lock")?
            .revision
            != navigation_before
    {
        return Err("privacy barrier did not cancel menu without navigation".into());
    }
    open_own_menu(app)?;
    super::taskbar_commands::service(app)
        .ok_or("service missing")?
        .shutdown();
    if inspect_primary_taskbar().map_err(|e| format!("actions restore: {e:?}"))? != before {
        return Err("actions shutdown did not restore geometry".into());
    }
    println!(
        "NATIVE_TASKBAR_ACTIONS_OK: authored own-window clicks, production host pipe/actor, real expanded 360x380 mini with persisted state and WebView invalidation, double statistics shared scope without mini, privacy cancels open menu, shutdown during native menu restores actual geometry; real SendInput acceptance separate"
    );
    Ok(())
}
fn until_action(app: &tauri::AppHandle, done: impl Fn() -> bool) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(10);
    while !done() {
        let snapshot = super::taskbar_commands::service(app)
            .ok_or("service missing")?
            .snapshot()
            .map_err(|e| e.to_string())?;
        if let Some(error) = snapshot.action_error {
            return Err(format!("actual window action failed: {error}"));
        }
        if Instant::now() >= until {
            return Err(format!("action functional deadline: {snapshot:?}"));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
fn wait_actions_ready(app: &tauri::AppHandle) -> Result<(), String> {
    until_action(app, || {
        let runtime = app.state::<super::RuntimeState>();
        let Some(service) = super::taskbar_commands::service(app) else {
            return false;
        };
        let Ok(snapshot) = service.snapshot() else {
            return false;
        };
        let Ok(configuration) = runtime
            .database
            .as_ref()
            .map_err(|e| e.code)
            .and_then(|db| db.taskbar_preferences().map_err(|e| e.code))
        else {
            return false;
        };
        snapshot.state == TaskbarRuntimeState::Embedded
            && snapshot.applied_settings_revision.as_ref() == Some(&configuration.settings_revision)
            && service.owned_host_pid().is_some()
    })
}
fn own_click_message(app: &tauri::AppHandle, message: u32) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumChildWindows, FindWindowW, GetClassNameW, GetWindowThreadProcessId,
            IsWindowVisible, SMTO_ABORTIFHUNG, SendMessageTimeoutW,
        },
    };
    struct Probe {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
        let probe = unsafe { &mut *(parameter as *mut Probe) };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut pid);
        }
        if pid == probe.pid && unsafe { IsWindowVisible(window) } != 0 {
            let mut class = [0; 128];
            let n = unsafe { GetClassNameW(window, class.as_mut_ptr(), 128) };
            if n > 0
                && String::from_utf16_lossy(&class[..n as usize])
                    .starts_with("TokenPulse.Taskbar.Readout.")
            {
                probe.windows.push(window);
            }
        }
        1
    }
    let pid = super::taskbar_commands::service(app)
        .and_then(|service| service.owned_host_pid())
        .ok_or("no own embedded host")?;
    let class: Vec<u16> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
    let root = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    if root.is_null() {
        return Err("taskbar root missing".into());
    }
    let mut probe = Probe {
        pid,
        windows: vec![],
    };
    unsafe {
        EnumChildWindows(root, Some(collect), (&mut probe as *mut Probe) as LPARAM);
    }
    if probe.windows.len() != 1 {
        return Err("own readout identity not unique".into());
    }
    if message == windows_sys::Win32::UI::WindowsAndMessaging::WM_CONTEXTMENU {
        if unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                probe.windows[0],
                message,
                probe.windows[0] as usize,
                -1,
            )
        } == 0
        {
            return Err("context menu post failed".into());
        }
        return Ok(());
    }
    if unsafe {
        SendMessageTimeoutW(
            probe.windows[0],
            message,
            0,
            0,
            SMTO_ABORTIFHUNG,
            1000,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err("own click message timed out".into());
    }
    Ok(())
}
fn own_menu(pid: u32) -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
        },
    };
    struct Probe {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
        let probe = unsafe { &mut *(parameter as *mut Probe) };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut pid);
        }
        if pid == probe.pid && unsafe { IsWindowVisible(window) } != 0 {
            let mut name = [0; 64];
            let n = unsafe { GetClassNameW(window, name.as_mut_ptr(), 64) };
            if n > 0 && String::from_utf16_lossy(&name[..n as usize]) == "#32768" {
                probe.windows.push(window);
            }
        }
        1
    }
    let mut probe = Probe {
        pid,
        windows: vec![],
    };
    unsafe {
        EnumWindows(Some(collect), (&mut probe as *mut Probe) as LPARAM);
    }
    (probe.windows.len() == 1).then(|| probe.windows[0])
}
fn open_own_menu(
    app: &tauri::AppHandle,
) -> Result<(u32, windows_sys::Win32::Foundation::HWND), String> {
    let pid = super::taskbar_commands::service(app)
        .and_then(|s| s.owned_host_pid())
        .ok_or("menu host absent")?;
    own_click_message(
        app,
        windows_sys::Win32::UI::WindowsAndMessaging::WM_RBUTTONDOWN,
    )?;
    own_click_message(
        app,
        windows_sys::Win32::UI::WindowsAndMessaging::WM_RBUTTONUP,
    )?;
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(window) = own_menu(pid) {
            return Ok((pid, window));
        }
        if Instant::now() >= until {
            return Err("own actual native menu did not appear".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}
fn select_own_menu(app: &tauri::AppHandle, id: u32) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GetGUIThreadInfo, GetMenuItemCount, GetMenuState, GetMenuStringW,
        GetWindowThreadProcessId, HMENU, MF_BYCOMMAND, MF_BYPOSITION, MF_CHECKED, PostMessageW,
        SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_CHAR,
    };
    let (pid, window) = open_own_menu(app)?;
    // MN_GETHMENU reads the real standard menu owned by the verified host process.
    let mut result = 0;
    if unsafe { SendMessageTimeoutW(window, 0x01e1, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut result) }
        == 0
    {
        return Err("own menu handle unavailable".into());
    }
    let menu = result as HMENU;
    let mut info: GUITHREADINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    let tid = unsafe { GetWindowThreadProcessId(window, std::ptr::null_mut()) };
    if unsafe { GetGUIThreadInfo(tid, &mut info) } == 0 || info.hwndMenuOwner.is_null() {
        return Err("own native menu owner unavailable".into());
    }
    let key_window = info.hwndMenuOwner;
    let mut owner_pid = 0;
    unsafe {
        GetWindowThreadProcessId(key_window, &mut owner_pid);
    }
    if owner_pid != pid {
        return Err("native menu owner identity mismatch".into());
    }
    if unsafe { GetMenuItemCount(menu) } != 6 {
        return Err("native menu items mismatch".into());
    }
    let expected_privacy = app
        .state::<super::RuntimeState>()
        .database
        .as_ref()
        .map_err(|e| e.code.to_string())?
        .display_settings()
        .map_err(|e| e.code.to_string())?
        .preferences
        .privacy;
    if (unsafe { GetMenuState(menu, 104, MF_BYCOMMAND) } & MF_CHECKED != 0) != expected_privacy {
        return Err("native menu privacy check differs from committed policy".into());
    }
    let mut name = [0; 128];
    let n = unsafe { GetMenuStringW(menu, 2, name.as_mut_ptr(), 128, MF_BYPOSITION) };
    if n <= 0 || String::from_utf16_lossy(&name[..n as usize]) != "任务栏设置(&T)" {
        return Err("native menu label mismatch".into());
    }
    // Standard menu mnemonics are character messages in the native modal loop.
    let mnemonic = match id {
        101 => 'f',
        102 => 's',
        103 => 't',
        104 => 'p',
        105 => 'h',
        _ => return Err("unsupported smoke menu choice".into()),
    };
    if unsafe { PostMessageW(key_window, WM_CHAR, mnemonic as usize, 1) } == 0 {
        return Err("own menu mnemonic post failed".into());
    }
    let until = Instant::now() + Duration::from_secs(3);
    while own_menu(pid).is_some() {
        if Instant::now() >= until {
            return Err(format!("own native menu did not select {id}"));
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
fn verify_menu_actions(
    app: &tauri::AppHandle,
    main: &tauri::WebviewWindow,
    mini: &tauri::WebviewWindow,
) -> Result<(), String> {
    wait_actions_ready(app)?;
    select_own_menu(app, 101)?;
    until_action(app, || mini.is_visible().unwrap_or(false))?;
    super::mini_smoke::evaluate(
        app,
        mini,
        r#"
      await wait(()=>document.querySelector('.mini-window.expanded'));
      await invoke('mini_window_action',{requestId:'menu-float-hide',request:{kind:'hide'}});
    "#,
    )?;
    select_own_menu(app, 103)?;
    until_action(app, || {
        app.state::<super::RuntimeState>()
            .main_navigation
            .lock()
            .is_ok_and(|n| {
                matches!(
                    n.intent,
                    Some(token_pulse_core::navigation::MainNavigationIntent::TaskbarSettings {})
                )
            })
    })?;
    select_own_menu(app, 102)?;
    until_action(app, || {
        app.state::<super::RuntimeState>()
            .main_navigation
            .lock()
            .is_ok_and(|n| n.mini_stats().is_some())
    })?;
    super::mini_smoke::evaluate(
        app,
        main,
        r#"
      await wait(()=>document.querySelector('.mini-stat-scope'));
    "#,
    )?;
    if mini.is_visible().map_err(|e| e.to_string())? {
        return Err("menu statistics also opened mini".into());
    }
    select_own_menu(app, 103)?;
    until_action(app, || {
        app.state::<super::RuntimeState>()
            .main_navigation
            .lock()
            .is_ok_and(|n| {
                matches!(
                    n.intent,
                    Some(token_pulse_core::navigation::MainNavigationIntent::TaskbarSettings {})
                )
            })
    })?;
    super::mini_smoke::evaluate(
        app,
        main,
        r#"
      await wait(()=>document.querySelector('[role=tab][aria-selected=true]')?.textContent==='任务栏显示');
      const navigation=await invoke('get_main_navigation',{requestId:'menu-navigation'});
      if(navigation.data.intent?.kind!=='taskbar_settings')throw new Error('MENU_SETTINGS_INTENT_LOST');
    "#,
    )?;
    let runtime = app.state::<super::RuntimeState>();
    let db = runtime.database.as_ref().map_err(|e| e.code.to_string())?;
    let privacy = db
        .display_settings()
        .map_err(|e| e.code.to_string())?
        .preferences
        .privacy;
    wait_actions_ready(app)?;
    select_own_menu(app, 104)?;
    until_action(app, || {
        db.display_settings()
            .is_ok_and(|s| s.preferences.privacy != privacy)
    })?;
    wait_actions_ready(app)?;
    super::mini_smoke::evaluate(
        app,
        mini,
        r#"
      const settings=await invoke('get_display_settings',{requestId:'menu-mini-privacy'});
      const current=await invoke('get_mini_usage',{requestId:'menu-mini-policy'});
      if(settings.data.preferences.privacy!==current.display_policy.privacy)throw new Error('MENU_PRIVACY_NOT_SHARED');
    "#,
    )?;
    select_own_menu(app, 104)?;
    until_action(app, || {
        db.display_settings()
            .is_ok_and(|s| s.preferences.privacy == privacy)
    })?;
    wait_actions_ready(app)?;
    select_own_menu(app, 105)?;
    wait(app, TaskbarRuntimeState::Disabled)?;
    if db
        .taskbar_preferences()
        .map_err(|e| e.code.to_string())?
        .preferences
        .enabled
    {
        return Err("menu hide was not persisted".into());
    }
    if !main.is_visible().map_err(|e| e.to_string())? {
        return Err("menu hide closed statistics".into());
    }
    println!(
        "NATIVE_TASKBAR_MENU_OK: actual Windows popup/names, authored own-menu mnemonic messages, settings navigation, coordinated shared privacy toggles and persisted disable"
    );
    Ok(())
}
fn wait(app: &tauri::AppHandle, state: TaskbarRuntimeState) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = super::taskbar_commands::service(app)
            .ok_or("taskbar service missing")?
            .snapshot()
            .map_err(|e| e.to_string())?;
        if snapshot.state == state {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(format!("taskbar functional state timeout: {snapshot:?}"));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
fn wait_fallback(app: &tauri::AppHandle, visible: bool) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let state = super::taskbar_commands::service(app)
            .ok_or("taskbar missing")?
            .snapshot()
            .map_err(|e| e.to_string())?;
        let actual = app
            .get_webview_window("mini")
            .is_some_and(|w| w.is_visible().unwrap_or(false));
        if state.fallback_visible == Some(visible) && actual == visible {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(format!(
                "fallback state timeout: expected={visible}, {state:?}, actual={actual}"
            ));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
pub fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    use token_pulse_taskbar::windows::topology::inspect_primary_taskbar;
    let path = &app.state::<super::RuntimeState>().data_directory;
    if !path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("native-probe-"))
    {
        return Err("taskbar check requires isolated smoke database".into());
    }
    let main = app.get_webview_window("main").ok_or("main missing")?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    let before = inspect_primary_taskbar().map_err(|e| format!("taskbar baseline: {e:?}"))?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      let current=await invoke('get_taskbar_preferences',{requestId:'taskbar-default'});
      if(current.data.preferences.enabled)throw new Error('TASKBAR_DEFAULT_ENABLED');
      const saved=await invoke('set_taskbar_preferences',{requestId:'taskbar-enable',request:{preferences:{...current.data.preferences,enabled:true,fallback_to_mini:false},expected_settings_revision:current.data.settings_revision}});
      if(!saved.data.preferences.enabled)throw new Error('TASKBAR_INTENT_NOT_SAVED');
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Embedded)?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const waitFor=async(read)=>{for(let i=0;i<100;i++){if(read())return;await new Promise(r=>setTimeout(r,50));}throw new Error('TASKBAR_UI_TIMEOUT');};
      document.querySelector('nav button:last-child').click();
      await waitFor(()=>[...document.querySelectorAll('[role=tab]')].some(n=>n.textContent==='任务栏显示'));
      [...document.querySelectorAll('[role=tab]')].find(n=>n.textContent==='任务栏显示').click();
      await waitFor(()=>document.querySelector('.taskbar-enable input')?.checked===true);
      await waitFor(()=>document.querySelector('.taskbar-runtime [role=status]')?.textContent==='已嵌入任务栏');
      if(document.querySelector('.taskbar-settings-panel').textContent.includes('显示隐私策略已变化'))throw new Error('TASKBAR_UI_PROTOCOL_STAMP');
    "#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      let denied=false;try{await invoke('get_taskbar_status',{requestId:'taskbar-mini-denied'});}catch{denied=true;}
      if(!denied)throw new Error('MINI_TASKBAR_COMMAND_ALLOWED');
    "#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const configured=await invoke('get_taskbar_preferences',{requestId:'taskbar-current'});
      const status=await invoke('get_taskbar_status',{requestId:'taskbar-actual'});
      if(status.data.state!=='embedded'||status.data.applied_settings_revision!==configured.data.settings_revision)throw new Error('TASKBAR_FAKE_STATUS');
      const display=await invoke('get_display_settings',{requestId:'taskbar-privacy-before'});
      await invoke('set_display_privacy',{requestId:'taskbar-privacy-change',request:{privacy:!display.data.preferences.privacy,expected_settings_revision:display.data.settings_revision}});
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Embedded)?;
    // A synthetic system message to our main HWND only, not a real machine suspend.
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, SendMessageW, WM_POWERBROADCAST,
    };
    let hwnd = main.hwnd().map_err(|e| e.to_string())?.0.cast();
    unsafe {
        SendMessageW(hwnd, WM_POWERBROADCAST, PBT_APMSUSPEND as usize, 0);
    }
    wait(app, TaskbarRuntimeState::Suspended)?;
    if inspect_primary_taskbar().map_err(|e| format!("suspended topology: {e:?}"))? != before {
        return Err("suspend did not restore original geometry".into());
    }
    unsafe {
        SendMessageW(hwnd, WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize, 0);
    }
    wait(app, TaskbarRuntimeState::Embedded)?;
    main.hide().map_err(|e| e.to_string())?;
    let last_update = super::taskbar_commands::service(app)
        .ok_or("service missing")?
        .snapshot()
        .map_err(|e| e.to_string())?
        .last_snapshot_at_ms;
    super::quota_commands::update_visibility(&main);
    thread::sleep(Duration::from_millis(1200));
    wait(app, TaskbarRuntimeState::Embedded)?;
    if super::taskbar_commands::service(app)
        .ok_or("service missing")?
        .snapshot()
        .map_err(|e| e.to_string())?
        .last_snapshot_at_ms
        == last_update
    {
        return Err("hidden main did not publish another actual snapshot".into());
    }
    super::show_main(app)?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const waitFor=async(read)=>{for(let i=0;i<100;i++){if(read())return;await new Promise(r=>setTimeout(r,50));}throw new Error('TASKBAR_UI_SAVE_TIMEOUT');};
      await waitFor(()=>document.querySelector('.taskbar-enable input')?.checked===true);
      document.querySelector('.taskbar-enable input').click();
      await waitFor(()=>document.querySelector('.taskbar-preferences button.primary')?.disabled===false);
      document.querySelector('.taskbar-preferences button.primary').click();
      await waitFor(()=>document.querySelector('.taskbar-runtime [role=status]')?.textContent==='已关闭');
      const current=await invoke('get_taskbar_preferences',{requestId:'taskbar-disable-ui-verify'});
      if(current.data.preferences.enabled)throw new Error('TASKBAR_UI_SAVE_NOT_PERSISTED');
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Disabled)?;
    if inspect_primary_taskbar().map_err(|e| format!("disabled topology: {e:?}"))? != before {
        return Err("manager disable did not restore original geometry".into());
    }
    // Destroy this test's own mini to cover automatic creation as well as hidden-window reuse.
    mini.destroy().map_err(|e| e.to_string())?;
    let until = Instant::now() + Duration::from_secs(3);
    while app.get_webview_window("mini").is_some() {
        if Instant::now() >= until {
            return Err("old isolated mini did not close".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    // A real unsupported position is a controlled fallback trigger; no machine setting changes.
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return Err("no foreground HWND to verify nonactivating fallback".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const current=await invoke('get_taskbar_preferences',{requestId:'taskbar-fallback-enable'});
      await invoke('set_taskbar_preferences',{requestId:'taskbar-fallback-save',request:{preferences:{...current.data.preferences,enabled:true,position:'application_right',fallback_to_mini:true},expected_settings_revision:current.data.settings_revision}});
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Unavailable)?;
    wait_fallback(app, true)?;
    let mini = app
        .get_webview_window("mini")
        .ok_or("fallback did not create mini")?;
    if unsafe { GetForegroundWindow() } != foreground {
        return Err("automatic fallback stole foreground focus".into());
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_NOACTIVATE,
    };
    if unsafe {
        GetWindowLongPtrW(
            mini.hwnd().map_err(|e| e.to_string())?.0.cast(),
            GWL_EXSTYLE,
        )
    } & WS_EX_NOACTIVATE as isize
        != 0
    {
        return Err("fallback left mini permanently nonactivating".into());
    }
    if inspect_primary_taskbar().map_err(|e| format!("fallback topology: {e:?}"))? != before {
        return Err("unsupported-position fallback changed taskbar geometry".into());
    }
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      await invoke('mini_window_action',{requestId:'taskbar-fallback-user-hide',request:{kind:'hide'}});
    "#,
    )?;
    wait_fallback(app, false)?;
    thread::sleep(Duration::from_millis(1500));
    if mini.is_visible().map_err(|e| e.to_string())? {
        return Err("fallback reopened user-hidden mini in same failure episode".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      await invoke('retry_taskbar_embed',{requestId:'taskbar-fallback-explicit-retry'});
    "#,
    )?;
    wait_fallback(app, true)?;
    if unsafe { GetForegroundWindow() } != foreground {
        return Err("reused fallback mini stole foreground focus".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const waitFor=async(read)=>{for(let i=0;i<100;i++){if(read())return;await new Promise(r=>setTimeout(r,50));}throw new Error('TASKBAR_FALLBACK_UI_TIMEOUT');};
      const read=()=>[...document.querySelectorAll('.taskbar-preferences label')].find(n=>n.textContent==='任务栏不可用时显示悬浮窗')?.querySelector('input');
      await waitFor(()=>read()?.checked===true); read().click();
      await waitFor(()=>document.querySelector('.taskbar-preferences button.primary')?.disabled===false);document.querySelector('.taskbar-preferences button.primary').click();
      await waitFor(()=>document.querySelector('.taskbar-preferences button.primary')?.disabled===true && read()?.checked===false);
      const current=await invoke('get_taskbar_preferences',{requestId:'taskbar-fallback-ui-verify'});
      if(current.data.preferences.fallback_to_mini)throw new Error('TASKBAR_FALLBACK_UI_NOT_SAVED');
    "#,
    )?;
    thread::sleep(Duration::from_millis(1200));
    if !mini.is_visible().map_err(|e| e.to_string())? {
        return Err("disabling fallback closed existing mini".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const current=await invoke('get_taskbar_preferences',{requestId:'taskbar-exit-enable'});
      await invoke('set_taskbar_preferences',{requestId:'taskbar-exit-enable-save',request:{preferences:{...current.data.preferences,enabled:true,position:'notification_left'},expected_settings_revision:current.data.settings_revision}});
    "#,
    )?;
    wait(app, TaskbarRuntimeState::Embedded)?;
    if !mini.is_visible().map_err(|e| e.to_string())? {
        return Err("successful re-embedding closed existing mini".into());
    }
    super::taskbar_commands::service(app)
        .ok_or("service missing")?
        .shutdown();
    if inspect_primary_taskbar().map_err(|e| format!("shutdown topology: {e:?}"))? != before {
        return Err("shutdown while embedded did not restore original geometry".into());
    }
    println!(
        "NATIVE_TASKBAR_MANAGER_OK: isolated SQLite DTO, real settings UI/status and save, native host embedding, shared privacy barrier, hidden-main refresh, synthetic power routing, nonactivating fallback/user-hide/explicit-retry, retained mini after recovery, disable and embedded shutdown geometry restore"
    );
    Ok(())
}
