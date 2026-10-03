//! Debug-only own-data scene. No source logs, account files or market rates are read.
use tauri::Manager;
use token_pulse_store::{Database, StoreResult};
pub fn seed(db: &Database) -> StoreResult<()> {
    use token_pulse_core::{
        domain::{
            EffectiveMetadata, NormalizedObservation, ObservationQuality, PhysicalPosition,
            ReaderContext, UsageObservation, UsageVector,
        },
        numeric::{DecimalInt, EpochMs},
        pricing::{PriceRuleDraft, PriceRuleMutation},
    };
    db.add_source(token_pulse_store::SourceRecord {
        source_id: "synthetic-source".into(),
        root_path: "synthetic-disabled".into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: false,
        created_at_ms: 1,
    })?;
    db.ensure_session(token_pulse_store::SessionRegistration {
        session_key: "synthetic-session".into(),
        provider_session_id: None,
        parent_key: None,
        parent_provider_id: None,
        created_at_ms: None,
        ledger_id: "synthetic-ledger".into(),
        registered_at_ms: 1,
    })?;
    db.register_file(token_pulse_store::FileRegistration {
        file_id: "synthetic-file".into(),
        source_id: "synthetic-source".into(),
        canonical_path: "synthetic-disabled.jsonl".into(),
        file_identity: None,
        file_generation_id: "synthetic-generation".into(),
        observed_size: 100,
        created_at_ms: 1,
        reader_context: ReaderContext::default(),
    })?;
    use token_pulse_store::batch::*;
    let usage = UsageVector {
        input_total: Some(100),
        cache_write_input: None,
        cached_input: Some(60),
        output_total: Some(10),
        reasoning_output: Some(2),
        reported_total: Some(110),
    };
    db.commit(WriteBatch {
        file_generation_id: "synthetic-generation".into(),
        expected_offset: 0,
        expected_checkpoint_revision: 0,
        next_offset: 100,
        observed_size: 100,
        anchors: vec![],
        reader_context: ReaderContext::default(),
        ledgers: vec![LedgerExpectation {
            session_key: "synthetic-session".into(),
            ledger_id: "synthetic-ledger".into(),
        }],
        observations: vec![ObservationWrite {
            observation_id: "synthetic-observation".into(),
            session_key: Some("synthetic-session".into()),
            payload_fingerprint: "synthetic-hash".into(),
            record: NormalizedObservation::Usage(UsageObservation {
                physical_position: PhysicalPosition {
                    file_generation_id: "synthetic-generation".into(),
                    byte_offset: 0,
                    byte_end: 100,
                },
                session_key: "synthetic-session".into(),
                event_time_ms: Some(1000),
                request_identity: None,
                stream_hint: Some("synthetic-stream".into()),
                last: Some(usage),
                cumulative: Some(usage),
                effective_metadata: EffectiveMetadata {
                    provider: Some("synthetic-provider".into()),
                    model: Some("synthetic-model".into()),
                    ..Default::default()
                },
                explicit_episode_start: true,
                model_context_window: None,
            }),
        }],
        events: vec![EventWrite {
            event_id: "synthetic-event".into(),
            ledger_id: "synthetic-ledger".into(),
            origin_observation_id: "synthetic-observation".into(),
            occurred_at_ms: 1000,
            semantic_key: None,
            episode_id: "synthetic-episode".into(),
            model: Some("synthetic-model".into()),
            project_id: None,
            turn_id: None,
            usage,
            calculation_method: "last_new_stream".into(),
        }],
        streams: vec![StreamWrite {
            ledger_id: "synthetic-ledger".into(),
            stream_key: "synthetic-stream".into(),
            episode_id: "synthetic-episode".into(),
            baseline: usage,
            observation_id: "synthetic-observation".into(),
            quality: ObservationQuality::Confirmed,
            expected_state_revision: None,
        }],
        provenance: vec![],
        pending: vec![],
        contexts: vec![],
        diagnostics: vec![],
        canonical: vec![],
    })?;
    db.mutate_price_rule(
        PriceRuleMutation::Create {
            draft: PriceRuleDraft {
                provider: "synthetic-provider".into(),
                model_exact: "synthetic-model".into(),
                source_id: None,
                currency: "USD".into(),
                effective_from_ms: EpochMs::new(0)?,
                effective_to_ms: None,
                priority: 0,
                input_rate_atoms: DecimalInt::parse("10")?,
                cached_rate_atoms: Some(DecimalInt::parse("5")?),
                output_rate_atoms: DecimalInt::parse("20")?,
                origin_reference: Some("explicit native synthetic fixture".into()),
            },
        },
        0,
        1,
    )?;
    Ok(())
}
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let result = verify(&app);
        match &result {
            Ok(()) => println!(
                "NATIVE_PRICE_REVALUE_OK: own-data startup cache, exact 900 atoms, pinned main IPC/CAS/idempotence, React start/basis/history, mini denied, shared privacy, unchanged consumption/checkpoint"
            ),
            Err(error) => eprintln!("NATIVE_PRICE_REVALUE_FAILED: {error}"),
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}
fn verify(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<super::RuntimeState>();
    if !state
        .data_directory
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("native-probe-"))
    {
        return Err("price scene requires isolated database".into());
    }
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let before=db.snapshot(|tx,r| Ok((r.data,tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='synthetic-generation'",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?))).map_err(|e|e.to_string())?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const status=await invoke('get_price_revalue_status',{requestId:'revalue-startup'});
      if(status.data.current_price_revision!=='1'||status.data.uncached_ledgers!=='0'||status.data.latest_job.state!=='succeeded'||status.data.latest_job.total_events!=='1')throw new Error('REVALUE_BOOT');
      const request={scope:{kind:'all'},basis:{mode:'specified_time',specified_at_ms:1500},expected_price_revision:'1',request_key:'native-revalue-manual'};
      const first=await invoke('start_price_revalue',{requestId:'revalue-manual',request});
      const duplicate=await invoke('start_price_revalue',{requestId:'revalue-duplicate',request});
      if(first.data.job_id!==duplicate.data.job_id||first.data.price_revision!=='1')throw new Error('REVALUE_IDEMPOTENCE');
      let finished=false;for(let i=0;i<100;i++){const current=await invoke('get_price_revalue_status',{requestId:'revalue-poll'});if(current.data.latest_job.job_id===first.data.job_id&&current.data.latest_job.state==='succeeded'){finished=true;break;}await new Promise(r=>setTimeout(r,30));}if(!finished)throw new Error('REVALUE_COMPLETION');
      const cancelled=await invoke('cancel_price_revalue',{requestId:'revalue-terminal-cancel',jobId:first.data.job_id});if(cancelled.data!=='already_finished')throw new Error('REVALUE_TERMINAL_CANCEL');
      for(const [modified,expected] of [[{...request,request_key:'stale',expected_price_revision:'0'},'REVISION_CONFLICT'],[{...request,request_key:'bad-scope',scope:{kind:'sessions',session_keys:['missing']}},'INVALID_QUERY'],[{...request,basis:{mode:'event_time'}},'REQUEST_KEY_CONFLICT']]){let denied=false;try{await invoke('start_price_revalue',{requestId:'revalue-reject',request:modified});}catch(e){denied=e.code===expected;}if(!denied)throw new Error(expected);}
      [...document.querySelectorAll('.sidebar nav button')].find(n=>n.textContent==='设置').click();
      await wait(()=>[...document.querySelectorAll('[role=tab]')].some(n=>n.textContent==='价格规则'));
      [...document.querySelectorAll('[role=tab]')].find(n=>n.textContent==='价格规则').click();
      await wait(()=>document.querySelector('.price-revalue')?.textContent.includes('重估完成'));
      const region=()=>document.querySelector('.price-revalue');
      const select=region().querySelector('select');Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(select,'specified_time');select.dispatchEvent(new Event('change',{bubbles:true}));
      await wait(()=>region().textContent.includes('取当前时刻'));
      [...region().querySelectorAll('button')].find(n=>n.textContent==='重估全部已确认用量').click();
      await wait(()=>region().textContent.includes('重估请求已接受。'));
      await wait(()=>region().textContent.includes('重估完成'));
      const current=await invoke('get_price_revalue_status',{requestId:'revalue-ui-result'});if(current.data.latest_job.automatic||current.data.latest_job.basis.mode!=='specified_time'||current.data.latest_job.completed_ledgers!=='1')throw new Error('REVALUE_UI_START');
      const fill=(node,value)=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(node,value);node.dispatchEvent(new Event('input',{bubbles:true}));};
      fill(document.querySelector('[aria-label="历史价格版本"]'),'0');document.querySelector('.price-version form button').click();
      await wait(()=>region()?.textContent.includes('历史价格版本只读'));
      if(region().textContent.includes('重估全部已确认用量'))throw new Error('REVALUE_HISTORY_WRITABLE');
      [...document.querySelectorAll('button')].find(n=>n.textContent==='刷新当前版本').click();await wait(()=>region()?.textContent.includes('重估全部已确认用量'));
    "#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        r#"
      for(const command of ['get_price_revalue_status','start_price_revalue','cancel_price_revalue']){let denied=false;try{await invoke(command,{requestId:'revalue-mini-denied',jobId:'unused',request:{scope:{kind:'all'},basis:{mode:'event_time'},expected_price_revision:'1',request_key:'mini'}});}catch{denied=true;}if(!denied)throw new Error('REVALUE_MINI_ALLOWED');}
    "#,
    )?;
    super::mini_smoke::evaluate(
        app,
        &main,
        r#"
      const settings=await invoke('get_display_settings',{requestId:'revalue-policy'});
      await invoke('set_display_privacy',{requestId:'revalue-private',request:{privacy:true,expected_settings_revision:settings.data.settings_revision}});
      const hidden=await invoke('get_price_revalue_status',{requestId:'revalue-private-state'});if(!hidden.display_policy.privacy)throw new Error('REVALUE_POLICY_MISSING');
      await wait(()=>!document.querySelector('.price-revalue'));
      const next=await invoke('get_display_settings',{requestId:'revalue-next-policy'});
      await invoke('set_display_privacy',{requestId:'revalue-public',request:{privacy:false,expected_settings_revision:next.data.settings_revision}});
      await wait(()=>document.querySelector('.price-revalue')?.textContent.includes('重估完成'));
      if(document.querySelector('.price-revalue select').value!=='event_time')throw new Error('REVALUE_OLD_DRAFT_RESTORED');
    "#,
    )?;
    db.snapshot(|tx,r| {
        if (r.data,tx.query_row("SELECT committed_offset,checkpoint_revision FROM file_generations WHERE file_generation_id='synthetic-generation'",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?)!=before {return Err(token_pulse_store::ErrorCode::DbCorrupt.into())}
        let priced:i64=tx.query_row("SELECT COUNT(*) FROM event_valuations e JOIN valuation_sets v USING(valuation_set_id) WHERE v.state='ready' AND v.price_revision=1 AND e.cost_atoms='900' AND e.currency='USD'",[],|r|r.get(0))?;
        if priced!=3 {return Err(token_pulse_store::ErrorCode::DbCorrupt.into())}Ok(())
    }).map_err(|e|format!("cache precision / consumption check: {e}"))?;
    Ok(())
}
