import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';

test.beforeEach(async ({ page }) => {
  // Explicit synthetic IPC QA data; not imported by the desktop application.
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    type Query = { filter: { range: { start_ms: number; timezone: string }; sources: { ids?: string[] }; sessions: { ids?: string[] } }; sort: string; page_size: number };
    let redacted = false;
    let serial = 0, bad: 'expired' | 'mismatch' | null = null, revision = '3', amount = '9.007199254740993', callbackId = 0, eventId = 0;
    const calls: { command: string; request: unknown }[] = [];
    const cursors = new Map<string, { query: string; offset: number; snapshot: string }>();
    const callbacks = new Map<number,(event: unknown) => void>(); const listeners = new Map<number,{ event: string; handler: number }>();
    const measure = { value: null, covered_total_tokens: '0', complete: false };
    const tokens = (total: string, events = '53') => ({ total_tokens: total, input_total: measure, cached_input: measure, noncached_input: measure, output_total: measure, reasoning_output: measure, cache_write_input: { value: null, covered_total_tokens: '0', complete: false }, session_count: events === '0' ? '0' : '1', usage_event_count: events, reliable_turn_count: null, reliable_turns_complete: false });
    const coverage = { state: 'unknown', pending_observation_count: '0', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '0', source_issues: [], format_issues: [], breakdown_complete: false };
    const price = (total: string) => ({ redacted: false, basis: { mode: 'event_time' }, currencies: total === '0' ? [] : [{ currency: 'USD', estimated_cost: amount, priced_total_tokens: String(BigInt(total)-1n) }, { currency: 'EUR', estimated_cost: '0.000000000000000', priced_total_tokens: '0' }], priced_total_tokens: total === '0' ? '0' : String(BigInt(total)-1n), unpriced_total_tokens: total === '0' ? '0' : '1', reasons: total === '0' ? [] : [{ code: 'unknown_model', total_tokens: '1', event_count: '1' }], calculating: false });
    Object.assign(window, { isTauri: true,
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: (_event: string, id: number) => { const listener = listeners.get(id); if (listener) callbacks.delete(listener.handler); listeners.delete(id); } },
      __TAURI_INTERNALS__: { transformCallback: (callback: (event: unknown) => void) => { callbacks.set(++callbackId, callback); return callbackId; }, invoke: async (command: string, args: Record<string, unknown>) => {
        calls.push({ command, request: args.request }); const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data });
      if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
        if (command === 'plugin:event|listen') { listeners.set(++eventId, { event: String(args.event), handler: Number(args.handler) }); return eventId; }
        if (command === 'plugin:event|unlisten') return null;
        if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
        if (command === 'get_sources') return response({ settings_revision: '1', sources: [] });
        if (command === 'get_dashboard_bundle') throw new Error('Synthetic bridge supplies events only');
        if (command === 'get_price_rules') return response({ price_revision: revision, rules: [], aliases: [] });
        if (command === 'close_query_snapshot') return response(null);
        if (command === 'query_sessions') { const query = (args.request as { query: Query }).query; return response({ meta: { snapshot_id: 'synthetic-drill', data_revision: '7', price_revision: revision, generated_at_ms: query.filter.range.start_ms+1000, parser_versions: [], accounting_versions: [], display_timezone: query.filter.range.timezone }, summary: tokens('0','0'), pricing: price('0'), coverage, sessions: [], next_cursor: null }); }
        if (command === 'query_usage_events') {
          const { query, cursor } = args.request as { query: Query; cursor: string | null };
          if (cursor && bad === 'expired') throw { code: 'SNAPSHOT_EXPIRED' };
          const saved = cursor ? cursors.get(cursor) : null;
          if (cursor && (!saved || saved.query !== JSON.stringify(query))) throw { code: 'CURSOR_INVALID' };
          const snapshot = saved?.snapshot ?? `synthetic-events-${++serial}`; const offset = saved?.offset ?? 0;
          const events = Array.from({ length: 53 }, (_, index) => {
            const total = index === 0 ? '9007199254740993' : index === 1 ? '0' : '1';
            const vector = { input_total: index === 2 ? null : total, cached_input: index === 2 ? null : '0', output_total: index === 2 ? null : '0', reasoning_output: null, cache_write_input: index === 2 || index === 5 ? null : index === 3 ? '1' : '0', reported_total: total };
            return { event_id: `synthetic-event-${index}`, session_key: 'synthetic-session', session_display_name: 'Synthetic 会话', occurred_at_ms: query.filter.range.start_ms+(53-index)*1000, model: index === 2 ? null : index === 0 ? 'Synthetic Model' : index === 1 ? 'Synthetic EUR Model' : 'Synthetic Zero Rate Model', provider: index === 2 ? null : 'Synthetic Provider', project_id: index === 2 ? null : 'project', project_display_name: index === 2 ? null : 'Synthetic Project', source_ids: ['synthetic-source','synthetic-mirror'], turn_id: null, total_tokens: total, usage: vector, raw_last: index === 0 ? { ...vector, input_total: '-1' } : null, raw_cumulative: index === 0 ? { ...vector, input_total: '9007199254741093', reported_total: '9007199254741093' } : null, request_input: index === 0 ? { input_tokens: '272001', binding: 'different_consumption' } : index === 1 ? { input_tokens: '0', binding: 'full_request' } : null, matched_price: index === 0 ? { rule_id: 'synthetic-rule', model_exact: 'Synthetic Canonical Model', introduced_revision: '2', basis: { kind: 'custom_rule', source_specific: true } } : index === 1 ? { rule_id: 'synthetic-eur-rule', model_exact: 'Synthetic EUR Model', introduced_revision: '1', basis: { kind: 'offline_standard_reference', catalog_id: 'openai-text-synthetic', reference_basis: 'global_api_reference' } } : index === 5 ? { rule_id: 'synthetic-zero-rate-rule', model_exact: 'Synthetic Zero Rate Model', introduced_revision: '1', basis: { kind: 'offline_assumed_reference', catalog_id: 'openai-text-synthetic', context: 'short', context_assumed: true, cache_write_assumed_zero: true, reference_basis: 'global_api_reference' } } : index === 3 ? { rule_id: 'synthetic-incomplete-rule', model_exact: 'Synthetic Zero Rate Model', introduced_revision: '1', basis: { kind: 'custom_rule', source_specific: false } } : null, calculation_method: 'last_with_baseline', quality_flags: ['confirmed'], price: index === 4 ? { status: 'unpriced', reason: 'incomplete_pricing_conditions' } : index === 3 ? { status: 'unpriced', reason: 'insufficient_usage' } : index === 2 ? { status: 'unpriced', reason: 'unknown_model' } : { status: 'priced', rule_id: index === 0 ? 'synthetic-rule' : index === 1 ? 'synthetic-eur-rule' : 'synthetic-zero-rate-rule', currency: index === 1 ? 'EUR' : 'USD', cost_atoms: index === 0 ? revision === '3' ? '9007199254740993' : '18014398509481986' : '0', estimated_cost: index === 0 ? amount : '0.000000000000000' }, parser_version: 'synthetic-parser', accounting_version: 'synthetic-accounting' };
          });
          const next = offset+query.page_size < events.length ? String(++serial).padStart(151,'a') : null;
          if (next) cursors.set(next, { query: JSON.stringify(query), offset: offset+query.page_size, snapshot });
          if (redacted) for (const event of events) { event.price = { status: 'redacted' } as typeof event.price; event.matched_price = null; }
          const summaryPrice = price('9007199254741044');
          if (redacted) { summaryPrice.redacted = true; summaryPrice.currencies = summaryPrice.currencies.map(value => ({ ...value, estimated_cost: null as unknown as string })); summaryPrice.reasons = []; }
          return response({ meta: { snapshot_id: snapshot, data_revision: cursor && bad === 'mismatch' ? '8' : '7', price_revision: revision, generated_at_ms: query.filter.range.start_ms+1000, parser_versions: ['synthetic-parser'], accounting_versions: ['synthetic-accounting'], display_timezone: query.filter.range.timezone }, summary: tokens('9007199254741044'), pricing: summaryPrice, coverage, events: events.slice(offset,offset+query.page_size), next_cursor: next });
        }
        throw new Error(`Unexpected synthetic command ${command}`);
      } }, __redactEventPrices: () => { redacted = true; }, __eventCalls: () => calls, __badEventPage: (value: typeof bad) => { bad = value; }, __eventListenerCount: () => [...listeners.values()].filter(listener => listener.event === 'price_rules_changed').length,
      __emitEventPriceChange: () => { revision = '4'; amount = '18.014398509481986'; for (const [id,listener] of listeners) if (listener.event === 'price_rules_changed') callbacks.get(listener.handler)?.({ event: listener.event, id, payload: { price_revision: revision, all_models: true } }); } });
  });
  await page.goto('/');
  await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '明细', exact: true }).click();
});

test('events retain precise vectors, unknown values, true zero price and stable pagination', async ({ page }) => {
  await expect(page.locator('.event-table>tbody>tr')).toHaveCount(50);
  await expect(page.locator('.event-table>tbody>tr').first().locator('td[title="9,007,199,254,740,993"]')).toBeVisible();
  await expect(page.locator('.event-table>tbody>tr').nth(1).getByText('EUR 0.00', { exact: true })).toBeVisible();
  await expect(page.locator('.event-table>tbody>tr').nth(2)).toContainText('未知项目');
  await expect(page.locator('.event-table>tbody>tr').nth(2).getByText('未计价', { exact: true })).toBeVisible();
  await page.screenshot({ path: 'test-results/events-1280.png' });
  await page.getByRole('button', { name: '查看 synthetic-event-0 核算依据', exact: true }).click();
  const evidence = page.getByLabel('synthetic-event-0 核算依据', { exact: true });
  await expect(evidence).toBeVisible(); await expect(evidence.locator('.raw-negative')).toHaveText('-1');
  await expect(evidence).toContainText('9,007,199,254,741,093');
  await expect(evidence).toContainText('USD 9.007199254740993');
  await expect(evidence).toContainText('synthetic-rule'); await expect(evidence).toContainText('最后用量（累计基线已核对）');
  await page.screenshot({ path: 'test-results/event-evidence-1280.png' });
  await page.getByRole('button', { name: '重新查询', exact: true }).click(); await expect(page.getByRole('status').filter({ hasText: '续页租约' })).toHaveCount(0);
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(page.locator('.event-table>tbody>tr')).toHaveCount(3); await expect(page.getByRole('button', { name: '下一页', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '上一页', exact: true }).click();
  await expect(page.locator('.event-table>tbody>tr')).toHaveCount(50); // Explicit replacement clears old snapshot evidence.
  await page.setViewportSize({ width: 960, height: 680 }); await page.getByRole('heading', { name: '明细', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/events-960.png' }); expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.locator('.event-table>tbody>tr').first().getByRole('button', { name: 'Synthetic 会话', exact: true }).click();
  await expect(page.getByRole('heading', { name: '会话', exact: true })).toBeVisible();
  await expect(page.getByRole('combobox', { name: '会话', exact: true })).toContainText('Synthetic 会话');
});

test('request input stays distinct from consumption, shows true zero and unknown, and never claims mode matching', async ({ page }, testInfo) => {
  await page.getByRole('button', { name: '查看 synthetic-event-0 核算依据', exact: true }).click();
  const evidence = page.getByLabel('synthetic-event-0 核算依据', { exact: true });
  await expect(evidence.getByText('272,001 Token', { exact: true })).toBeVisible();
  await expect(evidence.getByText('与本笔增量不同，不能据此选档', { exact: true })).toBeVisible();
  await expect(evidence.getByText('尚未采集，不能据此确认完整计费', { exact: true })).toBeVisible();
  await evidence.screenshot({ path: testInfo.outputPath('request-input-dark.png') });
  await page.getByRole('button', { name: '查看 synthetic-event-1 核算依据', exact: true }).click();
  const zero = page.getByLabel('synthetic-event-1 核算依据', { exact: true });
  await expect(zero.getByText('0 Token', { exact: true })).toBeVisible();
  await expect(zero.getByText('对应完整请求用量', { exact: true })).toBeVisible();
  await page.setViewportSize({ width: 960, height: 860 }); await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  const bounds = await zero.evaluate(element => {
    const panel = element.getBoundingClientRect();
    const viewport = element.closest('.event-table-wrap')!.getBoundingClientRect();
    const details = element.querySelector('dl')!.getBoundingClientRect();
    return { panelRight: panel.right, viewportRight: viewport.right, detailsRight: details.right };
  });
  expect(bounds.panelRight).toBeLessThanOrEqual(bounds.viewportRight);
  expect(bounds.detailsRight).toBeLessThanOrEqual(bounds.viewportRight);
  await zero.screenshot({ path: testInfo.outputPath('request-input-zero-light-960.png') });
  await page.getByRole('button', { name: '查看 synthetic-event-2 核算依据', exact: true }).click();
  await expect(page.getByLabel('synthetic-event-2 核算依据', { exact: true }).getByText('未知（缺少可核对的响应记录）', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '查看 synthetic-event-4 核算依据', exact: true }).click();
  await expect(page.getByLabel('synthetic-event-4 核算依据', { exact: true })).toContainText('计费条件尚未完整确认');
  await page.getByLabel('synthetic-event-4 核算依据', { exact: true }).screenshot({ path: testInfo.outputPath('conditional-unpriced-light-960.png') });
});

test('expired or mismatched continuations never append fresh data to a frozen event page', async ({ page }) => {
  type Bridge = { __badEventPage: (value: 'expired' | 'mismatch' | null) => void };
  await expect(page.locator('.event-table>tbody>tr')).toHaveCount(50);
  await page.getByRole('button', { name: '重新查询', exact: true }).click(); await expect(page.getByRole('status').filter({ hasText: '续页租约' })).toHaveCount(0);
  await page.evaluate(() => (window as unknown as Bridge).__badEventPage('mismatch'));
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('明细列表已变化');
  await expect(page.locator('.event-table>tbody>tr')).toHaveCount(50); await expect(page.getByRole('button', { name: '下一页', exact: true })).toBeDisabled();
  await page.evaluate(() => (window as unknown as Bridge).__badEventPage(null)); await page.getByRole('button', { name: '重新查询', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await page.evaluate(() => (window as unknown as Bridge).__badEventPage('expired')); await page.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('请重新查询'); await expect(page.locator('.event-table>tbody>tr')).toHaveCount(50);
});

test('price notification replaces the whole snapshot and navigation keeps bounded listeners but releases capabilities', async ({ page }) => {
  type Bridge = { __eventListenerCount: () => number; __emitEventPriceChange: () => void; __eventCalls: () => { command: string; request: { kind?: string; query?: { sort: string } } }[] };
  await expect(page.locator('.event-table>tbody>tr')).toHaveCount(50);
  await expect.poll(async () => page.evaluate(() => (window as unknown as Bridge).__eventListenerCount())).toBe(1);
  await page.evaluate(() => (window as unknown as Bridge).__emitEventPriceChange());
  await expect(page.locator('.event-table>tbody>tr').first().getByText('$18.01', { exact: true })).toBeVisible({ timeout: 3000 });
  await page.getByLabel('明细排序').selectOption('total_desc'); await expect(page.locator('.event-table>tbody>tr')).toHaveCount(50);
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await expect.poll(async () => page.evaluate(() => (window as unknown as Bridge).__eventListenerCount())).toBe(1);
  await expect(page.locator('.event-table')).toHaveCount(0);
  await expect.poll(async () => (await page.evaluate(() => (window as unknown as Bridge).__eventCalls())).filter(c => c.command === 'close_query_snapshot' && c.request.kind === 'usage_events').length).toBeGreaterThanOrEqual(3);
  const calls = await page.evaluate(() => (window as unknown as Bridge).__eventCalls());
  expect(calls.filter(c => c.command === 'close_query_snapshot' && c.request.kind === 'usage_events').length).toBeGreaterThanOrEqual(3);
});

test('display-only redacted price hides values, rules and tooltip amounts while preserving exact tokens', async ({ page }) => {
  await expect(page.locator('.event-table>tbody>tr')).toHaveCount(50);
  await page.evaluate(() => (window as unknown as { __redactEventPrices: () => void }).__redactEventPrices());
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect(page.locator('.event-table>tbody>tr').first()).toContainText('已隐藏');
  await expect(page.locator('.event-table>tbody>tr').first().locator('td[title="9,007,199,254,740,993"]')).toBeVisible();
  await page.locator('.event-table>tbody>tr').first().getByRole('button', { name: '查看 synthetic-event-0 核算依据', exact: true }).click();
  await expect(page.locator('.event-evidence')).toContainText('已隐藏');
  await expect(page.locator('.event-evidence')).not.toContainText('synthetic-rule');
  await expect(page.locator('.event-evidence')).not.toContainText('9.007199254740993');
  expect(await page.locator('[title*="9.007199254740993"]').count()).toBe(0);
});

test('cache-write evidence distinguishes unknown, zero, and positive included counts', async ({ page }) => {
  await page.getByRole('button', { name: '明细', exact: true }).click();
  for (const [index, expected] of [[1, '0'], [2, '—'], [3, '1']] as const) {
    await page.getByRole('button', { name: `查看 synthetic-event-${index} 核算依据`, exact: true }).click();
    const row = page.getByLabel(`synthetic-event-${index} 核算依据`, { exact: true }).getByRole('row').filter({ hasText: '缓存写入（输入包含项）' });
    await expect(row.getByRole('cell').first()).toHaveText(expected);
  }
  await expect(page.getByLabel('synthetic-event-3 核算依据', { exact: true })).toContainText('未计价');
  await page.getByLabel('synthetic-event-3 核算依据', { exact: true }).screenshot({ path: 'test-results/cache-write-evidence-panel.png' });
  await page.screenshot({ path: 'test-results/cache-write-evidence.png' });
});

test('same-snapshot pricing basis distinguishes custom scope, Standard assumptions and missing conditions', async ({ page }, testInfo) => {
  await page.getByRole('button', { name: '查看 synthetic-event-0 核算依据', exact: true }).click();
  let evidence = page.getByLabel('synthetic-event-0 核算依据', { exact: true });
  await expect(evidence).toContainText('来源专用自定义规则');
  await expect(evidence).toContainText('Synthetic Canonical Model');
  await expect(evidence).toContainText('该规则不证明请求实际模式或地区条件');
  await evidence.screenshot({ path: testInfo.outputPath('matched-custom-dark.png') });
  await page.getByRole('button', { name: '查看 synthetic-event-1 核算依据', exact: true }).click();
  evidence = page.getByLabel('synthetic-event-1 核算依据', { exact: true });
  await expect(evidence).toContainText('离线 Standard 平价参考');
  await expect(evidence).toContainText('请求实际模式未知');
  await expect(evidence).toContainText('openai-text-synthetic');
  await expect(evidence).not.toContainText('响应确认');
  await evidence.screenshot({ path: testInfo.outputPath('matched-standard-dark.png') });
  await page.getByRole('button', { name: '查看 synthetic-event-3 核算依据', exact: true }).click();
  evidence = page.getByLabel('synthetic-event-3 核算依据', { exact: true });
  await expect(evidence).toContainText('未计价');
  await expect(evidence).toContainText('自定义规则 · 全部来源');
  await expect(evidence).toContainText('synthetic-incomplete-rule');
  await page.getByRole('button', { name: '查看 synthetic-event-4 核算依据', exact: true }).click();
  evidence = page.getByLabel('synthetic-event-4 核算依据', { exact: true });
  await expect(evidence).toContainText('尚无可确认的匹配依据');
  await expect(evidence).not.toContainText('响应确认');
});


test('reference estimate shows mode, context and cache write assumptions without confirming actual mode', async ({ page }, testInfo) => {
  await page.getByRole('button', { name: '查看 synthetic-event-5 核算依据', exact: true }).click();
  const evidence = page.getByLabel('synthetic-event-5 核算依据', { exact: true });
  await expect(evidence).toContainText('Standard 参考估算 · 短上下文（假设）');
  await expect(evidence).toContainText('缓存写入量未知，估算暂按 0');
  await expect(evidence).toContainText('尚未采集，不能据此确认完整计费');
  await expect(evidence).not.toContainText('响应确认 Standard');
  await page.setViewportSize({ width: 960, height: 860 });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await evidence.screenshot({ path: testInfo.outputPath('assumed-reference-light-960.png') });
});
