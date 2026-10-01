//! Debug-only genuine global-key conflicts, mouse hit-testing and fail-open recovery checks.
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use tauri::Manager;
use token_pulse_core::shortcuts::{RecoveryShortcut, ShortcutRegistration};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::COLOR_WINDOW,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
unsafe extern "system" fn counter_window(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_CREATE {
        let creation = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, creation.lpCreateParams as isize);
        }
    }
    if message == WM_LBUTTONDOWN {
        let counter = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
        if counter != 0 {
            unsafe { &*(counter as *const AtomicUsize) }.fetch_add(1, Ordering::SeqCst);
        }
        return 0;
    }
    if message == WM_DESTROY {
        unsafe {
            PostQuitMessage(0);
        }
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
struct MouseTarget {
    hwnd: usize,
    counter: Arc<AtomicUsize>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for MouseTarget {
    fn drop(&mut self) {
        unsafe {
            PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl MouseTarget {
    fn create(window: &tauri::WebviewWindow) -> Result<Self, String> {
        let position = window.outer_position().map_err(|e| e.to_string())?;
        let size = window.outer_size().map_err(|e| e.to_string())?;
        let counter = Arc::new(AtomicUsize::new(0));
        let observed = counter.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let class: Vec<u16> = format!("TokenPulseMouseProbe{}", uuid::Uuid::new_v4())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
            let definition = WNDCLASSW {
                lpfnWndProc: Some(counter_window),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                hbrBackground: (COLOR_WINDOW + 1) as usize as _,
                ..unsafe { std::mem::zeroed() }
            };
            if unsafe { RegisterClassW(&definition) } == 0 {
                let _ = sender.send(Err("mouse target class registration failed".to_string()));
                return;
            }
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                    class.as_ptr(),
                    class.as_ptr(),
                    WS_POPUP,
                    position.x,
                    position.y,
                    size.width as i32,
                    size.height as i32,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    instance,
                    Arc::as_ptr(&observed) as _,
                )
            };
            if hwnd.is_null() {
                let _ = sender.send(Err("mouse target creation failed".to_string()));
                unsafe {
                    UnregisterClassW(class.as_ptr(), instance);
                }
                return;
            }
            unsafe {
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            let _ = sender.send(Ok(hwnd as usize));
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            unsafe {
                UnregisterClassW(class.as_ptr(), instance);
            }
        });
        let hwnd = receiver
            .recv_timeout(Duration::from_secs(3))
            .map_err(|e| e.to_string())??;
        Ok(Self {
            hwnd,
            counter,
            worker: Some(worker),
        })
    }
    fn click(&self, window: &tauri::WebviewWindow) -> Result<(), String> {
        let position = window.outer_position().map_err(|e| e.to_string())?;
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let mut original: POINT = unsafe { std::mem::zeroed() };
        unsafe {
            GetCursorPos(&mut original);
        }
        // A genuine desktop cursor hit-test: no synthetic window messages or DOM click.
        let mut inputs: [INPUT; 2] = unsafe { std::mem::zeroed() };
        inputs[0].r#type = INPUT_MOUSE;
        inputs[0].Anonymous.mi.dwFlags = MOUSEEVENTF_LEFTDOWN;
        inputs[1].r#type = INPUT_MOUSE;
        inputs[1].Anonymous.mi.dwFlags = MOUSEEVENTF_LEFTUP;
        let sent = unsafe {
            SetCursorPos(
                position.x + (120.0 * scale) as i32,
                position.y + (175.0 * scale) as i32,
            );
            SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32)
        };
        thread::sleep(Duration::from_millis(100));
        unsafe {
            SetCursorPos(original.x, original.y);
        }
        if sent != 2 {
            return Err("mouse SendInput failed".into());
        }
        Ok(())
    }
}
fn inspect(window: &tauri::WebviewWindow, enabled: bool) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as _;
    let mut alpha = 0;
    let mut key = 0;
    let mut flags = 0;
    unsafe {
        if (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TRANSPARENT as isize != 0) != enabled
            || GetLayeredWindowAttributes(hwnd, &mut key, &mut alpha, &mut flags) == 0
            || alpha != 204
        {
            return Err(format!(
                "native passthrough/opacity mismatch: enabled={enabled},alpha={alpha}"
            ));
        }
    }
    Ok(())
}
fn unown_key(app: &tauri::AppHandle) -> Result<RecoveryShortcut, String> {
    let target = app.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result = (|| {
            let state = target.state::<super::RuntimeState>();
            let mut hotkey = state
                .recovery_shortcut
                .lock()
                .map_err(|_| "key lock failed")?;
            let (id, key) = hotkey.active.take().ok_or("owned key missing")?;
            let hwnd = target
                .get_webview_window("main")
                .ok_or("main missing")?
                .hwnd()
                .map_err(|_| "main HWND missing")?
                .0 as _;
            if unsafe { UnregisterHotKey(hwnd, id) } == 0 {
                return Err("owned key release failed");
            }
            hotkey.status = ShortcutRegistration::Conflict;
            Ok(key)
        })();
        let _ = sender.send(result.map_err(str::to_owned));
    })
    .map_err(|e| e.to_string())?;
    receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|e| e.to_string())?
}
pub fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    let key = unown_key(app)?;
    let foreign = super::shortcuts_smoke::reserve(key.clone())?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const before=await invoke('get_mini_passthrough',{requestId:'native-pass-conflict-read'});
      let denied=false;try{await invoke('set_mini_passthrough',{requestId:'native-pass-conflict',request:{enabled:true,acknowledged_recovery:before.data.recovery_shortcut,expected_settings_revision:before.data.settings_revision}});}catch(e){denied=e.code==='SHORTCUT_CONFLICT';}
      const after=await invoke('get_mini_passthrough',{requestId:'native-pass-conflict-after'});
      if(!denied || after.data.enabled || after.data.persisted_enabled || after.data.recovery_registration!=='conflict')throw new Error('PASS_ENABLED_WITH_UNOWNED_KEY');
    "#,
    )?;
    inspect(&mini, false)?;
    drop(foreign);
    let target = app.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        super::shortcuts::initialize(&target);
        let _ = sender.send(());
    })
    .map_err(|e| e.to_string())?;
    receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|e| e.to_string())?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      await invoke('mini_window_action',{requestId:'native-pass-pin',request:{kind:'set_pinned',pinned:true}});
    "#,
    )?;
    let target = MouseTarget::create(&mini)?;
    unsafe {
        SetWindowPos(
            mini.hwnd().map_err(|e| e.to_string())?.0 as _,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
    target.click(&mini)?;
    if target.counter.load(Ordering::SeqCst) != 0 {
        return Err(
            "mouse target already covered the interactive mini before enabling passthrough".into(),
        );
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const region=document.querySelector('section[aria-label="鼠标穿透设置"]');
      [...region.querySelectorAll('button')].find(b=>b.textContent==='刷新穿透状态').click();
      await wait(()=>!region.querySelector('input[type=checkbox]').disabled);
      region.querySelector('input[type=checkbox]').click();
      [...region.querySelectorAll('button')].find(b=>b.textContent==='开启鼠标穿透').click();
      await wait(()=>region.querySelector('[aria-label="鼠标穿透状态"]')?.textContent==='穿透已开启');
    "#,
    )?;
    inspect(&mini, true)?;
    target.click(&mini)?;
    if target.counter.load(Ordering::SeqCst) != 1 {
        return Err("genuine mouse click did not pass through to independent native HWND".into());
    }
    drop(target);
    super::shortcuts_smoke::send_key(&key)?;
    for _ in 0..100 {
        if inspect(&mini, false).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    inspect(&mini, false)?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      await wait(()=>document.querySelector('[aria-label="鼠标穿透状态"]')?.textContent==='穿透已关闭');
      const value=await invoke('get_mini_passthrough',{requestId:'native-pass-key-recovery'});
      if(value.data.enabled || value.data.persisted_enabled)throw new Error('KEY_RECOVERY_NOT_PERSISTED');
    "#,
    )?;
    let state = app.state::<super::RuntimeState>();
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let fault =
        token_pulse_store::rusqlite::Connection::open(db.path()).map_err(|e| e.to_string())?;
    fault.execute_batch("CREATE TRIGGER reject_pass_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'native pass failure'); END;").map_err(|e|e.to_string())?;
    let result = super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const before=await invoke('get_mini_passthrough',{requestId:'native-pass-fault-read'});
      let failed=false;try{await invoke('set_mini_passthrough',{requestId:'native-pass-enable-fault',request:{enabled:true,acknowledged_recovery:before.data.recovery_shortcut,expected_settings_revision:before.data.settings_revision}});}catch(e){failed=e.code==='DB_WRITE_FAILED';}
      const after=await invoke('get_mini_passthrough',{requestId:'native-pass-fault-after'});
      if(!failed || after.data.enabled || after.data.persisted_enabled || after.data.settings_revision!==before.data.settings_revision)throw new Error('PASS_ENABLE_FAULT_NOT_ROLLED_BACK');
    "#,
    );
    fault
        .execute_batch("DROP TRIGGER reject_pass_revision;")
        .map_err(|e| e.to_string())?;
    result?;
    inspect(&mini, false)?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const before=await invoke('get_mini_passthrough',{requestId:'native-pass-enable-read'});
      await invoke('set_mini_passthrough',{requestId:'native-pass-enable',request:{enabled:true,acknowledged_recovery:before.data.recovery_shortcut,expected_settings_revision:before.data.settings_revision}});
    "#,
    )?;
    inspect(&mini, true)?;
    fault.execute_batch("CREATE TRIGGER reject_pass_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'native recovery failure'); END;").map_err(|e|e.to_string())?;
    super::shortcuts_smoke::send_key(&key)?;
    for _ in 0..100 {
        if inspect(&mini, false).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    inspect(&mini, false)?;
    let result = super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const after=await invoke('get_mini_passthrough',{requestId:'native-pass-recovery-fault-read'});
      if(after.data.enabled || !after.data.persisted_enabled)throw new Error('RECOVERY_FAILURE_REENABLED_MOUSE');
      await wait(()=>document.querySelector('[aria-label="鼠标穿透状态"]')?.textContent.includes('保存状态尚未同步'));
    "#,
    );
    fault
        .execute_batch("DROP TRIGGER reject_pass_revision;")
        .map_err(|e| e.to_string())?;
    result?;
    if !mini.is_visible().map_err(|e| e.to_string())? {
        return Err("failed recovery save prevented showing mini".into());
    }
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const region=document.querySelector('section[aria-label="鼠标穿透设置"]');[...region.querySelectorAll('button')].find(b=>b.textContent==='关闭鼠标穿透').click();
      await wait(()=>region.querySelector('[aria-label="鼠标穿透状态"]')?.textContent==='穿透已关闭');
    "#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      const value=await invoke('get_mini_passthrough',{requestId:'native-mini-pass-read'});
      let denied=false;try{await invoke('set_mini_passthrough',{requestId:'native-mini-pass-write',request:{enabled:false,acknowledged_recovery:null,expected_settings_revision:value.data.settings_revision}});}catch{denied=true;}
      if(!denied || value.data.enabled || value.data.persisted_enabled)throw new Error('MINI_PASS_PERMISSION');
    "#,
    )?;
    mini.hide().map_err(|e| e.to_string())?;
    println!(
        "NATIVE_MINI_PASSTHROUGH_OK: actual external hotkey conflict denies enable, explicit settings acknowledgement, genuine mouse SendInput reaches independent HWND, genuine recovery key clears style and preference, alpha retained, Writer enable rollback, recovery works with failed persistence and visible retry, mini read-only"
    );
    Ok(())
}
