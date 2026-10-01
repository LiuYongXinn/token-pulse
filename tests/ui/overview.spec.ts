import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  // Explicit synthetic UI DTO bridge. No fixture is imported into production.
  await page.addInitScript(() => {
    let fail = false, deferNext = false, release: (() => void) | null = null;
    const sources = ['a', 'b'].map(id => ({ source_id: `synthetic-${id}`, root_path: `E:\\synthetic-qa-${id}\\.codex`, origin: 'custom', enabled: true, removed: false, readability: 'readable', capabilities: { physical_identity: 'available', byte_seek: 'available', watcher: 'available', polling_required: true }, last_scan_at_ms: 1000, last_success_at_ms: 1000, error: null }));
    const complete = (n: number, total: number) => ({ value: String(n), covered_total_tokens: String(total), complete: true });
    const unknown = { value: null, covered_total_tokens: '0', complete: false };
    const tokens = (partial = false) => ({ total_tokens: partial ? '17' : '683067', input_total: partial ? unknown : complete(630630, 683067), cached_input: partial ? unknown : complete(429566, 683067), noncached_input: partial ? unknown : complete(201064, 683067), output_total: partial ? unknown : complete(52437, 683067), reasoning_output: partial ? unknown : complete(19926, 683067), session_count: '1', usage_event_count: partial ? '1' : '5', reliable_turn_count: null, reliable_turns_complete: false });
    const coverage = { state: 'partial', pending_observation_count: '2', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '1', source_issues: [{ source_id: 'synthetic-a', code: 'scan_evidence_missing', last_success_ms: 1000 }], format_issues: [], breakdown_complete: true };
    const empty = { total_tokens: '0', input_total: unknown, cached_input: unknown, noncached_input: unknown, output_total: unknown, reasoning_output: unknown, session_count: '0', usage_event_count: '0', reliable_turn_count: null, reliable_turns_complete: false };
    const price = (partial: boolean) => ({ redacted: false, basis: { mode: 'event_time' }, currencies: partial ? [] : [{ currency: 'USD', estimated_cost: '0.871234567890123', priced_total_tokens: '650000' }], priced_total_tokens: partial ? '0' : '650000', unpriced_total_tokens: partial ? '17' : '33067', reasons: [{ code: 'missing_rule', total_tokens: partial ? '17' : '33067', event_count: '1' }], calculating: false });
    Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: { invoke: async (command: string, args: Record<string, unknown>) => {
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, data });
      if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic-test', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
      if (command === 'get_sources') return response({ settings_revision: '1', sources });
      if (command === 'get_price_rules') return response({ price_revision: '3', rules: [], aliases: [] });
      if (command === 'get_dashboard_bundle') {
        if (fail) throw new Error('synthetic refresh failure');
        const r = args.request as { filter: { sources: { ids?: string[] }; range: { start_ms: number; end_ms: number; timezone: string } }; grain: string; heatmap_range: { start_ms: number; end_ms: number } };
        const partial = r.filter.sources.ids?.includes('synthetic-b') ?? false;
        const step = r.grain === 'hour' ? 3_600_000 : 86_400_000;
        const series = Array.from({ length: Math.ceil((r.filter.range.end_ms - r.filter.range.start_ms) / step) }, (_, i) => {
          const start = r.filter.range.start_ms + i * step;
          // The UI bridge only illustrates rendering; real DST bucketing is independently tested in Rust.
          const count = i === 0 ? tokens(partial) : empty;
          return { start_ms: start, end_ms: Math.min(start + step, r.filter.range.end_ms), display_label: new Date(start).toISOString().slice(0, 16), utc_offset: '+08:00', totals: count, coverage: { ...coverage, breakdown_complete: i === 0 && !partial } };
        });
        const heatmap = Array.from({ length: 182 }, (_, i) => ({ start_ms: r.heatmap_range.start_ms + i * 86_400_000, end_ms: r.heatmap_range.start_ms + (i + 1) * 86_400_000, display_label: `synthetic-day-${i + 1}`, utc_offset: '+08:00', totals: i % 5 === 0 ? tokens(partial) : empty, coverage: { ...coverage, breakdown_complete: !partial && i % 5 === 0 } }));
        const data = { meta: { snapshot_id: String(args.requestId), data_revision: '7', price_revision: '3', generated_at_ms: r.filter.range.start_ms + 1000, parser_versions: ['synthetic'], accounting_versions: ['synthetic'], display_timezone: r.filter.range.timezone }, summary: tokens(partial), pricing: price(partial), coverage: { ...coverage, breakdown_complete: !partial }, series, heatmap, recent_sessions: [{ session_key: 'synthetic-session', display_name: 'synthetic-ui-session', latest_at_ms: r.filter.range.start_ms + 1000, latest_model: 'synthetic-model', latest_project_id: 'synthetic-project', latest_project_name: 'Synthetic QA Project', summary: tokens(partial), pricing: price(partial) }] };
        if (deferNext) { deferNext = false; await new Promise<void>(resolve => { release = resolve; }); }
        return response(data);
      }
      throw new Error(`unexpected synthetic command ${command}`);
    } }, __setSyntheticDashboardFailure: (value: boolean) => { fail = value; }, __deferSyntheticDashboard: () => { deferNext = true; }, __releaseSyntheticDashboard: () => { release?.(); release = null; } });
  });
  await page.goto('/');
});

test('overview preserves prototype layout, known breakdown and real unknown states under shared filters', async ({ page }) => {
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await expect(page.getByText('$0.87', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '用量趋势' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '最近会话' })).toBeVisible();
  await expect(page.getByLabel('完整 Token 分解')).toBeVisible();
  await expect(page.getByText('尚未连接账户服务')).toBeVisible();
  const bounds = await page.evaluate(() => ({ left: document.querySelector('.overview-left')!.getBoundingClientRect().right, right: document.querySelector('.overview-right')!.getBoundingClientRect().left }));
  expect(bounds.left).toBeLessThan(bounds.right);
  const activity = page.getByRole('group', { name: '近 26 周每日活动' });
  await expect(activity.getByRole('button')).toHaveCount(182);
  await activity.getByRole('button').nth(1).focus();
  await expect(page.locator('.activity-panel .chart-caption')).toHaveText('synthetic-day-2 · 0 Token · 存在采集或解释缺口');
  await activity.getByRole('button').first().click();
  await expect(page.locator('.activity-panel .chart-caption')).toHaveText('synthetic-day-1 · 683,067 Token · 存在采集或解释缺口');
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

test('same-filter refresh retains prior values on error and older-filter replies cannot replace the new scope', async ({ page }) => {
  await expect(page.getByLabel('683,067 Token', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as { __deferSyntheticDashboard: () => void }).__deferSyntheticDashboard());
  await page.getByLabel('来源', { exact: true }).selectOption('synthetic-b');
  await expect(page.getByRole('heading', { name: '正在读取统计快照' })).toBeVisible();
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
