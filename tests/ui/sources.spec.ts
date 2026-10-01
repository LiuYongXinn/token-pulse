import { expect, test } from '@playwright/test';

test('source settings use explicitly mocked DTOs and preserve disabled and unknown states', async ({ page }) => {
  // Test-only native bridge. This fixture is never imported by the production application.
  await page.addInitScript(() => {
    Object.defineProperty(window, 'isTauri', { value: true });
    let revision = 1;
    const source = { source_id: 'synthetic-source', root_path: 'E:\\synthetic-fixture\\.codex', origin: 'custom', enabled: true, removed: false, readability: 'awaiting_directory', capabilities: { physical_identity: 'not_probed', byte_seek: 'not_probed', watcher: 'unavailable', polling_required: true }, last_scan_at_ms: null, last_success_at_ms: null, error: null };
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: { invoke: async (command: string, args: Record<string, unknown>) => {
      let data: unknown;
      if (command === 'get_app_status') data = { version: 'synthetic-test', development: true, data_directory: 'E:\\synthetic-test-data', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' };
      else if (command === 'get_sources') data = { settings_revision: String(revision), sources: [{ ...source }] };
      else if (command === 'manage_source') {
        if (args.expectedSettingsRevision !== String(revision)) throw { code: 'REVISION_CONFLICT' };
        const action = args.action as { kind: string };
        if (action.kind === 'pause') { source.enabled = false; source.readability = 'disabled'; }
        if (action.kind === 'resume') { source.enabled = true; source.removed = false; source.readability = 'awaiting_directory'; }
        if (action.kind === 'retain_remove') { source.enabled = false; source.removed = true; source.readability = 'disabled'; }
        revision += 1;
        data = { settings_revision: String(revision), sources: [{ ...source }] };
      } else throw { code: 'INVALID_QUERY' };
      return { api_version: 1, request_id: args.requestId, data };
    } } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '设置', exact: true }).click();
  const panel = page.getByRole('tabpanel', { name: '数据来源设置' });
  await expect(panel.getByText('E:\\synthetic-fixture\\.codex', { exact: true })).toBeVisible();
  await expect(panel.getByText('等待目录出现')).toBeVisible();
  await expect(panel.getByText('尚无成功记录')).toHaveCount(2);
  await expect(panel.getByText('尚未探测', { exact: true })).toBeVisible();
  await panel.getByRole('button', { name: '暂停采集' }).click();
  await expect(panel.getByText('已暂停', { exact: true })).toBeVisible();
  await panel.getByRole('button', { name: '恢复采集' }).click();
  await panel.getByRole('button', { name: '移除来源并保留历史' }).click();
  await expect(panel.getByText('已移除 · 历史保留')).toBeVisible();
  await expect(panel.getByRole('button', { name: '恢复采集' })).toBeVisible();
});
