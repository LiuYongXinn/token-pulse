import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';

test.beforeEach(async ({ page }) => {
  // Explicit synthetic browser QA bridge. No production demo fallback.
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    const calls: { command: string; request: unknown }[] = [];
    let serial = 0, expired = false, deferSlow = false, releaseSlow: (() => void) | null = null;
    const leases = new Map<string, { query: unknown; meta: unknown }>();
    const measure = { value: null, covered_total_tokens: '0', complete: false };
    const totals = { total_tokens: '0', input_total: measure, cached_input: measure, noncached_input: measure, output_total: measure, reasoning_output: measure, cache_write_input: { value: null, covered_total_tokens: '0', complete: false }, session_count: '0', usage_event_count: '0', reliable_turn_count: null, reliable_turns_complete: false };
    const coverage = { state: 'unknown', pending_observation_count: '0', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '0', source_issues: [], format_issues: [], breakdown_complete: false };
    const pricing = { redacted: false, basis: { mode: 'event_time' }, currencies: [], priced_total_tokens: '0', unpriced_total_tokens: '0', reasons: [], calculating: false };
    Object.assign(window, { isTauri: true, __facetCalls: calls, __expireFacet: () => { expired = true; }, __deferFacet: () => { deferSlow = true; }, __releaseFacet: () => { releaseSlow?.(); }, __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} }, __TAURI_INTERNALS__: { transformCallback: () => 0, invoke: async (command: string, args: Record<string, unknown>) => {
      if (command === 'plugin:event|listen' || command === 'plugin:event|unlisten') return 0;
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data });
      if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
      if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic-test', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
      if (command === 'get_sources') return response({ settings_revision: '1', sources: [] });
      const meta = { snapshot_id: String(args.requestId), data_revision: '7', price_revision: '3', generated_at_ms: 1000, parser_versions: [], accounting_versions: [], display_timezone: 'UTC' };
      if (command === 'get_dashboard_bundle') return response({ meta, summary: totals, pricing, coverage, series: [], heatmap: [], recent_sessions: [] });
      if (command === 'get_grouped_usage') { calls.push({ command, request: args.request }); return response({ meta, summary: totals, pricing, coverage, total_group_count: '0', truncated: false, groups: [] }); }
      if (command === 'close_query_snapshot') {
        const request = args.request as { request: { cursor: string; query: unknown } };
        calls.push({ command, request });
        const lease = leases.get(request.request.cursor);
        if (lease && JSON.stringify(lease.query) !== JSON.stringify(request.request.query)) throw { code: 'CURSOR_INVALID' };
        leases.delete(request.request.cursor); return response(null);
      }
      if (command === 'get_filter_options') {
        const request = args.request as { query: { dimension: string; search: string }; cursor: string | null };
        calls.push({ command, request });
        if (request.cursor) {
          const lease = leases.get(request.cursor);
          if (expired || !lease) { expired = false; leases.delete(request.cursor); throw { code: 'SNAPSHOT_EXPIRED' }; }
          leases.delete(request.cursor);
          return response({ meta: lease.meta, dimension: request.query.dimension, options: [{ key: 'synthetic-beta', display_name: 'Synthetic Beta', count: '17' }], next_cursor: null });
        }
        const snapshot = { ...meta, snapshot_id: `synthetic-facet-${++serial}` };
        const cursor = String(serial).padStart(151, 'a');
        if (request.query.search === 'slow' && deferSlow) { deferSlow = false; await new Promise<void>(resolve => { releaseSlow = resolve; }); }
        if (request.query.search === 'beta') return response({ meta: snapshot, dimension: request.query.dimension, options: [{ key: 'synthetic-beta', display_name: 'Synthetic Beta', count: '17' }], next_cursor: null });
        if (request.query.search === 'empty') return response({ meta: snapshot, dimension: request.query.dimension, options: [], next_cursor: null });
        if (leases.size >= 2) throw { code: 'SNAPSHOT_EXPIRED' };
        leases.set(cursor, { query: request.query, meta: snapshot });
        const labels: Record<string, string> = { models: '模型', projects: '项目', sessions: '会话' };
        return response({ meta: snapshot, dimension: request.query.dimension, options: [{ key: null, display_name: `未知${labels[request.query.dimension]}`, count: '9007199254740993' }, { key: `synthetic-${request.query.dimension}`, display_name: `Synthetic ${labels[request.query.dimension]}`, count: '23' }], next_cursor: cursor });
      }
      throw new Error(`Unexpected synthetic command ${command}`);
    } } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '模型', exact: true }).click();
  await expect(page.getByRole('heading', { name: '当前筛选暂无用量' })).toBeVisible();
});

test('searchable dimensions keep exact counts, stable pages, unknown selections and shared scope across pages', async ({ page }) => {
  await page.getByRole('combobox', { name: '模型', exact: true }).click();
  await expect(page.getByRole('textbox', { name: '搜索模型' })).toBeFocused();
  await expect(page.getByRole('option', { name: /未知模型/ })).toContainText('9,007,199,254,740,993');
  await expect(page.locator('.facet-count[title="9,007,199,254,740,993 条用量记录"]')).toBeVisible();
  await page.screenshot({ path: 'test-results/filters-1280.png', fullPage: true });
  await page.getByRole('button', { name: '加载下一页' }).click();
  await expect(page.getByRole('listbox').getByRole('option')).toHaveCount(3);
  await page.getByRole('option', { name: /Synthetic Beta/ }).click();
  await expect(page.getByRole('combobox', { name: '模型', exact: true })).toContainText('Synthetic Beta');
  await page.getByLabel('日期范围').selectOption('last7');
  await page.getByRole('button', { name: '项目', exact: true }).click();
  await page.getByRole('combobox', { name: '项目', exact: true }).click();
  await page.getByRole('option', { name: /未知项目/ }).click();
  await expect(page.getByRole('combobox', { name: '项目', exact: true })).toContainText('未知项目');
  await page.getByRole('combobox', { name: '会话', exact: true }).click();
  await page.getByRole('option', { name: /Synthetic 会话/ }).click();
  await expect.poll(async () => page.evaluate(() => {
    const calls = (window as unknown as { __facetCalls: { command: string; request: { filter: unknown } }[] }).__facetCalls;
    return calls.filter(call => call.command === 'get_grouped_usage').at(-1)?.request.filter;
  })).toMatchObject({ models: { kind: 'ids', ids: ['synthetic-beta'], include_unknown: false }, projects: { kind: 'ids', ids: [], include_unknown: true }, sessions: { kind: 'ids', ids: ['synthetic-sessions'], include_unknown: false } });
  await page.setViewportSize({ width: 960, height: 680 });
  await page.getByRole('combobox', { name: '项目', exact: true }).click();
  await expect(page.getByRole('listbox').getByRole('option')).toHaveCount(2);
  expect(await page.getByRole('option', { name: /未知项目/ }).locator('.facet-label').evaluate(element => element.getBoundingClientRect().width)).toBeGreaterThan(80);
  await page.screenshot({ path: 'test-results/filters-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('combobox', { name: '项目', exact: true })).toBeFocused();
  await page.getByRole('button', { name: '重置筛选' }).click();
  await expect(page.getByLabel('日期范围')).toHaveValue('today');
  for (const name of ['模型','项目','会话']) await expect(page.getByRole('combobox', { name, exact: true })).toContainText(`全部${name}`);
});

test('switching and dismissing candidates releases the exact old capability before a new query', async ({ page }) => {
  await page.getByRole('combobox', { name: '模型', exact: true }).click();
  await expect(page.getByRole('listbox').getByRole('option')).toHaveCount(2);
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('option', { name: /未知模型/ })).toBeFocused();
  await page.keyboard.press('End');
  await expect(page.getByRole('option', { name: /Synthetic 模型/ })).toBeFocused();
  await page.keyboard.press('Home');
  await expect(page.getByRole('option', { name: /未知模型/ })).toBeFocused();
  await page.getByRole('combobox', { name: '项目', exact: true }).click();
  await expect(page.getByRole('option', { name: /未知项目/ })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect.poll(async () => page.evaluate(() => (window as unknown as { __facetCalls: { command: string }[] }).__facetCalls.filter(call => call.command === 'close_query_snapshot').length)).toBe(2);
  const commands = await page.evaluate(() => (window as unknown as { __facetCalls: { command: string }[] }).__facetCalls.filter(call => call.command.includes('filter') || call.command === 'close_query_snapshot').map(call => call.command));
  expect(commands).toEqual(['get_filter_options','close_query_snapshot','get_filter_options','close_query_snapshot']);
});

test('expired continuation never appends a new snapshot and requery replaces old candidates', async ({ page }) => {
  await page.getByRole('combobox', { name: '模型', exact: true }).click();
  await expect(page.getByRole('listbox').getByRole('option')).toHaveCount(2);
  await page.evaluate(() => (window as unknown as { __expireFacet: () => void }).__expireFacet());
  await page.getByRole('button', { name: '加载下一页' }).click();
  await expect(page.getByRole('alert')).toHaveText('请重新查询以继续查看记录。');
  await expect(page.getByRole('listbox').getByRole('option')).toHaveCount(2);
  await expect(page.getByRole('option', { name: /Synthetic Beta/ })).toHaveCount(0);
  await page.getByRole('button', { name: '重新查询', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByRole('listbox').getByRole('option')).toHaveCount(2);
});

test('search serializes late replies, closes their original query and cleans up when navigating away', async ({ page }) => {
  await page.getByRole('combobox', { name: '模型', exact: true }).click();
  await expect(page.getByRole('listbox').getByRole('option')).toHaveCount(2);
  await page.evaluate(() => (window as unknown as { __deferFacet: () => void }).__deferFacet());
  await page.getByRole('textbox', { name: '搜索模型' }).fill('slow');
  await expect.poll(async () => page.evaluate(() => (window as unknown as { __facetCalls: { command: string; request: { query?: { search: string } } }[] }).__facetCalls.some(call => call.command === 'get_filter_options' && call.request.query?.search === 'slow'))).toBe(true);
  await page.getByRole('textbox', { name: '搜索模型' }).fill('beta');
  await expect(page.getByRole('option', { name: /未知模型/ })).toHaveCount(0);
  await page.evaluate(() => (window as unknown as { __releaseFacet: () => void }).__releaseFacet());
  await expect(page.getByRole('listbox').getByRole('option')).toHaveCount(1);
  await expect(page.getByRole('option', { name: /Synthetic Beta/ })).toBeVisible();
  await page.getByRole('textbox', { name: '搜索模型' }).fill('empty');
  await expect(page.getByText('当前范围没有匹配候选。')).toBeVisible();
  await page.getByRole('textbox', { name: '搜索模型' }).fill('中'.repeat(257));
  await expect(page.getByRole('alert')).toContainText('最多 256 个字符');
  await page.getByRole('textbox', { name: '搜索模型' }).fill('');
  await expect(page.getByRole('listbox').getByRole('option')).toHaveCount(2);
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await expect.poll(async () => page.evaluate(() => {
    const calls = (window as unknown as { __facetCalls: { command: string; request: { request?: { query: { search: string } } } }[] }).__facetCalls;
    return calls.filter(call => call.command === 'close_query_snapshot').map(call => call.request.request?.query.search);
  })).toEqual(['', 'slow', '']);
});
