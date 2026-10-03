import { installSyntheticCalendar } from './calendar-bridge';
import { test, expect } from '@playwright/test';

test('explicit source reread keeps its own retry key, progress, safe cancellation and original rebuild action', async ({ page }, testInfo) => {
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    let job: Record<string, unknown> | null = null, key: string | null = null, lost = true;
    Object.assign(window, { isTauri: true, __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} }, __TAURI_INTERNALS__: { transformCallback: () => 0, invoke: async (command: string, args: Record<string, unknown>) => {
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data });
      if (command.startsWith('plugin:event|')) return 0;
      if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
      if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
      if (command === 'get_sources') return response({ settings_revision: '1', sources: [] });
      if (command === 'query_diagnostics') return response({ data_revision: '1', issues: [], has_more: false });
      if (command === 'get_taskbar_status') return response({ revision: '0', state: 'disabled', applied_settings_revision: null, issue: null, error: null, compact: null, fallback_visible: null, fallback_error: null, action_error: null, last_cleanup: null, last_snapshot_at_ms: null });
      if (command === 'get_rebuild_status') return response(structuredClone(job));
      if (command === 'start_source_reread') {
        const request = args.request as { kind: string; scope: { kind: string }; request_key: string };
        if (request.kind !== 'rebuild' || request.scope.kind !== 'all' || (key !== null && key !== request.request_key)) throw Error('read intent or key changed');
        key = request.request_key;
        job ??= { job_id: 'synthetic-reread', kind: 'rebuild', state: 'running', phase: 'reading_source_files', discovered_files: '2', discovery_complete: true, processed_files: '0', processed_bytes: '9007199254740993', accepted_events: '0', pending_observations: '0', can_cancel: true, error: null, created_at_ms: 1000, updated_at_ms: 2000 };
        if (lost) { lost = false; throw Error('synthetic lost source-read reply'); }
        return response(structuredClone(job));
      }
      if (command === 'cancel_job') { job!.state = 'cancelling'; return response('accepted'); }
      throw Error(`unexpected source-read fixture command ${command}`);
    } }, __completeSourceRead: () => { job!.state = 'cancelled'; job!.can_cancel = false; } });
  });
  await page.goto('/'); await page.getByRole('button', { name: '采集诊断', exact: true }).click();
  const panel = page.getByRole('region', { name: '基本重建进度' });
  const read = panel.getByRole('button', { name: '重读已启用来源', exact: true });
  const rebuild = panel.getByRole('button', { name: '重建全部账本', exact: true });
  await expect(read).toBeEnabled(); await read.click();
  await expect(panel.getByText('synthetic lost source-read reply')).toBeVisible();
  await expect(rebuild).toBeDisabled(); await read.click();
  await expect(panel.getByText('来源重读已提交，正在执行。')).toBeVisible();
  await expect(panel.getByText(/正在重读已启用来源/)).toBeVisible();
  await expect(panel.getByText('已处理 9,007,199,254,740,993 字节')).toBeVisible();
  await expect(read).toBeDisabled(); await expect(rebuild).toBeDisabled();
  await page.screenshot({ path: testInfo.outputPath('source-reread-dark.png'), fullPage: true });
  await page.setViewportSize({ width: 960, height: 860 }); await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('source-reread-light-960.png'), fullPage: true });
  await panel.getByRole('button', { name: '取消重建', exact: true }).click();
  await expect(panel.getByText('正在安全取消')).toBeVisible();
  await page.evaluate(() => (window as unknown as { __completeSourceRead: () => void }).__completeSourceRead());
  await panel.getByRole('button', { name: '刷新重建状态' }).click();
  await expect(panel.getByText('已取消', { exact: true })).toBeVisible();
  await expect(read).toBeEnabled(); await expect(rebuild).toBeEnabled();
});

test('synthetic job IPC fixture shows accepted cancellation until final state and retains request idempotency', async ({ page }) => {
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    let jobs: Record<string, unknown>[] = [];let requestKey:string|null=null;let lostResponse=true;
    Object.assign(window, {isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{transformCallback:()=>0,invoke:async(command:string,args:Record<string,unknown>)=>{
      if (command === 'plugin:event|listen' || command === 'plugin:event|unlisten') return 0;
      const response=(data:unknown)=>({api_version:1,request_id:args.requestId,display_policy:{settings_revision:'1',privacy:false},data});
      if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
      if(command==='get_app_status')return response({version:'synthetic-test',development:true,data_directory:'synthetic-test',collector:'ready',storage:'ready',storage_error:null,quota:'not_configured',taskbar:'not_implemented'});
      if(command==='query_diagnostics')return response({data_revision:'1',issues:[],has_more:false});
      if(command==='get_rebuild_status')return response(structuredClone(jobs[0]??null));
      if(command==='get_sources')return response({settings_revision:'1',sources:[]});
      if(command==='get_taskbar_status')return response({revision:'0',state:'disabled',applied_settings_revision:null,issue:null,error:null,compact:null,fallback_visible:null,fallback_error:null,action_error:null,last_cleanup:null,last_snapshot_at_ms:null});
      if(command==='start_job') {
        const request=args.request as {request_key:string};
        if(requestKey!==null && requestKey!==request.request_key)throw new Error('lost idempotency key');
        requestKey=request.request_key;
        if(jobs.length===0)jobs=[{job_id:'synthetic-job',kind:'rebuild',state:'running',phase:'replaying_observations',discovered_files:'2',discovery_complete:true,processed_files:'1',processed_bytes:'92233720368547758080',accepted_events:'25',pending_observations:'1',can_cancel:true,error:null,created_at_ms:1000,updated_at_ms:2000}];
        if(lostResponse){lostResponse=false;throw new Error('synthetic lost reply');}return response(structuredClone(jobs[0]));
      }
      if(command==='cancel_job') {jobs[0].state='cancelling';return response('accepted');}
      throw new Error(`unexpected synthetic command ${command}`);
    }},__setSyntheticJobState:(state:string)=>{jobs[0].state=state;jobs[0].can_cancel=false;}});
  });
  await page.goto('/');await page.getByRole('button',{name:'采集诊断',exact:true}).click();
  await expect(page.getByRole('button',{name:'重建全部账本'})).toBeEnabled();
  await page.getByRole('button',{name:'重建全部账本'}).click();await expect(page.getByText('synthetic lost reply')).toBeVisible();
  await page.getByRole('button',{name:'重建全部账本'}).click();await expect(page.getByText('重建已提交，正在执行。')).toBeVisible();
  await expect(page.getByText('已处理 92,233,720,368,547,758,080 字节')).toBeVisible();
  await expect(page.getByText('synthetic-job', {exact:true})).toHaveCount(0);
  await expect(page.getByRole('button', {name:'重建全部账本'})).toBeDisabled();
  await page.getByRole('button',{name:'取消重建',exact:true}).click();await expect(page.getByText('已请求安全取消，等待当前批次结束。')).toBeVisible();await expect(page.getByText('正在安全取消')).toBeVisible();await expect(page.getByRole('button',{name:'等待安全取消'})).toBeDisabled();
  await page.evaluate(()=>{(window as unknown as {__setSyntheticJobState:(s:string)=>void}).__setSyntheticJobState('cancelled');});
  await page.getByRole('button',{name:'刷新重建状态'}).click();await expect(page.getByText('已取消',{exact:true})).toBeVisible();await expect(page.getByRole('button',{name:'取消重建',exact:true})).toHaveCount(0);
});

test('basic diagnostics show source failures, unknown scan times and one rebuild without history or internal phases', async ({ page }, testInfo) => {
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    let revision = 1, failRead = false, failProgress = false;
    const source = { source_id: 'fixture', root_path: 'E:\\synthetic-diagnostics\\.codex', origin: 'custom', enabled: true, removed: false, readability: 'unreadable', capabilities: { physical_identity: 'not_probed', byte_seek: 'not_probed', watcher: 'unavailable', polling_required: true }, last_scan_at_ms: null as number | null, last_success_at_ms: null, error: 'SOURCE_UNREADABLE' };
    const job = { job_id: 'internal-job-id', kind: 'rebuild', state: 'running', phase: 'internal_new_phase', discovered_files: '0', discovery_complete: false, processed_files: '0', processed_bytes: '9007199254740993', accepted_events: '25', pending_observations: '1', can_cancel: true, error: null, created_at_ms: 1000, updated_at_ms: 2000 };
    Object.assign(window, { isTauri: true, __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} }, __TAURI_INTERNALS__: { transformCallback: () => 0, invoke: async (command: string, args: Record<string, unknown>) => {
      if (command === 'plugin:event|listen' || command === 'plugin:event|unlisten') return 0;
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data });
      if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
      if (command === 'get_app_status') return response({ version: 'synthetic', development: true, data_directory: 'synthetic', collector: 'error', storage: 'ready', storage_error: null, quota: 'ready', taskbar: 'not_implemented' });
      if (command === 'get_sources') { if (failRead) throw { code: 'SOURCE_UNREADABLE' }; return response({ settings_revision: String(revision), sources: [{ ...source }] }); }
      if (command === 'query_diagnostics') return response({data_revision:'1',issues:[],has_more:false});
      if (command === 'get_rebuild_status') { if (failProgress) throw { code: 'DB_CORRUPT' }; return response({ ...job }); }
      if (command === 'get_taskbar_status') return response({ revision: '0', state: 'disabled', applied_settings_revision: null, issue: null, error: null, compact: null, fallback_visible: null, fallback_error: null, action_error: null, last_cleanup: null, last_snapshot_at_ms: null });
      if (command === 'manage_source') {
        if (args.expectedSettingsRevision !== String(revision)) throw { code: 'REVISION_CONFLICT' };
        const action = args.action as { kind: string };
        if (action.kind === 'pause') { source.enabled = false; source.readability = 'disabled'; }
        if (action.kind === 'resume') { source.enabled = true; source.readability = 'unreadable'; }
        if (action.kind === 'detect') source.last_scan_at_ms = 1000;
        ++revision; return response({ settings_revision: String(revision), sources: [{ ...source }] });
      }
      throw new Error(`unexpected synthetic command ${command}`);
    } }, __failDiagnosticReads: () => { failRead = true; failProgress = true; } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '采集诊断', exact: true }).click();
  const sources = page.getByRole('region', { name: '来源采集状态' });
  const progress = page.locator('.jobs-panel');
  await expect(sources.getByText('E:\\synthetic-diagnostics\\.codex', { exact: true })).toBeVisible();
  await expect(sources.getByText('尚无扫描记录')).toBeVisible();
  await expect(sources.getByText('尚无成功记录')).toBeVisible();
  await expect(sources.getByText('无法读取所选来源，请检查目录和访问权限。')).toBeVisible();
  await expect(progress.getByText('文件 0 / 发现中')).toBeVisible();
  await expect(progress.getByText('已处理 9,007,199,254,740,993 字节')).toBeVisible();
  await expect(page.getByText('internal_new_phase', { exact: true })).toHaveCount(0);
  await expect(page.getByText('internal-job-id', { exact: true })).toHaveCount(0);
  await expect(page.getByText('账户额度', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '启用 WSL 来源' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '添加自定义目录' })).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath('diagnostics-running.png'), fullPage: true });
  await page.setViewportSize({ width: 960, height: 860 });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  await expect(progress.getByText('已处理 9,007,199,254,740,993 字节')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('diagnostics-light-960.png'), fullPage: true });
  await sources.getByRole('button', { name: '暂停采集' }).click();
  await expect(sources.getByText('已暂停', { exact: true })).toBeVisible();
  await sources.getByRole('button', { name: '恢复采集' }).click();
  await sources.getByRole('button', { name: '重新检测来源' }).click();
  await expect(sources.getByText('尚无扫描记录')).toHaveCount(0);
  await page.evaluate(() => (window as unknown as { __failDiagnosticReads: () => void }).__failDiagnosticReads());
  await sources.getByRole('button', { name: '刷新来源' }).click();
  await progress.getByRole('button', { name: '刷新重建状态' }).click();
  await expect(sources.getByText('保留上次来源状态')).toBeVisible();
  await expect(progress.getByRole('alert')).toContainText('显示上次读取的状态');
  await expect(progress.getByText('已处理 9,007,199,254,740,993 字节')).toBeVisible();
  await expect(progress.getByRole('button', { name: '取消重建' })).toBeDisabled();
});
