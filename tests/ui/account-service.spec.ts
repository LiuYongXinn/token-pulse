import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';

type QA = { __quotaQA: { calls: () => { command: string; request?: Record<string, unknown>; selectionHandle?: string }[]; conflict: () => void; fail: (value: boolean) => void; privacy: () => void; authorization: () => void; listeners: () => number } };
test.beforeEach(async ({ page }) => {
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    // Isolated synthetic DTOs. No credentials, native service or production demo data.
    let revision = '9007199254740993', privacy = false, fail = false, authorization = false, serial = 0;
    let config = { settings_revision: revision, executable_display_path: null as string | null, home_display_path: null as string | null, executable_sha256: null as string | null, configured: false, auto_connect: false };
    let quota = { connection_epoch: 'synthetic-0', quota_revision: '0', state: 'disconnected', selected_limit_id: null as string | null, available_limits: [] as { limit_id: string; display_name: string | null }[], fetched_at_ms: null as number | null, last_attempt_at_ms: null as number | null, windows: [] as { window_id: string; duration_mins: number | null; used_percent: number | null; remaining_percent: number | null; resets_at_ms: number | null }[], error_code: null };
    const selections = new Map<string, typeof config>();
    const calls: { command: string; request?: Record<string, unknown>; selectionHandle?: string }[] = [];
    let callback = 0, event = 0;
    const callbacks = new Map<number, (event: unknown) => void>(), listeners = new Map<number, { event: string; handler: number }>();
    const notify = (name: string, payload: unknown) => { for (const [id, value] of listeners) if (value.event === name) callbacks.get(value.handler)?.({ event: name, id, payload }); };
    Object.assign(window, { isTauri: true, __quotaQA: { calls: () => calls, conflict: () => { revision = String(BigInt(revision) + 1n); config.settings_revision = revision; }, fail: (value: boolean) => { fail = value; }, privacy: () => { privacy = true; revision = String(BigInt(revision) + 1n); notify('display_policy_changed', { settings_revision: revision, privacy }); }, authorization: () => { authorization = true; }, listeners: () => [...listeners.values()].filter(v => ['settings_changed', 'account_quota_changed'].includes(v.event)).length },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: (_name: string, id: number) => { const value = listeners.get(id); if (value) callbacks.delete(value.handler); listeners.delete(id); } },
      __TAURI_INTERNALS__: { transformCallback: (fn: (event: unknown) => void) => { callbacks.set(++callback, fn); return callback; }, invoke: async (command: string, args: { requestId: string; request?: Record<string, unknown>; selectionHandle?: string; event?: string; handler?: number }) => {
        calls.push({ command, request: args.request, selectionHandle: args.selectionHandle });
        const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: revision, privacy }, data });
        if (command === 'plugin:event|listen') { listeners.set(++event, { event: args.event!, handler: args.handler! }); return event; }
        if (command === 'plugin:event|unlisten') return null;
        if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args as never));
        if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic-test', collector: 'not_configured', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
        if (command === 'get_sources') return response({ settings_revision: revision, sources: [] });
        if (command === 'get_account_service_config') return response({ ...config, settings_revision: revision });
        if (command === 'get_account_quota') return response(quota);
        if (command === 'cancel_account_service_selection') { selections.delete(args.selectionHandle!); return response(null); }
        const request = args.request!;
        if (command === 'choose_account_service') {
          if (request.expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' };
          const base = selections.get(String(request.base_selection_handle)) ?? config;
          const selected = { ...base, settings_revision: revision };
          if (request.kind === 'executable') Object.assign(selected, { executable_display_path: 'E:\\synthetic-account\\codex.exe', executable_sha256: 'a'.repeat(64), configured: true });
          if (request.kind === 'home') selected.home_display_path = 'E:\\synthetic-account\\home';
          if (request.kind === 'default_home') selected.home_display_path = null;
          const handle = `synthetic-capability-${++serial}`; selections.set(handle, selected); if (request.base_selection_handle) selections.delete(String(request.base_selection_handle));
          return response({ selection_handle: handle, preview: selected, expires_at_ms: Date.now() + 300000 });
        }
        if (command === 'save_account_service_config') {
          if (request.expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' };
          if (fail) throw { code: 'DB_WRITE_FAILED' };
          const selected = selections.get(String(request.selection_handle)); if (!selected) throw { code: 'STALE_CONFIRMATION' };
          revision = String(BigInt(revision) + 1n); config = { ...selected, auto_connect: Boolean(request.auto_connect), settings_revision: revision }; selections.delete(String(request.selection_handle));
          return response(config);
        }
        if (command === 'manage_account_connection') {
          if (request.expected_connection_epoch !== quota.connection_epoch) throw { code: 'QUOTA_STALE_EPOCH' };
          if (request.kind === 'connect') {
            if (request.expected_settings_revision !== revision || request.acknowledged_executable_sha256 !== config.executable_sha256) throw { code: 'STALE_CONFIRMATION' };
            quota = { ...quota, connection_epoch: `synthetic-${++serial}`, quota_revision: String(BigInt(quota.quota_revision) + 1n), state: authorization ? 'authorization_required' : 'ready', selected_limit_id: authorization ? null : 'codex', available_limits: authorization ? [] : [{ limit_id: 'codex', display_name: 'Codex 合成额度' }, { limit_id: 'other', display_name: null }], fetched_at_ms: authorization ? null : Date.now(), windows: authorization ? [] : [{ window_id: 'weekly', duration_mins: 10080, used_percent: 25, remaining_percent: 75, resets_at_ms: Date.now() + 86400000 }, { window_id: 'zero', duration_mins: 300, used_percent: 100, remaining_percent: 0, resets_at_ms: Date.now() - 1 }, { window_id: 'unknown', duration_mins: null, used_percent: null, remaining_percent: null, resets_at_ms: null }] };
          } else if (request.kind === 'disconnect') quota = { ...quota, connection_epoch: `synthetic-${++serial}`, quota_revision: String(BigInt(quota.quota_revision) + 1n), state: 'disconnected', available_limits: [], selected_limit_id: null, windows: [], fetched_at_ms: null };
          else { if (request.expected_quota_revision !== quota.quota_revision) throw { code: 'REVISION_CONFLICT' }; quota = { ...quota, selected_limit_id: String(request.limit_id), quota_revision: String(BigInt(quota.quota_revision) + 1n), windows: [] }; }
          return response(quota);
        }
        if (command === 'refresh_account_quota') return response({ status: 'rate_limited', retry_after_ms: null, quota });
        throw { code: 'INVALID_QUERY' };
      } },
    });
  });
});

test('explicit selection and save precede connection; quotas keep zero and unknown values', async ({ page }) => {
  await page.goto('/'); await page.getByRole('button', { name: '设置', exact: true }).click();
  const region = page.getByRole('region', { name: '账户额度连接' });
  await expect(region.getByText('尚未配置账户服务')).toBeVisible();
  await expect(region.getByRole('button', { name: '连接已保存服务' })).toBeDisabled();
  await region.getByRole('button', { name: '选择账户服务程序' }).click();
  await region.getByRole('button', { name: '选择账户服务 Home' }).click();
  await region.getByRole('checkbox').check();
  await expect(region.getByRole('button', { name: '连接已保存服务' })).toBeDisabled();
  await region.getByRole('button', { name: '保存账户连接配置' }).click();
  expect(await page.evaluate(() => (window as unknown as QA).__quotaQA.calls().filter(v => v.command === 'manage_account_connection'))).toEqual([]);
  await region.getByRole('button', { name: '连接已保存服务' }).click();
  await expect(region.getByText('剩余 75%', { exact: true })).toBeVisible();
  await expect(region.getByText('剩余 0%', { exact: true })).toBeVisible();
  await expect(region.getByText('剩余 —', { exact: true })).toBeVisible();
  await expect(region.getByText('重置：等待额度更新')).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as QA).__quotaQA.calls().filter(v => v.command === 'manage_account_connection')[0].request)).toEqual({ kind: 'connect', expected_settings_revision: '9007199254740994', expected_connection_epoch: 'synthetic-0', acknowledged_executable_sha256: 'a'.repeat(64) });
  await region.scrollIntoViewIfNeeded(); await page.screenshot({ path: 'test-results/account-service-settings.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await region.getByRole('button', { name: '刷新账户额度' }).click(); await expect(region.getByText('请在 稍后刷新额度。')).toBeVisible();
  await region.getByLabel('账户额度桶').selectOption('other'); await expect(region.getByText('剩余 75%')).toHaveCount(0);
  await region.getByRole('button', { name: '断开本次连接' }).click(); await expect(region.getByRole('status')).toContainText('未连接');
  await expect(region.getByRole('checkbox')).toBeChecked();
  await page.getByRole('button', { name: '模型', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__quotaQA.listeners())).toBe(1);
});

test('failed writes retain draft; revision conflict requires deliberate reselection', async ({ page }) => {
  await page.goto('/'); await page.getByRole('button', { name: '设置', exact: true }).click(); const region = page.getByRole('region', { name: '账户额度连接' });
  await region.getByRole('button', { name: '选择账户服务程序' }).click(); await page.evaluate(() => (window as unknown as QA).__quotaQA.fail(true));
  await region.getByRole('button', { name: '保存账户连接配置' }).click(); await expect(region.getByRole('alert')).toContainText('写入失败'); await expect(region.getByText('待保存的连接配置')).toBeVisible();
  await page.evaluate(() => { (window as unknown as QA).__quotaQA.fail(false); (window as unknown as QA).__quotaQA.conflict(); });
  await region.getByRole('button', { name: '刷新连接状态' }).click(); await region.getByRole('button', { name: '保存账户连接配置' }).click(); await expect(region.getByRole('alert')).toContainText('已发生变化');
  await region.getByRole('button', { name: '放弃账户配置草稿' }).click(); await region.getByRole('button', { name: '选择账户服务程序' }).click(); await region.getByRole('button', { name: '保存账户连接配置' }).click();
  await expect(region.getByText('连接配置已保存；用于下次连接或启动，当前连接保持原状。')).toBeVisible();
});

test('privacy clears path and capabilities; authorization required stays distinct from quota', async ({ page }) => {
  await page.goto('/'); await page.getByRole('button', { name: '设置', exact: true }).click(); const region = page.getByRole('region', { name: '账户额度连接' });
  await region.getByRole('button', { name: '选择账户服务程序' }).click(); await region.getByRole('button', { name: '保存账户连接配置' }).click();
  await page.evaluate(() => (window as unknown as QA).__quotaQA.authorization()); await region.getByRole('button', { name: '连接已保存服务' }).click(); await expect(region.getByText('需要账户授权', { exact: true })).toBeVisible();
  await region.getByRole('button', { name: '选择账户服务 Home' }).click();
  await page.evaluate(() => (window as unknown as QA).__quotaQA.privacy());
  await expect(region.getByText('E:\\synthetic-account\\home', { exact: true })).toHaveCount(0); await expect(region.getByText('E:\\synthetic-account\\codex.exe', { exact: true })).toHaveCount(0);
  await expect(region.getByRole('button', { name: '连接已保存服务' })).toBeDisabled();
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__quotaQA.calls().filter(v => v.command === 'cancel_account_service_selection').length)).toBeGreaterThan(0);
});
