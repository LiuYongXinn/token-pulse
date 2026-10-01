import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  // Explicit synthetic DTO bridge, never loaded by production code.
  await page.addInitScript(() => {
    type Query = { filter: { range: { start_ms: number; end_ms: number; timezone: string }; sources: { ids?: string[] }; sessions: { ids?: string[] } }; price_basis: unknown; sort: string; page_size: number };
    let id = 0, expired = false, defer = false, release: (() => void) | null = null;
    const calls: { command: string; args: Record<string, unknown> }[] = [];
    const cursors = new Map<string, { query: string; offset: number; snapshot: string }>();
    const measure = { value: null, covered_total_tokens: '0', complete: false };
    const tokens = (total: string, count = '1') => ({ total_tokens: total, input_total: measure, cached_input: measure, noncached_input: measure, output_total: measure, reasoning_output: measure, session_count: count, usage_event_count: '2', reliable_turn_count: '1', reliable_turns_complete: false });
    const price = (total: string) => ({ redacted: false, basis: { mode: 'event_time' }, currencies: [], priced_total_tokens: '0', unpriced_total_tokens: total, reasons: total === '0' ? [] : [{ code: 'insufficient_usage', total_tokens: total, event_count: '2' }], calculating: false });
    const coverage = { state: 'partial', pending_observation_count: '1', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '0', source_issues: [], format_issues: [], breakdown_complete: false };
    Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: { invoke: async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, data });
      if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
      if (command === 'get_sources') return response({ settings_revision: '1', sources: [{ source_id: 'empty', root_path: 'Synthetic Empty Source', origin: 'custom', enabled: true, removed: false, readability: 'readable', capabilities: { physical_identity: 'available', byte_seek: 'available', watcher: 'available', polling_required: true }, last_scan_at_ms: 1000, last_success_at_ms: 1000, error: null }] });
      if (command === 'get_dashboard_bundle') throw new Error('Synthetic bridge supplies sessions only');
      if (command === 'get_price_rules') return response({ price_revision: '3', rules: [], aliases: [] });
      if (command === 'close_query_snapshot') return response(null);
      if (command === 'query_sessions') {
        const { query, cursor } = args.request as { query: Query; cursor: string | null };
        if (cursor !== null && expired) throw { code: 'SNAPSHOT_EXPIRED' };
        const stored = cursor === null ? null : cursors.get(cursor);
        if (cursor !== null && (!stored || stored.query !== JSON.stringify(query))) throw { code: 'CURSOR_INVALID' };
        const snapshot = stored?.snapshot ?? `synthetic-session-snapshot-${++id}`;
        const offset = stored?.offset ?? 0;
        const empty = query.filter.sources.ids?.includes('empty') ?? false;
        let sessions = empty ? [] : Array.from({ length: 55 }, (_, index) => {
          const key = `synthetic-session-${index}`;
          const total = index === 0 ? '9007199254740993' : String(100 - index);
          return { session_key: key, display_name: `Synthetic 会话 ${index}`, latest_at_ms: query.filter.range.start_ms + (55 - index) * 1000, latest_model: index === 1 ? null : 'Synthetic Model', latest_project_id: index === 1 ? null : 'project', latest_project_name: index === 1 ? null : 'Synthetic Project', parent_key: index === 0 ? 'synthetic-session-1' : null, parent_display_name: index === 0 ? 'Synthetic 会话 1' : null, parent_provider_id: index === 0 ? 'parent-provider' : index === 1 ? 'unresolved-parent' : null, child_count: index === 1 ? '1' : '0', summary: tokens(total), pricing: price(total), coverage, latest_context: { context_tokens: index === 0 ? '9007199254740993' : null, model_context_window: null, percentage: null, observed_at_ms: index === 0 ? query.filter.range.end_ms + 12000 : null, quality: index === 0 ? 'confirmed' : 'unknown' } };
        });
        if (query.filter.sessions.ids) sessions = sessions.filter(s => query.filter.sessions.ids!.includes(s.session_key));
        const total = sessions.reduce((sum, s) => sum + BigInt(s.summary.total_tokens), 0n).toString();
        const next = offset + query.page_size < sessions.length ? `${++id}`.padStart(151, 'a') : null;
        if (next !== null) cursors.set(next, { query: JSON.stringify(query), offset: offset + query.page_size, snapshot });
        const data = { meta: { snapshot_id: snapshot, data_revision: '7', price_revision: '3', generated_at_ms: query.filter.range.start_ms + 1000, parser_versions: ['synthetic'], accounting_versions: ['synthetic'], display_timezone: query.filter.range.timezone }, summary: tokens(total, String(sessions.length)), pricing: price(total), coverage, sessions: sessions.slice(offset, offset + query.page_size), next_cursor: next };
        if (defer) { defer = false; await new Promise<void>(resolve => { release = resolve; }); }
        return response(data);
      }
      throw new Error(`Unexpected synthetic command ${command}`);
    } }, __syntheticSessionCalls: () => calls, __expireSyntheticSessions: () => { expired = true; }, __resetSyntheticSessions: () => { expired = false; }, __deferSyntheticSessions: () => { defer = true; }, __releaseSyntheticSessions: () => { release?.(); release = null; } });
  });
  await page.goto('/');
  await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '会话', exact: true }).click();
});

test('sessions show exact consumption and independent context, stable pages and an accessible drawer', async ({ page }) => {
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await expect(page.locator('.session-table tbody tr').first().locator('td.numeric[title="9,007,199,254,740,993"]')).toBeVisible();
  await expect(page.locator('.session-table tbody tr').nth(1)).toContainText('未知项目');
  await expect(page.locator('.session-table tbody tr').nth(1)).toContainText('父会话尚未解析');
  await expect(page.locator('.session-table tbody tr').first().locator('td').nth(4)).toContainText('2 / 1');
  await page.screenshot({ path: 'test-results/sessions-1280.png' });
  const trigger = page.getByRole('button', { name: 'Synthetic 会话 0', exact: true });
  await trigger.click();
  const drawer = page.getByRole('dialog', { name: 'Synthetic 会话 0', exact: true });
  await expect(drawer).toBeVisible();
  await expect(drawer.getByRole('heading', { name: '所选范围累计消耗' })).toBeVisible();
  await expect(drawer.getByRole('heading', { name: '最近请求上下文' })).toBeVisible();
  await expect(drawer.getByText('窗口容量未知', { exact: true })).toBeVisible();
  await expect(drawer).toContainText('最近上下文不表示所选日期内累计消费');
  await expect(page.getByRole('button', { name: '关闭会话详情' })).toBeFocused();
  expect(await page.locator('.workspace').evaluate(element => (element as HTMLElement).inert)).toBe(true);
  await page.screenshot({ path: 'test-results/session-drawer-top-1280.png' });
  await page.keyboard.press('Shift+Tab');
  await expect(drawer.getByRole('button', { name: '在主窗口筛选此会话' })).toBeFocused();
  await page.screenshot({ path: 'test-results/session-drawer-1280.png' });
  await page.keyboard.press('Escape');
  await expect(drawer).toHaveCount(0); await expect(trigger).toBeFocused();
  expect(await page.locator('.workspace').evaluate(element => (element as HTMLElement).inert)).toBe(false);
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(page.locator('.session-table tbody tr')).toHaveCount(5);
  await expect(page.getByRole('button', { name: '下一页', exact: true })).toBeDisabled();
  await expect(page.locator('.session-pagination')).toContainText('第 2 页 / 2 页');
  await page.getByRole('button', { name: '上一页', exact: true }).click();
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.setViewportSize({ width: 960, height: 680 });
  await page.getByRole('heading', { name: '会话', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/sessions-960.png' });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await trigger.click();
  await page.getByRole('dialog').getByRole('button', { name: 'Synthetic 会话 1', exact: true }).click();
  await expect(page.getByRole('combobox', { name: '会话', exact: true })).toContainText('Synthetic 会话 1');
  await expect(page.locator('.session-table tbody tr')).toHaveCount(1);
  await expect(page.locator('.session-table tbody tr')).toContainText('Synthetic 会话 1');
});

test('expired pages preserve the frozen page and explicit requery replaces it without mixing results', async ({ page }) => {
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.evaluate(() => (window as unknown as { __expireSyntheticSessions: () => void }).__expireSyntheticSessions());
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('查询快照已过期');
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await expect(page.getByRole('button', { name: '下一页', exact: true })).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __resetSyntheticSessions: () => void }).__resetSyntheticSessions());
  await page.getByRole('button', { name: '重新查询', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await expect(page.getByRole('button', { name: '下一页', exact: true })).toBeEnabled();
});

test('sort and scope changes serialize release, clear obsolete values and clean up late results on navigation', async ({ page }) => {
  type Calls = { command: string; args: { request?: { kind?: string; query: { sort: string; filter: { sources: unknown } }; request?: { query: { sort: string }; cursor: string }; cursor: string | null } } }[];
  type Bridge = { __syntheticSessionCalls: () => Calls; __deferSyntheticSessions: () => void; __releaseSyntheticSessions: () => void };
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.getByLabel('会话排序').selectOption('total_desc');
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  const calls = await page.evaluate(() => (window as unknown as Bridge).__syntheticSessionCalls());
  const closed = calls.findIndex(c => c.command === 'close_query_snapshot' && c.args.request?.kind === 'sessions');
  const sorted = calls.findIndex(c => c.command === 'query_sessions' && c.args.request?.query.sort === 'total_desc');
  expect(closed).toBeGreaterThanOrEqual(0); expect(sorted).toBeGreaterThan(closed);
  expect(calls[closed].args.request?.request?.query.sort).toBe('latest_desc');
  await page.getByLabel('来源', { exact: true }).selectOption('empty');
  await expect(page.getByRole('heading', { name: '当前筛选暂无消费会话' })).toBeVisible();
  await expect(page.locator('.session-table')).toHaveCount(0);
  await page.evaluate(() => (window as unknown as Bridge).__deferSyntheticSessions());
  await page.getByLabel('来源', { exact: true }).selectOption('');
  await expect(page.getByRole('heading', { name: '正在读取会话快照' })).toBeVisible();
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.evaluate(() => (window as unknown as Bridge).__releaseSyntheticSessions());
  await expect.poll(async () => (await page.evaluate(() => (window as unknown as Bridge).__syntheticSessionCalls())).filter(c => c.command === 'close_query_snapshot').length).toBeGreaterThanOrEqual(3);
  await expect(page.locator('.session-table')).toHaveCount(0);
});
