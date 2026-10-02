import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';
type QA = { __notifyQA: { calls: () => string[]; busy: () => void; cleanup: () => void; private: (value: boolean) => void; delay: () => void; deliver: () => void } };
test.beforeEach(async ({ page }) => {
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    // Explicit test-only synthetic bridge, no production demonstration or filesystem access.
    Object.defineProperty(window, 'isTauri', { value: true });
    let privacy = false, revision = '1', serial = 0, configured: boolean | null = false, registered = false, failBusy = false, cleanup = false, delay = false;
    let deliver = () => {};
    const calls: string[] = [], callbacks = new Map<number, (event: unknown) => void>(), listeners: { event: string; handler: number }[] = [];
    const plans = new Map<string, { enable: boolean; epoch: string }>();
    const home = 'E:\\synthetic-notify-home', id = 'b'.repeat(32), prior = '["original-fixture.exe", "quoted synthetic argument"]';
    const emit = (event: string, payload: unknown) => listeners.filter(l => l.event === event).forEach(l => callbacks.get(l.handler)?.({ event, payload }));
    Object.assign(window, { __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} }, __notifyQA: {
      calls: () => [...calls], busy: () => { failBusy = true; }, cleanup: () => { cleanup = true; }, delay: () => { delay = true; }, deliver: () => deliver(),
      private: (value: boolean) => { privacy = value; revision = String(BigInt(revision) + 1n); emit('display_policy_changed', { settings_revision: revision, privacy }); },
    } });
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: { transformCallback: (fn: (event: unknown) => void) => { callbacks.set(++serial, fn); return serial; }, invoke: async (command: string, args: Record<string, unknown>) => {
      calls.push(command);
      if (command === 'plugin:event|listen') { listeners.push({ event: String(args.event), handler: Number(args.handler) }); return serial++; }
      if (command === 'plugin:event|unlisten') return 0;
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: revision, privacy }, data });
      if (command === 'get_display_settings') return response({ settings_version: 1, settings_revision: revision, preferences: { theme: 'dark', privacy, display_timezone: 'Asia/Shanghai' } });
      if (command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
      if (command === 'get_main_navigation' || command === 'get_mini_stats_request') return response(null);
      if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic', collector: 'ready', storage: 'error', storage_error: 'DB_WRITE_FAILED', quota: 'not_configured', taskbar: 'not_implemented' });
      if (command === 'get_sources') return response({ settings_revision: revision, sources: [{ source_id: 'synthetic-notify', root_path: privacy ? '路径已隐藏' : home, origin: 'custom', enabled: true, removed: false, readability: 'readable', capabilities: { physical_identity: 'available', byte_seek: 'available', watcher: 'available', polling_required: true }, last_scan_at_ms: null, last_success_at_ms: null, error: null }] });
      if (command === 'get_account_service_config') return response({ settings_revision: revision, executable_display_path: null, home_display_path: null, executable_sha256: null, configured: false, auto_connect: false });
      if (command === 'get_account_quota') return response({ connection_epoch: 'synthetic-disconnected', quota_revision: '0', state: 'disconnected', selected_limit_id: null, available_limits: [], fetched_at_ms: null, last_attempt_at_ms: null, windows: [], error_code: null });
      if (command === 'get_notify_integrations') return response({ ready: true, listener_count: registered && configured ? 1 : 0, service_issue: null, registrations: registered ? [{ registration_id: id, home_path: privacy ? null : home, configured, current_executable: true, chain_original: true, issue: null }] : [], registry_issue: null, redacted: privacy });
      const request = args.request as { kind: string; source_id?: string; chain_original?: boolean | null };
      if (command === 'prepare_notify_integration') {
        if (privacy) throw { code: 'PERMISSION_DENIED' };
        const plan = (++serial).toString(16).padStart(32, '0'), enable = request.kind !== 'disable'; plans.set(plan, { enable, epoch: revision });
        const result = response({ plan_id: plan, registration_id: id, operation: enable ? 'enable' : 'disable', home_path: home,
          before_notify: enable ? prior : '["synthetic-tokenpulse.exe"]', after_notify: enable ? '["synthetic-tokenpulse.exe"]' : prior,
          creates_config: false, can_chain_original: true, chain_original: request.chain_original !== false, settings_revision: revision, expires_in_seconds: 120, redacted: false });
        if (delay) { delay = false; return new Promise(resolve => { deliver = () => resolve(result); }); } return result;
      }
      if (command === 'release_notify_preview') { plans.delete(String(args.planId)); return response(null); }
      if (command === 'apply_notify_integration') {
        const plan = plans.get(String(args.planId)); if (privacy || !plan || plan.epoch !== revision) throw { code: 'STALE_CONFIRMATION' };
        if (failBusy) { failBusy = false; throw { code: 'NOTIFY_INTEGRATION_FAILED', details: { notify_issue: 'busy' } }; }
        configured = plan.enable; registered = configured || cleanup; plans.delete(String(args.planId));
        return response({ registration_id: id, configured, retired: !registered, cleanup_issue: cleanup && !configured ? 'busy' : null });
      }
      if (command === 'retire_notify_integration') { registered = false; cleanup = false; return response({ registration_id: id, configured: null, retired: true, cleanup_issue: null }); }
      throw { code: 'INVALID_QUERY' };
    } } });
  });
});
async function open(page: import('@playwright/test').Page) {
  await page.goto('/'); await page.getByRole('button', { name: '设置', exact: true }).click();
  const region = page.getByRole('region', { name: 'Codex 通知接入' });
  await expect(region.getByText('暂无正在监听的通知接入')).toBeVisible();
  await region.getByLabel('通知接入来源').selectOption('synthetic-notify'); return region;
}
test('review and default original precede enable; stop also reviews restore', async ({ page }) => {
  const region = await open(page); await region.getByRole('button', { name: '预览启用通知', exact: true }).click();
  const review = region.getByRole('region', { name: '通知配置预览' });
  await expect(review).toContainText('original-fixture.exe'); await expect(review).toContainText('继续调用原通知命令');
  expect((await page.evaluate(() => (window as unknown as QA).__notifyQA.calls())).filter(c => c === 'apply_notify_integration')).toHaveLength(0);
  await region.screenshot({ path: 'test-results/notify-dark-preview.png' });
  await page.setViewportSize({ width: 960, height: 900 });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  await region.screenshot({ path: 'test-results/notify-light-preview.png' });
  await review.getByRole('button', { name: '确认启用通知' }).click(); await expect(region.getByRole('heading', { name: '通知已启用', exact: true })).toBeVisible();
  await region.getByRole('button', { name: '预览停用通知' }).click(); await expect(review).toContainText('恢复后 notify');
  await review.getByRole('button', { name: '确认停用通知' }).click(); await expect(region.getByRole('status')).toContainText('原通知配置已恢复');
  await expect(region.locator('.notify-registration')).toHaveCount(0);
});
test('busy keeps review for retry and completed undo reports cleanup separately', async ({ page }) => {
  const region = await open(page); await region.getByRole('button', { name: '预览启用通知', exact: true }).click();
  await page.evaluate(() => (window as unknown as QA).__notifyQA.busy()); await region.getByRole('button', { name: '确认启用通知' }).click();
  await expect(region.getByRole('alert')).toContainText('其他程序使用'); await expect(region.getByRole('region', { name: '通知配置预览' })).toBeVisible();
  await region.getByRole('button', { name: '确认启用通知' }).click(); await region.getByRole('button', { name: '预览停用通知' }).click();
  await page.evaluate(() => (window as unknown as QA).__notifyQA.cleanup()); await region.getByRole('button', { name: '确认停用通知' }).click();
  await expect(region.getByRole('status')).toContainText('通知已停用；接入记录待清理');
  await region.getByRole('button', { name: '清理停用记录' }).click(); await expect(region.locator('.notify-registration')).toHaveCount(0);
});
test('late private review is released and cannot return after revealing the UI', async ({ page }) => {
  const region = await open(page); await page.evaluate(() => (window as unknown as QA).__notifyQA.delay());
  await region.getByRole('button', { name: '预览启用通知', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__notifyQA.calls().includes('prepare_notify_integration'))).toBe(true);
  await page.evaluate(() => (window as unknown as QA).__notifyQA.private(true)); await expect(region.getByText('隐私模式已隐藏通知路径与配置。关闭后可预览接入或停用。')).toBeVisible();
  await expect(page.locator('.source-list .source-path')).toHaveText('路径已隐藏');
  await page.evaluate(() => (window as unknown as QA).__notifyQA.deliver());
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__notifyQA.calls().includes('release_notify_preview'))).toBe(true);
  await expect(region).not.toContainText('synthetic-notify-home');
  await page.evaluate(() => (window as unknown as QA).__notifyQA.private(false));
  await expect(region.getByLabel('通知接入来源')).toBeEnabled(); await expect(region.getByRole('region', { name: '通知配置预览' })).toHaveCount(0);
});
