import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';

type QA = { __navigationCacheQA: { reads: () => string[]; concurrency: () => number; inFlight: () => number; resetConcurrency: () => void; hold: () => void; release: () => void; fail: () => void; total: (value: string) => void; statusFail: () => void } };

test.beforeEach(async ({ page }, info) => {
  const cold = info.title.includes('unwarmed');
  await installSyntheticCalendar(page);
  await page.addInitScript(({ cold }) => {
    const reads: string[] = [], pending: (() => void)[] = [];
    let hold = cold, fail = false, serial = 0, version = 1, statusFail = false;
    let inFlight = 0, maximum = 0;
    const unknown = { value: null, covered_total_tokens: '0', complete: false };
    const summary = { total_tokens: '777', input_total: unknown, cached_input: unknown, noncached_input: unknown, output_total: unknown, reasoning_output: unknown, cache_write_input: unknown, session_count: '0', usage_event_count: '0', reliable_turn_count: null, reliable_turns_complete: false };
    const pricing = { redacted: false, basis: { mode: 'event_time' }, currencies: [], priced_total_tokens: '0', unpriced_total_tokens: '777', reasons: [], calculating: false };
    const coverage = { state: 'partial', pending_observation_count: '0', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '0', source_issues: [], format_issues: [], breakdown_complete: false };
    Object.assign(window, {
      isTauri: true,
      __navigationCacheQA: { reads: () => [...reads], concurrency: () => maximum, inFlight: () => inFlight, resetConcurrency: () => { maximum = inFlight; }, hold: () => { hold = true; }, release: () => { hold = false; for (const resolve of pending.splice(0)) resolve(); }, fail: () => { fail = true; }, total: (value: string) => { summary.total_tokens = value; ++version; }, statusFail: () => { statusFail = true; } },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
      __TAURI_INTERNALS__: { transformCallback: () => 0, invoke: async (command: string, args: Record<string, unknown>) => {
        if (command === 'plugin:event|listen' || command === 'plugin:event|unlisten') return 0;
        const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data: structuredClone(data) });
        if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
        if (command === 'get_usage_revision') return response({ database_id: 'a'.repeat(32), data_revision: String(version), price_revision: '1', usage_view_revision: String(version) });
        if (command === 'get_app_status' && statusFail) throw new Error('Synthetic status disconnected');
        if (command === 'get_app_status') return response({ version: 'synthetic', development: true, data_directory: 'synthetic', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
        if (command === 'get_sources') return response({ settings_revision: '1', sources: [{ source_id: 'synthetic', root_path: 'Synthetic Source', enabled: true, removed: false }] });
        if (command === 'get_account_quota') return response({ connection_epoch: 'synthetic', quota_revision: '0', state: 'disconnected', selected_limit_id: null, available_limits: [], windows: [], fetched_at_ms: null, last_attempt_at_ms: null, error_code: null });
        if (command === 'get_main_navigation' || command === 'get_mini_stats_request' || command === 'close_query_snapshot') return response(null);
        const request = args.request as { dimension?: string; filter?: { range: { timezone: string } }; query?: { filter: { range: { timezone: string } } } };
        if (['get_dashboard_bundle', 'get_grouped_usage', 'query_sessions', 'query_usage_events'].includes(command)) {
          reads.push(request.dimension ?? command);
          maximum = Math.max(maximum, ++inFlight);
          await Promise.resolve();
          if (hold) await new Promise<void>(resolve => pending.push(resolve));
          --inFlight;
          if (fail) throw new Error('Synthetic background read failed');
          const bundle = { meta: { snapshot_id: `synthetic-${++serial}`, data_revision: '1', price_revision: '1', generated_at_ms: Date.now(), parser_versions: [], accounting_versions: [], display_timezone: (request.filter ?? request.query!.filter).range.timezone }, summary, pricing, coverage };
          if (command === 'get_dashboard_bundle') return response({ ...bundle, series: [], heatmap: [], recent_sessions: [] });
          if (command === 'get_grouped_usage') return response({ ...bundle, groups: [], total_group_count: '0', truncated: false });
          if (command === 'query_sessions') return response({ ...bundle, sessions: [], next_cursor: null });
          return response({ ...bundle, events: [], next_cursor: null });
        }
        throw new Error(`Unexpected synthetic command ${command}`);
      } },
    });
  }, { cold });
  await page.clock.install();
  await page.goto('/');
  if (cold) { await expect(page.getByRole('heading', { name: 'Token 分解' })).toBeVisible(); return; }
  await expect(page.getByLabel('777 Token', { exact: true })).toBeVisible();
  await expect.poll(async () => page.evaluate(() => new Set((window as unknown as QA).__navigationCacheQA.reads()).size)).toBe(5);
  expect(await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.concurrency())).toBeLessThanOrEqual(3);
  await expect.poll(async () => page.evaluate(() => (window as unknown as QA).__navigationCacheQA.inFlight())).toBe(0);
});

test('a slow refresh retains cached content and dispatches foreground on independent resources', async ({ page }) => {
  await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.resetConcurrency());
  const before = await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.reads().length);
  await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.hold());
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '模型', exact: true }).click();
  await expect.poll(async () => page.evaluate(() => (window as unknown as QA).__navigationCacheQA.reads().slice(-3))).toContain('models');
  await page.clock.runFor(6_000);
  await expect(page.getByLabel('777 Token', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.concurrency())).toBeLessThanOrEqual(3);
  await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.release());
  await expect.poll(async () => page.evaluate(() => (window as unknown as QA).__navigationCacheQA.reads().length)).toBeGreaterThanOrEqual(before + 5);
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('lightweight unchanged revision checks avoid heavy reads and changed versions refresh pages', async ({ page }) => {
  const before = await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.reads().length);
  await page.clock.runFor(30_100);
  expect(await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.reads().length)).toBe(before);
  await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.total('888'));
  await page.clock.runFor(30_300);
  await expect.poll(async () => page.evaluate(count => new Set((window as unknown as QA).__navigationCacheQA.reads().slice(count)).size, before)).toBe(5);
  for (const name of ['模型', '项目', '会话', '明细', '总览']) {
    await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name, exact: true }).click();
    await expect(page.getByLabel('888 Token', { exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: /正在读取/ })).toHaveCount(0);
  }
});

test('all five pages are prepared before navigation and repeated switches render cached statistics immediately', async ({ page }) => {
  const before = await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.reads().length);
  await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.hold());
  // Check the first painted frame, so a loading flash cannot hide behind auto-wait.
  for (const name of ['模型', '项目', '会话', '明细', '总览', '项目', '会话', '明细', '模型', '总览']) {
    await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name, exact: true }).click();
    const painted = await page.evaluate(() => new Promise<boolean>(resolve => requestAnimationFrame(() => resolve(Boolean(document.querySelector('[aria-label="777 Token"]')) && !document.querySelector('main')!.innerText.includes('正在读取')))));
    expect(painted).toBe(true);
  }
  expect(await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.reads().length)).toBe(before);
});

test('delayed and failed same-scope refreshes retain content while a new date clears obsolete statistics', async ({ page }) => {
  for (const name of ['会话', '明细']) {
    await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name, exact: true }).click();
    await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.hold());
    await page.getByRole('button', { name: '重新查询', exact: true }).click();
    await expect(page.getByLabel('777 Token', { exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: /正在读取/ })).toHaveCount(0);
    await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.release());
    await expect(page.getByRole('button', { name: '重新查询', exact: true })).toBeEnabled();
  }
  await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.fail());
  await page.getByRole('button', { name: '重新查询', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Synthetic background read failed');
  await expect(page.getByLabel('777 Token', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.hold());
  await page.getByLabel('日期范围').selectOption('last7');
  await expect(page.getByLabel('777 Token', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('table', { name: '明细用量' })).toBeVisible();
  await expect(page.getByRole('heading', { name: /正在读取/ })).toHaveCount(0);
});

test('unwarmed pages show complete structure while six second reads block and foreground promotes', async ({ page }) => {
  const nav = page.getByRole('navigation', { name: '主导航' });
  for (const name of ['模型', '项目', '会话', '明细', '总览']) {
    await nav.getByRole('button', { name, exact: true }).click();
    expect(await page.evaluate(() => new Promise<boolean>(resolve => requestAnimationFrame(() => resolve(!document.querySelector('main')!.innerText.includes('正在读取') && document.querySelector('main')!.innerText.includes('—')))))).toBe(true);
    await expect(page.getByLabel('777 Token', { exact: true })).toHaveCount(0);
  }
  await page.clock.runFor(6000);
  await expect(page.getByRole('heading', { name: 'Token 分解' })).toBeVisible();
  await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.release());
  await expect(page.getByLabel('777 Token', { exact: true })).toBeVisible();
});

test('visited date scopes restore their own snapshot on the first frame and status failures retain it', async ({ page }) => {
  await page.getByLabel('日期范围').selectOption('last7');
  await expect(page.getByLabel('777 Token', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.hold());
  await page.getByLabel('日期范围').selectOption('today');
  await expect(page.getByLabel('777 Token', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as QA).__navigationCacheQA.statusFail());
  await page.clock.runFor(2100);
  await expect(page.getByRole('alert')).toContainText('Synthetic status disconnected');
  await expect(page.getByLabel('777 Token', { exact: true })).toBeVisible();
});
