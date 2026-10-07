import { expect, test, type Page } from '@playwright/test';
import type { TaskbarPreferences, TaskbarRuntimeSnapshot } from '../../ui/src/shared/generated/contracts';
type QA = { __taskbarQA: { calls: () => { command: string; request: unknown }[]; mini: (visible: boolean) => void; rejectMini: (value: boolean) => void; holdMini: () => void; releaseMini: () => void; external: () => void; reject: (value: boolean) => void; failRead: (value: boolean) => void; status: (value: Partial<TaskbarRuntimeSnapshot>) => void; listeners: () => number } };

test.beforeEach(async ({ page }) => {
  // Browser-only synthetic protocol. Production never loads this bridge or these values.
  await page.addInitScript(() => {
    let revision = '9007199254740993', reject = false, failRead = false;
    let miniVisible = false, rejectMini = false, holdMini = false;
    const miniWaits: (() => void)[] = [];
    let preferences = { enabled: false, display: { layout: 'two_rows', show_tokens: true, show_costs: true, show_quota: true, show_weekly_reset: true }, position: 'notification_left', fallback_to_mini: true };
    let status = { revision: '9007199254740993', state: 'disabled', applied_settings_revision: null, issue: null, error: null, compact: null, fallback_visible: null, fallback_error: null, action_error: null, last_cleanup: null, last_snapshot_at_ms: null };
    const calls: { command: string; request: unknown }[] = [], callbacks = new Map<number, (value: unknown) => void>(), listeners = new Map<number, { event: string; handler: number }>();
    let callbackId = 0, eventId = 0;
    const notify = (event: string) => { for (const [id, listener] of listeners) if (listener.event === event) callbacks.get(listener.handler)?.({ event, id, payload: event === 'taskbar_status_changed' ? status : { settings_revision: revision } }); };
    Object.assign(window, {
      isTauri: true,
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: (_event: string, id: number) => { const listener = listeners.get(id); if (listener) callbacks.delete(listener.handler); listeners.delete(id); } },
      __TAURI_INTERNALS__: {
        transformCallback: (callback: (value: unknown) => void) => { callbacks.set(++callbackId, callback); return callbackId; },
        invoke: async (command: string, args: Record<string, unknown>) => {
          calls.push({ command, request: args.request ?? args.action });
          const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, ...(command.includes('taskbar') ? {} : { display_policy: { settings_revision: '1', privacy: false } }), data });
          if (command === 'plugin:event|listen') { listeners.set(++eventId, { event: String(args.event), handler: Number(args.handler) }); return eventId; }
          if (command === 'plugin:event|unlisten') return null;
          if (command === 'get_app_status') return response({ version: 'synthetic-taskbar-test', development: true, data_directory: 'synthetic', collector: 'ready', storage: 'error', storage_error: null, quota: 'not_configured', taskbar: 'not_configured' });
          if (command === 'get_sources') return response({ settings_revision: revision, sources: [] });
          if (command === 'list_jobs') return response([]);
          if (command === 'get_rebuild_status') return response(null);
          if (command === 'query_diagnostics') return response({data_revision:'1',issues:[],has_more:false});
          if (command === 'get_taskbar_preferences') { if (failRead) throw { code: 'DB_READ_FAILED' }; return response({ preferences: structuredClone(preferences), settings_revision: revision }); }
          if (command === 'get_taskbar_status') return response(structuredClone(status));
          if (command === 'set_taskbar_preferences') {
            const request = args.request as { preferences: typeof preferences; expected_settings_revision: string };
            if (request.expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' };
            if (reject) throw { code: 'DB_WRITE_FAILED' };
            preferences = structuredClone(request.preferences); revision = String(BigInt(revision) + 1n); notify('settings_changed');
            // Deliberately keep actual native status unchanged: persistence cannot prove embedding.
            return response({ preferences: structuredClone(preferences), settings_revision: revision });
          }
          if (command === 'get_mini_visibility') return response(miniVisible);
          if (command === 'perform_window_action') {
            if (args.action === 'show_mini' || args.action === 'hide_mini') {
              if (rejectMini) throw { code: 'WINDOW_UNAVAILABLE' };
              if (holdMini) { holdMini = false; await new Promise<void>(resolve => miniWaits.push(resolve)); }
              miniVisible = args.action === 'show_mini'; notify('mini_visibility_changed');
            }
            return response(null);
          }
          if (command === 'retry_taskbar_embed') return response(null);
          throw new Error('Unexpected synthetic taskbar command ' + command);
        },
      },
      __taskbarQA: {
        mini: (visible: boolean) => { miniVisible = visible; notify('mini_visibility_changed'); },
        rejectMini: (value: boolean) => { rejectMini = value; },
        holdMini: () => { holdMini = true; }, releaseMini: () => miniWaits.splice(0).forEach(resolve => resolve()),
        calls: () => calls, external: () => { revision = String(BigInt(revision) + 1n); notify('settings_changed'); }, reject: (value: boolean) => { reject = value; }, failRead: (value: boolean) => { failRead = value; },
        status: (value: unknown) => { status = { ...status, ...value as typeof status }; notify('taskbar_status_changed'); },
        listeners: () => [...listeners.values()].filter(v => v.event === 'taskbar_status_changed').length,
      },
    });
  });
});
async function open(page: Page) { await page.goto('/'); await page.getByRole('button', { name: '设置', exact: true }).click(); await page.getByRole('tab', { name: '任务栏显示' }).click(); const panel = page.getByRole('tabpanel', { name: '任务栏显示设置' }); await expect(panel.getByRole('checkbox', { name: '启用任务栏显示' })).toBeEnabled(); return panel; }

test('mini shortcuts toggle both ways and synchronize external visibility and failures', async ({ page }, testInfo) => {
  await page.goto('/');
  const sidebar = page.locator('.sidebar-bottom'), header = page.locator('.head-actions');
  await expect(sidebar.getByRole('button', { name: '显示悬浮窗', exact: true })).toBeEnabled();
  await sidebar.getByRole('button', { name: '显示悬浮窗', exact: true }).click();
  await expect(sidebar.getByRole('button', { name: '隐藏悬浮窗' })).toHaveAttribute('aria-pressed', 'true');
  await expect(header.getByRole('button', { name: '隐藏小窗' })).toBeEnabled();
  await page.locator('.sidebar-bottom').screenshot({ path: testInfo.outputPath('display-shortcuts-visible.png') });
  await header.getByRole('button', { name: '隐藏小窗' }).click();
  await expect(sidebar.getByRole('button', { name: '显示悬浮窗' })).toHaveAttribute('aria-pressed', 'false');
  await expect(header.getByRole('button', { name: '显示小窗' })).toBeEnabled();
  expect(await page.evaluate(() => (window as unknown as QA).__taskbarQA.calls().filter(v => v.command === 'perform_window_action').map(v => v.request))).toEqual(['show_mini', 'hide_mini']);
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.mini(true));
  await expect(sidebar.getByRole('button', { name: '隐藏悬浮窗' })).toBeVisible();
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.mini(false));
  await expect(header.getByRole('button', { name: '显示小窗' })).toBeVisible();
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.rejectMini(true));
  await sidebar.getByRole('button', { name: '显示悬浮窗' }).click();
  await expect(page.getByRole('alert').filter({ hasText: '悬浮窗切换失败' })).toBeVisible();
  await expect(sidebar.getByRole('button', { name: '显示悬浮窗' })).toHaveAttribute('aria-pressed', 'false');
  await page.evaluate(() => { (window as unknown as QA).__taskbarQA.rejectMini(false); (window as unknown as QA).__taskbarQA.holdMini(); });
  await header.getByRole('button', { name: '显示小窗' }).click();
  await expect(sidebar.getByRole('button', { name: '正在处理…' })).toBeDisabled();
  await expect(header.getByRole('button', { name: '正在处理…' })).toBeDisabled();
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.releaseMini());
  await expect(sidebar.getByRole('button', { name: '隐藏悬浮窗' })).toBeEnabled();
  await expect(page.getByRole('alert').filter({ hasText: '悬浮窗切换失败' })).toHaveCount(0);
  await sidebar.getByRole('button', { name: '隐藏悬浮窗' }).click();
  await expect(sidebar.getByRole('button', { name: '显示悬浮窗' })).toBeEnabled();
});

test('sidebar toggles taskbar, stays in sync with settings and preserves display options', async ({ page }, testInfo) => {
  const panel = await open(page), sidebar = page.locator('.sidebar-bottom');
  await expect(sidebar.getByRole('button', { name: '显示悬浮窗', exact: true })).toBeVisible();
  await sidebar.getByRole('button', { name: '显示任务栏', exact: true }).click();
  await expect(sidebar.getByRole('button', { name: '隐藏任务栏', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(panel.getByRole('checkbox', { name: '启用任务栏显示' })).toBeChecked();
  await panel.getByRole('checkbox', { name: '费用估算', exact: true }).uncheck();
  await panel.getByRole('checkbox', { name: '任务栏不可用时显示悬浮窗', exact: true }).uncheck();
  await panel.getByRole('combobox', { name: '任务栏显示布局' }).selectOption('single_row');
  await panel.getByRole('combobox', { name: '任务栏显示位置' }).selectOption('application_right');
  await panel.getByRole('button', { name: '保存任务栏设置' }).click();
  await expect(panel.getByRole('button', { name: '保存任务栏设置' })).toBeDisabled();
  await sidebar.getByRole('button', { name: '隐藏任务栏', exact: true }).click();
  await expect(sidebar.getByRole('button', { name: '显示任务栏', exact: true })).toHaveAttribute('aria-pressed', 'false');
  await expect(panel.getByRole('checkbox', { name: '启用任务栏显示' })).not.toBeChecked();
  expect(await page.evaluate(() => (window as unknown as QA).__taskbarQA.calls().filter(v => v.command === 'set_taskbar_preferences').at(-1)?.request)).toEqual({
    preferences: { enabled: false, display: { layout: 'single_row', show_tokens: true, show_costs: false, show_quota: true, show_weekly_reset: true }, position: 'application_right', fallback_to_mini: false },
    expected_settings_revision: '9007199254740995',
  });
  await panel.getByRole('checkbox', { name: '启用任务栏显示' }).check();
  await panel.getByRole('button', { name: '保存任务栏设置' }).click();
  await expect(sidebar.getByRole('button', { name: '隐藏任务栏', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '总览', exact: true }).click();
  await expect(sidebar.getByRole('button', { name: '隐藏任务栏', exact: true })).toBeVisible();
  await page.locator('.sidebar').screenshot({ path: testInfo.outputPath('sidebar-taskbar-light-1280.png') });
  await page.setViewportSize({ width: 760, height: 860 });
  await page.evaluate(() => document.documentElement.dataset.theme = 'dark');
  await expect(sidebar.getByRole('button', { name: '隐藏任务栏', exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.locator('.sidebar').screenshot({ path: testInfo.outputPath('sidebar-taskbar-dark-760.png') });
});

test('sidebar reports read and write failures without claiming the toggle succeeded', async ({ page }) => {
  await page.addInitScript(() => (window as unknown as QA).__taskbarQA.failRead(true));
  await page.goto('/');
  const sidebar = page.locator('.sidebar-bottom');
  await expect(sidebar.getByRole('button', { name: '显示任务栏', exact: true })).toBeDisabled();
  await expect(sidebar.getByRole('alert')).toContainText('读取失败');
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.failRead(false));
  await sidebar.getByRole('button', { name: '重试任务栏设置' }).click();
  await expect(sidebar.getByRole('alert')).toHaveCount(0);
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.reject(true));
  await sidebar.getByRole('button', { name: '显示任务栏', exact: true }).click();
  await expect(sidebar.getByRole('alert')).toContainText('写入失败');
  await expect(sidebar.getByRole('button', { name: '显示任务栏', exact: true })).toHaveAttribute('aria-pressed', 'false');
  await expect(sidebar.getByRole('button', { name: '隐藏任务栏', exact: true })).toHaveCount(0);
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.reject(false));
  await sidebar.getByRole('button', { name: '显示任务栏', exact: true }).click();
  await expect(sidebar.getByRole('button', { name: '隐藏任务栏', exact: true })).toBeVisible();
  await expect(sidebar.getByRole('alert')).toHaveCount(0);
});

test('real DTO switches use exact CAS, saved enabled does not invent embedded or fallback', async ({ page }) => {
  const panel = await open(page);
  await expect(panel.getByRole('status')).toHaveText('已关闭'); await expect(panel.getByText('尚未确认', { exact: true })).toHaveCount(2);
  await panel.getByRole('checkbox', { name: '启用任务栏显示' }).check();
  await panel.getByRole('checkbox', { name: '费用估算', exact: true }).uncheck();
  await panel.getByRole('combobox', { name: '任务栏显示布局' }).selectOption('single_row');
  await panel.getByRole('combobox', { name: '任务栏显示位置' }).selectOption('application_right');
  await panel.getByRole('button', { name: '保存任务栏设置' }).click();
  await expect(panel.getByRole('button', { name: '保存任务栏设置' })).toBeDisabled(); await expect(panel.getByRole('status')).toHaveText('已关闭');
  const expected: TaskbarPreferences = { enabled: true, display: { layout: 'single_row', show_tokens: true, show_costs: false, show_quota: true, show_weekly_reset: true }, position: 'application_right', fallback_to_mini: true };
  expect(await page.evaluate(() => (window as unknown as QA).__taskbarQA.calls().filter(v => v.command === 'set_taskbar_preferences').at(-1)?.request)).toEqual({ preferences: expected, expected_settings_revision: '9007199254740993' });
  await expect(panel.getByRole('button', { name: '重试任务栏嵌入' })).toBeDisabled();
  await panel.getByRole('button', { name: '显示悬浮窗', exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as QA).__taskbarQA.calls().filter(v => v.command === 'perform_window_action').length)).toBe(1);
});

test('conflicts and write failure retain input; empty display selection is refused before invoke', async ({ page }) => {
  const panel = await open(page);
  await panel.getByRole('checkbox', { name: '启用任务栏显示' }).check();
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.external()); await panel.getByRole('button', { name: '刷新任务栏设置' }).click();
  await expect(panel.getByRole('checkbox', { name: '启用任务栏显示' })).toBeChecked();
  await panel.getByRole('button', { name: '保存任务栏设置' }).click(); await expect(panel.getByRole('alert')).toContainText('已发生变化');
  await panel.getByRole('button', { name: '重置任务栏草稿' }).click(); await expect(panel.getByRole('checkbox', { name: '启用任务栏显示' })).not.toBeChecked();
  await panel.getByRole('checkbox', { name: '启用任务栏显示' }).check(); await page.evaluate(() => (window as unknown as QA).__taskbarQA.reject(true));
  await panel.getByRole('button', { name: '保存任务栏设置' }).click(); await expect(panel.getByRole('alert')).toContainText('写入失败'); await expect(panel.getByRole('checkbox', { name: '启用任务栏显示' })).toBeChecked();
  await panel.getByRole('button', { name: '重置任务栏草稿' }).click();
  for (const name of ['Token 用量', '费用估算', '账户剩余额度', '周重置时间']) await panel.getByRole('checkbox', { name, exact: true }).uncheck();
  await expect(panel.getByRole('alert')).toContainText('至少选择一项'); await expect(panel.getByRole('button', { name: '保存任务栏设置' })).toBeDisabled();
  expect(await page.evaluate(() => (window as unknown as QA).__taskbarQA.calls().filter(v => v.command === 'set_taskbar_preferences').length)).toBe(2);
});

test('runtime events requery, lower revisions cannot roll back, diagnostics shares real state and unsubscribes', async ({ page }) => {
  const panel = await open(page); await panel.getByRole('checkbox', { name: '启用任务栏显示' }).check(); await panel.getByRole('button', { name: '保存任务栏设置' }).click();
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.status({ revision: '9007199254740995', state: 'unavailable', issue: 'insufficient_space', error: 'TASKBAR_EMBED_FAILED', last_cleanup: 'external_change' }));
  await expect(panel.getByRole('status')).toHaveText('任务栏显示不可用'); await expect(panel).toContainText('任务栏剩余空间不足'); await expect(panel.getByRole('button', { name: '重试任务栏嵌入' })).toBeEnabled();
  await panel.getByRole('button', { name: '重试任务栏嵌入' }).click();
  expect(await page.evaluate(() => (window as unknown as QA).__taskbarQA.calls().filter(v => v.command === 'retry_taskbar_embed').length)).toBe(1);
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.status({ revision: '9007199254740994', state: 'embedded', compact: false, issue: null, error: null }));
  await panel.getByRole('button', { name: '刷新任务栏设置' }).click(); await expect(panel.getByRole('status')).toHaveText('任务栏显示不可用');
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.status({ revision: '9007199254740996', state: 'embedded', compact: true, issue: null, error: null }));
  await expect(panel.getByRole('status')).toHaveText('已嵌入任务栏'); await expect(panel).toContainText('精简显示');
  await page.getByRole('button', { name: '采集诊断', exact: true }).click();
  const runtime = page.getByLabel('任务栏实际运行状态'); await expect(runtime.getByRole('status')).toHaveText('已嵌入任务栏');
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__taskbarQA.listeners())).toBe(1);
  await page.getByRole('button', { name: '模型', exact: true }).click(); await expect.poll(() => page.evaluate(() => (window as unknown as QA).__taskbarQA.listeners())).toBe(1); // visited panels keep one shared subscription
});

test('read failure preserves explicit draft and layout fits dark and light at ordinary window widths', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 960 }); const panel = await open(page);
  await panel.getByRole('checkbox', { name: '启用任务栏显示' }).check(); await page.evaluate(() => (window as unknown as QA).__taskbarQA.failRead(true));
  await panel.getByRole('button', { name: '刷新任务栏设置' }).click(); await expect(panel.getByRole('alert')).toContainText('偏好读取失败'); await expect(panel.getByRole('checkbox', { name: '启用任务栏显示' })).toBeChecked();
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.failRead(false)); await panel.getByRole('button', { name: '刷新任务栏设置' }).click(); await expect(panel.getByRole('alert')).toHaveCount(0);
  await page.screenshot({ path: 'test-results/taskbar-settings-dark-1280.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.setViewportSize({ width: 960, height: 900 }); await page.evaluate(() => document.documentElement.dataset.theme = 'light');
  await page.screenshot({ path: 'test-results/taskbar-settings-light-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('fallback preference persists independently; failed fallback remains unknown and success preserves native failure', async ({ page }) => {
  const panel = await open(page);
  await panel.getByRole('checkbox', { name: '任务栏不可用时显示悬浮窗', exact: true }).uncheck();
  await panel.getByRole('button', { name: '保存任务栏设置' }).click();
  await expect(panel.getByRole('checkbox', { name: '任务栏不可用时显示悬浮窗', exact: true })).not.toBeChecked();
  expect(await page.evaluate(() => ((window as unknown as QA).__taskbarQA.calls().filter(v => v.command === 'set_taskbar_preferences').at(-1)?.request as { preferences: TaskbarPreferences }).preferences.fallback_to_mini)).toBe(false);
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.status({ revision: '9007199254740994', state: 'unavailable', issue: 'host_unavailable', error: 'TASKBAR_EMBED_FAILED', fallback_visible: null, fallback_error: 'WINDOW_UNAVAILABLE' }));
  await expect(panel.getByRole('alert')).toContainText('小窗回退失败'); await expect(panel.getByRole('status')).toHaveText('任务栏显示不可用');
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.status({ revision: '9007199254740995', fallback_visible: true, fallback_error: null }));
  await expect(panel.getByRole('alert')).toHaveCount(0); await expect(panel.getByRole('status')).toHaveText('任务栏显示不可用'); await expect(panel).toContainText('回退小窗已显示');
  await page.screenshot({ path: 'test-results/taskbar-fallback-state-dark-1280.png', fullPage: true });
});


test('window action error is independent from embedding and fallback and clears after success', async ({ page }) => {
  const panel = await open(page);
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.status({ revision: '9007199254740994', state: 'embedded', compact: false, action_error: 'WINDOW_UNAVAILABLE' }));
  await expect(panel.getByRole('status')).toHaveText('已嵌入任务栏'); await expect(panel.getByRole('alert')).toContainText('任务栏窗口操作失败');
  await page.evaluate(() => (window as unknown as QA).__taskbarQA.status({ revision: '9007199254740995', action_error: null }));
  await expect(panel.getByRole('alert')).toHaveCount(0); await expect(panel.getByRole('status')).toHaveText('已嵌入任务栏');
});

