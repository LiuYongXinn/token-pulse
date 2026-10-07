//! Debug-only real Windows floating-window acceptance in an isolated probe directory.
use std::{thread, time::Duration};
use tauri::{Listener, Manager, WebviewWindow};

pub(super) fn evaluate(
    app: &tauri::AppHandle,
    window: &WebviewWindow,
    script: &str,
) -> Result<(), String> {
    evaluate_with_timeout(app, window, script, Duration::from_secs(8))
}
pub(super) fn evaluate_with_timeout(
    app: &tauri::AppHandle,
    window: &WebviewWindow,
    script: &str,
    timeout: Duration,
) -> Result<(), String> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let event = format!("native-mini-probe-{}", uuid::Uuid::new_v4());
    let listener = app.listen(event.clone(), move |e| {
        let _ = sender.try_send(e.payload().to_owned());
    });
    let event_json = serde_json::to_string(&event).map_err(|e| e.to_string())?;
    let code = format!(
        r#"(async()=>{{
      const invoke=window.__TAURI_INTERNALS__.invoke;
      const wait=async (check,attempts=100)=>{{for(let i=0;i<attempts;i++){{if(check())return;await new Promise(r=>setTimeout(r,30));}}throw new Error('UI_WAIT_FAILED:'+check.toString());}};
      let result=true;try{{ {script} }}catch(error){{result={{error:error instanceof Error ? error.message : error.code ?? 'IPC_REJECTED'}};}}
      await invoke('plugin:event|emit',{{event:{event_json},payload:result}});
    }})();"#
    );
    window.eval(code).map_err(|e| e.to_string())?;
    let result = receiver.recv_timeout(timeout);
    app.unlisten(listener);
    if result.as_deref() == Ok("true") {
        Ok(())
    } else {
        Err(format!("native mini WebView check failed: {result:?}"))
    }
}
fn size(window: &WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    let logical = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);
    if (logical.width - width).abs() > 1.0 || (logical.height - height).abs() > 1.0 {
        return Err(format!("mini logical size mismatch: {logical:?}"));
    }
    #[cfg(windows)]
    verify_shape(window)?;
    Ok(())
}
#[cfg(windows)]
pub(super) fn verify_shape(window: &WebviewWindow) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{POINT, RECT},
        Graphics::Gdi::{
            ClientToScreen, CreateRectRgn, DeleteObject, GetRgnBox, GetWindowRgn, PtInRegion,
        },
        UI::WindowsAndMessaging::{GetClientRect, GetWindowRect},
    };
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0.cast();
    unsafe {
        let mut client = RECT::default();
        let mut outer = RECT::default();
        let mut origin = POINT::default();
        if GetClientRect(hwnd, &mut client) == 0
            || GetWindowRect(hwnd, &mut outer) == 0
            || ClientToScreen(hwnd, &mut origin) == 0
        {
            return Err("native mini client bounds unavailable".into());
        }
        let region = CreateRectRgn(0, 0, 0, 0);
        if region.is_null() {
            return Err("native region allocation failed".into());
        }
        let result = (|| {
            let mut bounds = RECT::default();
            if GetWindowRgn(hwnd, region) == 0 || GetRgnBox(region, &mut bounds) == 0 {
                return Err("mini is still a rectangular native window".into());
            }
            let left = origin.x - outer.left;
            let top = origin.y - outer.top;
            let right = left + client.right;
            let bottom = top + client.bottom;
            if (bounds.left, bounds.top, bounds.right, bounds.bottom) != (left, top, right, bottom)
            {
                return Err(format!(
                    "native mini region does not match client: {:?}, expected {:?}",
                    (bounds.left, bounds.top, bounds.right, bounds.bottom),
                    (left, top, right, bottom)
                ));
            }
            // Four clipped corners plus four retained edge midpoints; detects a stale
            // compact region after expansion and missing bottom/right border pixels.
            for (x, y) in [
                (left, top),
                (right - 1, top),
                (left, bottom - 1),
                (right - 1, bottom - 1),
            ] {
                if PtInRegion(region, x, y) != 0 {
                    return Err("native mini square corner remains".into());
                }
            }
            for (x, y) in [
                ((left + right) / 2, top),
                ((left + right) / 2, bottom - 1),
                (left, (top + bottom) / 2),
                (right - 1, (top + bottom) / 2),
            ] {
                if PtInRegion(region, x, y) == 0 {
                    return Err("native mini region clips a straight edge".into());
                }
            }
            Ok(())
        })();
        DeleteObject(region);
        result
    }
}
#[cfg(windows)]
pub(super) fn start_placement(app: tauri::AppHandle) {
    thread::spawn(move || {
        let result = (|| {
            super::mini_window::show(&app)?;
            let mini = app.get_webview_window("mini").ok_or("mini missing")?;
            evaluate(
                &app,
                &mini,
                "await wait(()=>document.querySelector('button[aria-label=\"展开小窗\"]')); if(getComputedStyle(document.querySelector('.mini-window')).borderTopLeftRadius!=='15px')throw new Error('MINI_RADIUS_MISMATCH');",
            )?;
            verify_shape_interactions(&mini)?;
            verify_placement_restore(&app, &mini)
        })();
        match result {
            Ok(()) => {
                println!(
                    "NATIVE_MINI_OUTER_BOUNDS_OK: actual Win32 outer rectangle, missing-monitor fallback and moved-event edge clamp; fixed client sizes retained; rounded native region clips all four corners in compact/expanded/recreated windows"
                );
                app.exit(0);
            }
            Err(error) => {
                eprintln!("NATIVE_MINI_OUTER_BOUNDS_FAILED: {error}");
                app.exit(1);
            }
        }
    });
}
#[cfg(windows)]
fn verify_shape_interactions(window: &WebviewWindow) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetLayeredWindowAttributes, LWA_ALPHA};
    super::mini_opacity::apply(window, 80).map_err(|e| e.to_string())?;
    for pinned in [false, true] {
        window
            .set_always_on_top(pinned)
            .map_err(|e| e.to_string())?;
        verify_shape(window)?;
    }
    window.hide().map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    verify_shape(window)?;
    let mut alpha = 0;
    let mut key = 0;
    let mut flags = 0;
    if unsafe {
        GetLayeredWindowAttributes(
            window.hwnd().map_err(|e| e.to_string())?.0.cast(),
            &mut key,
            &mut alpha,
            &mut flags,
        )
    } == 0
        || alpha != 204
        || flags != LWA_ALPHA
    {
        return Err("rounded native window lost whole-window opacity".into());
    }
    super::mini_opacity::apply(window, 100).map_err(|e| e.to_string())?;
    verify_shape(window)?;
    println!(
        "NATIVE_MINI_SHAPE_INTERACTIONS_OK: four clipped corners and intact edges survive alpha 100/80%, pin changes and hide/show"
    );
    Ok(())
}
#[cfg(windows)]
fn wait_for_outer_fit(window: &WebviewWindow) -> Result<(), String> {
    use windows_sys::Win32::{Foundation::RECT, UI::WindowsAndMessaging::GetWindowRect};
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    for _ in 0..200 {
        let monitor = window
            .current_monitor()
            .map_err(|e| e.to_string())?
            .ok_or("native monitor missing")?;
        let work = monitor.work_area();
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd.0 as _, &mut rect) } == 0 {
            return Err("independent native outer rectangle unavailable".into());
        }
        if rect.left >= work.position.x
            && rect.top >= work.position.y
            && i64::from(rect.right) <= i64::from(work.position.x) + i64::from(work.size.width)
            && i64::from(rect.bottom) <= i64::from(work.position.y) + i64::from(work.size.height)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(30));
    }
    Err("complete native mini frame did not fit actual work area".into())
}
pub fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("main missing")?;
    evaluate(
        app,
        &main,
        r#"
      const button=[...document.querySelectorAll('.sidebar-bottom button')].find(b=>b.textContent==='显示悬浮窗');
      if(!button || button.disabled)throw new Error('MINI_ENTRY_MISSING');button.click();
    "#,
    )?;
    let mut mini = None;
    for _ in 0..100 {
        mini = app.get_webview_window("mini");
        if mini
            .as_ref()
            .is_some_and(|w| w.is_visible().unwrap_or(false))
        {
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    let mini = mini.ok_or("mini window was not created")?;
    size(&mini, 280.0, 220.0)?;
    evaluate(
        app,
        &mini,
        r#"
      await wait(()=>document.querySelector('.mini-health')?.textContent==='尚无已确认用量');
      const quota=await invoke('get_account_quota',{requestId:'native-mini-quota'});
      if(quota.data.state!=='disconnected' || quota.data.windows.length!==0 || quota.data.fetched_at_ms!==null || !quota.display_policy)throw new Error('MINI_QUOTA_DEFAULT_INCORRECT');
      let disconnected=false;try{await invoke('refresh_account_quota',{requestId:'native-mini-quota-refresh'});}catch(error){disconnected=error.code==='QUOTA_DISCONNECTED';}
      if(!disconnected)throw new Error('MINI_QUOTA_REFRESH_FAKE');
      await wait(()=>document.querySelector('.mini-quota')?.textContent.includes('未连接'));
      if(document.querySelector('.mini-tokens')?.textContent!=='—' || document.querySelector('.mini-quota')?.textContent.includes('0%'))throw new Error('UNKNOWN_VALUES_REPLACED');
      if(document.querySelector('.mini-cost')?.textContent.includes('$0.00'))throw new Error('UNKNOWN_COST_ZERO');
      for(const command of ['get_sources','get_rebuild_status','get_price_rules','set_display_theme','perform_window_action']) {
        let denied=false;try{await invoke(command,{requestId:'mini-denied',action:'quit',revision:null,request:{theme:'light',expected_settings_revision:'13'}});}catch{denied=true;}
        if(!denied)throw new Error('MINI_PERMISSION_TOO_BROAD');
      }
      document.querySelector('button[aria-label="展开小窗"]').click();
      await wait(()=>document.querySelector('button[aria-label="收起小窗"]'));
      if(document.querySelector('button[aria-label="小窗置顶"]')?.getAttribute('aria-pressed')!=='true')throw new Error('DEFAULT_PIN_MISSING');
      document.querySelector('button[aria-label="小窗置顶"]').click();
      await wait(()=>document.querySelector('button[aria-label="小窗置顶"]')?.getAttribute('aria-pressed')==='false');
      document.querySelector('button[aria-label="小窗置顶"]').click();
      await wait(()=>document.querySelector('button[aria-label="小窗置顶"]')?.getAttribute('aria-pressed')==='true');
    "#,
    )?;
    size(&mini, 360.0, 380.0)?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_TOPMOST,
        };
        let style = unsafe {
            GetWindowLongPtrW(mini.hwnd().map_err(|e| e.to_string())?.0 as _, GWL_EXSTYLE)
        };
        if style & WS_EX_TOPMOST as isize == 0 {
            return Err("native mini topmost flag missing".into());
        }
    }
    evaluate(
        app,
        &main,
        r#"
      const s=await invoke('get_display_settings',{requestId:'mini-shared-theme-read'});
      await invoke('set_display_theme',{requestId:'mini-shared-theme',request:{theme:'light',expected_settings_revision:s.data.settings_revision}});
      await wait(()=>document.documentElement.dataset.theme==='light');
    "#,
    )?;
    evaluate(
        app,
        &mini,
        r#"
      await wait(()=>document.documentElement.dataset.theme==='light');
      document.querySelector('button[aria-label="小窗隐私模式"]').click();
      await wait(()=>document.querySelector('button[aria-label="小窗隐私模式"]')?.getAttribute('aria-pressed')==='true');
      await wait(()=>document.querySelector('.mini-cost')?.textContent.includes('已隐藏'));
      // Privacy hides optimistically; wait for the settings write before checking the persisted DTO.
      await wait(()=>!document.querySelector('button[aria-label="小窗隐私模式"]')?.disabled);
      const s=await invoke('get_display_settings',{requestId:'mini-private-read'});
      if(!s.data.preferences.privacy || !s.display_policy.privacy)throw new Error('SHARED_PRIVACY_NOT_COMMITTED');
    "#,
    )?;
    evaluate(
        app,
        &main,
        r#"
      const s=await invoke('get_display_settings',{requestId:'main-mini-policy-read'});
      const status=await invoke('get_app_status',{requestId:'main-mini-policy-status'});
      if(!s.data.preferences.privacy || !status.display_policy.privacy || status.data.data_directory.includes('native-probe-'))throw new Error('MAIN_NOT_PRIVATE');
      if(document.querySelector('.date-range-label')?.textContent!=='2024-02-28 — 2024-02-29' || document.querySelector('.price-instant-label')?.textContent!=='2024-02-29T00:00:00.123Z')throw new Error('MAIN_SCOPE_CHANGED');
    "#,
    )?;
    evaluate(
        app,
        &mini,
        r#"
      await wait(()=>!document.querySelector('button[aria-label="小窗隐私模式"]')?.disabled);
      document.querySelector('button[aria-label="小窗隐私模式"]').click();
      await wait(()=>document.querySelector('button[aria-label="小窗隐私模式"]')?.getAttribute('aria-pressed')==='false');
      await wait(()=>!document.querySelector('button[aria-label="隐藏小窗"]')?.disabled);
      document.querySelector('button[aria-label="隐藏小窗"]').click();
    "#,
    )?;
    thread::sleep(Duration::from_millis(100));
    if mini.is_visible().map_err(|e| e.to_string())? {
        return Err("mini hide did not hide".into());
    }
    // Exactly the same implementation is used by the tray recover-interaction entry.
    super::mini_window::show(app)?;
    if !mini.is_visible().map_err(|e| e.to_string())? {
        return Err("mini restore did not show existing window".into());
    }
    size(&mini, 360.0, 380.0)?;
    evaluate(
        app,
        &mini,
        r#"
      document.querySelector('button[aria-label="收起小窗"]').click();
      await wait(()=>document.querySelector('button[aria-label="展开小窗"]'));
      document.querySelector('button[aria-label="小窗置顶"]').click();
      await wait(()=>document.querySelector('button[aria-label="小窗置顶"]')?.getAttribute('aria-pressed')==='false');
    "#,
    )?;
    size(&mini, 280.0, 220.0)?;
    mini.close().map_err(|e| e.to_string())?;
    thread::sleep(Duration::from_millis(100));
    if mini.is_visible().map_err(|e| e.to_string())? || app.get_webview_window("mini").is_none() {
        return Err("mini close did not preserve hidden window".into());
    }
    evaluate(
        app,
        &main,
        r#"
      const s=await invoke('get_display_settings',{requestId:'mini-theme-restore-read'});
      await invoke('set_display_theme',{requestId:'mini-theme-restore',request:{theme:'dark',expected_settings_revision:s.data.settings_revision}});
    "#,
    )?;
    verify_scope_editor(app, &main, &mini)?;
    verify_stats_navigation(app, &main, &mini)?;
    verify_placement_restore(app, &mini)?;
    println!(
        "NATIVE_MINI_OK: two real WebViews, 280x220/360x380 DIP, native topmost, constrained IPC, shared theme/privacy, independent main filter, hide/restore/close"
    );
    Ok(())
}

fn verify_placement_restore(app: &tauri::AppHandle, mini: &WebviewWindow) -> Result<(), String> {
    use token_pulse_core::{
        numeric::EpochMs,
        placement::{MiniPreferenceChange, WindowPlacement},
    };
    super::mini_window::show(app)?;
    let monitor = mini
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("native monitor missing")?;
    let work = monitor.work_area();
    let x = work.position.x + (80.0 * monitor.scale_factor()).round() as i32;
    let y = work.position.y + (90.0 * monitor.scale_factor()).round() as i32;
    mini.set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    super::mini_window::save_current_placement(mini).map_err(|e| e.to_string())?;
    evaluate(
        app,
        mini,
        r#"
      await invoke('mini_window_action',{requestId:'mini-restore-expanded',request:{kind:'set_expanded',expanded:true}});
      await invoke('mini_window_action',{requestId:'mini-restore-pinned',request:{kind:'set_pinned',pinned:false}});
    "#,
    )?;
    let runtime = app.state::<super::RuntimeState>();
    let db = runtime.database.as_ref().map_err(|e| e.to_string())?;
    let saved = db.mini_window_preferences().map_err(|e| e.to_string())?;
    if !saved.interaction.expanded
        || saved.interaction.pinned
        || saved.placement.as_ref().is_none_or(|p| {
            (p.offset_x_dip - 80.0).abs() > 1.0 || (p.offset_y_dip - 90.0).abs() > 1.0
        })
    {
        return Err("native preferences not saved".into());
    }
    mini.destroy().map_err(|e| e.to_string())?;
    for _ in 0..100 {
        if app.get_webview_window("mini").is_none() {
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    *runtime
        .mini_window
        .lock()
        .map_err(|_| "probe state lock failed")? = Default::default();
    super::mini_window::show(app)?;
    let restored = app
        .get_webview_window("mini")
        .ok_or("restored mini missing")?;
    size(&restored, 360.0, 380.0)?;
    let position = restored.outer_position().map_err(|e| e.to_string())?;
    if (position.x - x).abs() > 1 || (position.y - y).abs() > 1 {
        return Err("native restored placement differs".into());
    }
    evaluate(
        app,
        &restored,
        r#"
      await wait(()=>document.querySelector('button[aria-label="收起小窗"]'));
      await wait(()=>document.querySelector('button[aria-label="小窗置顶"]')?.getAttribute('aria-pressed')==='false');
    "#,
    )?;
    // This debug-only probe owns an isolated UUID directory. Inject the fault through a
    // separate SQLite test connection, without exposing the private Writer to application code.
    let fault =
        token_pulse_store::rusqlite::Connection::open(db.path()).map_err(|e| e.to_string())?;
    fault
        .busy_timeout(Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    fault.execute_batch("CREATE TRIGGER reject_native_mini_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'native probe failure'); END;").map_err(|e|e.to_string())?;
    let check = evaluate(
        app,
        &restored,
        r#"
      let rejected=false;try{await invoke('mini_window_action',{requestId:'mini-failed-pin',request:{kind:'set_pinned',pinned:true}});}catch(error){rejected=error.code==='DB_WRITE_FAILED';}
      const state=await invoke('mini_window_action',{requestId:'mini-failed-pin-read',request:{kind:'read'}});
      if(!rejected || state.data.pinned)throw new Error('FAILED_NATIVE_PIN_NOT_ROLLED_BACK');
    "#,
    );
    fault
        .execute_batch("DROP TRIGGER reject_native_mini_revision;")
        .map_err(|e| e.to_string())?;
    check?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_TOPMOST,
        };
        let style = unsafe {
            GetWindowLongPtrW(
                restored.hwnd().map_err(|e| e.to_string())?.0 as _,
                GWL_EXSTYLE,
            )
        };
        if style & WS_EX_TOPMOST as isize != 0 {
            return Err("failed pin left native topmost enabled".into());
        }
    }
    db.update_mini_window_preferences(
        MiniPreferenceChange::Placement(WindowPlacement {
            monitor: Some("disconnected-probe-monitor".into()),
            offset_x_dip: 100_000.0,
            offset_y_dip: 100_000.0,
        }),
        EpochMs::new(token_pulse_collector::jobs::now_ms().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    restored.destroy().map_err(|e| e.to_string())?;
    for _ in 0..100 {
        if app.get_webview_window("mini").is_none() {
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    super::mini_window::show(app)?;
    let fallback = app
        .get_webview_window("mini")
        .ok_or("fallback mini missing")?;
    let monitor = fallback
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("fallback monitor missing")?;
    let work = monitor.work_area();
    #[cfg(windows)]
    wait_for_outer_fit(&fallback)?;
    let position = fallback.outer_position().map_err(|e| e.to_string())?;
    let actual = fallback.outer_size().map_err(|e| e.to_string())?;
    if position.x < work.position.x
        || position.y < work.position.y
        || i64::from(position.x) + i64::from(actual.width)
            > i64::from(work.position.x) + i64::from(work.size.width)
        || i64::from(position.y) + i64::from(actual.height)
            > i64::from(work.position.y) + i64::from(work.size.height)
    {
        return Err(format!(
            "fallback mini is outside actual working area: position={position:?} size={actual:?} work={work:?} scale={}",
            monitor.scale_factor()
        ));
    }
    #[cfg(windows)]
    {
        // Exercise the ordinary native Moved event, not a direct fit_current call.
        // A client-only clamp leaves the real Win32 frame beyond these right / bottom edges.
        for expanded in [false, true] {
            evaluate(
                app,
                &fallback,
                &format!(
                    "await invoke('mini_window_action',{{requestId:'mini-outer-size',request:{{kind:'set_expanded',expanded:{expanded}}}}});"
                ),
            )?;
            fallback
                .set_position(tauri::PhysicalPosition::new(
                    work.position.x + i32::try_from(work.size.width).map_err(|e| e.to_string())?
                        - 1,
                    work.position.y + i32::try_from(work.size.height).map_err(|e| e.to_string())?
                        - 1,
                ))
                .map_err(|e| e.to_string())?;
            wait_for_outer_fit(&fallback)?;
            let (width, height) = if expanded {
                (360.0, 380.0)
            } else {
                (280.0, 220.0)
            };
            size(&fallback, width, height)?;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos,
            };
            let outer = fallback.outer_size().map_err(|e| e.to_string())?;
            let scale = fallback.scale_factor().map_err(|e| e.to_string())?;
            let hwnd = fallback.hwnd().map_err(|e| e.to_string())?.0.cast();
            if unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    i32::try_from(outer.width).map_err(|e| e.to_string())?
                        - (14.0 * scale).round() as i32,
                    i32::try_from(outer.height).map_err(|e| e.to_string())?
                        - (7.0 * scale).round() as i32,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
                )
            } == 0
                || size(&fallback, width, height).is_ok()
            {
                return Err("authored native client resize was not observed".into());
            }
            let deadline = std::time::Instant::now() + Duration::from_secs(6);
            while size(&fallback, width, height).is_err() {
                if std::time::Instant::now() >= deadline {
                    return Err(
                        "mini fixed client did not recover after actual native resize".into(),
                    );
                }
                thread::sleep(Duration::from_millis(30));
            }
            wait_for_outer_fit(&fallback)?;
        }
        println!(
            "NATIVE_MINI_CLIENT_RESIZE_OK: fixed compact/expanded logical client restored by actual Resized event before fitting and saving outer bounds"
        );
    }
    fallback.hide().map_err(|e| e.to_string())?;
    println!(
        "NATIVE_MINI_PLACEMENT_OK: actual persisted position/expanded/pin, recreated WebView, missing-monitor work-area clamp, failed SQLite write rolls back Win32 topmost"
    );
    Ok(())
}

fn verify_scope_editor(
    app: &tauri::AppHandle,
    main: &WebviewWindow,
    mini: &WebviewWindow,
) -> Result<(), String> {
    super::mini_window::show(app)?;
    evaluate(
        app,
        mini,
        r#"
      document.querySelector('button[aria-label="展开小窗"]').click();
      await wait(()=>document.querySelector('button[aria-label="选择小窗会话与起点"]:not(:disabled)'));
      document.querySelector('button[aria-label="选择小窗会话与起点"]').click();
      await wait(()=>document.querySelector('.mini-options button'));
      const option=[...document.querySelectorAll('.mini-options button')].find(b=>b.textContent.includes('native-probe-context'));
      if(!option)throw new Error('REGISTERED_ZERO_USAGE_SESSION_MISSING');option.click();
      await wait(()=>document.querySelector('select[aria-label="小窗消耗起点"]'));
      const change=(element,value)=>{Object.getOwnPropertyDescriptor(element instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype,'value').set.call(element,value);element.dispatchEvent(new Event('input',{bubbles:true}));element.dispatchEvent(new Event('change',{bubbles:true}));};
      change(document.querySelector('select[aria-label="小窗消耗起点"]'),'fixed');
      await wait(()=>document.querySelector('input[aria-label="小窗固定起点（UTC）"]'));
      change(document.querySelector('input[aria-label="小窗固定起点（UTC）"]'),'2024-02-29T01:02:03.123');
      [...document.querySelectorAll('.mini-editor-footer button')].find(b=>b.textContent==='应用小窗范围').click();
      await wait(()=>!document.querySelector('.mini-scope-editor'));
      const scope=await invoke('get_mini_scope',{requestId:'mini-editor-fixed-read'});
      if(scope.data.mini_scope.session_key!=='native-probe-context' || scope.data.mini_scope.start.start_ms!==Date.parse('2024-02-29T01:02:03.123Z'))throw new Error('EDITOR_FIXED_START_LOST');
      const page=await invoke('query_mini_sessions',{requestId:'mini-editor-list-read',request:{query:{search:'native-probe-context',page_size:1},cursor:null}});
      if(page.data.options.length!==1 || page.data.options[0].session_key!=='native-probe-context' || page.data.next_cursor!==null)throw new Error('EDITOR_CANDIDATE_DTO_INVALID');
      const filter={range:{start_ms:0,end_ms:1000,timezone:'Asia/Tokyo'},sources:{kind:'all'},models:{kind:'all'},projects:{kind:'all'},sessions:{kind:'all'}};
      let denied=false;try{await invoke('close_query_snapshot',{requestId:'mini-close-main-denied',request:{kind:'sessions',request:{query:{filter,price_basis:{mode:'event_time'},sort:'latest_desc',page_size:1},cursor:'a'.repeat(151)}}});}catch(error){if(error.code!=='PERMISSION_DENIED')throw new Error('UNEXPECTED_CLOSE_REJECTION:'+JSON.stringify(error));denied=true;}
      if(!denied)throw new Error('MINI_CAN_CLOSE_MAIN_QUERY');
      await wait(()=>!document.querySelector('button[aria-label="选择小窗会话与起点"]')?.disabled);
      document.querySelector('button[aria-label="选择小窗会话与起点"]').click();
      await wait(()=>document.querySelector('select[aria-label="小窗消耗起点"]'));
      change(document.querySelector('select[aria-label="小窗消耗起点"]'),'today');
      [...document.querySelectorAll('.mini-editor-footer button')].find(b=>b.textContent==='应用小窗范围').click();
      await wait(()=>!document.querySelector('.mini-scope-editor'));
      const today=await invoke('get_mini_scope',{requestId:'mini-editor-today-read'});
      if(today.data.mini_scope.session_key!=='native-probe-context' || today.data.mini_scope.start.kind!=='today')throw new Error('EDITOR_TODAY_NOT_SAVED');
    "#,
    )?;
    evaluate(
        app,
        main,
        r#"
      if(document.querySelector('.date-range-label')?.textContent!=='2024-02-28 — 2024-02-29' || document.querySelector('.price-instant-label')?.textContent!=='2024-02-29T00:00:00.123Z')throw new Error('EDITOR_CHANGED_MAIN_FILTER');
    "#,
    )?;
    println!(
        "NATIVE_MINI_SCOPE_OK: registered zero-usage session picker, exact UTC millisecond input, daily start, main filter unchanged, mini cannot close main query leases"
    );
    Ok(())
}

fn verify_stats_navigation(
    app: &tauri::AppHandle,
    main: &WebviewWindow,
    mini: &WebviewWindow,
) -> Result<(), String> {
    super::mini_window::show(app)?;
    evaluate(
        app,
        mini,
        r#"
      const s=await invoke('get_mini_scope',{requestId:'mini-navigation-scope-read'});
      await invoke('set_mini_scope',{requestId:'mini-navigation-scope',request:{mini_scope:{kind:'session',session_key:'native-probe-context',start:{kind:'fixed',start_ms:1709179200123}},expected_settings_revision:s.data.settings_revision}});
      await wait(()=>document.querySelector('.mini-scope')?.textContent.includes('native-probe-context'));
      await wait(()=>document.querySelector('.mini-range span')?.textContent.includes('.123'));
      await wait(()=>!document.querySelector('button[aria-label="打开小窗范围统计"]')?.disabled);
      let rejected=false;try { await invoke('open_mini_stats',{requestId:'mini-stale-navigation',request:{expected_settings_revision:s.data.settings_revision}}); } catch(error) { rejected=error.code==='REVISION_CONFLICT'; }
      if(!rejected)throw new Error('STALE_SCOPE_NAVIGATION_ACCEPTED');
      document.querySelector('button[aria-label="打开小窗范围统计"]').click();
    "#,
    )?;
    evaluate(
        app,
        main,
        r#"
      let intent;
      for(let i=0;i<100;i++) { intent=await invoke('get_mini_stats_request',{requestId:'main-mini-navigation-read'}); if(intent.data)break; await new Promise(r=>setTimeout(r,30)); }
      if(intent.data?.mini_scope.session_key!=='native-probe-context' || intent.data.calendar.range.start_ms!==1709179200123)throw new Error('EXACT_INTENT_LOST');
      await wait(()=>document.querySelector('.mini-stat-scope')?.textContent.includes('.123'));
      if(document.querySelector('select[aria-label="日期范围"]'))throw new Error('ROUNDED_DAY_PRESENTATION');
      if(document.querySelector('select[aria-label="计价依据"]')?.value!=='event_time' || document.querySelector('select[aria-label="来源"]')?.value!=='')throw new Error('MAIN_FILTERS_NOT_RESET');
    "#,
    )?;
    let mut observed = false;
    for _ in 0..100 {
        let runtime = app.state::<super::RuntimeState>();
        let intent = runtime
            .main_navigation
            .lock()
            .map_err(|_| "probe request lock failed")?
            .mini_stats()
            .ok_or("navigation intent missing")?;
        let query = runtime
            .native_dashboard_request
            .lock()
            .map_err(|_| "probe query lock failed")?
            .clone();
        if let Some(query) = query {
            use token_pulse_core::protocol::{DimensionSelection, PriceBasis};
            if matches!(&query.filter.sessions, DimensionSelection::Ids { ids, include_unknown: false } if ids == &["native-probe-context"])
                && matches!(query.filter.sources, DimensionSelection::All {})
                && matches!(query.price_basis, PriceBasis::EventTime {})
                && query.filter.range.start_ms == intent.calendar.range.start_ms
                && query.filter.range.end_ms == intent.calendar.range.end_ms
                && query.filter.range.timezone == intent.calendar.range.timezone
            {
                observed = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(30));
    }
    if !observed {
        return Err("real main dashboard request did not preserve exact mini scope".into());
    }
    evaluate(
        app,
        main,
        r#"
      [...document.querySelectorAll('button')].find(b=>b.textContent==='改用主窗口日期').click();
      await wait(()=>document.querySelector('select[aria-label="日期范围"]'));
      const miniScope=await invoke('get_mini_scope',{requestId:'main-independent-mini-check'});
      if(miniScope.data.mini_scope.start.start_ms!==1709179200123)throw new Error('MAIN_CHANGED_MINI_SCOPE');
    "#,
    )?;
    mini.hide().map_err(|e| e.to_string())?;
    println!(
        "NATIVE_MINI_NAVIGATION_OK: explicit fixed-session intent, exact millisecond range, real main dashboard query, main calendar recovery without changing mini scope"
    );
    Ok(())
}
