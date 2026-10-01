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
    let (price_sender, price_receiver) = std::sync::mpsc::sync_channel(4);
    let price_listener = app.listen("price_rules_changed", move |event| {
        let _ = price_sender.try_send(event.payload().to_owned());
    });
    let (settings_sender, settings_receiver) = std::sync::mpsc::sync_channel(4);
    let settings_listener = app.listen("settings_changed", move |event| {
        let _ = settings_sender.try_send(event.payload().to_owned());
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
                const range={start_ms:0,end_ms:86400000,timezone:'UTC'};
                const request={filter:{range,sources:{kind:'all'},models:{kind:'all'},projects:{kind:'all'},sessions:{kind:'all'}},price_basis:{mode:'event_time'},grain:'hour',heatmap_range:{...range,end_ms:172800000}};
                const dashboard=await invoke('get_dashboard_bundle',{requestId:'native-smoke-dashboard',request});
                ok=ok && dashboard.api_version===1 && dashboard.request_id==='native-smoke-dashboard'
                    && dashboard.data.meta.snapshot_id==='native-smoke-dashboard' && dashboard.data.meta.price_revision==='3'
                    && dashboard.data.summary.total_tokens==='0' && dashboard.data.summary.input_total.value===null
                    && dashboard.data.pricing.currencies.length===0 && dashboard.data.recent_sessions.length===0
                    && dashboard.data.series.length===24 && dashboard.data.heatmap.length===2
                    && dashboard.data.series.every(bucket=>bucket.totals.total_tokens==='0' && bucket.coverage.state==='unknown')
                    && dashboard.data.meta.parser_versions.length===0;
                let invalidRange=false;
                try {await invoke('get_dashboard_bundle',{requestId:'native-smoke-dashboard-invalid',request:{...request,heatmap_range:{...request.heatmap_range,timezone:'Asia/Shanghai'}}});} catch(error) {invalidRange=error.code==='INVALID_QUERY';}
                ok=ok && invalidRange;
                const sessionsRequest={query:{filter:request.filter,price_basis:request.price_basis,sort:'latest_desc',page_size:200},cursor:null};
                const detail=await invoke('get_session_bundle',{requestId:'native-smoke-detail',request:{session_key:'native-probe-context',filter:request.filter,price_basis:request.price_basis}});
                ok=ok && detail.api_version===1 && detail.request_id==='native-smoke-detail'
                    && detail.data.identity.session_key==='native-probe-context' && detail.data.meta.price_revision==='3'
                    && detail.data.summary.total_tokens==='0' && detail.data.latest_selected_activity===null
                    && detail.data.latest_context.context_tokens===null && detail.data.children.length===0
                    && detail.data.child_count==='0' && !detail.data.children_truncated && detail.data.classifications.length===0;
                let missingDetail=false;
                try {await invoke('get_session_bundle',{requestId:'native-smoke-detail-missing',request:{session_key:'missing-detail',filter:request.filter,price_basis:request.price_basis}});} catch(error) {missingDetail=error.code==='INVALID_QUERY';}
                ok=ok && missingDetail;
                const turnsRequest={query:{session_key:'native-probe-context',filter:request.filter,price_basis:request.price_basis,page_size:50},cursor:null};
                const turns=await invoke('query_turns',{requestId:'native-smoke-turns',request:turnsRequest});
                ok=ok && turns.api_version===1 && turns.request_id==='native-smoke-turns'
                    && turns.data.session_key==='native-probe-context' && turns.data.meta.price_revision==='3'
                    && turns.data.turns.length===0 && turns.data.next_cursor===null
                    && turns.data.summary.reliable_turn_count===null && !turns.data.summary.reliable_turns_complete
                    && turns.data.unidentified_usage_event_count==='0';
                let badTurnCursor=false;
                try {await invoke('query_turns',{requestId:'native-smoke-turn-cursor',request:{...turnsRequest,cursor:'a'.repeat(151)}});} catch(error) {badTurnCursor=error.code==='CURSOR_INVALID';}
                ok=ok && badTurnCursor;
                const calendar=await invoke('resolve_calendar_selection',{requestId:'native-smoke-calendar',request:{timezone:'America/New_York',selection:{kind:'custom',start_date:'2026-11-01',end_date_inclusive:'2026-11-01'}}});
                ok=ok && calendar.api_version===1 && calendar.request_id==='native-smoke-calendar'
                    && calendar.data.range.start_ms===Date.parse('2026-11-01T04:00:00Z')
                    && calendar.data.range.end_ms===Date.parse('2026-11-02T05:00:00Z')
                    && calendar.data.range.timezone==='America/New_York' && typeof calendar.data.local_today==='string'
                    && calendar.data.heatmap_range.start_ms<calendar.data.heatmap_range.end_ms;
                let invalidDate=false;
                try {await invoke('resolve_calendar_selection',{requestId:'native-smoke-calendar-invalid',request:{timezone:'UTC',selection:{kind:'custom',start_date:'2026-02-29',end_date_inclusive:'2026-03-01'}}});} catch(error) {invalidDate=error.code==='INVALID_QUERY';}
                ok=ok && invalidDate;
                const initialDisplay=await invoke('get_display_settings',{requestId:'native-smoke-display'});
                ok=ok && initialDisplay.api_version===1 && initialDisplay.request_id==='native-smoke-display'
                    && initialDisplay.data.preferences.display_timezone===Intl.DateTimeFormat().resolvedOptions().timeZone && initialDisplay.data.settings_revision==='1';
                const initializedDisplay=await invoke('set_display_timezone',{requestId:'native-smoke-timezone-init',request:{kind:'initialize',system_timezone:'America/New_York'}});
                ok=ok && initializedDisplay.data.preferences.display_timezone===initialDisplay.data.preferences.display_timezone && initializedDisplay.data.settings_revision==='1';
                const firstZone=initialDisplay.data.preferences.display_timezone==='America/New_York'?'Asia/Shanghai':'America/New_York';
                const firstChange=await invoke('set_display_timezone',{requestId:'native-smoke-timezone-first-change',request:{kind:'set',display_timezone:firstZone,expected_settings_revision:'1'}});
                ok=ok && firstChange.data.preferences.display_timezone===firstZone && firstChange.data.settings_revision==='2';
                const changedDisplay=await invoke('set_display_timezone',{requestId:'native-smoke-timezone-set',request:{kind:'set',display_timezone:'UTC',expected_settings_revision:'2'}});
                ok=ok && changedDisplay.data.preferences.display_timezone==='UTC' && changedDisplay.data.settings_revision==='3';
                for (const [suffix,change] of [['init',{kind:'initialize',system_timezone:'Asia/Shanghai'}],['noop',{kind:'set',display_timezone:'UTC',expected_settings_revision:'3'}]]) {
                    const kept=await invoke('set_display_timezone',{requestId:`native-smoke-timezone-${suffix}`,request:change});
                    ok=ok && kept.data.preferences.display_timezone==='UTC' && kept.data.settings_revision==='3';
                }
                let timezoneConflict=false;
                try {await invoke('set_display_timezone',{requestId:'native-smoke-timezone-stale',request:{kind:'set',display_timezone:'Asia/Shanghai',expected_settings_revision:'1'}});} catch(error) {timezoneConflict=error.code==='REVISION_CONFLICT';}
                ok=ok && timezoneConflict;
                const sessions=await invoke('query_sessions',{requestId:'native-smoke-sessions',request:sessionsRequest});
                ok=ok && sessions.api_version===1 && sessions.request_id==='native-smoke-sessions'
                    && sessions.data.meta.snapshot_id.startsWith('query-') && sessions.data.meta.price_revision==='3'
                    && sessions.data.summary.total_tokens==='0' && sessions.data.summary.input_total.value===null
                    && sessions.data.sessions.length===0 && sessions.data.next_cursor===null;
                for(const [suffix,bad,code] of [['size',{...sessionsRequest,query:{...sessionsRequest.query,page_size:201}},'INVALID_QUERY'],['cursor',{...sessionsRequest,cursor:'a'.repeat(151)},'CURSOR_INVALID']]) {
                    let rejected=false;
                    try {await invoke('query_sessions',{requestId:`native-smoke-sessions-${suffix}`,request:bad});} catch(error) {rejected=error.code===code;}
                    ok=ok && rejected;
                }
                for(const sort of ['time_desc','total_desc']) {
                    const eventsRequest={query:{...sessionsRequest.query,sort},cursor:null};
                    const events=await invoke('query_usage_events',{requestId:`native-smoke-events-${sort}`,request:eventsRequest});
                    ok=ok && events.api_version===1 && events.request_id===`native-smoke-events-${sort}`
                        && events.data.meta.snapshot_id.startsWith('query-') && events.data.meta.price_revision==='3'
                        && events.data.events.length===0 && events.data.next_cursor===null
                        && events.data.summary.total_tokens==='0' && events.data.summary.input_total.value===null;
                    let invalid=false;
                    try { await invoke('query_usage_events',{requestId:'native-smoke-events-cursor',request:{...eventsRequest,cursor:'a'.repeat(151)}}); } catch(error) { invalid=error.code==='CURSOR_INVALID'; }
                    ok=ok && invalid;
                }
                for(const dimension of ['models','projects']) {
                    const groupedRequest={filter:request.filter,price_basis:request.price_basis,dimension,sort:'total_desc',limit:200};
                    const grouped=await invoke('get_grouped_usage',{requestId:`native-smoke-groups-${dimension}`,request:groupedRequest});
                    ok=ok && grouped.data.meta.snapshot_id===`native-smoke-groups-${dimension}`
                        && grouped.data.meta.price_revision==='3' && grouped.data.groups.length===0
                        && grouped.data.total_group_count==='0' && !grouped.data.truncated
                        && grouped.data.summary.total_tokens==='0' && grouped.data.summary.input_total.value===null
                        && grouped.data.pricing.currencies.length===0 && grouped.data.coverage.state==='unknown';
                    let invalidLimit=false;
                    try { await invoke('get_grouped_usage',{requestId:'native-smoke-groups-invalid',request:{...groupedRequest,limit:201}}); } catch(error) { invalidLimit=error.code==='INVALID_QUERY'; }
                    ok=ok && invalidLimit;
                }
                const guide=document.querySelector('main .empty h2');
                for(const dimension of ['sources','models','projects','sessions']) {
                    const facetRequest={query:{filter:request.filter,dimension,search:'中文%_',page_size:200},cursor:null};
                    const facet=await invoke('get_filter_options',{requestId:`native-smoke-facet-${dimension}`,request:facetRequest});
                    ok=ok && facet.api_version===1 && facet.request_id===`native-smoke-facet-${dimension}`
                        && facet.data.dimension===dimension && facet.data.meta.snapshot_id.startsWith('query-')
                        && facet.data.meta.price_revision==='3' && facet.data.options.length===0 && facet.data.next_cursor===null;
                    let invalidSize=false;
                    try {await invoke('get_filter_options',{requestId:'native-smoke-facet-invalid',request:{...facetRequest,query:{...facetRequest.query,page_size:201}}});} catch(error) {invalidSize=error.code==='INVALID_QUERY';}
                    let invalidCursor=false;
                    try {await invoke('get_filter_options',{requestId:'native-smoke-facet-cursor',request:{...facetRequest,cursor:'a'.repeat(151)}});} catch(error) {invalidCursor=error.code==='CURSOR_INVALID';}
                    let invalidClose=false;
                    try {await invoke('close_query_snapshot',{requestId:'native-smoke-facet-close',request:{kind:'filter_options',request:{...facetRequest,cursor:'a'.repeat(151)}}});} catch(error) {invalidClose=error.code==='CURSOR_INVALID';}
                    ok=ok && invalidSize && invalidCursor && invalidClose;
                }
                ok=ok && guide?.textContent==='添加 Codex 数据来源'
                    && document.querySelectorAll('nav[aria-label="主导航"] button').length===7
                    && document.querySelector('select[aria-label="日期范围"]')?.value==='today'
                    && !document.querySelector('.total-number');
                const waitFor=async predicate=> {
                    const deadline=Date.now()+2500;
                    while(!predicate()) {
                        if(Date.now()>=deadline) throw new Error('UI did not render');
                        await new Promise(resolve=>setTimeout(resolve,25));
                    }
                };
                for(const name of ['模型','项目','会话']) {
                    const trigger=document.querySelector(`button[role="combobox"][aria-label="${name}"]`);
                    if(!trigger || trigger.disabled) throw new Error('Advanced filter missing');
                    trigger.click();
                    await waitFor(()=>document.querySelector('.facet-popup')?.textContent.includes('当前范围没有匹配候选。'));
                    ok=ok && document.querySelector(`input[aria-label="搜索${name}"]`)===document.activeElement
                        && document.querySelectorAll('.facet-options [role="option"]').length===0;
                    document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
                    await waitFor(()=>!document.querySelector('.facet-popup'));
                    ok=ok && document.activeElement===trigger;
                }
                for(const [name,label,emptyHeading] of [['模型','模型统计汇总','当前筛选暂无可信消费'],['项目','项目统计汇总','当前筛选暂无可信消费'],['会话','会话统计汇总','当前筛选暂无消费会话'],['明细','明细统计汇总','当前筛选暂无可信事件']]) {
                    [...document.querySelectorAll('nav button')].find(button=>button.textContent===name)?.click();
                    await waitFor(()=>document.querySelector(`section[aria-label="${label}"]`) && document.querySelector('.group-empty h2')?.textContent===emptyHeading);
                    ok=ok && document.querySelector('h1')?.textContent===name
                        && document.querySelector('.group-total[aria-label="0 Token"]')!==null
                        && document.querySelector('.group-stat-strip .cost-number')?.textContent==='未计价';
                    if(name==='会话') {
                        const sort=document.querySelector('select[aria-label="会话排序"]');
                        const size=document.querySelector('select[aria-label="会话每页数量"]');
                        ok=ok && sort?.value==='latest_desc' && size?.value==='50'
                            && [...document.querySelectorAll('.session-pagination button')].every(button=>button.disabled);
                        sort.value='total_desc'; sort.dispatchEvent(new Event('change',{bubbles:true}));
                        size.value='100'; size.dispatchEvent(new Event('change',{bubbles:true}));
                        await waitFor(()=>document.querySelector('.group-empty h2')?.textContent===emptyHeading);
                        ok=ok && sort.value==='total_desc' && size.value==='100';
                    }
                    if(name==='明细') {
                        const sort=document.querySelector('select[aria-label="明细排序"]');
                        const size=document.querySelector('select[aria-label="明细每页数量"]');
                        ok=ok && sort?.value==='time_desc' && size?.value==='50'
                            && [...document.querySelectorAll('.session-pagination button')].every(button=>button.disabled);
                        sort.value='total_desc'; sort.dispatchEvent(new Event('change',{bubbles:true}));
                        size.value='100'; size.dispatchEvent(new Event('change',{bubbles:true}));
                        await waitFor(()=>document.querySelector('.group-empty h2')?.textContent===emptyHeading);
                        ok=ok && sort.value==='total_desc' && size.value==='100';
                    }
                }
                [...document.querySelectorAll('nav button')].find(button=>button.textContent==='设置')?.click();
                await waitFor(()=>document.querySelectorAll('[role="tab"]').length===5);
                [...document.querySelectorAll('[role="tab"]')].find(button=>button.textContent==='显示与窗口')?.click();
                await waitFor(()=>document.querySelector('input[aria-label="统计时区"]')?.value==='UTC');
                const timezoneInput=document.querySelector('input[aria-label="统计时区"]');
                Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(timezoneInput,'Asia/Tokyo');
                timezoneInput.dispatchEvent(new Event('input',{bubbles:true}));
                await waitFor(()=>!document.querySelector('.display-setting-actions button[type="submit"]')?.disabled);
                document.querySelector('.display-setting-actions button[type="submit"]').click();
                await waitFor(()=>document.querySelector('.display-notice')?.textContent.includes('已保存时区 Asia/Tokyo'));
                const savedByUI=await invoke('get_display_settings',{requestId:'native-smoke-display-ui'});
                ok=ok && savedByUI.data.preferences.display_timezone==='Asia/Tokyo' && savedByUI.data.settings_revision==='4';
                [...document.querySelectorAll('nav button')].find(button=>button.textContent==='总览')?.click();
                await waitFor(()=>document.querySelector('main .empty h2')?.textContent==='添加 Codex 数据来源');
                ok=ok && document.querySelector('.filters')?.textContent.includes('Asia/Tokyo');
            } catch (_) {}
            await invoke('plugin:event|emit', { event: 'native-smoke-ipc', payload: ok });
        })();
    "#,
        )
        .map_err(|e| e.to_string())?;
    let ipc = receiver.recv_timeout(Duration::from_secs(10));
    app.unlisten(listener);
    app.unlisten(price_listener);
    app.unlisten(settings_listener);
    if ipc.as_deref() != Ok("true") {
        return Err("WebView get_app_status IPC failed".into());
    }
    let settings_events = settings_receiver.try_iter().collect::<Vec<_>>();
    if settings_events.len() != 3 {
        return Err("timezone changes did not emit exactly three settings notifications".into());
    }
    for (payload, revision) in settings_events.iter().zip(["2", "3", "4"]) {
        let event: token_pulse_core::settings::SettingsChanged =
            serde_json::from_str(payload).map_err(|e| e.to_string())?;
        if event.settings_revision.as_str() != revision {
            return Err("settings notification revision is incorrect".into());
        }
    }
    let price_events = price_receiver.try_iter().collect::<Vec<_>>();
    if price_events.len() != 3 {
        return Err(
            "successful price publications did not emit exactly three notifications".into(),
        );
    }
    for (payload, revision) in price_events.iter().zip(["1", "2", "3"]) {
        let event: token_pulse_core::pricing::PriceChanged =
            serde_json::from_str(payload).map_err(|e| e.to_string())?;
        if event.price_revision.as_str() != revision || !event.all_models {
            return Err("price notification revision or whole-model marker is incorrect".into());
        }
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
