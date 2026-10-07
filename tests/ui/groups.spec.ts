import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';

test.beforeEach(async ({ page }) => {
  // Explicit synthetic DTO bridge, confined to browser QA. Never a production fallback.
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    let many = false, redacted = false;
    const measure = (value: string | null, total: string, complete: boolean) => ({ value, covered_total_tokens: value === null ? '0' : total, complete });
    const tokens = (total: string, known = false) => ({ total_tokens: total, input_total: measure(known ? total : null, total, known), cached_input: measure(known ? '1' : null, total, known), noncached_input: measure(known ? String(BigInt(total)-1n) : null, total, known), output_total: measure(known ? '0' : null, total, known), reasoning_output: measure(known ? '0' : null, total, known), cache_write_input: { value: null, covered_total_tokens: '0', complete: false }, session_count: total === '0' ? '0' : '1', usage_event_count: total === '0' ? '0' : '1', reliable_turn_count: null, reliable_turns_complete: false });
    const coverage = { state: 'unknown', pending_observation_count: '0', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '0', source_issues: [], format_issues: [], breakdown_complete: false };
    const price = (total: string, currency: string | null, amount = '0.000000000000000') => ({ redacted, basis: { mode: 'event_time' }, currencies: currency === null ? [] : [{ currency, estimated_cost: redacted ? null : amount, priced_total_tokens: total }], priced_total_tokens: currency === null ? '0' : total, unpriced_total_tokens: currency === null ? total : '0', reasons: currency === null && total !== '0' ? [{ code: 'unknown_model', total_tokens: total, event_count: '1' }] : [], calculating: false });
    const huge = '9007199254740993';
    Object.assign(window, { isTauri: true, __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} }, __TAURI_INTERNALS__: { transformCallback: () => 0, invoke: async (command: string, args: Record<string, unknown>) => {
      if (command === 'plugin:event|listen' || command === 'plugin:event|unlisten') return 0;
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data });
      if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
      if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic-test', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
      if (command === 'get_sources') return response({ settings_revision: '1', sources: ['a','b'].map(id => ({ source_id: `synthetic-${id}`, root_path: `E:\\synthetic-qa-${id}\\.codex`, origin: 'custom', enabled: true, removed: false, readability: 'readable', capabilities: { physical_identity: 'available', byte_seek: 'available', watcher: 'available', polling_required: true }, last_scan_at_ms: 1000, last_success_at_ms: 1000, error: null })) });
      if (command === 'get_price_rules') return response({ price_revision: '3', rules: [], aliases: [] });
      if (command === 'get_dashboard_bundle') throw new Error('Synthetic bridge does not supply overview data');
      if (command === 'get_grouped_usage') {
        const r = args.request as { filter: { range: { start_ms: number; timezone: string }; sources: { ids?: string[] } }; dimension: string; sort: string; limit: number };
        const empty = r.filter.sources.ids?.includes('synthetic-b') ?? false;
        const project = r.dimension === 'projects';
        const base = [
          { key: 'synthetic-known', display_name: project ? 'Synthetic project alias' : 'synthetic-model · synthetic-provider', totals: tokens(huge, true), pricing: price(huge, 'USD', '9.007199254740993'), coverage: { ...coverage, breakdown_complete: true } },
          { key: 'synthetic-zero', display_name: project ? 'Synthetic zero cost project' : 'synthetic-zero · synthetic-provider', totals: tokens('23'), pricing: price('23', 'EUR'), coverage },
          { key: null, display_name: project ? '未知项目' : '未知模型', totals: tokens('17'), pricing: price('17', null), coverage },
        ];
        if (many) for (let i=0;i<198;i++) base.push({ key: `synthetic-extra-${i}`, display_name: `Synthetic additional ${i}`, totals: tokens('1'), pricing: price('1', null), coverage });
        const groups = empty ? [] : base.sort((a,b) => r.sort === 'name_asc' ? (a.display_name < b.display_name ? -1 : a.display_name > b.display_name ? 1 : 0) : BigInt(a.totals.total_tokens) > BigInt(b.totals.total_tokens) ? -1 : BigInt(a.totals.total_tokens) < BigInt(b.totals.total_tokens) ? 1 : (a.key ?? '').localeCompare(b.key ?? '')).slice(0, r.limit);
        const total = empty ? '0' : String(BigInt(huge)+40n+(many ? 198n : 0n));
        const pricing = empty ? price('0', null) : { ...price(total, null), currencies: [...price(huge,'USD','9.007199254740993').currencies,...price('23','EUR').currencies], priced_total_tokens: String(BigInt(huge)+23n), unpriced_total_tokens: String(17+(many ? 198 : 0)), reasons: [{ code: 'unknown_model', total_tokens: String(17+(many ? 198 : 0)), event_count: String(1+(many ? 198 : 0)) }] };
        return response({ meta: { snapshot_id: args.requestId, data_revision: many ? '8' : '7', price_revision: '3', generated_at_ms: r.filter.range.start_ms+1000, parser_versions: empty ? [] : ['synthetic'], accounting_versions: empty ? [] : ['synthetic'], display_timezone: r.filter.range.timezone }, summary: tokens(total), pricing, coverage, total_group_count: empty ? '0' : String(base.length), truncated: !empty && base.length>groups.length, groups });
      }
      throw new Error(`Unexpected synthetic command ${command}`);
    } }, __setSyntheticGroups: (more: boolean, hide: boolean) => { many = more; redacted = hide; } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '模型', exact: true }).click();
});

test('model table keeps precise large tokens, mixed currencies, true zero, unknown ratios and shared source filters', async ({ page }) => {
  await expect(page.getByLabel('9,007,199,254,741,033 Token', { exact: true })).toBeVisible();
  await expect(page.locator('.group-stat-strip').getByText('USD 9.01', { exact: true })).toBeVisible();
  await expect(page.locator('.group-stat-strip').getByText('EUR 0.00', { exact: true })).toBeVisible();
  await expect(page.locator('td[title="9,007,199,254,740,993"]')).toBeVisible();
  const unknown = page.getByRole('row').filter({ hasText: '未知模型' });
  await expect(unknown.getByText('未计价', { exact: true })).toBeVisible();
  await expect(unknown.locator('td').nth(3)).toHaveText('—');
  // A single non-USD subtotal preserves its currency label even when the amount is zero.
  await expect(page.getByRole('row').filter({ hasText: 'synthetic-zero ·' }).getByText('EUR 0.00', { exact: true })).toBeVisible();
  await page.screenshot({ path: 'test-results/models-1280.png', fullPage: true });
  await page.getByLabel('模型排序').selectOption('name_asc');
  await expect(page.locator('.group-table tbody tr').first()).toContainText('synthetic-model');
  await page.getByLabel('日期范围').selectOption('last7');
  await page.getByRole('button', { name: '项目', exact: true }).click();
  await expect(page.getByLabel('日期范围')).toHaveValue('last7');
  await expect(page.getByRole('heading', { name: 'Synthetic project alias' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '未知项目', exact: true })).toBeVisible();
  await page.setViewportSize({ width: 960, height: 680 });
  await page.screenshot({ path: 'test-results/projects-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.getByLabel('来源', { exact: true }).selectOption('synthetic-b');
  await expect(page.getByRole('heading', { name: '当前筛选暂无用量' })).toBeVisible();
  await expect(page.getByLabel('0 Token', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '模型', exact: true }).click();
  await expect(page.getByLabel('来源', { exact: true })).toHaveValue('synthetic-b');
  await expect(page.getByRole('heading', { name: '当前筛选暂无用量' })).toBeVisible();
  await page.getByLabel('来源', { exact: true }).selectOption('');
  await expect(page.getByRole('table')).toBeVisible();
  await expect(page.getByRole('columnheader', { name: '已计价比例' })).toHaveCount(0);
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '价格规则', exact: true }).click();
  await expect(page.getByRole('tabpanel', { name: '价格规则设置' })).toBeVisible();
});

test('group limits report undisplayed categories and redacted summaries hide every amount', async ({ page }) => {
  await expect(page.getByRole('table')).toBeVisible();
  await page.evaluate(() => (window as unknown as { __setSyntheticGroups: (more: boolean, hide: boolean) => void }).__setSyntheticGroups(true, false));
  await page.getByLabel('模型显示数量').selectOption('50');
  await expect(page.locator('.group-table tbody tr')).toHaveCount(50);
  await expect(page.getByText('201 个模型分类 · 已显示 50 个')).toBeVisible();
  await expect(page.getByText('还有未显示的模型，请缩小筛选范围。')).toBeVisible();
  await expect(page.getByLabel('9,007,199,254,741,231 Token', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as { __setSyntheticGroups: (more: boolean, hide: boolean) => void }).__setSyntheticGroups(false, true));
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect(page.locator('.group-table tbody tr')).toHaveCount(3);
  await expect(page.getByText('已隐藏', { exact: true })).toHaveCount(4);
  await expect(page.getByText('USD 9.01', { exact: true })).toHaveCount(0);
  await expect(page.locator('[title*="9.007199254740993"]')).toHaveCount(0);
  await page.setViewportSize({ width: 960, height: 680 });
  await page.screenshot({ path: 'test-results/models-redacted-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
