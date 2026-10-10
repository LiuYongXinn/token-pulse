import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';
type QA = { __diagnosticsQA: { fail: () => void; hold: () => void; release: () => void; privacy: () => void } };
test.beforeEach(async ({ page }) => {
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    let revision = '1', privacy = false, fail = false, hold = false;
    let release: (() => void) | null = null;
    const callbacks = new Map<number, (event: unknown) => void>(), listeners = new Map<number, { event: string; handler: number }>();
    let callback = 0, listener = 0;
    const sources = ['a', 'b'].map(id => ({ source_id: id, root_path: `E:\\synthetic-${id}\\.codex`, origin: 'custom', enabled: true, removed: false, readability: 'readable', capabilities: { physical_identity: 'available', byte_seek: 'available', watcher: 'available', polling_required: true }, last_scan_at_ms: 1000, last_success_at_ms: 1000, error: null }));
    const issues = [
      { issue_id: 'record', source_id: 'a', kind: 'log_record', code: 'UNSUPPORTED_FORMAT', path: 'E:\\synthetic-a\\.codex\\sessions\\log.jsonl', byte_offset: '9007199254740993' },
      { issue_id: 'pending', source_id: 'a', kind: 'unconfirmed_usage', code: null, path: 'E:\\synthetic-a\\.codex\\sessions\\log.jsonl', byte_offset: '0' },
      { issue_id: 'anchor', source_id: 'a', kind: 'unattributed_usage', code: null, path: null, byte_offset: null },
      { issue_id: 'missing', source_id: 'b', kind: 'missing_file', code: null, path: 'E:\\synthetic-b\\.codex\\archived_sessions\\missing.jsonl', byte_offset: null },
    ];
    Object.assign(window, { isTauri: true, __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} }, __TAURI_INTERNALS__: {
      transformCallback: (fn: (event: unknown) => void) => { callbacks.set(++callback, fn); return callback; },
      invoke: async (command: string, args: Record<string, unknown>) => {
        if (command === 'plugin:event|listen') { listeners.set(++listener, { event: String(args.event), handler: Number(args.handler) }); return listener; }
        if (command === 'plugin:event|unlisten') { listeners.delete(Number(args.eventId)); return null; }
        const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: revision, privacy }, data });
        if (command === 'get_display_settings' || command === 'resolve_calendar_selection') {
          const data = window.__syntheticCalendar(command, args);
          if (command === 'get_display_settings') Object.assign(data as object, { settings_revision: revision });
          return response(data);
        }
        if (command === 'get_app_status') return response({ version: 'synthetic', development: true, data_directory: 'synthetic', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_configured' });
        if (command === 'get_sources') return response({ settings_revision: revision, sources: sources.map(s => ({ ...s, root_path: privacy ? '来源已隐藏' : s.root_path })) });
        if (command === 'get_rebuild_status') return response(null);
        if (command === 'get_taskbar_status') return response({ revision: '1', state: 'disabled', applied_settings_revision: null, issue: null, error: null, compact: null, fallback_visible: null, fallback_error: null, action_error: null, last_cleanup: null, last_snapshot_at_ms: null });
        if (command === 'query_diagnostics') {
          if (fail) throw { code: 'DB_CORRUPT' };
          const source = (args.request as { source_id: string | null }).source_id;
          const rows = issues.filter(i => source === null || i.source_id === source).map(i => ({ ...i, path: privacy && i.path !== null ? '位置已隐藏' : i.path }));
          const result = response({ data_revision: '9007199254740993', issues: rows, has_more: false });
          if (hold) { hold = false; return new Promise(resolve => { release = () => resolve(result); }); }
          return result;
        }
        throw new Error(`unexpected synthetic command ${command}`);
      },
    }, __diagnosticsQA: {
      fail: () => { fail = true; }, hold: () => { hold = true; }, release: () => release?.(),
      privacy: () => { privacy = true; revision = '2'; for (const [id, l] of listeners) if (l.event === 'display_policy_changed') callbacks.get(l.handler)?.({ event: l.event, id, payload: { settings_revision: revision, privacy } }); },
    } });
  });
  await page.goto('/'); await page.getByRole('button', { name: '采集诊断', exact: true }).click();
});
test('necessary positions preserve precise and unknown offsets and switching sources discards prior rows', async ({ page }, info) => {
  const panel = page.getByRole('region', { name: '采集问题与必要位置' });
  await expect(panel.getByText('字节偏移 9,007,199,254,740,993', { exact: false })).toBeVisible();
  await expect(panel.getByText('当前日志记录格式尚不支持，该记录未计入可信消费。')).toBeVisible();
  await expect(panel.getByText('累计基线尚未确定，该段用量未计入可信消费。')).toBeVisible();
  await expect(panel.getByText('位置：尚无文件位置')).toBeVisible();
  await expect(panel.locator('li')).toHaveCount(4);
  await page.screenshot({ path: info.outputPath('diagnostic-positions-dark.png'), fullPage: true });
  await page.evaluate(() => (window as unknown as QA).__diagnosticsQA.hold());
  await panel.getByRole('combobox', { name: '诊断来源' }).selectOption('b');
  await expect(panel.locator('li')).toHaveCount(0);
  await page.evaluate(() => (window as unknown as QA).__diagnosticsQA.release());
  await expect(panel.locator('li')).toHaveCount(1);
  await expect(panel.getByText('已采集的文件当前未找到，保存的历史消费仍保留。')).toBeVisible();
  await page.evaluate(() => (window as unknown as QA).__diagnosticsQA.fail());
  await panel.getByRole('button', { name: '刷新问题' }).click();
  await expect(panel.getByRole('alert')).toContainText('显示上次读取的问题');
  await expect(panel.locator('li')).toHaveCount(1);
});
test('a delayed unredacted reply cannot restore paths after shared privacy changes', async ({ page }, info) => {
  const panel = page.getByRole('region', { name: '采集问题与必要位置' });
  await expect(panel.locator('li')).toHaveCount(4);
  await page.evaluate(() => (window as unknown as QA).__diagnosticsQA.hold());
  await panel.getByRole('button', { name: '刷新问题' }).click();
  await page.evaluate(() => (window as unknown as QA).__diagnosticsQA.privacy());
  await expect(panel.getByText('位置已隐藏', { exact: false })).toHaveCount(3);
  await page.evaluate(() => (window as unknown as QA).__diagnosticsQA.release());
  await expect(panel.getByText('synthetic-a', { exact: false })).toHaveCount(0);
  await expect(panel.getByText('synthetic-b', { exact: false })).toHaveCount(0);
  await page.setViewportSize({ width: 960, height: 860 });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: info.outputPath('diagnostic-positions-private-light.png'), fullPage: true });
});

test('title index problems identify the metadata file and preserve usage meaning', async ({ page }) => {
  await page.evaluate(() => {
    const runtime = (window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args: Record<string, unknown>) => Promise<unknown> } }).__TAURI_INTERNALS__;
    const invoke = runtime.invoke;
    runtime.invoke = async (command, args) => {
      if (command !== 'query_diagnostics') return invoke(command, args);
      return { api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data: { data_revision: '7', has_more: false, issues: ['TITLE_INDEX_INVALID','TITLE_INDEX_UNREADABLE'].map((code, n) => ({ issue_id: `title-${n}`, source_id: 'a', kind: 'log_record', code, path: 'E:/synthetic-a/.codex/session_index.jsonl', byte_offset: n === 0 ? '65537' : null })) } };
    };
  });
  const panel = page.getByRole('region', { name: '采集问题与必要位置' });
  await panel.getByRole('button', { name: '刷新问题' }).click();
  await expect(panel).toContainText('已跳过；其他标题和用量继续采集');
  await expect(panel).toContainText('标题索引暂时无法读取');
  await expect(panel).toContainText('session_index.jsonl');
  await expect(panel).toContainText('字节偏移 65,537');
});
