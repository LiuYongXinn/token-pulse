//! Debug-only native hotkey acceptance; all writes target the current isolated probe database.
use std::{sync::mpsc, thread, time::Duration};
use tauri::Manager;
use token_pulse_core::shortcuts::RecoveryShortcut;
use windows_sys::Win32::{
    Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, GetLastError},
    UI::Input::KeyboardAndMouse::*,
};
pub(super) struct ReservedKey {
    stop: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for ReservedKey {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub(super) fn reserve(key: RecoveryShortcut) -> Result<ReservedKey, String> {
    let (ready, read) = mpsc::sync_channel(1);
    let (stop, wait) = mpsc::channel();
    let worker = thread::spawn(move || {
        let registered = unsafe {
            RegisterHotKey(
                std::ptr::null_mut(),
                0x5410,
                key.modifiers() | MOD_NOREPEAT,
                key.virtual_key().expect("validated probe key"),
            )
        } != 0;
        let code = if registered {
            0
        } else {
            unsafe { GetLastError() }
        };
        let _ = ready.send(code);
        if registered {
            let _ = wait.recv();
            unsafe {
                UnregisterHotKey(std::ptr::null_mut(), 0x5410);
            }
        }
    });
    let code = read
        .recv_timeout(Duration::from_secs(3))
        .map_err(|e| e.to_string())?;
    if code != 0 {
        let _ = worker.join();
        return Err(format!("REGISTER_PROBE_CODE:{code}"));
    }
    Ok(ReservedKey {
        stop: Some(stop),
        worker: Some(worker),
    })
}
pub(super) fn send_key(key: &RecoveryShortcut) -> Result<(), String> {
    let mut codes = Vec::new();
    if key.control {
        codes.push(VK_CONTROL);
    }
    if key.alt {
        codes.push(VK_MENU);
    }
    if key.shift {
        codes.push(VK_SHIFT);
    }
    codes.push(key.virtual_key().map_err(|e| e.to_string())? as u16);
    let inputs: Vec<INPUT> = codes
        .iter()
        .map(|code| (*code, 0))
        .chain(codes.iter().rev().map(|code| (*code, KEYEVENTF_KEYUP)))
        .map(|(code, flags)| {
            // INPUT is a Win32 POD union; every active KEYBDINPUT field is initialized.
            let mut input: INPUT = unsafe { std::mem::zeroed() };
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki.wVk = code;
            input.Anonymous.ki.dwFlags = flags;
            input
        })
        .collect();
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent != inputs.len() as u32 {
        return Err("native SendInput did not deliver all key transitions".into());
    }
    Ok(())
}
pub(super) fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("main missing")?;
    let key = RecoveryShortcut {
        key: "U".into(),
        ..Default::default()
    };
    let previously_owned = app
        .state::<super::RuntimeState>()
        .recovery_shortcut
        .lock()
        .map_err(|_| "probe recovery lock failed")?
        .active
        .as_ref()
        .map(|(_, key)| key.clone());
    let available = reserve(key.clone())?;
    drop(available);
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const settings=[...document.querySelectorAll('nav button')].find(b=>b.textContent==='设置');settings.click();
      await wait(()=>document.querySelector('[role="tablist"]'));
      [...document.querySelectorAll('[role="tab"]')].find(b=>b.textContent==='显示与窗口').click();
      await wait(()=>document.querySelector('select[aria-label="恢复快捷键按键"]') && !document.querySelector('select[aria-label="恢复快捷键按键"]').disabled);
      const select=document.querySelector('select[aria-label="恢复快捷键按键"]');Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(select,'U');select.dispatchEvent(new Event('change',{bubbles:true}));
      await wait(()=>[...document.querySelectorAll('.recovery-shortcut button')].some(b=>b.textContent==='保存恢复快捷键'));
      [...document.querySelectorAll('.recovery-shortcut button')].find(b=>b.textContent==='保存恢复快捷键').click();
      await wait(()=>document.querySelector('.recovery-shortcut [role="status"]')?.textContent.includes('已注册') && ![...document.querySelectorAll('.recovery-shortcut button')].some(b=>b.textContent==='保存恢复快捷键'));
      const saved=await invoke('get_recovery_shortcut',{requestId:'native-recovery-read'});
      if(saved.data.shortcut.key!=='U' || saved.data.registration!=='ready')throw new Error('HOTKEY_UI_NOT_COMMITTED');
    "#,
    )?;
    if reserve(key.clone()).err()
        != Some(format!(
            "REGISTER_PROBE_CODE:{ERROR_HOTKEY_ALREADY_REGISTERED}"
        ))
    {
        return Err("actual configured key was not globally reserved".into());
    }
    if let Some(old) = previously_owned {
        let released = reserve(old)?;
        drop(released);
    }
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    mini.set_ignore_cursor_events(true)
        .map_err(|e| e.to_string())?;
    mini.hide().map_err(|e| e.to_string())?;
    send_key(&key)?;
    let mut visible = false;
    for _ in 0..100 {
        if mini.is_visible().unwrap_or(false) {
            visible = true;
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    if !visible {
        return Err("real keyboard input did not recover the mini window".into());
    }
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      const current=await invoke('get_recovery_shortcut',{requestId:'mini-recovery-status'});
      if(current.data.registration!=='ready' || current.data.shortcut.key!=='U')throw new Error('MINI_RECOVERY_STATUS_INVALID');
      let denied=false;try{await invoke('set_recovery_shortcut',{requestId:'mini-recovery-write-denied',request:{shortcut:{control:true,alt:true,shift:true,key:'Z'},expected_settings_revision:current.data.settings_revision}});}catch{denied=true;}
      if(!denied)throw new Error('MINI_CAN_REPLACE_RECOVERY_KEY');
    "#,
    )?;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_TRANSPARENT,
    };
    if unsafe { GetWindowLongPtrW(mini.hwnd().map_err(|e| e.to_string())?.0 as _, GWL_EXSTYLE) }
        & WS_EX_TRANSPARENT as isize
        != 0
    {
        return Err("hotkey recovery left mouse ignore enabled".into());
    }
    let blocked = RecoveryShortcut {
        key: "V".into(),
        ..Default::default()
    };
    let reserved = reserve(blocked.clone())?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const before=await invoke('get_recovery_shortcut',{requestId:'native-hotkey-conflict-before'});
      const select=document.querySelector('select[aria-label="恢复快捷键按键"]');Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(select,'V');select.dispatchEvent(new Event('change',{bubbles:true}));
      [...document.querySelectorAll('.recovery-shortcut button')].find(b=>b.textContent==='保存恢复快捷键').click();
      await wait(()=>document.querySelector('.recovery-shortcut [role="alert"]')?.textContent.includes('其他应用占用'));
      const current=await invoke('get_recovery_shortcut',{requestId:'native-hotkey-conflict-after'});
      if(current.data.shortcut.key!=='U' || current.data.registration!=='ready' || current.data.settings_revision!==before.data.settings_revision || select.value!=='V')throw new Error('NATIVE_CONFLICT_REPLACED_OR_LOST_DRAFT');
    "#,
    )?;
    drop(reserved);
    let runtime = app.state::<super::RuntimeState>();
    let db = runtime.database.as_ref().map_err(|e| e.to_string())?;
    let fault =
        token_pulse_store::rusqlite::Connection::open(db.path()).map_err(|e| e.to_string())?;
    fault.execute_batch("CREATE TRIGGER reject_hotkey_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'native hotkey failure'); END;").map_err(|e|e.to_string())?;
    let check = super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const before=await invoke('get_recovery_shortcut',{requestId:'native-hotkey-fault-before'});
      let rejected=false;try{await invoke('set_recovery_shortcut',{requestId:'native-hotkey-fault',request:{shortcut:{control:true,alt:true,shift:true,key:'V'},expected_settings_revision:before.data.settings_revision}});}catch(error){rejected=error.code==='DB_WRITE_FAILED';}
      const current=await invoke('get_recovery_shortcut',{requestId:'native-hotkey-fault-after'});
      if(!rejected || current.data.shortcut.key!=='U' || current.data.registration!=='ready' || current.data.settings_revision!==before.data.settings_revision)throw new Error('NATIVE_WRITER_FAULT_LOST_RECOVERY');
    "#,
    );
    fault
        .execute_batch("DROP TRIGGER reject_hotkey_revision;")
        .map_err(|e| e.to_string())?;
    check?;
    let released = reserve(blocked)?;
    drop(released);
    if reserve(key).err()
        != Some(format!(
            "REGISTER_PROBE_CODE:{ERROR_HOTKEY_ALREADY_REGISTERED}"
        ))
    {
        return Err("writer rollback released the old recovery key".into());
    }
    mini.hide().map_err(|e| e.to_string())?;
    println!(
        "NATIVE_RECOVERY_SHORTCUT_OK: actual settings UI registration, global ownership, SendInput to WM_HOTKEY recovery, cursor ignore cleared, external conflict keeps key/draft, Writer failure releases candidate and retains old registration"
    );
    Ok(())
}
