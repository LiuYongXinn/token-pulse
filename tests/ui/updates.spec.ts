import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';
type QA = { __updatesQA: { set: (phase: string, overrides?: Record<string, unknown>) => void; calls: () => { command: string; request: unknown }[]; late: () => void; deliver: () => void; privacy: () => void; subscriptions: () => number } };
test.beforeEach(async ({ page }) => {
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    // Explicit synthetic IPC fixtures. Never used by the shipped application.
    Object.defineProperty(window, 'isTauri', { value: true });
    let serial = 0, revision = '90071992547409930', privacy = false, settingsRevision = '1', delayed = false;
    let release = () => {};
    let state: Record<string, unknown> = { update_revision: revision, phase: 'idle', current_version: '0.1.0', release: null, last_checked_at_ms: null, downloaded_bytes: null, total_bytes: null, issue: null };
    const callbacks = new Map<number, (event: unknown) => void>(), listeners: { id: number; event: string; handler: number }[] = [], calls: { command: string; request: unknown }[] = [];
    const emit = (event: string, payload: unknown) => listeners.filter(l => l.event === event).forEach(l => callbacks.get(l.handler)?.({ event, payload }));
    const set = (phase: string, overrides: Record<string, unknown> = {}) => { revision = String(BigInt(revision) + 1n); state = { ...state, phase, update_revision: revision, ...overrides }; emit('updates_changed', { phase: 'ready_to_install', update_revision: '999999999999999999999' }); };
    Object.assign(window, { __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} }, __updatesQA: {
      set, calls: () => [...calls], subscriptions: () => listeners.filter(l => l.event === 'updates_changed').length, late: () => { delayed = true; }, deliver: () => release(), privacy: () => { privacy = !privacy; settingsRevision = String(BigInt(settingsRevision) + 1n); emit('display_policy_changed', { settings_revision: settingsRevision, privacy }); },
    } });
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: { transformCallback: (fn: (event: unknown) => void) => { callbacks.set(++serial, fn); return serial; }, invoke: async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, request: args.request });
      if (command === 'plugin:event|listen') { const id = ++serial; listeners.push({ id, event: String(args.event), handler: Number(args.handler) }); return id; }
      if (command === 'plugin:event|unlisten') { const i = listeners.findIndex(l => l.id === args.eventId); if (i >= 0) listeners.splice(i, 1); return null; }
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: settingsRevision, privacy }, data });
      if (command === 'get_display_settings') return response({ settings_version: 1, settings_revision: settingsRevision, preferences: { theme: 'dark', privacy, display_timezone: 'Asia/Shanghai' } });
      if (command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
      if (command === 'get_main_navigation' || command === 'get_mini_stats_request') return response(null);
      if (command === 'get_app_status') return response({ version: '0.1.0', development: false, data_directory: 'synthetic', storage: 'error', storage_error: 'DB_WRITE_FAILED', collector: 'ready', quota: 'not_configured', taskbar: 'not_implemented' });
      if (command === 'get_sources') return response({ settings_revision: settingsRevision, sources: [] });
      if (command === 'get_account_service_config') return response({ settings_revision: settingsRevision, executable_display_path: null, home_display_path: null, executable_sha256: null, configured: false, auto_connect: false });
      if (command === 'get_account_quota') return response({ connection_epoch: 'synthetic', quota_revision: '0', state: 'disconnected', windows: [], available_limits: [], selected_limit_id: null, fetched_at_ms: null, last_attempt_at_ms: null, error_code: null });
      if (command === 'get_notify_integrations') return response({ ready: false, listener_count: 0, service_issue: null, registrations: [], registry_issue: null, redacted: privacy });
      if (command === 'get_update_status') { const snapshot = response({ ...state }); if (delayed) { delayed = false; return new Promise(resolve => { release = () => resolve(snapshot); }); } return snapshot; }
      if (command === 'check_for_updates') { set('checking', { release: null, issue: null, downloaded_bytes: null, total_bytes: null }); return response({ ...state }); }
      if (command === 'download_update' || command === 'install_update') {
        if ((args.request as { expected_update_revision: string }).expected_update_revision !== revision) throw { code: 'REVISION_CONFLICT' };
        set(command === 'download_update' ? 'downloading' : 'installing', command === 'download_update' ? { downloaded_bytes: '0', total_bytes: null } : {}); return response({ ...state });
      }
      throw { code: 'INVALID_QUERY' };
    } } });
  });
});
async function open(page: import('@playwright/test').Page) {
  await page.goto('/'); await page.getByRole('button', { name: '设置', exact: true }).click(); await page.getByRole('tab', { name: '软件更新', exact: true }).click();
  const panel = page.getByRole('tabpanel', { name: '软件更新', exact: true }); await expect(panel.getByRole('status')).toHaveText('尚未检查更新'); return panel;
}
test('missing configuration and failed verification cannot claim current or allow installation', async ({ page }) => {
  const panel = await open(page);
  await expect(panel).toContainText('最近成功检查尚未提供');
  await page.evaluate(() => (window as unknown as QA).__updatesQA.set('unavailable', { issue: 'publication_not_configured' }));
  await expect(panel.getByRole('status')).toHaveText('更新不可用'); await expect(panel.getByRole('button', { name: '检查更新', exact: true })).toBeDisabled();
  await page.evaluate(() => (window as unknown as QA).__updatesQA.set('error', { issue: 'signature_invalid', release: { version: '0.2.0', notes: '<script>window.BAD=true</script>', published_at_ms: null } }));
  await expect(panel.getByRole('alert')).toContainText('签名或版本校验失败'); await expect(panel.getByRole('button', { name: '安装更新', exact: true })).toHaveCount(0); await expect(panel.getByRole('region', { name: '版本说明' })).toContainText('<script>');
  expect(await page.evaluate(() => 'BAD' in window)).toBe(false);
  await panel.getByRole('button', { name: '检查更新', exact: true }).click(); await expect(panel.getByRole('status')).toHaveText('正在检查更新'); await expect(panel.getByRole('button', { name: '检查更新', exact: true })).toBeDisabled();
  await page.evaluate(() => (window as unknown as QA).__updatesQA.set('current', { last_checked_at_ms: 0 })); await expect(panel.getByRole('status')).toHaveText('当前已是最新版本'); await expect(panel).toContainText('1970年1月1日');
});
test('exact progress and revisions survive stale reads, private changes and empty invalidation events', async ({ page }) => {
  const panel = await open(page);
  await page.evaluate(() => (window as unknown as QA).__updatesQA.set('available', { release: { version: '0.2.0', notes: 'Synthetic release for update UI acceptance.', published_at_ms: null } }));
  await expect(panel.getByRole('status')).toHaveText('发现新版本'); await panel.getByRole('button', { name: '下载更新', exact: true }).click(); await expect(panel).toContainText('0 字节已下载 / 总大小未知');
  const calls = await page.evaluate(() => (window as unknown as QA).__updatesQA.calls()); expect(calls.find(c => c.command === 'download_update')?.request).toEqual({ expected_update_revision: '90071992547409931' });
  await page.evaluate(() => (window as unknown as QA).__updatesQA.late()); await panel.getByRole('button', { name: '刷新更新状态' }).click();
  await page.evaluate(() => (window as unknown as QA).__updatesQA.set('verifying', { downloaded_bytes: '90071992547409930', total_bytes: '90071992547409930' }));
  await expect(panel.getByRole('status')).toHaveText('正在验证签名'); await expect(panel).toContainText('90071992547409930 字节已下载 / 90071992547409930 字节'); await expect(panel.getByRole('progressbar')).toHaveAttribute('value', '100');
  await page.evaluate(() => (window as unknown as QA).__updatesQA.deliver()); await expect(panel.getByRole('status')).toHaveText('正在验证签名');
  await page.evaluate(() => (window as unknown as QA).__updatesQA.privacy()); await expect(panel.getByRole('status')).toHaveText('正在验证签名'); await expect(panel.getByRole('button', { name: '安装更新', exact: true })).toHaveCount(0);
});
test('install review binds the displayed version and exact revision; stale review cannot install', async ({ page }) => {
  const panel = await open(page);
  await page.evaluate(() => (window as unknown as QA).__updatesQA.set('ready_to_install', { release: { version: '0.2.0', notes: 'Synthetic release: signed provider acceptance is separate.', published_at_ms: null }, downloaded_bytes: '1024', total_bytes: '1024' }));
  await panel.getByRole('button', { name: '安装更新', exact: true }).click(); const review = panel.getByRole('region', { name: '确认安装更新' }); await expect(review).toContainText('0.2.0');
  expect((await page.evaluate(() => (window as unknown as QA).__updatesQA.calls())).filter(c => c.command === 'install_update')).toHaveLength(0);
  await page.evaluate(() => (window as unknown as QA).__updatesQA.set('ready_to_install', { release: { version: '0.3.0', notes: 'New synthetic release.', published_at_ms: null } }));
  await expect(review.getByRole('alert')).toContainText('状态已变化'); await expect(review.getByRole('button', { name: '确认安装并关闭应用' })).toBeDisabled();
  await review.getByRole('button', { name: '取消安装' }).click(); await panel.getByRole('button', { name: '安装更新', exact: true }).click(); await expect(review).toContainText('0.3.0');
  await panel.screenshot({ path: 'test-results/updates-dark-review.png' }); await page.setViewportSize({ width: 960, height: 900 }); await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; }); await panel.screenshot({ path: 'test-results/updates-light-review.png' });
  await review.getByRole('button', { name: '确认安装并关闭应用' }).click(); await expect(panel.getByRole('status')).toHaveText('正在启动安装器');
  expect((await page.evaluate(() => (window as unknown as QA).__updatesQA.calls())).filter(c => c.command === 'install_update')).toEqual([{ command: 'install_update', request: { expected_update_revision: '90071992547409932' } }]);
});

test('leaving the page releases subscription and late reads cannot overwrite a reopened page', async ({ page }) => {
  const panel = await open(page);
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__updatesQA.subscriptions())).toBe(1);
  await page.evaluate(() => (window as unknown as QA).__updatesQA.late()); await panel.getByRole('button', { name: '刷新更新状态' }).click();
  await page.getByRole('tab', { name: '数据来源', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__updatesQA.subscriptions())).toBe(0);
  await page.evaluate(() => (window as unknown as QA).__updatesQA.set('available', { release: { version: '0.4.0', notes: null, published_at_ms: null } }));
  await page.getByRole('tab', { name: '软件更新', exact: true }).click(); await expect(panel.getByRole('status')).toHaveText('发现新版本');
  await page.evaluate(() => (window as unknown as QA).__updatesQA.deliver()); await expect(panel.getByRole('status')).toHaveText('发现新版本'); await expect(panel).toContainText('0.4.0');
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__updatesQA.subscriptions())).toBe(1);
});
