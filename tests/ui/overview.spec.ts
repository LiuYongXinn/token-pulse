import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';
import { syntheticQuota } from './quota-fixture';

for (const viewport of [{ width: 944, height: 560 }, { width: 1264, height: 649 }]) {
  test(`small work-area client ${viewport.width}x${viewport.height} keeps navigation and settings reachable`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
    const columns = await page.evaluate(() => ({
      left: document.querySelector('.overview-left')!.getBoundingClientRect().right,
      right: document.querySelector('.overview-right')!.getBoundingClientRect().left,
    }));
    expect(columns.left).toBeLessThan(columns.right);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width);
    await page.screenshot({ path: `test-results/small-work-area-overview-${viewport.width}.png`, fullPage: true });
    await page.getByRole('button', { name: '设置', exact: true }).click();
    await page.getByRole('tab', { name: '任务栏显示', exact: true }).click();
    await expect(page.getByRole('checkbox', { name: '启用任务栏显示' })).toBeChecked();
    await page.getByRole('checkbox', { name: '启用任务栏显示' }).uncheck();
    const save = page.getByRole('button', { name: '保存任务栏设置', exact: true });
    await expect(save).toBeEnabled();
    await save.scrollIntoViewIfNeeded();
    await expect(save).toBeInViewport();
    await page.getByRole('button', { name: '重置任务栏草稿', exact: true }).click();
    await expect(page.getByRole('checkbox', { name: '启用任务栏显示' })).toBeChecked();
    await page.locator('main > footer').scrollIntoViewIfNeeded();
    await expect(page.locator('main > footer')).toBeInViewport();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width);
    await page.screenshot({ path: `test-results/small-work-area-settings-${viewport.width}.png`, fullPage: true });
    await page.getByRole('button', { name: '总览', exact: true }).click();
    await expect(page.getByRole('heading', { name: '总览', exact: true })).toBeInViewport();
  });
}

test.beforeEach(async ({ page }) => {
  // Explicit synthetic UI DTO bridge. No fixture is imported into production.
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    let privacy = false, policyRevision = '1';
    let miniIntent: unknown = null, navigationRevision = '0';
    let deferNavigation = false; const navigationWaits: (() => void)[] = [];
    let fail = false, deferNext = false, release: (() => void) | null = null;
    let lastDashboardRequest: unknown = null;
    let priceRevision = '3', cost = '0.871234567890123', reads = 0, hidden = false;
    let quota: Record<string, unknown> = { connection_epoch: 'synthetic-overview-disconnected', quota_revision: '0', state: 'disconnected', selected_limit_id: null, available_limits: [], fetched_at_ms: null, last_attempt_at_ms: null, windows: [], error_code: null };
    let callbackId = 0, eventId = 0;
    const callbacks = new Map<number, (event: unknown) => void>();
    const listeners = new Map<number, { event: string; handler: number }>();
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden });
    const sources = ['a', 'b'].map(id => ({ source_id: `synthetic-${id}`, root_path: `E:\\synthetic-qa-${id}\\.codex`, origin: 'custom', enabled: true, removed: false, readability: 'readable', capabilities: { physical_identity: 'available', byte_seek: 'available', watcher: 'available', polling_required: true }, last_scan_at_ms: 1000, last_success_at_ms: 1000, error: null }));
    const complete = (n: number, total: number) => ({ value: String(n), covered_total_tokens: String(total), complete: true });
    const unknown = { value: null, covered_total_tokens: '0', complete: false };
    const tokens = (partial = false) => ({ total_tokens: partial ? '17' : '683067', input_total: partial ? unknown : complete(630630, 683067), cached_input: partial ? unknown : complete(429566, 683067), noncached_input: partial ? unknown : complete(201064, 683067), output_total: partial ? unknown : complete(52437, 683067), reasoning_output: partial ? unknown : complete(19926, 683067), cache_write_input: partial ? unknown : complete(12000, 683067), session_count: '1', usage_event_count: partial ? '1' : '5', reliable_turn_count: null, reliable_turns_complete: false });
    const coverage = { state: 'partial', pending_observation_count: '2', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '1', source_issues: [{ source_id: 'synthetic-a', code: 'scan_evidence_missing', last_success_ms: 1000 }], format_issues: [], breakdown_complete: true };
    const empty = { total_tokens: '0', input_total: unknown, cached_input: unknown, noncached_input: unknown, output_total: unknown, reasoning_output: unknown, cache_write_input: { value: null, covered_total_tokens: '0', complete: false }, session_count: '0', usage_event_count: '0', reliable_turn_count: null, reliable_turns_complete: false };
    const price = (partial: boolean, basis: { mode: string } = { mode: 'event_time' }) => ({ redacted: privacy, basis, currencies: partial ? [] : [{ currency: 'USD', estimated_cost: privacy ? null : basis.mode === 'specified_time' ? '2.321234567890123' : cost, priced_total_tokens: '650000' }], priced_total_tokens: partial ? '0' : '650000', unpriced_total_tokens: partial ? '17' : '33067', reasons: privacy ? [] : [{ code: 'missing_rule', total_tokens: partial ? '17' : '33067', event_count: '1' }], calculating: false });
    Object.assign(window, { isTauri: true, __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: (_event: string, id: number) => { const listener = listeners.get(id); if (listener) callbacks.delete(listener.handler); listeners.delete(id); } }, __TAURI_INTERNALS__: { transformCallback: (callback: (event: unknown) => void) => { callbacks.set(++callbackId, callback); return callbackId; }, invoke: async (command: string, args: Record<string, unknown>) => {
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: policyRevision, privacy }, data });
      if (command === 'get_mini_stats_request') return response(miniIntent);
      if (command === 'get_main_navigation') { const data = { revision: navigationRevision, intent: structuredClone(miniIntent) }; if (deferNavigation) { deferNavigation = false; await new Promise<void>(resolve => navigationWaits.push(resolve)); } return response(data); }
      if (command === 'get_taskbar_preferences') return response({settings_revision: policyRevision, preferences:{enabled:true,position:'notification_left',fallback_to_mini:true,display:{layout:'two_rows',show_tokens:true,show_costs:true,show_quota:true,show_weekly_reset:true}}});
      if (command === 'get_taskbar_status') return response({revision:'1',state:'embedded',applied_settings_revision:policyRevision,issue:null,error:null,compact:false,fallback_visible:null,fallback_error:null,action_error:null,last_cleanup:null,last_snapshot_at_ms:null});
      if (command === 'get_display_settings') return response({ settings_version: 1, settings_revision: policyRevision, preferences: { theme: 'dark', privacy, display_timezone: 'Asia/Shanghai' } });
      if (command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
      if (command === 'plugin:event|listen') { listeners.set(++eventId, { event: String(args.event), handler: Number(args.handler) }); return eventId; }
      if (command === 'plugin:event|unlisten') return null;
      if (command === 'get_account_quota') return response(structuredClone(quota));
      if (command === 'refresh_account_quota') return response({ status: 'in_flight', retry_after_ms: null, quota });
      if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: privacy ? '应用数据目录（已隐藏）' : 'synthetic-test', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
      if (command === 'get_sources') return response({ settings_revision: policyRevision, sources: privacy ? sources.map(s => ({ ...s, root_path: '来源 #' + s.source_id })) : sources });
      if (command === 'get_price_rules') return response({ price_revision: '3', rules: [], aliases: [] });
      if (command === 'get_diagnostics') return response({ data_revision: '7', issues: [], has_more: false });
      if (command === 'get_dashboard_bundle') {
        ++reads; lastDashboardRequest = structuredClone(args.request);
        if (fail) throw new Error('synthetic refresh failure');
        const r = args.request as { price_basis: { mode: string }; filter: { sources: { ids?: string[] }; range: { start_ms: number; end_ms: number; timezone: string } }; grain: string; heatmap_range: { start_ms: number; end_ms: number } };
        const partial = r.filter.sources.ids?.includes('synthetic-b') ?? false;
        const step = r.grain === 'hour' ? 3_600_000 : 86_400_000;
        const series = Array.from({ length: Math.ceil((r.filter.range.end_ms - r.filter.range.start_ms) / step) }, (_, i) => {
          const start = r.filter.range.start_ms + i * step;
          // The UI bridge only illustrates rendering; real DST bucketing is independently tested in Rust.
          const count = i === 0 ? tokens(partial) : empty;
          return { start_ms: start, end_ms: Math.min(start + step, r.filter.range.end_ms), display_label: new Date(start).toISOString().slice(0, 16), utc_offset: '+08:00', totals: count, coverage: { ...coverage, breakdown_complete: i === 0 && !partial } };
        });
        const heatmap = Array.from({ length: 182 }, (_, i) => ({ start_ms: r.heatmap_range.start_ms + i * 86_400_000, end_ms: r.heatmap_range.start_ms + (i + 1) * 86_400_000, display_label: `synthetic-day-${i + 1}`, utc_offset: '+08:00', totals: i % 5 === 0 ? tokens(partial) : empty, coverage: { ...coverage, breakdown_complete: !partial && i % 5 === 0 } }));
        const data = { meta: { snapshot_id: String(args.requestId), data_revision: '7', price_revision: priceRevision, generated_at_ms: r.filter.range.start_ms + 1000, parser_versions: ['synthetic'], accounting_versions: ['synthetic'], display_timezone: r.filter.range.timezone }, summary: tokens(partial), pricing: price(partial, r.price_basis), coverage: { ...coverage, breakdown_complete: !partial }, series, heatmap, recent_sessions: [{ session_key: 'synthetic-session', display_name: privacy ? '会话 #synthetic-session' : 'synthetic-ui-session', latest_at_ms: r.filter.range.start_ms + 1000, latest_model: 'synthetic-model', latest_project_id: 'synthetic-project', latest_project_name: privacy ? '项目 #synthetic-project' : 'Synthetic QA Project', summary: tokens(partial), pricing: price(partial) }] };
        if (deferNext) { deferNext = false; await new Promise<void>(resolve => { release = resolve; }); }
        return response(data);
      }
      throw new Error(`unexpected synthetic command ${command}`);
    } }, __setSyntheticCoverage: (value: Record<string, unknown>) => { Object.assign(coverage, value); }, __setSyntheticDashboardFailure: (value: boolean) => { fail = value; }, __deferSyntheticDashboard: () => { deferNext = true; }, __releaseSyntheticDashboard: () => { release?.(); release = null; },
    __setSyntheticQuota: (value: Record<string, unknown>) => { quota = structuredClone(value); for (const [id, listener] of listeners) if (listener.event === 'account_quota_changed') callbacks.get(listener.handler)?.({ event: listener.event, id, payload: { connection_epoch: quota.connection_epoch, quota_revision: quota.quota_revision, state: quota.state } }); },
    __lastDashboardRequest: () => lastDashboardRequest,
    __requestSyntheticMiniStats: (all = false, id = 'synthetic-mini-open') => {
      navigationRevision=String(BigInt(navigationRevision)+1n); miniIntent = { kind: 'mini_stats', request: { request_id: id, mini_scope: all ? { kind: 'today_all_sources' } : { kind: 'session', session_key: 'synthetic-fixed', start: { kind: 'fixed', start_ms: 1709179200123 } }, calendar: { range: { start_ms: 1709179200123, end_ms: 1709203200457, timezone: 'UTC' }, heatmap_range: { start_ms: Date.parse('2023-09-01T00:00:00Z'), end_ms: Date.parse('2024-03-01T00:00:00Z'), timezone: 'UTC' }, local_today: '2024-02-29' } } };
      for (const [id, listener] of listeners) if (listener.event === 'main_navigation_changed') callbacks.get(listener.handler)?.({ event: listener.event, id, payload: null });
    },
    __taskbarNavigationQA: { settings: () => { navigationRevision=String(BigInt(navigationRevision)+1n);miniIntent={kind:'taskbar_settings'}; for(const [id,listener] of listeners)if(listener.event==='main_navigation_changed')callbacks.get(listener.handler)?.({event:listener.event,id,payload:{kind:'mini_stats'}}); }, hold:()=>{deferNavigation=true;},release:()=>navigationWaits.splice(0).forEach(resolve=>resolve()), revision:(value:string)=>{navigationRevision=value;}, listeners:()=>[...listeners.values()].filter(v=>v.event==='main_navigation_changed').length },
    __syntheticPriceState: () => ({ reads, listeners: [...listeners.values()].filter(listener => listener.event === 'price_rules_changed').length }),
    __setSyntheticHidden: (value: boolean) => { hidden = value; document.dispatchEvent(new Event('visibilitychange')); },
    __emitSyntheticPrivacyChange: (value: boolean) => { privacy = value; policyRevision = String(BigInt(policyRevision) + 1n); for (const [id, listener] of listeners) if (listener.event === 'display_policy_changed' || listener.event === 'settings_changed') callbacks.get(listener.handler)?.({ event: listener.event, id, payload: { settings_revision: policyRevision, privacy } }); },
    __emitSyntheticPriceChange: () => { priceRevision = String(Number(priceRevision) + 1); cost = '1.231234567890123'; for (const [id, listener] of listeners) if (listener.event === 'price_rules_changed') callbacks.get(listener.handler)?.({ event: listener.event, id, payload: { price_revision: priceRevision, all_models: true } }); } });
  });
  await page.goto('/');
});

test('overview preserves prototype layout, known breakdown and real unknown states under shared filters', async ({ page }) => {
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await expect(page.getByText('$0.87', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '用量趋势' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '最近会话' })).toBeVisible();
  await expect(page.getByLabel('完整 Token 分解')).toBeVisible();
  await expect(page.getByRole('region', { name: '账户额度总览' })).toContainText('未连接');
  const bounds = await page.evaluate(() => ({ left: document.querySelector('.overview-left')!.getBoundingClientRect().right, right: document.querySelector('.overview-right')!.getBoundingClientRect().left }));
  expect(bounds.left).toBeLessThan(bounds.right);
  const activity = page.getByRole('group', { name: '近 26 周每日活动' });
  await expect(activity.getByRole('button')).toHaveCount(182);
  await activity.getByRole('button').nth(1).focus();
  await expect(page.locator('.activity-panel .chart-caption')).toHaveText('synthetic-day-2 · 0 Token · 已统计可信用量，部分记录待核对');
  await activity.getByRole('button').first().click();
  await expect(page.getByLabel('日期范围')).toHaveValue('custom');
  await expect(page.getByTitle('编辑已应用日期')).toBeVisible();
  await page.screenshot({ path: 'test-results/overview-known-1280.png', fullPage: true });
  await page.getByRole('button', { name: '日', exact: true }).click();
  await expect(page.getByRole('button', { name: '日', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('group', { name: '各时间桶可信 Token' }).getByRole('button')).toHaveCount(1);
  await page.getByLabel('日期范围').selectOption('last7');
  await page.getByRole('button', { name: '模型', exact: true }).click();
  await page.getByRole('button', { name: '总览', exact: true }).click();
  await expect(page.getByLabel('日期范围')).toHaveValue('last7');
  await expect(page.getByRole('group', { name: '各时间桶可信 Token' }).getByRole('button')).toHaveCount(7);
  await page.getByLabel('来源', { exact: true }).selectOption('synthetic-b');
  await expect(page.getByLabel('17 Token', { exact: true })).toBeVisible();
  await expect(page.getByText('未计价', { exact: true })).toBeVisible();
  await expect(page.getByLabel('完整 Token 分解')).toHaveCount(0);
  await expect(page.getByText('分项覆盖不足或无输入')).toBeVisible();
  await page.setViewportSize({ width: 960, height: 680 });
  await page.screenshot({ path: 'test-results/overview-partial-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.getByRole('button', { name: /覆盖 0% · 查看依据/ }).click();
  await expect(page.getByRole('tabpanel', { name: '价格规则设置' })).toBeVisible();
});

test('coverage shows verification progress separately and keeps confirmed totals available', async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 680 });
  await page.evaluate(() => (window as unknown as { __setSyntheticCoverage: (v: Record<string, unknown>) => void }).__setSyntheticCoverage({ state: 'unknown', pending_observation_count: '0', pending_file_count: '0', verifying_file_count: '89', source_issues: [{ source_id: 'synthetic-a', code: 'source_scan_verifying', last_success_ms: 1000 }] }));
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect(page.locator('.overview-coverage')).toContainText('正在校验来源覆盖 · 待校验文件 89');
  await expect(page.locator('.overview-coverage')).not.toContainText('待核对用量记录');
  await expect(page.locator('.overview-coverage')).not.toContainText('待采集文件');
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(960);
  await page.screenshot({ path: 'test-results/overview-verifying-960.png', fullPage: true });
  await page.evaluate(() => (window as unknown as { __setSyntheticCoverage: (v: Record<string, unknown>) => void }).__setSyntheticCoverage({ state: 'partial', pending_observation_count: '15', verifying_file_count: '0', source_issues: [] }));
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect(page.locator('.overview-coverage')).toContainText('待核对用量记录 15');
  await expect(page.locator('.overview-coverage')).not.toContainText('文件 0');
  await page.getByRole('button', { name: '查看诊断', exact: true }).click();
  await expect(page.getByRole('heading', { name: '采集诊断', exact: true })).toBeVisible();
});

test('same-filter refresh retains prior values on error and older-filter replies cannot replace the new scope', async ({ page }) => {
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as { __deferSyntheticDashboard: () => void }).__deferSyntheticDashboard());
  await page.getByLabel('来源', { exact: true }).selectOption('synthetic-b');
  await expect(page.getByRole('heading', { name: 'Token 分解' })).toBeVisible(); await expect(page.locator('.total-number')).toContainText('—');
  await expect(page.getByLabel('683,067 Token', { exact: true })).toHaveCount(0);
  await page.evaluate(() => (window as unknown as { __releaseSyntheticDashboard: () => void }).__releaseSyntheticDashboard());
  await expect(page.getByLabel('17 Token', { exact: true })).toBeVisible();
  await page.getByLabel('来源', { exact: true }).selectOption('');
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as { __setSyntheticDashboardFailure: (v: boolean) => void }).__setSyntheticDashboardFailure(true));
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('synthetic refresh failure');
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await expect(page.getByRole('alert')).toContainText('保留上次快照');
  await page.evaluate(() => { const w = window as unknown as { __setSyntheticDashboardFailure: (v: boolean) => void; __deferSyntheticDashboard: () => void }; w.__setSyntheticDashboardFailure(false); w.__deferSyntheticDashboard(); });
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect(page.getByText('正在刷新…', { exact: true })).toBeVisible();
  await page.getByLabel('来源', { exact: true }).selectOption('synthetic-b');
  await expect(page.getByLabel('17 Token', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as { __releaseSyntheticDashboard: () => void }).__releaseSyntheticDashboard());
  await expect(page.getByLabel('17 Token', { exact: true })).toBeVisible();
  await expect(page.getByLabel('683,067 Token', { exact: true })).toHaveCount(0);
});

test('price notifications refresh whole bundles and warm offscreen views with bounded subscriptions', async ({ page }) => {
  type Bridge = { __syntheticPriceState: () => { reads: number; listeners: number }; __emitSyntheticPriceChange: () => void; __setSyntheticHidden: (v: boolean) => void };
  const state = () => page.evaluate(() => (window as unknown as Bridge).__syntheticPriceState());
  await expect(page.getByText('$0.87', { exact: true })).toBeVisible();
  await expect.poll(async () => (await state()).listeners).toBe(1);
  const before = (await state()).reads;
  await page.evaluate(() => (window as unknown as Bridge).__emitSyntheticPriceChange());
  await expect(page.getByText('$1.23', { exact: true })).toBeVisible({ timeout: 3000 });
  expect((await state()).reads).toBe(before + 1);
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as Bridge).__setSyntheticHidden(true));
  const hiddenReads = (await state()).reads;
  await page.evaluate(() => (window as unknown as Bridge).__emitSyntheticPriceChange());
  await page.waitForTimeout(150);
  expect((await state()).reads).toBe(hiddenReads);
  await expect(page.getByText('$1.23', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as Bridge).__setSyntheticHidden(false));
  await expect.poll(async () => (await state()).reads).toBe(before + 2);
  await page.getByLabel('来源', { exact: true }).selectOption('synthetic-b');
  await expect(page.getByLabel('17 Token', { exact: true })).toBeVisible();
  await expect.poll(async () => (await state()).listeners).toBe(1);
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await expect.poll(async () => (await state()).listeners).toBe(1);
  const offscreenReads = (await state()).reads;
  await page.evaluate(() => (window as unknown as Bridge).__emitSyntheticPriceChange());
  await expect.poll(async () => (await state()).reads).toBe(offscreenReads + 1);
  await page.getByRole('button', { name: '总览', exact: true }).click();
  await expect(page.getByLabel('17 Token', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '正在读取统计快照' })).toHaveCount(0);
});

test('heatmap day updates backend date selection while keeping source, grain and independent history', async ({ page }) => {
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '日', exact: true }).click();
  await page.getByLabel('来源', { exact: true }).selectOption('synthetic-b'); await expect(page.getByLabel('17 Token', { exact: true })).toBeVisible();
  type Bridge = { __lastDashboardRequest: () => { filter: { range: { start_ms: number; end_ms: number; timezone: string }; sources: unknown }; grain: string; heatmap_range: { start_ms: number; end_ms: number } } };
  const before = await page.evaluate(() => (window as unknown as Bridge).__lastDashboardRequest());
  const expectedStart = before.heatmap_range.start_ms + 3 * 86_400_000;
  const expectedDate = new Date(expectedStart + 8 * 3_600_000).toISOString().slice(0, 10);
  await page.getByRole('group', { name: '近 26 周每日活动' }).getByRole('button').nth(3).click();
  await expect(page.getByLabel('日期范围')).toHaveValue('custom'); await expect(page.getByTitle('编辑已应用日期')).toHaveText(expectedDate + ' — ' + expectedDate);
  await expect.poll(async () => page.evaluate(() => (window as unknown as Bridge).__lastDashboardRequest().filter.range.start_ms)).toBe(expectedStart);
  const after = await page.evaluate(() => (window as unknown as Bridge).__lastDashboardRequest());
  expect(after.filter.range.end_ms).toBe(expectedStart + 86_400_000); expect(after.filter.sources).toEqual(before.filter.sources);
  expect(after.heatmap_range).toEqual(before.heatmap_range); expect(after.grain).toBe('day');
  await page.getByTitle('编辑已应用日期').click(); await page.setViewportSize({ width: 960, height: 680 });
  await page.screenshot({ path: 'test-results/date-picker-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
test('long custom history changes trend grain to daily without reducing the selected range', async ({ page }) => {
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await page.getByLabel('日期范围').selectOption('custom');
  await page.getByLabel('开始日期').fill('2025-10-01'); await page.getByLabel('结束日期（包含当天）').fill('2026-09-30');
  await page.getByRole('button', { name: '应用日期' }).click();
  await expect(page.getByRole('button', { name: '日', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: '小时', exact: true })).toBeDisabled();
  await expect(page.getByRole('group', { name: '各时间桶可信 Token' }).getByRole('button')).toHaveCount(365);
  await expect(page.getByTitle('编辑已应用日期')).toHaveText('2025-10-01 — 2026-09-30');
});

test('explicit price time changes estimates, preserves token scope and stays fixed across refresh', async ({ page }) => {
  await expect(page.getByText('$0.87', { exact: true })).toBeVisible();
  await page.getByLabel('计价依据').selectOption('specified_time');
  await expect(page.getByText('$2.32', { exact: true })).toBeVisible(); await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await page.getByTitle('编辑明确估价时点（UTC）').click(); await page.getByLabel('估价时点（UTC）').fill('2024-02-29T00:00:00.123');
  await page.screenshot({ path: 'test-results/price-basis-1280.png', fullPage: true });
  await page.getByRole('button', { name: '应用估价时点' }).click();
  await expect(page.getByTitle('编辑明确估价时点（UTC）')).toHaveText('2024-02-29T00:00:00.123Z');
  type Bridge = { __lastDashboardRequest: () => { price_basis: unknown; filter: unknown } };
  await expect.poll(async () => page.evaluate(() => (window as unknown as Bridge).__lastDashboardRequest().price_basis)).toEqual({ mode: 'specified_time', specified_at_ms: Date.parse('2024-02-29T00:00:00.123Z') });
  const before = await page.evaluate(() => (window as unknown as Bridge).__lastDashboardRequest());
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect(page.getByText('$2.32', { exact: true })).toBeVisible();
  const after = await page.evaluate(() => (window as unknown as Bridge).__lastDashboardRequest()); expect(after.price_basis).toEqual(before.price_basis); expect(after.filter).toEqual(before.filter);
  await page.setViewportSize({ width: 960, height: 680 }); await page.getByTitle('编辑明确估价时点（UTC）').click(); await page.screenshot({ path: 'test-results/price-basis-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.getByLabel('估价时点（UTC）').press('Escape'); await expect(page.getByRole('form', { name: '指定估价时点' })).toHaveCount(0);
  await expect(page.getByTitle('编辑明确估价时点（UTC）')).toBeFocused();
  await page.getByRole('button', { name: '重置筛选' }).click(); await expect(page.getByLabel('计价依据')).toHaveValue('event_time'); await expect(page.getByText('$0.87', { exact: true })).toBeVisible();
});

test('global privacy removes overview amounts, original paths and names including titles without hiding tokens', async ({ page }) => {
  await expect(page.getByText('$0.87', { exact: true })).toBeVisible();
  await page.getByLabel('来源', { exact: true }).selectOption('synthetic-a');
  await expect(page.getByText('$0.87', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as { __emitSyntheticPrivacyChange: (value: boolean) => void }).__emitSyntheticPrivacyChange(true));
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await expect(page.locator('.cost-number').first()).toHaveText('已隐藏');
  await expect(page.getByLabel('来源', { exact: true })).toHaveValue('synthetic-a');
  const html = await page.locator('body').evaluate(e => e.innerHTML);
  for (const original of ['$0.87', '0.871234567890123', 'synthetic-qa-a', 'synthetic-ui-session', 'Synthetic QA Project']) expect(html).not.toContain(original);
  await page.evaluate(() => (window as unknown as { __emitSyntheticPrivacyChange: (value: boolean) => void }).__emitSyntheticPrivacyChange(false));
  await expect(page.getByText('$0.87', { exact: true })).toBeVisible();
  await expect(page.getByLabel('来源', { exact: true })).toHaveValue('synthetic-a');
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
});


test('explicit mini navigation imports exact milliseconds and session, resets price/source, and remains independent afterwards', async ({ page }) => {
  await page.getByLabel('日期范围').selectOption('last7'); await page.getByLabel('来源', { exact: true }).selectOption('synthetic-b');
  await page.getByLabel('计价依据').selectOption('specified_time');
  await page.evaluate(() => (window as unknown as { __requestSyntheticMiniStats: () => void }).__requestSyntheticMiniStats());
  await expect(page.getByLabel('从小窗带入的精确范围')).toContainText('2024/02/29');
  await expect(page.getByLabel('从小窗带入的精确范围')).toContainText('.123');
  await expect(page.getByLabel('日期范围')).toHaveCount(0);
  await expect(page.getByLabel('来源', { exact: true })).toHaveValue(''); await expect(page.getByLabel('计价依据')).toHaveValue('event_time');
  await expect(page.getByRole('combobox', { name: '会话', exact: true })).toContainText('固定会话（小窗）');
  await expect.poll(async () => page.evaluate(() => (window as unknown as { __lastDashboardRequest: () => unknown }).__lastDashboardRequest())).toMatchObject({ filter: { range: { start_ms: 1709179200123, end_ms: 1709203200457, timezone: 'UTC' }, sources: { kind: 'all' }, sessions: { kind: 'ids', ids: ['synthetic-fixed'], include_unknown: false } }, price_basis: { mode: 'event_time' } });
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect(page.getByLabel('从小窗带入的精确范围')).toContainText('.123');
  await page.screenshot({ path: 'test-results/mini-stats-main-1280.png', fullPage: true });
  await page.getByRole('button', { name: '改用主窗口日期' }).click(); await expect(page.getByLabel('日期范围')).toHaveValue('last7');
  await page.evaluate(() => (window as unknown as { __setSyntheticHidden: (v: boolean) => void }).__setSyntheticHidden(false));
  await expect(page.getByLabel('从小窗带入的精确范围')).toHaveCount(0);
  await page.evaluate(() => (window as unknown as { __requestSyntheticMiniStats: (v: boolean, id: string) => void }).__requestSyntheticMiniStats(true, 'synthetic-mini-open-2'));
  await expect(page.getByRole('combobox', { name: '会话', exact: true })).toContainText('全部会话');
  await page.getByRole('button', { name: '重置筛选' }).click(); await expect(page.getByLabel('日期范围')).toHaveValue('today');
  await expect(page.getByLabel('从小窗带入的精确范围')).toHaveCount(0);
});


test('overview quota is account-scoped, actual-period aware and hidden by shared privacy', async ({ page }) => {
  const card = page.getByRole('region', { name: '账户额度总览' });
  await expect(card).toContainText('未连接');
  await page.evaluate(value => (window as unknown as { __setSyntheticQuota(v: unknown): void }).__setSyntheticQuota(value), syntheticQuota());
  await expect(card).toContainText('周额度剩余 14%');
  await expect(card).toContainText('2 小时额度剩余 0%');
  await expect(card).toContainText('未知周期剩余 —');
  await card.getByRole('button', { name: '刷新账户额度' }).click(); await expect(card).toContainText('额度读取正在进行。');
  await page.getByLabel('来源', { exact: true }).selectOption('synthetic-b');
  await expect(page.getByLabel('17 Token', { exact: true })).toBeVisible();
  await expect(card).toContainText('周额度剩余 14%');
  await expect(card).toContainText('SYNTHETIC ACCOUNT BUCKET');
  await page.screenshot({ path: 'test-results/overview-account-quota.png' });
  await page.evaluate(() => (window as unknown as { __emitSyntheticPrivacyChange(v: boolean): void }).__emitSyntheticPrivacyChange(true));
  await expect(card).toContainText('隐私模式已隐藏账户额度');
  await expect(card).not.toContainText('14%'); await expect(card.getByRole('progressbar')).toHaveCount(0);
});


test('retained navigation orders taskbar settings against late stats with exact revisions and ignores repeated visibility', async ({ page }) => {
  type QA = { __taskbarNavigationQA: { settings(): void; hold(): void; release(): void; revision(value: string): void; listeners(): number }; __requestSyntheticMiniStats(all?: boolean, id?: string): void; __setSyntheticHidden(value: boolean): void };
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__taskbarNavigationQA.listeners())).toBe(1);
  await page.evaluate(() => { const qa=(window as unknown as QA); qa.__taskbarNavigationQA.revision('9007199254740993'); qa.__taskbarNavigationQA.hold(); qa.__requestSyntheticMiniStats(false,'old-held-stats'); });
  await page.evaluate(() => (window as unknown as QA).__taskbarNavigationQA.settings());
  await expect(page.getByRole('tab', { name:'任务栏显示', exact:true })).toHaveAttribute('aria-selected','true');
  await page.evaluate(() => (window as unknown as QA).__taskbarNavigationQA.release());
  await expect(page.getByRole('heading',{name:'设置',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'模型',exact:true}).click();
  await page.evaluate(() => {const qa=(window as unknown as QA);qa.__setSyntheticHidden(true);qa.__setSyntheticHidden(false);});
  await expect(page.getByRole('heading',{name:'模型',exact:true})).toBeVisible();
  await page.evaluate(() => { const qa=(window as unknown as QA);qa.__taskbarNavigationQA.revision('9007199254740992');qa.__taskbarNavigationQA.settings(); });
  await expect(page.getByRole('heading',{name:'模型',exact:true})).toBeVisible();
  await page.evaluate(() => { const qa=(window as unknown as QA);qa.__taskbarNavigationQA.revision('9007199254740995');qa.__requestSyntheticMiniStats(false,'new-shared-stats'); });
  await expect(page.getByLabel('从小窗带入的精确范围')).toContainText('.123');
  await expect(page.getByRole('heading',{name:'总览',exact:true})).toBeVisible();
});

// Cache writes are an included component, not another stacked total.
test('cache-write quantity is shown without changing the trusted total', async ({ page }) => {
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  const measure = page.locator('.breakdown-measures .measure').filter({ has: page.locator('dt', { hasText: '缓存写入（输入包含项）' }) });
  await expect(measure.locator('dd')).toContainText('12,000');
  await expect(page.getByText('非缓存命中输入', { exact: true })).toBeVisible();
  await page.screenshot({ path: 'test-results/cache-write-overview.png', fullPage: true });
});
