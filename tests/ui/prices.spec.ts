import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  // Explicit synthetic native bridge for UI tests only; never included in a production build.
  await page.addInitScript(() => {
    let revision = 0, nextId = 0;
    let rules: Record<string, unknown>[] = [];
    const history = new Map<number, Record<string, unknown>[]>([[0, []]]);
    Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: { invoke: async (command: string, args: Record<string, unknown>) => {
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, data });
      const current = () => ({ price_revision: String(revision), rules: structuredClone(rules), aliases: [] });
      if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic-test', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
      if (command === 'get_sources') return response({ settings_revision: '1', sources: [] });
      if (command === 'get_price_rules') {
        if (args.revision === null) return response(current());
        const value = Number(args.revision); if (!history.has(value)) throw { code: 'INVALID_QUERY' };
        return response({ price_revision: String(value), rules: structuredClone(history.get(value)), aliases: [] });
      }
      if (command === 'save_price_rule' || command === 'retire_price_rule') {
        if (args.expectedPriceRevision !== String(revision)) throw { code: 'REVISION_CONFLICT' };
        const request = args.request as { kind: string; rule_id?: string; draft: Record<string, unknown> } | undefined;
        if (request?.kind === 'create' && rules.length > 0) throw { code: 'PRICE_RULE_CONFLICT' };
        const id = command === 'retire_price_rule' ? args.ruleId : request?.rule_id;
        if (id !== undefined) rules = rules.filter(rule => rule.rule_id !== id);
        revision += 1;
        if (request) rules.push({ ...request.draft, rule_id: `synthetic-${++nextId}`, introduced_revision: String(revision), retired_revision: null, created_at_ms: 1000, origin: 'custom' });
        history.set(revision, structuredClone(rules)); return response(current());
      }
      throw new Error(`unexpected synthetic command ${command}`);
    } }, __advanceSyntheticPriceRevision: () => { revision += 1; history.set(revision, structuredClone(rules)); } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '价格规则', exact: true }).click();
});

test('price editor preserves precise rates, null cache prices, immutable history and retirement', async ({ page }) => {
  const panel = page.getByRole('tabpanel', { name: '价格规则设置' });
  await expect(panel.getByText('此版本暂无价格规则')).toBeVisible();
  await panel.getByRole('button', { name: '新增规则', exact: true }).click();
  const editor = panel.getByRole('form', { name: '新增价格规则' });
  await expect(editor.getByLabel('输入单价 / 百万 Token', { exact: true })).toHaveValue('');
  await editor.getByLabel('提供方', { exact: true }).fill('synthetic-provider');
  await editor.getByLabel('确切模型标识', { exact: true }).fill('fixture-only-model');
  await editor.getByLabel('输入单价 / 百万 Token', { exact: true }).fill('9007199254740993.000000001');
  await editor.getByLabel('输出单价 / 百万 Token', { exact: true }).fill('0.000000001');
  await editor.getByLabel('开始时间（UTC）', { exact: true }).fill('2026-10-01T00:00:00.123');
  await editor.getByLabel('价格依据或链接（可留空）', { exact: true }).fill('Synthetic UI test fixture');
  await page.screenshot({ path: 'test-results/prices-editor.png', fullPage: true });
  await editor.getByRole('button', { name: '保存并发布版本' }).click();
  await expect(panel.getByText('当前价格版本 1', { exact: true })).toBeVisible();
  const row = panel.getByRole('row').filter({ hasText: 'fixture-only-model' });
  await expect(row.getByRole('cell', { name: '9007199254740993.000000001', exact: true })).toBeVisible();
  await expect(row.getByRole('cell', { name: '未知', exact: true })).toBeVisible();
  await expect(row.getByText('2026-10-01 00:00:00.123 UTC 起', { exact: true })).toBeVisible();
  await page.screenshot({ path: 'test-results/prices-list.png', fullPage: true });
  await page.setViewportSize({ width: 960, height: 680 });
  await page.screenshot({ path: 'test-results/prices-list-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.setViewportSize({ width: 1280, height: 860 });
  await row.getByRole('button', { name: '编辑', exact: true }).click();
  const replacement = panel.getByRole('form', { name: '替换价格规则' });
  await expect(replacement.getByLabel('开始时间（UTC）', { exact: true })).toHaveValue('2026-10-01T00:00:00.123');
  await replacement.getByLabel('缓存输入单价 / 百万 Token（可留空）', { exact: true }).fill('0');
  await replacement.getByRole('button', { name: '保存并发布版本' }).click();
  await expect(panel.getByText('当前价格版本 2', { exact: true })).toBeVisible();
  await expect(row.getByRole('cell', { name: '0', exact: true })).toBeVisible();
  await panel.getByLabel('历史价格版本').fill('1'); await panel.getByRole('button', { name: '查看', exact: true }).click();
  await expect(panel.getByText('历史价格版本 1', { exact: true })).toBeVisible();
  await expect(row.getByRole('cell', { name: '未知', exact: true })).toBeVisible();
  await expect(panel.getByRole('button', { name: '新增规则', exact: true })).toBeDisabled();
  await expect(row.getByText('只读', { exact: true })).toBeVisible();
  await panel.getByRole('button', { name: '刷新当前版本' }).click();
  await row.getByRole('button', { name: '退休', exact: true }).click();
  await expect(panel.getByText('当前价格版本 3', { exact: true })).toBeVisible();
  await expect(panel.getByText('此版本暂无价格规则')).toBeVisible();
  await page.setViewportSize({ width: 960, height: 680 });
  await page.screenshot({ path: 'test-results/prices-empty-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test('revision conflicts preserve unsaved prices even after manually refreshing current rules', async ({ page }) => {
  const panel = page.getByRole('tabpanel', { name: '价格规则设置' });
  await panel.getByRole('button', { name: '新增规则', exact: true }).click();
  const editor = panel.getByRole('form', { name: '新增价格规则' });
  await editor.getByLabel('提供方', { exact: true }).fill('synthetic-provider');
  await editor.getByLabel('确切模型标识', { exact: true }).fill('unsaved-fixture');
  await editor.getByLabel('输入单价 / 百万 Token', { exact: true }).fill('1.000000001');
  await editor.getByLabel('输出单价 / 百万 Token', { exact: true }).fill('0');
  await page.evaluate(() => (window as unknown as { __advanceSyntheticPriceRevision: () => void }).__advanceSyntheticPriceRevision());
  await panel.getByRole('button', { name: '刷新当前版本' }).click();
  await expect(panel.getByText('当前价格版本 1', { exact: true })).toBeVisible();
  await editor.getByRole('button', { name: '保存并发布版本' }).click();
  await expect(panel.getByRole('alert')).toContainText('配置或作业状态已发生变化');
  await expect(editor.getByLabel('输入单价 / 百万 Token', { exact: true })).toHaveValue('1.000000001');
  await expect(editor.getByText('基于版本 0 发布。模型与提供方须填写日志中的确切标识。')).toBeVisible();
  await editor.getByRole('button', { name: '取消编辑' }).click();
  await panel.getByRole('button', { name: '新增规则', exact: true }).click();
  await expect(editor.getByText('基于版本 1 发布。模型与提供方须填写日志中的确切标识。')).toBeVisible();
  await editor.getByLabel('提供方', { exact: true }).fill('synthetic-provider');
  await editor.getByLabel('确切模型标识', { exact: true }).fill('conflict-fixture');
  await editor.getByLabel('输入单价 / 百万 Token', { exact: true }).fill('0');
  await editor.getByLabel('输出单价 / 百万 Token', { exact: true }).fill('0');
  await editor.getByRole('button', { name: '保存并发布版本' }).click();
  await expect(panel.getByText('当前价格版本 2', { exact: true })).toBeVisible();
  await panel.getByRole('button', { name: '新增规则', exact: true }).click();
  await editor.getByLabel('提供方', { exact: true }).fill('synthetic-provider');
  await editor.getByLabel('确切模型标识', { exact: true }).fill('conflict-fixture');
  await editor.getByLabel('输入单价 / 百万 Token', { exact: true }).fill('0');
  await editor.getByLabel('输出单价 / 百万 Token', { exact: true }).fill('0');
  await editor.getByRole('button', { name: '保存并发布版本' }).click();
  await expect(panel.getByRole('alert')).toContainText('同一范围和优先级的价格规则有效期重叠');
  await expect(editor.getByLabel('确切模型标识', { exact: true })).toHaveValue('conflict-fixture');
  await expect(panel.getByText('当前价格版本 2', { exact: true })).toBeVisible();
});
