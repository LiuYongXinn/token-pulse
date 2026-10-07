import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';

test('source settings use explicitly mocked DTOs and preserve disabled and unknown states', async ({ page }) => {
  // Test-only native bridge. This fixture is never imported by the production application.
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    Object.defineProperty(window, 'isTauri', { value: true });
    let revision = 1;
    let readyAt = 0;
    const source = { source_id: 'synthetic-source', root_path: 'E:\\synthetic-fixture\\.codex', origin: 'custom', enabled: true, removed: false, readability: 'awaiting_directory', capabilities: { physical_identity: 'not_probed', byte_seek: 'not_probed', watcher: 'unavailable', polling_required: true }, last_scan_at_ms: null, last_success_at_ms: null, error: null };
    Object.assign(window, { __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} } });
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: { transformCallback: () => 0, invoke: async (command: string, args: Record<string, unknown>) => {
      let data: unknown;
      if (command === 'plugin:event|listen' || command === 'plugin:event|unlisten') return 0;
      if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return { api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data: window.__syntheticCalendar(command, args) };
      if (command === 'get_app_status') data = { version: 'synthetic-test', development: true, data_directory: 'E:\\synthetic-test-data', collector: source.enabled && Date.now() >= readyAt ? 'ready' : 'not_configured', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' };
      else if (command === 'get_account_service_config') data = { settings_revision: String(revision), executable_display_path: null, home_display_path: null, executable_sha256: null, configured: false, auto_connect: false };
      else if (command === 'get_account_quota') data = { connection_epoch: 'synthetic-disconnected', quota_revision: '0', state: 'disconnected', selected_limit_id: null, available_limits: [], fetched_at_ms: null, last_attempt_at_ms: null, windows: [], error_code: null };
      else if (command === 'get_sources') data = { settings_revision: String(revision), sources: [{ ...source }] };
      else if (command === 'manage_source') {
        if (args.expectedSettingsRevision !== String(revision)) throw { code: 'REVISION_CONFLICT' };
        const action = args.action as { kind: string };
        if (action.kind === 'pause') { source.enabled = false; source.readability = 'disabled'; }
        if (action.kind === 'resume') { source.enabled = true; source.removed = false; source.readability = 'awaiting_directory'; readyAt = Date.now() + 1000; }
        if (action.kind === 'retain_remove') { source.enabled = false; source.removed = true; source.readability = 'disabled'; }
        revision += 1;
        data = { settings_revision: String(revision), sources: [{ ...source }] };
      } else throw { code: 'INVALID_QUERY' };
      return { api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data };
    } } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '设置', exact: true }).click();
  const panel = page.getByRole('tabpanel', { name: '数据来源设置' });
  await expect(panel.locator('.source-list .source-path')).toHaveText('E:\\synthetic-fixture\\.codex');
  await expect(panel.getByText('等待目录出现')).toBeVisible();
  await expect(panel.getByText('尚无成功记录')).toHaveCount(2);
  await expect(panel.getByText('文件监听能力', { exact: true })).toHaveCount(0);
  await panel.getByRole('button', { name: '暂停采集' }).click();
  await expect(panel.getByText('已暂停', { exact: true })).toBeVisible();
  const status = page.locator('.sidebar-bottom');
  await expect(status).toContainText('采集已暂停 · 历史保留');
  await panel.getByRole('button', { name: '恢复采集' }).click();
  // The source mutation returns before the service acknowledges its new configuration.
  // No manual refresh: a later runtime result must update the status on its own.
  await expect(status).toContainText('正在采集本地来源');
  await page.evaluate(() => {
    const control = window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args: Record<string, unknown>) => Promise<{ data: { collector: string } }> }; releaseOldPoll?: () => void };
    const invoke = control.__TAURI_INTERNALS__.invoke;
    let held = false;
    control.__TAURI_INTERNALS__.invoke = async (command, args) => {
      const response = await invoke(command, args);
      if (command === 'get_app_status' && !held) {
        held = true;
        response.data.collector = 'error';
        return new Promise(resolve => { control.releaseOldPoll = () => resolve(response); });
      }
      return response;
    };
  });
  await expect.poll(() => page.evaluate(() => typeof (window as unknown as { releaseOldPoll?: () => void }).releaseOldPoll)).toBe('function');
  await panel.getByRole('button', { name: '暂停采集' }).click();
  await expect(status).toContainText('采集已暂停 · 历史保留');
  await page.evaluate(async () => {
    (window as unknown as { releaseOldPoll: () => void }).releaseOldPoll();
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  });
  await expect(status).toContainText('采集已暂停 · 历史保留');
  await expect(status).not.toContainText('采集需要处理');
  await panel.getByRole('button', { name: '恢复采集' }).click();
  await panel.getByRole('button', { name: '移除来源并保留历史' }).click();
  await expect(panel.getByText('已移除 · 历史保留')).toBeVisible();
  await expect(status).toContainText('采集已停止 · 历史保留');
  await expect(panel.getByRole('button', { name: '恢复采集' })).toBeVisible();
});
