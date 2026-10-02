import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { installSyntheticCalendar } from './calendar-bridge';
import type { OfflinePriceCatalog } from '../../ui/src/shared/generated/contracts';

// Public factual bundled rates; all IPC, accounts and application usage here are synthetic.
const catalog = JSON.parse(readFileSync('crates/token-pulse-core/data/offline-prices.json', 'utf8')) as OfflinePriceCatalog;
test.beforeEach(async ({ page }) => {
  await installSyntheticCalendar(page);
  await page.addInitScript((facts) => {
    let revision = '1', fail = false, hold = false;
    let release: (() => void) | null = null;
    Object.assign(window, {
      isTauri: true, __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
      __offlineFixture: { advance: () => { revision = '2'; }, fail: () => { fail = true; }, hold: () => { hold = true; }, release: () => { release?.(); } },
      __TAURI_INTERNALS__: { transformCallback: () => 0, invoke: async (command: string, args: Record<string, unknown>) => {
        if (command === 'plugin:event|listen' || command === 'plugin:event|unlisten') return 0;
        const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data });
        if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
        if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic-test', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
        if (command === 'get_sources') return response({ settings_revision: '1', sources: [] });
        if (command === 'get_price_revalue_status') return response({ current_price_revision: revision, active_job: null, latest_job: null, uncached_ledgers: '0' });
        if (command === 'get_price_rules') return response({ price_revision: args.revision ?? revision, rules: [], aliases: [] });
        if (command === 'get_offline_price_catalog') {
          const requested = args.revision ?? revision;
          if (fail) { fail = false; throw { code: 'DB_WRITE_FAILED' }; }
          if (hold && requested === '1') { hold = false; await new Promise<void>(resolve => { release = resolve; }); }
          return response({ price_revision: requested, catalog: requested === '0' ? null : { ...facts, catalog_id: requested === '1' ? facts.catalog_id : 'openai-text-synthetic-next' } });
        }
        throw new Error(`Unexpected synthetic command ${command}`);
      } },
    });
  }, catalog);
  await page.goto('/');
  await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '价格规则', exact: true }).click();
});

test('browse exact factual prices, conditions, modes and absent catalog history', async ({ page }) => {
  const region = page.getByRole('region', { name: '离线价格目录', exact: true });
  await expect(region).toContainText('51 个模型 / 172 条单价');
  await region.getByLabel('查找目录模型').fill('GPT-6.1-SOL');
  const rows = region.locator('tbody tr');
  await expect(rows).toHaveCount(2);
  await expect(rows.first()).toContainText('需请求档位与缓存写入');
  const short = rows.filter({ hasText: '输入 ≤ 272K' });
  await expect(short).toContainText('2.50');
  await expect(short.getByRole('link', { name: '官方来源' })).toHaveAttribute('href', 'https://developers.openai.com/api/docs/pricing');
  await region.getByLabel('目录处理模式').selectOption('fast');
  await expect(rows.first()).toContainText('非默认参考模式');
  await region.getByLabel('查找目录模型').fill('gpt-5.3-codex');
  await expect(rows).toHaveCount(1);
  await expect(rows).toContainText('3.50');
  await region.getByLabel('目录处理模式').selectOption('standard');
  await expect(rows).toContainText('已接自动参考计价');
  await expect(rows).toContainText('1.75');
  await region.getByLabel('查找目录模型').fill('gpt-5.3-codex-unknown');
  await expect(region).toContainText('没有匹配的目录条目');
  await page.getByLabel('历史价格版本').fill('0');
  await page.getByRole('button', { name: '查看', exact: true }).click();
  await expect(region).toContainText('此价格版本尚无内置目录');
  await expect(region.locator('tbody')).toHaveCount(0);
  await page.getByRole('button', { name: '刷新当前版本', exact: true }).click();
  await expect(region).toContainText('51 个模型 / 172 条单价');
  await region.getByLabel('查找目录模型').fill('gpt-6.1-sol');
  await region.getByLabel('目录处理模式').selectOption('standard');
  await region.scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/offline-prices-dark.png' });
  await page.setViewportSize({ width: 960, height: 860 });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  await region.locator('.offline-table-wrap').scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/offline-prices-light-960.png' });
  const outer = await page.evaluate(() => ({ viewport: innerWidth, width: document.documentElement.scrollWidth }));
  expect(outer.width).toBeLessThanOrEqual(outer.viewport);
});

test('failed catalog is independently retryable and stale response cannot replace captured newer revision', async ({ page }) => {
  const region = page.getByRole('region', { name: '离线价格目录', exact: true });
  await expect(region).toContainText('openai-text-2026-10-02');
  const fixture = async (method: 'advance' | 'fail' | 'hold' | 'release') => page.evaluate(name => {
    (window as unknown as { __offlineFixture: Record<string, () => void> }).__offlineFixture[name]();
  }, method);
  await fixture('fail');
  await region.getByRole('button', { name: '重新读取目录' }).click();
  await expect(region.getByRole('alert')).toContainText('目录读取失败');
  await expect(page.getByRole('button', { name: '新增规则', exact: true })).toBeEnabled();
  await region.getByRole('button', { name: '重新读取目录' }).click();
  await expect(region).toContainText('openai-text-2026-10-02');
  await fixture('hold');
  await region.getByRole('button', { name: '重新读取目录' }).click();
  await expect(region).toContainText('正在读取版本 1');
  await fixture('advance');
  await page.getByRole('button', { name: '刷新当前版本', exact: true }).click();
  await expect(region).toContainText('openai-text-synthetic-next');
  await expect(region).toContainText('价格版本 2');
  await fixture('release');
  await expect(region).toContainText('openai-text-synthetic-next');
  await expect(region).not.toContainText('openai-text-2026-10-02');
});
