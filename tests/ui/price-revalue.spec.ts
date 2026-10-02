import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';

test.beforeEach(async ({ page }) => {
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    // Explicit synthetic DTO bridge, excluded from every production bundle.
    let revision = '1', privacy = false, readFailure = false, failStart = false;
    let lastKey: string | null = null; const keys: string[] = [];
    let job: Record<string, unknown> | null = null, request: Record<string, unknown> | null = null;
    Object.assign(window, { isTauri: true, __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
      __revalueFixture: { advance: () => { revision = '2'; }, fail: (value: boolean) => { readFailure = value; }, privacy: () => { privacy = true; }, request: () => request, ambiguousStart: () => { failStart = true; }, keys: () => keys,
        finish: () => { if (job) job = { ...job, state: 'cancelled', can_cancel: false, error: 'JOB_CANCELLED' }; } },
      __TAURI_INTERNALS__: { transformCallback: () => 0, invoke: async (command: string, args: Record<string, unknown>) => {
        if (command === 'plugin:event|listen' || command === 'plugin:event|unlisten') return 0;
        const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: privacy ? '2' : '1', privacy }, data });
        if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
        if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic-test', collector: 'ready', storage: 'ready', quota: 'not_configured', taskbar: 'not_implemented' });
        if (command === 'get_sources') return response({ settings_revision: '1', sources: [] });
        if (command === 'get_price_rules') return response({ price_revision: args.revision ?? revision, rules: [], aliases: [] });
        if (command === 'get_offline_price_catalog') return response({ price_revision: args.revision ?? revision, catalog: null });
        if (command === 'get_price_revalue_status') {
          if (readFailure) throw { code: 'DB_WRITE_FAILED' };
          return response({ current_price_revision: revision, active_job: job && ['queued', 'running', 'cancelling'].includes(String(job.state)) ? job : null, latest_job: job, uncached_ledgers: '2' });
        }
        if (command === 'start_price_revalue') {
          request = args.request as Record<string, unknown>; if (request.expected_price_revision !== revision) throw { code: 'REVISION_CONFLICT' };
          keys.push(String(request.request_key));
          if (job && lastKey === request.request_key) return response(job);
          lastKey = String(request.request_key);
          job = { job_id: 'synthetic-price-job', state: 'running', automatic: false, price_revision: revision, basis: request.basis, total_ledgers: '2', completed_ledgers: '1', total_events: '9007199254740993', processed_events: '9007199254740992', can_cancel: true, error: null, created_at_ms: 1000, updated_at_ms: 2000 };
          if (failStart) { failStart = false; throw new Error('synthetic response unavailable'); }
          return response(job);
        }
        if (command === 'cancel_price_revalue') { if (job) job = { ...job, state: 'cancelling' }; return response('accepted'); }
        throw new Error(`unexpected synthetic command ${command}`);
      } },
    });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '价格规则', exact: true }).click();
});

test('specified basis, exact progress and cancellation retain ordinary layout', async ({ page }) => {
  const panel = page.getByRole('region', { name: '后台费用重估' });
  await expect(panel.getByText('暂无重估任务。')).toBeVisible();
  await panel.getByRole('combobox', { name: '计价依据' }).selectOption('specified_time');
  await panel.getByTitle('编辑明确估价时点（UTC）').click();
  await panel.getByLabel('估价时点（UTC）', { exact: true }).fill('2026-10-02T00:00:00.123');
  await panel.getByRole('button', { name: '应用估价时点' }).click();
  await panel.getByRole('button', { name: '重估全部已确认用量' }).click();
  await expect(panel.getByText('正在重估', { exact: true })).toBeVisible();
  await expect(panel.getByText('已处理记录 9,007,199,254,740,992 / 9,007,199,254,740,993 · 完成账本 1 / 2')).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { __revalueFixture: { request: () => unknown } }).__revalueFixture.request())).toMatchObject({ expected_price_revision: '1', scope: { kind: 'all' }, basis: { mode: 'specified_time', specified_at_ms: 1790899200123 } });
  await page.screenshot({ path: 'test-results/price-revalue-dark-1280.png', fullPage: true });
  await panel.getByRole('button', { name: '取消当前重估' }).click();
  await expect(panel.getByRole('button', { name: '等待取消完成…' })).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __revalueFixture: { finish: () => void } }).__revalueFixture.finish());
  await panel.getByRole('button', { name: '刷新重估状态' }).click();
  await expect(panel.getByText('已取消重估', { exact: true })).toBeVisible();
  await expect(panel.getByRole('button', { name: '重估全部已确认用量' })).toBeEnabled();
  await page.setViewportSize({ width: 960, height: 860 });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  await page.screenshot({ path: 'test-results/price-revalue-light-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('read failure and stale or historical versions cannot become a new job', async ({ page }) => {
  const panel = page.getByRole('region', { name: '后台费用重估' });
  await expect(panel.getByText('暂无重估任务。')).toBeVisible();
  await page.evaluate(() => (window as unknown as { __revalueFixture: { advance: () => void } }).__revalueFixture.advance());
  await panel.getByRole('button', { name: '刷新重估状态' }).click();
  await expect(panel.getByText('当前规则已变化，请刷新当前版本后重估。')).toBeVisible();
  await expect(panel.getByRole('button', { name: '重估全部已确认用量' })).toBeDisabled();
  await page.getByRole('button', { name: '刷新当前版本', exact: true }).click();
  await expect(panel.getByRole('button', { name: '重估全部已确认用量' })).toBeEnabled();
  await page.evaluate(() => (window as unknown as { __revalueFixture: { fail: (v: boolean) => void } }).__revalueFixture.fail(true));
  await panel.getByRole('button', { name: '刷新重估状态' }).click();
  await expect(panel.getByRole('alert')).toContainText('重估状态读取失败');
  await expect(panel.getByRole('button', { name: '重估全部已确认用量' })).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __revalueFixture: { fail: (v: boolean) => void } }).__revalueFixture.fail(false));
  await page.getByLabel('历史价格版本').fill('1');
  await page.locator('.price-version form').getByRole('button', { name: '查看', exact: true }).click();
  await expect(panel.getByText('历史价格版本只读；回到当前版本后可创建重估任务。')).toBeVisible();
  await expect(panel.getByRole('button', { name: '重估全部已确认用量' })).toHaveCount(0);
});

test('latest privacy projection unmounts progress and its editor', async ({ page }) => {
  const panel = page.getByRole('region', { name: '后台费用重估' });
  await expect(panel.getByText('暂无重估任务。')).toBeVisible();
  await panel.getByRole('combobox', { name: '计价依据' }).selectOption('specified_time');
  await panel.getByTitle('编辑明确估价时点（UTC）').click();
  await page.evaluate(() => (window as unknown as { __revalueFixture: { privacy: () => void } }).__revalueFixture.privacy());
  await panel.getByRole('button', { name: '刷新重估状态' }).click();
  await expect(page.getByRole('heading', { name: '价格规则已隐藏' })).toBeVisible();
  await expect(panel).toHaveCount(0);
});

test('retry after an unknown submission outcome keeps the original idempotency key', async ({ page }) => {
  const panel = page.getByRole('region', { name: '后台费用重估' });
  await expect(panel.getByText('暂无重估任务。')).toBeVisible();
  await page.evaluate(() => (window as unknown as { __revalueFixture: { ambiguousStart: () => void } }).__revalueFixture.ambiguousStart());
  await panel.getByRole('button', { name: '重估全部已确认用量' }).click();
  await expect(panel.getByRole('alert')).toContainText('synthetic response unavailable');
  await expect(panel.getByText('正在重估', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as { __revalueFixture: { finish: () => void } }).__revalueFixture.finish());
  await panel.getByRole('button', { name: '刷新重估状态' }).click();
  await expect(panel.getByRole('button', { name: '重估全部已确认用量' })).toBeEnabled();
  await panel.getByRole('button', { name: '重估全部已确认用量' }).click();
  await expect(panel.getByText('重估请求已接受。')).toBeVisible();
  const keys = await page.evaluate(() => (window as unknown as { __revalueFixture: { keys: () => string[] } }).__revalueFixture.keys());
  expect(keys).toHaveLength(2); expect(keys[0]).toBe(keys[1]);
});
