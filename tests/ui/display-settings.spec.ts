import { expect, test } from '@playwright/test';
type OpacityQA = { __opacityQA: { reject: (value: boolean) => void; unsupported: () => void; read: () => number } };
type ShortcutQA = { __shortcutQA: { conflict: (value: boolean) => void; status: (value: string) => void; read: () => { control: boolean; alt: boolean; shift: boolean; key: string } } };
type PassQA = { __passQA: { reject: (value: boolean) => void; mismatch: () => void; read: () => boolean; listeners: () => number } };
test.use({ timezoneId: 'UTC' });

test('passthrough needs the real window and explicit acknowledged recovery, uses exact revision and restores', async ({ page }) => {
  await page.goto('/'); await openSettings(page);
  const region = page.getByRole('region', { name: '鼠标穿透设置' });
  await expect(region.getByRole('button', { name: '开启鼠标穿透' })).toBeDisabled();
  await region.getByRole('button', { name: '显示小窗并恢复交互' }).click();
  await expect(region.getByRole('checkbox')).toBeEnabled(); await region.getByRole('checkbox').check();
  await region.getByRole('button', { name: '开启鼠标穿透' }).click(); await expect(region.getByRole('status')).toContainText('穿透已开启');
  expect(await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'set_mini_passthrough').at(-1)?.request)).toEqual({ enabled: true, acknowledged_recovery: { control: true, alt: true, shift: true, key: 'T' }, expected_settings_revision: '9007199254740993' });
  await region.getByRole('button', { name: '显示小窗并恢复交互' }).click(); await expect(region.getByRole('status')).toContainText('穿透已关闭');
  await page.getByRole('button', { name: '模型', exact: true }).click();
  await expect.poll(async () => page.evaluate(() => (window as unknown as PassQA).__passQA.listeners())).toBe(1);
});

test('unregistered recovery gates enable and revisions retain acknowledged draft on conflict', async ({ page }) => {
  await page.goto('/'); await openSettings(page); const region = page.getByRole('region', { name: '鼠标穿透设置' });
  await region.getByRole('button', { name: '显示小窗并恢复交互' }).click();
  await page.evaluate(() => (window as unknown as ShortcutQA).__shortcutQA.status('conflict')); await region.getByRole('button', { name: '刷新穿透状态' }).click();
  await expect(region.getByRole('checkbox')).toBeDisabled(); await expect(region.getByRole('button', { name: '开启鼠标穿透' })).toBeDisabled();
  expect(await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'set_mini_passthrough'))).toEqual([]);
  await page.evaluate(() => (window as unknown as ShortcutQA).__shortcutQA.status('ready')); await region.getByRole('button', { name: '刷新穿透状态' }).click();
  await region.getByRole('checkbox').check(); await page.evaluate(() => (window as unknown as QA).__calendarQA.externalChange());
  await region.getByRole('button', { name: '刷新穿透状态' }).click(); await region.getByRole('button', { name: '开启鼠标穿透' }).click();
  await expect(region.getByRole('alert')).toContainText('已发生变化'); await expect(region.getByRole('checkbox')).toBeChecked();
  expect(await page.evaluate(() => (window as unknown as PassQA).__passQA.read())).toBe(false);
});

test('failed recovery persistence exposes native off and pending saved state with an accessible retry', async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 900 }); await page.goto('/'); await openSettings(page);
  const region = page.getByRole('region', { name: '鼠标穿透设置' });
  await page.evaluate(() => { (window as unknown as PassQA).__passQA.mismatch(); (window as unknown as PassQA).__passQA.reject(true); });
  await region.getByRole('button', { name: '刷新穿透状态' }).click(); await expect(region.getByRole('status')).toContainText('保存状态尚未同步');
  await region.getByRole('button', { name: '关闭鼠标穿透' }).click(); await expect(region.getByRole('alert')).toContainText('写入失败'); await expect(region.getByRole('status')).toContainText('穿透已关闭');
  await page.evaluate(() => (window as unknown as PassQA).__passQA.reject(false)); await region.getByRole('button', { name: '关闭鼠标穿透' }).click();
  await expect(region.getByRole('status')).not.toContainText('尚未同步'); await region.scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/mini-passthrough-settings-960.png' }); expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('native opacity keeps exact revision and explicit drafts across conflict and writer rejection', async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 900 }); await page.goto('/');
  await page.getByRole('button', { name: '设置', exact: true }).click(); await page.getByRole('tab', { name: '显示与窗口' }).click();
  const panel = page.getByRole('region', { name: '小窗透明度设置' });
  const slider = panel.getByRole('slider', { name: '悬浮窗透明度' });
  await expect(slider).toBeEnabled(); await expect(slider).toHaveValue('100');
  await slider.fill('80'); await panel.getByRole('button', { name: '保存小窗透明度' }).click();
  await expect(panel.getByRole('status')).toContainText('已保存 80%');
  expect(await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'set_mini_opacity').at(-1)?.request)).toEqual({ opacity_percent: 80, expected_settings_revision: '9007199254740993' });
  await slider.fill('75'); await page.evaluate(() => (window as unknown as QA).__calendarQA.externalChange());
  await panel.getByRole('button', { name: '刷新透明度' }).click(); await expect(slider).toHaveValue('75');
  await panel.getByRole('button', { name: '保存小窗透明度' }).click(); await expect(panel.getByRole('alert')).toContainText('已发生变化');
  await panel.getByRole('button', { name: '重置透明度草稿' }).click(); await expect(slider).toHaveValue('80');
  await slider.fill('70'); await page.evaluate(() => (window as unknown as OpacityQA).__opacityQA.reject(true));
  await panel.getByRole('button', { name: '保存小窗透明度' }).click(); await expect(panel.getByRole('alert')).toContainText('写入');
  await expect(slider).toHaveValue('70'); expect(await page.evaluate(() => (window as unknown as OpacityQA).__opacityQA.read())).toBe(80);
  await panel.scrollIntoViewIfNeeded(); await page.screenshot({ path: 'test-results/mini-opacity-settings-960.png' });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('unsupported opacity exposes real saved value and disables mutation', async ({ page }) => {
  await page.goto('/'); await page.evaluate(() => (window as unknown as OpacityQA).__opacityQA.unsupported());
  await page.getByRole('button', { name: '设置', exact: true }).click(); await page.getByRole('tab', { name: '显示与窗口' }).click();
  const panel = page.getByRole('region', { name: '小窗透明度设置' });
  await expect(panel.getByRole('status')).toContainText('当前平台暂不支持');
  await expect(panel.getByRole('slider')).toBeDisabled(); await expect(panel.getByRole('button', { name: '保存小窗透明度' })).toBeDisabled();
  expect(await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'set_mini_opacity'))).toEqual([]);
});

test.beforeEach(async ({ page }) => {
  // This isolated browser bridge models DTO and conflict behavior; production imports none of it.
  await page.addInitScript(() => {
    let theme = sessionStorage.getItem('synthetic-theme') ?? 'dark', rejectTheme = false;
    let shortcut = { control: true, alt: true, shift: true, key: 'T' }, shortcutStatus = 'ready', shortcutConflict = false;
    let opacity = 100, opacitySupported = true, rejectOpacity = false;
    let pass = false, persistedPass = false, miniPresent = false, rejectPass = false;
    let timezone: string | null = 'Asia/Shanghai', revision = '9007199254740993', failRead = false, badCalendar = false;
    const calls: { command: string; request: unknown }[] = [];
    let callbackId = 0, eventId = 0;
    const callbacks = new Map<number, (event: unknown) => void>(), listeners = new Map<number, { event: string; handler: number }>();
    const snapshot = () => ({ settings_version: 1, settings_revision: revision, preferences: { theme, privacy: false, display_timezone: timezone } });
    const notify = () => { for (const [id, listener] of listeners) if (listener.event === 'settings_changed') callbacks.get(listener.handler)?.({ event: listener.event, id, payload: { settings_revision: revision } }); };
    const measure = { value: null, covered_total_tokens: '0', complete: false };
    const summary = { total_tokens: '0', input_total: measure, cached_input: measure, noncached_input: measure, output_total: measure, reasoning_output: measure, cache_write_input: { value: null, covered_total_tokens: '0', complete: false }, session_count: '0', usage_event_count: '0', reliable_turn_count: null, reliable_turns_complete: false };
    const coverage = { state: 'unknown', pending_observation_count: '0', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '0', source_issues: [], format_issues: [], breakdown_complete: false };
    const pricing = { redacted: false, basis: { mode: 'event_time' }, currencies: [], priced_total_tokens: '0', unpriced_total_tokens: '0', reasons: [], calculating: false };
    Object.assign(window, { isTauri: true,
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: (_event: string, id: number) => { const listener = listeners.get(id); if (listener) callbacks.delete(listener.handler); listeners.delete(id); } },
      __TAURI_INTERNALS__: { transformCallback: (callback: (event: unknown) => void) => { callbacks.set(++callbackId, callback); return callbackId; }, invoke: async (command: string, args: Record<string, unknown>) => {
        calls.push({ command, request: args.request }); const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data });
        if (command === 'plugin:event|listen') { listeners.set(++eventId, { event: String(args.event), handler: Number(args.handler) }); return eventId; }
        if (command === 'plugin:event|unlisten') return null;
        if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
        if (command === 'get_sources') return response({ settings_revision: revision, sources: [] });
        if (command === 'get_mini_passthrough') return response({ enabled: pass, persisted_enabled: persistedPass, window_present: miniPresent, supported: true, recovery_shortcut: shortcut, recovery_registration: shortcutStatus, settings_revision: revision });
        if (command === 'perform_window_action') { miniPresent = true; pass = false; if (!rejectPass && persistedPass) { persistedPass = false; revision = String(BigInt(revision) + 1n); notify(); } for (const [id, listener] of listeners) if (listener.event === 'mini_interaction_changed') callbacks.get(listener.handler)?.({ event: listener.event, id, payload: null }); return response(null); }
        if (command === 'set_mini_passthrough') {
          const r = args.request as { enabled: boolean; acknowledged_recovery: typeof shortcut | null; expected_settings_revision: string };
          if (r.expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' };
          if (r.enabled && (shortcutStatus !== 'ready' || !miniPresent || JSON.stringify(r.acknowledged_recovery) !== JSON.stringify(shortcut))) throw { code: 'SHORTCUT_UNAVAILABLE' };
          if (!r.enabled) pass = false;
          if (rejectPass) throw { code: 'DB_WRITE_FAILED' };
          pass = r.enabled; if (persistedPass !== pass) { persistedPass = pass; revision = String(BigInt(revision) + 1n); notify(); }
          return response({ enabled: pass, persisted_enabled: persistedPass, window_present: miniPresent, supported: true, recovery_shortcut: shortcut, recovery_registration: shortcutStatus, settings_revision: revision });
        }
        if (command === 'get_mini_opacity') return response({ opacity_percent: opacity, supported: opacitySupported, settings_revision: revision });
        if (command === 'set_mini_opacity') {
          const r = args.request as { opacity_percent: number; expected_settings_revision: string };
          if (r.expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' };
          if (rejectOpacity) throw { code: 'DB_WRITE_FAILED' };
          if (!opacitySupported || !Number.isInteger(r.opacity_percent) || r.opacity_percent < 70 || r.opacity_percent > 100) throw { code: 'INVALID_QUERY' };
          if (opacity !== r.opacity_percent) { opacity = r.opacity_percent; revision = String(BigInt(revision) + 1n); notify(); }
          return response({ opacity_percent: opacity, supported: opacitySupported, settings_revision: revision });
        }
        if (command === 'get_recovery_shortcut') return response({ shortcut, registration: shortcutStatus, settings_revision: revision });
        if (command === 'set_recovery_shortcut') {
          const r = args.request as { shortcut: typeof shortcut; expected_settings_revision: string };
          if (r.expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' };
          if (shortcutConflict) throw { code: 'SHORTCUT_CONFLICT' };
          if (JSON.stringify(r.shortcut) !== JSON.stringify(shortcut)) { shortcut = r.shortcut; revision = String(BigInt(revision) + 1n); notify(); }
          shortcutStatus = 'ready'; return response({ shortcut, registration: shortcutStatus, settings_revision: revision });
        }
        if (command === 'get_display_settings') { if (failRead) throw { code: 'UNSUPPORTED_SETTINGS_VERSION' }; return response(snapshot()); }
        if (command === 'set_display_theme') {
          const r = args.request as { theme: string; expected_settings_revision: string };
          if (rejectTheme || r.expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' };
          if (!['dark', 'light', 'system'].includes(r.theme)) throw { code: 'INVALID_QUERY' };
          if (theme !== r.theme) { theme = r.theme; sessionStorage.setItem('synthetic-theme', theme); revision = String(BigInt(revision) + 1n); notify(); }
          return response(snapshot());
        }
        if (command === 'set_display_timezone') {
          const r = args.request as { kind: string; system_timezone?: string; display_timezone?: string; expected_settings_revision?: string };
          const value = r.kind === 'initialize' ? r.system_timezone! : r.display_timezone!;
          if (!['UTC', 'Asia/Shanghai', 'America/New_York'].includes(value)) throw { code: 'INVALID_QUERY' };
          if (r.kind === 'set' && r.expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' };
          if ((r.kind === 'initialize' && timezone === null) || (r.kind === 'set' && timezone !== value)) { timezone = value; revision = String(BigInt(revision) + 1n); notify(); }
          return response(snapshot());
        }
        if (command === 'resolve_calendar_selection') {
          if (badCalendar) throw { code: 'INVALID_QUERY' };
          const r = args.request as { timezone: string; selection: { kind: string; start_date?: string; end_date_inclusive?: string } };
          // Fixed QA dates independently expected below; the Rust resolver covers calendar conversion.
          const offset = r.timezone === 'Asia/Shanghai' ? -8 : r.timezone === 'America/New_York' ? 4 : 0;
          const end = Date.parse('2026-11-02T00:00:00Z') + (r.timezone === 'America/New_York' ? 5 : offset) * 3_600_000;
          const start = Date.parse('2026-11-01T00:00:00Z') + offset * 3_600_000 - (r.selection.kind === 'last7' ? 6 : r.selection.kind === 'last30' ? 29 : 0) * 86_400_000;
          const customStart = r.selection.kind === 'custom' ? Date.parse(r.selection.start_date! + 'T00:00:00Z') + offset * 3_600_000 : start;
          const customEnd = r.selection.kind === 'custom' ? Date.parse(r.selection.end_date_inclusive! + 'T00:00:00Z') + 86_400_000 + (r.timezone === 'America/New_York' ? 5 : offset) * 3_600_000 : end;
          return response({ range: { start_ms: customStart, end_ms: customEnd, timezone: r.timezone }, heatmap_range: { start_ms: Date.parse('2026-05-04T04:00:00Z'), end_ms: end, timezone: r.timezone }, local_today: '2026-11-01' });
        }
        if (command === 'get_grouped_usage' || command === 'get_dashboard_bundle') {
          const r = args.request as { filter: { range: { timezone: string } }; dimension?: string };
          const common = { meta: { snapshot_id: String(args.requestId), data_revision: '0', price_revision: '0', generated_at_ms: 1, parser_versions: [], accounting_versions: [], display_timezone: r.filter.range.timezone }, summary, coverage, pricing };
          return response(command === 'get_grouped_usage' ? { ...common, dimension: r.dimension, groups: [], total_group_count: '0', truncated: false } : { ...common, series: [], heatmap: [], recent_sessions: [] });
        }
        throw new Error(`Unexpected synthetic command ${command}`);
      } }, __passQA: { reject: (value: boolean) => { rejectPass = value; }, mismatch: () => { miniPresent = true; pass = false; persistedPass = true; notify(); }, read: () => pass, listeners: () => [...listeners.values()].filter(v => v.event === 'mini_interaction_changed').length }, __opacityQA: { reject: (value: boolean) => { rejectOpacity = value; }, unsupported: () => { opacitySupported = false; notify(); }, read: () => opacity }, __shortcutQA: { conflict: (value: boolean) => { shortcutConflict = value; }, status: (value: string) => { shortcutStatus = value; notify(); }, read: () => shortcut }, __themeQA: { reject: (value: boolean) => { rejectTheme = value; }, external: (value: string) => { theme = value; revision = String(BigInt(revision) + 1n); notify(); } }, __calendarQA: { calls: () => calls, snapshot, failRead: (value: boolean) => { failRead = value; }, badCalendar: (value: boolean) => { badCalendar = value; }, reset: () => { timezone = null; revision = '0'; }, externalChange: () => { timezone = 'UTC'; revision = String(BigInt(revision) + 1n); notify(); }, listeners: () => [...listeners.values()].filter(value => value.event === 'settings_changed').length } });
  });
});
type QA = { __calendarQA: { calls: () => { command: string; request: unknown }[]; snapshot: () => { settings_revision: string; preferences: { theme: 'dark', privacy: false, display_timezone: string | null } }; failRead: (v: boolean) => void; badCalendar: (v: boolean) => void; reset: () => void; externalChange: () => void; listeners: () => number } };
async function openSettings(page: import('@playwright/test').Page) { await page.getByRole('button', { name: '设置', exact: true }).click(); await page.getByRole('tab', { name: '显示与窗口' }).click(); }

test('saved timezone drives all pages, exact revisions and backend DST boundaries', async ({ page }) => {
  await page.goto('/'); await expect(page.locator('.filters')).toContainText('Asia/Shanghai');
  await openSettings(page); await expect(page.getByLabel('统计时区')).toHaveValue('Asia/Shanghai');
  await page.getByLabel('统计时区').fill('America/New_York'); await page.getByRole('button', { name: '保存统计时区' }).click();
  await expect(page.locator('.display-notice[role="status"]')).toContainText('已保存时区 America/New_York');
  expect((await page.evaluate(() => (window as unknown as QA).__calendarQA.snapshot())).settings_revision).toBe('9007199254740994');
  await page.screenshot({ path: 'test-results/display-timezone-1280.png', fullPage: true });
  await page.getByRole('button', { name: '模型', exact: true }).click(); await expect(page.getByLabel('模型统计汇总')).toBeVisible();
  await expect.poll(async () => page.evaluate(() => (window as unknown as QA).__calendarQA.listeners())).toBe(1);
  const call = await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'get_grouped_usage').at(-1));
  expect(call?.request).toMatchObject({ filter: { range: { start_ms: Date.parse('2026-11-01T04:00:00Z'), end_ms: Date.parse('2026-11-02T05:00:00Z'), timezone: 'America/New_York' } } });
  await page.getByLabel('日期范围').selectOption('last7'); await expect(page.locator('.filters')).toContainText('America/New_York');
  await expect.poll(async () => page.evaluate(() => { const r = (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'get_grouped_usage').at(-1)?.request as { filter: { range: { start_ms: number } } } | undefined; return r?.filter.range.start_ms; })).toBe(Date.parse('2026-10-26T04:00:00Z'));
  await openSettings(page); await expect(page.getByLabel('统计时区')).toHaveValue('America/New_York');
  await expect.poll(async () => page.evaluate(() => (window as unknown as QA).__calendarQA.listeners())).toBe(1);
  await page.setViewportSize({ width: 960, height: 680 }); await page.screenshot({ path: 'test-results/display-timezone-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test('conflicts preserve draft and original revision until explicit reset, invalid timezone preserves stored state', async ({ page }) => {
  await page.goto('/'); await openSettings(page); await expect(page.getByLabel('统计时区')).toHaveValue('Asia/Shanghai');
  await page.getByLabel('统计时区').fill('America/New_York'); await page.evaluate(() => (window as unknown as QA).__calendarQA.externalChange());
  await expect.poll(async () => page.evaluate(() => (window as unknown as QA).__calendarQA.snapshot().settings_revision)).toBe('9007199254740994');
  await page.getByRole('button', { name: '刷新显示设置' }).click(); await expect(page.getByLabel('统计时区')).toHaveValue('America/New_York');
  await page.getByRole('button', { name: '保存统计时区' }).click(); await expect(page.getByRole('alert')).toContainText('已发生变化');
  expect((await page.evaluate(() => (window as unknown as QA).__calendarQA.snapshot())).preferences.display_timezone).toBe('UTC');
  await page.getByRole('button', { name: '重置为当前值' }).click(); await expect(page.getByLabel('统计时区')).toHaveValue('UTC');
  await page.getByLabel('统计时区').fill('Invalid/Zone'); await page.getByRole('button', { name: '保存统计时区' }).click();
  await expect(page.getByRole('alert')).toContainText('请求参数'); await expect(page.getByLabel('统计时区')).toHaveValue('Invalid/Zone');
  expect((await page.evaluate(() => (window as unknown as QA).__calendarQA.snapshot())).settings_revision).toBe('9007199254740994');
});

test('unreadable settings issue no guessed statistics queries and recover explicitly', async ({ page }) => {
  await page.addInitScript(() => { document.addEventListener('DOMContentLoaded', () => (window as unknown as QA).__calendarQA.failRead(true)); });
  await page.goto('/'); await expect(page.getByRole('heading', { name: 'Token 分解' })).toBeVisible();
  await expect(page.getByRole('alert')).toContainText('已有配置已保留');
  expect(await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'resolve_calendar_selection' || v.command === 'get_dashboard_bundle'))).toEqual([]);
  await page.evaluate(() => (window as unknown as QA).__calendarQA.failRead(false)); await page.getByRole('button', { name: '重试显示设置' }).click();
  await expect(page.getByRole('heading', { name: '添加 Codex 数据来源' })).toBeVisible();
});

test('first start initializes system timezone once and failed new calendar clears prior scope', async ({ page }) => {
  await page.addInitScript(() => { document.addEventListener('DOMContentLoaded', () => (window as unknown as QA).__calendarQA.reset()); });
  // Explicit browser system zone; it is validated and stored by the synthetic bridge.
  await page.goto('/'); await expect(page.locator('.filters')).toContainText('UTC');
  const calls = await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'set_display_timezone'));
  expect(calls).toHaveLength(1); expect(calls[0].request).toEqual({ kind: 'initialize', system_timezone: 'UTC' });
  await page.getByRole('button', { name: '模型', exact: true }).click(); await expect(page.getByLabel('模型统计汇总')).toBeVisible();
  await page.evaluate(() => (window as unknown as QA).__calendarQA.badCalendar(true)); await page.getByLabel('日期范围').selectOption('last7');
  await expect(page.getByLabel('模型统计汇总')).toContainText('—'); await expect(page.getByRole('heading', { name: /正在读取/ })).toHaveCount(0);
  await expect(page.getByRole('alert')).toContainText('统计日期解析失败');
});

test('custom date draft requires explicit apply, rejects reversal, preserves scope on escape and shares inclusive backend range', async ({ page }) => {
  await page.goto('/'); await expect(page.locator('.filters')).toContainText('Asia/Shanghai');
  await page.getByRole('button', { name: '模型', exact: true }).click(); await expect(page.getByLabel('模型统计汇总')).toBeVisible();
  await page.getByLabel('日期范围').selectOption('custom'); await expect(page.getByRole('dialog', { name: '自定义日期' })).toBeVisible();
  await expect(page.getByLabel('开始日期')).toBeFocused();
  await page.getByLabel('开始日期').fill('2026-02-28'); await page.getByLabel('结束日期（包含当天）').fill('2026-02-27');
  await page.getByRole('button', { name: '应用日期' }).click(); await expect(page.getByRole('alert')).toContainText('不能早于');
  expect(await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().some(v => v.command === 'resolve_calendar_selection' && (v.request as { selection: { kind: string } }).selection.kind === 'custom'))).toBe(false);
  await page.getByLabel('开始日期').press('Escape'); await expect(page.getByRole('dialog')).toHaveCount(0); await expect(page.getByLabel('日期范围')).toHaveValue('today');
  await page.getByLabel('日期范围').selectOption('custom'); await page.getByLabel('开始日期').fill('2024-02-28'); await page.getByLabel('结束日期（包含当天）').fill('2024-02-29');
  await page.screenshot({ path: 'test-results/date-picker-1280.png', fullPage: true });
  await page.getByRole('button', { name: '应用日期' }).click(); await expect(page.getByLabel('模型统计汇总')).toBeVisible(); await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect.poll(async () => page.evaluate(() => ((window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'get_grouped_usage').at(-1)?.request as { filter: { range: { end_ms: number } } }).filter.range.end_ms)).toBe(Date.parse('2024-02-29T16:00:00Z'));
  await page.getByRole('button', { name: '项目', exact: true }).click(); await expect(page.getByLabel('项目统计汇总')).toBeVisible();
  await expect(page.getByTitle('编辑已应用日期')).toHaveText('2024-02-28 — 2024-02-29');
  await page.getByTitle('编辑已应用日期').click(); await expect(page.getByLabel('开始日期')).toHaveValue('2024-02-28'); await page.getByRole('button', { name: '取消', exact: true }).click();
  await page.getByRole('button', { name: '重置筛选' }).click(); await expect(page.getByLabel('日期范围')).toHaveValue('today');
});

type ThemeQA = { __themeQA: { reject: (value: boolean) => void; external: (value: string) => void } };
test('saved theme changes real surfaces, persists reload and keeps timezone and layout', async ({ page }) => {
  await page.goto('/'); await openSettings(page); await expect(page.getByLabel('应用主题')).toHaveValue('dark');
  await page.getByLabel('应用主题').selectOption('light'); await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByLabel('统计时区')).toHaveValue('Asia/Shanghai');
  expect(await page.locator('html').evaluate(e => getComputedStyle(e).color)).toBe('rgb(41, 41, 41)');
  expect(await page.locator('.panel:visible').evaluate(e => getComputedStyle(e).backgroundColor)).toBe('rgb(255, 255, 255)');
  await page.screenshot({ path: 'test-results/theme-light-1280.png', fullPage: true });
  await page.setViewportSize({ width: 960, height: 680 }); await page.screenshot({ path: 'test-results/theme-light-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.reload(); await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('button', { name: '模型', exact: true }).click(); await expect(page.getByLabel('模型统计汇总')).toBeVisible();
  await expect(page.locator('.filters')).toContainText('Asia/Shanghai');
});

test('system follows media changes without new setting writes, explicit themes ignore system and external updates preserve drafts', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' }); await page.goto('/'); await openSettings(page);
  await page.getByLabel('应用主题').selectOption('system'); await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.emulateMedia({ colorScheme: 'light' }); await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByLabel('应用主题')).toHaveValue('system');
  expect(await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'set_display_theme'))).toHaveLength(1);
  await page.getByLabel('应用主题').selectOption('dark'); await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.emulateMedia({ colorScheme: 'light' }); await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByLabel('统计时区').fill('America/New_York'); await page.evaluate(() => (window as unknown as ThemeQA).__themeQA.external('light'));
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light'); await expect(page.getByLabel('统计时区')).toHaveValue('America/New_York');
  await page.getByRole('button', { name: '保存统计时区' }).click(); await expect(page.getByRole('alert')).toContainText('已发生变化');
});

test('theme save failure preserves confirmed theme and stored configuration', async ({ page }) => {
  await page.goto('/'); await openSettings(page); await page.evaluate(() => (window as unknown as ThemeQA).__themeQA.reject(true));
  await page.getByLabel('应用主题').selectOption('light'); await expect(page.getByRole('alert')).toContainText('已发生变化');
  await expect(page.getByLabel('应用主题')).toHaveValue('dark'); await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  expect((await page.evaluate(() => (window as unknown as QA).__calendarQA.snapshot())).settings_revision).toBe('9007199254740993');
});

test('recovery shortcut uses exact CAS, native conflict keeps the old key and settings refresh retains a draft', async ({ page }) => {
  await page.goto('/'); await openSettings(page);
  const panel = page.getByRole('region', { name: '恢复快捷键设置' });
  await expect(panel).toContainText('恢复快捷键已注册');
  await page.getByLabel('恢复快捷键按键').selectOption('U');
  await page.getByRole('button', { name: '保存恢复快捷键' }).click();
  expect(await page.evaluate(() => (window as unknown as ShortcutQA).__shortcutQA.read())).toEqual({ control: true, alt: true, shift: true, key: 'U' });
  expect(await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'set_recovery_shortcut').at(-1)?.request)).toEqual({ shortcut: { control: true, alt: true, shift: true, key: 'U' }, expected_settings_revision: '9007199254740993' });
  await page.getByLabel('恢复快捷键按键').selectOption('V');
  await page.evaluate(() => (window as unknown as ShortcutQA).__shortcutQA.conflict(true));
  await page.getByRole('button', { name: '保存恢复快捷键' }).click();
  await expect(panel.getByRole('alert')).toContainText('其他应用占用');
  await expect(page.getByLabel('恢复快捷键按键')).toHaveValue('V');
  expect((await page.evaluate(() => (window as unknown as ShortcutQA).__shortcutQA.read())).key).toBe('U');
  await page.evaluate(() => { (window as unknown as ShortcutQA).__shortcutQA.conflict(false); (window as unknown as QA).__calendarQA.externalChange(); });
  await page.getByRole('button', { name: '刷新快捷键状态' }).click();
  await page.getByRole('button', { name: '保存恢复快捷键' }).click();
  await expect(panel.getByRole('alert')).toContainText('已发生变化');
  await expect(page.getByLabel('恢复快捷键按键')).toHaveValue('V');
  await page.getByRole('button', { name: '重置快捷键草稿' }).click();
  await expect(page.getByLabel('恢复快捷键按键')).toHaveValue('U');
  await page.setViewportSize({ width: 960, height: 680 });
  await panel.scrollIntoViewIfNeeded(); await page.screenshot({ path: 'test-results/recovery-shortcut-settings-960.png' });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('recovery registration can retry a conflict without changing its desired key and invalid modifier issues no IPC', async ({ page }) => {
  await page.goto('/'); await page.evaluate(() => (window as unknown as ShortcutQA).__shortcutQA.status('conflict')); await openSettings(page);
  await expect(page.getByRole('button', { name: '重新注册恢复快捷键' })).toBeEnabled();
  await page.getByRole('button', { name: '重新注册恢复快捷键' }).click();
  await expect(page.getByRole('region', { name: '恢复快捷键设置' })).toContainText('恢复快捷键已注册');
  const before = await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'set_recovery_shortcut').length);
  await page.getByLabel('恢复快捷键 Ctrl').uncheck(); await page.getByLabel('恢复快捷键 Alt').uncheck();
  await page.getByRole('button', { name: '保存恢复快捷键' }).click();
  await expect(page.getByRole('region', { name: '恢复快捷键设置' }).getByRole('alert')).toContainText('至少需要 Ctrl 或 Alt');
  expect(await page.evaluate(() => (window as unknown as QA).__calendarQA.calls().filter(v => v.command === 'set_recovery_shortcut').length)).toBe(before);
  expect(await page.getByLabel('恢复快捷键按键').locator('option').allTextContents()).not.toContain('F12');
});
