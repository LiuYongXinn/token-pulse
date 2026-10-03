import { expect, test } from '@playwright/test';
import { installSyntheticCalendar } from './calendar-bridge';

test.beforeEach(async ({ page }) => {
  // Explicit synthetic DTO bridge, never loaded by production code.
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    type Query = { filter: { range: { start_ms: number; end_ms: number; timezone: string }; sources: { ids?: string[] }; sessions: { ids?: string[] } }; price_basis: unknown; sort: string; page_size: number };
    let miniScope: unknown = { kind: 'today_all_sources' }, rejectMini = false;
    let privacy = false, settingsRevision = '1', rejectPrivacy = false, freezePrivacyReply = false;
    let callbackId = 0, eventId = 0;
    const callbacks = new Map<number, (event: unknown) => void>(), listeners = new Map<number, { event: string; handler: number }>();
    const notifyPrivacy = () => {
      for (const [id, listener] of listeners) if (listener.event === 'display_policy_changed' || listener.event === 'settings_changed') callbacks.get(listener.handler)?.({ event: listener.event, id, payload: { settings_revision: settingsRevision, privacy } });
    };
    // Synthetic display oracle tests client invalidation; Rust privacy tests own actual field policy.
    const hide = (value: unknown): unknown => {
      if (Array.isArray(value)) return value.map(hide);
      if (typeof value !== 'object' || value === null) return value;
      const data = value as Record<string, unknown>;
      if ('currencies' in data && 'redacted' in data) return { ...data, redacted: true, currencies: [], reasons: [] };
      return Object.fromEntries(Object.entries(data).map(([key, val]) => [key, val !== null && ['display_name', 'latest_project_name', 'project_display_name', 'parent_display_name', 'parent_provider_id', 'root_path', 'data_directory'].includes(key) ? '隐藏标签' : hide(val)]));
    };
    let id = 0, expired = false, defer = false, release: (() => void) | null = null;
    let detailRevision = 0, deferDetail = false, failDetail = false;
    const detailReleases: (() => void)[] = [];
    const calls: { command: string; args: Record<string, unknown> }[] = [];
    const cursors = new Map<string, { query: string; offset: number; snapshot: string }>();
    const turnCursors = new Map<string, { query: string; offset: number; snapshot: string }>();
    let turnsExpired = false;
    const measure = { value: null, covered_total_tokens: '0', complete: false };
    const tokens = (total: string, count = '1') => ({ total_tokens: total, input_total: measure, cached_input: measure, noncached_input: measure, output_total: measure, reasoning_output: measure, cache_write_input: { value: null, covered_total_tokens: '0', complete: false }, session_count: count, usage_event_count: '2', reliable_turn_count: '1', reliable_turns_complete: false });
    const price = (total: string, basis: unknown = { mode: 'event_time' }) => ({ redacted: false, basis, currencies: [], priced_total_tokens: '0', unpriced_total_tokens: total, reasons: total === '0' ? [] : [{ code: 'insufficient_usage', total_tokens: total, event_count: '2' }], calculating: false });
    const coverage = { state: 'partial', pending_observation_count: '1', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '0', source_issues: [], format_issues: [], breakdown_complete: false };
    Object.assign(window, { isTauri: true, __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: (_event: string, id: number) => { const listener = listeners.get(id); if (listener) callbacks.delete(listener.handler); listeners.delete(id); } }, __TAURI_INTERNALS__: { transformCallback: (callback: (event: unknown) => void) => { callbacks.set(++callbackId, callback); return callbackId; }, invoke: async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      if (command === 'plugin:event|listen') { listeners.set(++eventId, { event: String(args.event), handler: Number(args.handler) }); return eventId; }
      if (command === 'plugin:event|unlisten') return null;
      const response = (data: unknown) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: settingsRevision, privacy }, data: privacy ? hide(data) : data });
      if (command === 'get_display_settings') return response({ settings_version: 1, settings_revision: settingsRevision, preferences: { theme: 'dark', privacy, display_timezone: 'Asia/Shanghai' } });
      if (command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
      if (command === 'set_display_privacy') {
        const mutation = args.request as { privacy: boolean; expected_settings_revision: string };
        if (rejectPrivacy || mutation.expected_settings_revision !== settingsRevision) throw { code: 'REVISION_CONFLICT' };
        if (mutation.privacy !== privacy) { privacy = mutation.privacy; settingsRevision = String(BigInt(settingsRevision) + 1n); notifyPrivacy(); }
        return response({ settings_version: 1, settings_revision: settingsRevision, preferences: { theme: 'dark', privacy, display_timezone: 'Asia/Shanghai' } });
      }
      if (command === 'get_filter_options') {
        const q = args.request as { query: { dimension: string; filter: { range: { timezone: string } } } };
        return response({ meta: { snapshot_id: 'facet', data_revision: '7', price_revision: '3', generated_at_ms: 1000, parser_versions: [], accounting_versions: [], display_timezone: q.query.filter.range.timezone }, dimension: q.query.dimension, options: [{ key: 'synthetic-session-0', display_name: 'Synthetic 会话 0', count: '1' }], next_cursor: null });
      }
      if (command === 'get_mini_scope') return response({ settings_revision: settingsRevision, mini_scope: miniScope });
      if (command === 'set_mini_scope') { const r = args.request as { mini_scope: unknown; expected_settings_revision: string }; if (rejectMini || r.expected_settings_revision !== settingsRevision) throw { code: 'REVISION_CONFLICT' }; miniScope = r.mini_scope; settingsRevision = String(BigInt(settingsRevision) + 1n); return response({ settings_revision: settingsRevision, mini_scope: miniScope }); }
      if (command === 'perform_window_action') return response(null);
      if (command === 'get_app_status') return response({ version: 'synthetic-test', development: true, data_directory: 'synthetic', collector: 'ready', storage: 'ready', storage_error: null, quota: 'not_configured', taskbar: 'not_implemented' });
      if (command === 'get_sources') return response({ settings_revision: '1', sources: [{ source_id: 'empty', root_path: 'Synthetic Empty Source', origin: 'custom', enabled: true, removed: false, readability: 'readable', capabilities: { physical_identity: 'available', byte_seek: 'available', watcher: 'available', polling_required: true }, last_scan_at_ms: 1000, last_success_at_ms: 1000, error: null }] });
      if (command === 'get_dashboard_bundle') throw new Error('Synthetic bridge supplies sessions only');
      if (command === 'get_price_rules') return response({ price_revision: '3', rules: [], aliases: [] });
      if (command === 'close_query_snapshot') return response(null);
      if (command === 'query_turns') {
        const { query, cursor } = args.request as { query: Omit<Query, 'sort'> & { session_key: string }; cursor: string | null };
        if (cursor !== null && turnsExpired) throw { code: 'SNAPSHOT_EXPIRED' };
        const stored = cursor === null ? null : turnCursors.get(cursor);
        if (cursor !== null && (!stored || stored.query !== JSON.stringify(query))) throw { code: 'CURSOR_INVALID' };
        const offset = stored?.offset ?? 0, snapshot = stored?.snapshot ?? `synthetic-turn-snapshot-${++id}`;
        const turns = Array.from({ length: 23 }, (_, index) => {
          const total = index === 0 ? '18446744073709551614' : String(100 - index);
          return { turn_id: `synthetic-turn-${index}`, first_at_ms: query.filter.range.start_ms + (23 - index) * 1000, last_at_ms: query.filter.range.start_ms + (23 - index) * 1000 + 500, summary: { ...tokens(total), usage_event_count: '2', reliable_turns_complete: true }, pricing: price(total, query.price_basis) };
        });
        const total = (turns.reduce((sum, t) => sum + BigInt(t.summary.total_tokens), 0n) + 7n).toString();
        const next = offset + query.page_size < turns.length ? `${++id}`.padStart(151, 't') : null;
        if (next !== null) turnCursors.set(next, { query: JSON.stringify(query), offset: offset + query.page_size, snapshot });
        return response({ meta: { snapshot_id: snapshot, data_revision: '10', price_revision: '3', generated_at_ms: query.filter.range.start_ms + 3000, parser_versions: ['synthetic'], accounting_versions: ['synthetic'], display_timezone: query.filter.range.timezone }, session_key: query.session_key, summary: { ...tokens(total), usage_event_count: '49', reliable_turn_count: '23' }, pricing: price(total, query.price_basis), coverage, unidentified_usage_event_count: '3', turns: turns.slice(offset, offset + query.page_size), next_cursor: next });
      }
      if (command === 'get_session_bundle') {
        const request = args.request as { session_key: string; filter: Query['filter']; price_basis: unknown };
        if (failDetail) throw { code: 'DB_CORRUPT' };
        const index = Number(request.session_key.split('-').at(-1));
        const identity = { session_key: request.session_key, display_name: `Synthetic 会话 ${index}`, parent_key: index === 0 ? 'synthetic-session-1' : null, parent_display_name: index === 0 ? 'Synthetic 会话 1' : null, parent_provider_id: index === 0 ? 'parent-provider' : 'unresolved-parent' };
        const total = detailRevision === 0 ? '321' : '654';
        const data = { meta: { snapshot_id: `synthetic-detail-${detailRevision}`, data_revision: String(8 + detailRevision), price_revision: String(3 + detailRevision), generated_at_ms: request.filter.range.start_ms + 2000, parser_versions: ['synthetic'], accounting_versions: ['synthetic'], display_timezone: request.filter.range.timezone }, identity, summary: tokens(total), pricing: price(total, request.price_basis), coverage, latest_selected_activity: { occurred_at_ms: request.filter.range.start_ms + 1000, model: `Synthetic Detail Model ${detailRevision}`, project_id: 'detail-project', project_display_name: 'Synthetic Detail Project' }, latest_context: { context_tokens: index === 0 ? '9007199254740993' : null, model_context_window: null, percentage: null, observed_at_ms: index === 0 ? request.filter.range.end_ms + 12000 : null, quality: index === 0 ? 'confirmed' : 'unknown' }, child_count: index === 0 ? '3' : '0', children: index === 0 ? [{ session_key: 'synthetic-session-2', display_name: 'Synthetic 会话 2', parent_key: request.session_key, parent_display_name: identity.display_name, parent_provider_id: null }] : [], children_truncated: index === 0, classifications: [{ kind: 'inherited', reason_code: 'inherited_prefix', observation_count: '1' }, { kind: 'pending', reason_code: 'lineage_pending', observation_count: '2' }] };
        if (deferDetail) await new Promise<void>(resolve => { detailReleases.push(resolve); });
        return response(data);
      }
      if (command === 'query_sessions') {
        const { query, cursor } = args.request as { query: Query; cursor: string | null };
        if (cursor !== null && expired) throw { code: 'SNAPSHOT_EXPIRED' };
        const stored = cursor === null ? null : cursors.get(cursor);
        if (cursor !== null && (!stored || stored.query !== JSON.stringify(query))) throw { code: 'CURSOR_INVALID' };
        const snapshot = stored?.snapshot ?? `synthetic-session-snapshot-${++id}`;
        const offset = stored?.offset ?? 0;
        const empty = query.filter.sources.ids?.includes('empty') ?? false;
        let sessions = empty ? [] : Array.from({ length: 55 }, (_, index) => {
          const key = `synthetic-session-${index}`;
          const total = index === 0 ? '9007199254740993' : String(100 - index);
          return { session_key: key, display_name: `Synthetic 会话 ${index}`, latest_at_ms: query.filter.range.start_ms + (55 - index) * 1000, latest_model: index === 1 ? null : 'Synthetic Model', latest_project_id: index === 1 ? null : 'project', latest_project_name: index === 1 ? null : 'Synthetic Project', parent_key: index === 0 ? 'synthetic-session-1' : null, parent_display_name: index === 0 ? 'Synthetic 会话 1' : null, parent_provider_id: index === 0 ? 'parent-provider' : index === 1 ? 'unresolved-parent' : null, child_count: index === 1 ? '1' : '0', summary: tokens(total), pricing: price(total, query.price_basis), coverage, latest_context: { context_tokens: index === 0 ? '9007199254740993' : null, model_context_window: null, percentage: null, observed_at_ms: index === 0 ? query.filter.range.end_ms + 12000 : null, quality: index === 0 ? 'confirmed' : 'unknown' } };
        });
        if (query.filter.sessions.ids) sessions = sessions.filter(s => query.filter.sessions.ids!.includes(s.session_key));
        const total = sessions.reduce((sum, s) => sum + BigInt(s.summary.total_tokens), 0n).toString();
        const next = offset + query.page_size < sessions.length ? `${++id}`.padStart(151, 'a') : null;
        if (next !== null) cursors.set(next, { query: JSON.stringify(query), offset: offset + query.page_size, snapshot });
        const data = { meta: { snapshot_id: snapshot, data_revision: '7', price_revision: '3', generated_at_ms: query.filter.range.start_ms + 1000, parser_versions: ['synthetic'], accounting_versions: ['synthetic'], display_timezone: query.filter.range.timezone }, summary: tokens(total, String(sessions.length)), pricing: price(total, query.price_basis), coverage, sessions: sessions.slice(offset, offset + query.page_size), next_cursor: next };
        const serialized = freezePrivacyReply ? response(data) : null;
        if (defer) { defer = false; await new Promise<void>(resolve => { release = resolve; }); }
        return serialized ?? response(data);
      }
      throw new Error(`Unexpected synthetic command ${command}`);
    } }, __syntheticSessionCalls: () => calls, __miniPinQA: { reject: (v: boolean) => { rejectMini = v; } }, __expireSyntheticSessions: () => { expired = true; }, __resetSyntheticSessions: () => { expired = false; }, __deferSyntheticSessions: () => { defer = true; }, __releaseSyntheticSessions: () => { release?.(); release = null; }, __reviseSyntheticDetail: () => { ++detailRevision; }, __failSyntheticDetail: (fail: boolean) => { failDetail = fail; }, __deferSyntheticDetail: () => { deferDetail = true; }, __releaseSyntheticDetail: () => { deferDetail = false; for (const release of detailReleases.splice(0)) release(); } });
    Object.assign(window, { __privacyQA: { external: (value: boolean) => { privacy = value; settingsRevision = String(BigInt(settingsRevision) + 1n); notifyPrivacy(); }, reject: (value: boolean) => { rejectPrivacy = value; }, deferOld: () => { freezePrivacyReply = true; defer = true; }, listeners: () => [...listeners.values()].filter(l => l.event === 'display_policy_changed').length }, __expireSyntheticTurns: () => { turnsExpired = true; }, __resetSyntheticTurns: () => { turnsExpired = false; } });
  });
  await page.goto('/');
  await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '会话', exact: true }).click();
});

test('sessions show exact consumption and independent context, stable pages and an accessible drawer', async ({ page }) => {
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await expect(page.locator('.session-table tbody tr').first().locator('td.numeric[title="9,007,199,254,740,993"]')).toBeVisible();
  await expect(page.locator('.session-table tbody tr').nth(1)).toContainText('未知项目');
  await expect(page.locator('.session-table tbody tr').nth(1)).toContainText('父会话尚未解析');
  await expect(page.locator('.session-table tbody tr').first().locator('td').nth(4)).toContainText('2 / 1');
  await page.screenshot({ path: 'test-results/sessions-1280.png' });
  const trigger = page.getByRole('button', { name: 'Synthetic 会话 0', exact: true });
  await trigger.click();
  const drawer = page.getByRole('dialog', { name: 'Synthetic 会话 0', exact: true });
  await expect(drawer).toBeVisible();
  await expect(drawer.getByRole('heading', { name: '所选范围累计消耗' })).toBeVisible();
  await expect(drawer.getByRole('heading', { name: '最近请求上下文' })).toBeVisible();
  await expect(drawer.getByText('窗口容量未知', { exact: true })).toBeVisible();
  await expect(drawer).toContainText('最近上下文不表示所选日期内累计消费');
  await expect(drawer.locator('.session-drawer-total')).toContainText('321');
  await expect(drawer).toContainText('详情整体快照 · 数据 8 / 价格 3');
  await expect(drawer.getByRole('heading', { name: '当前账本分类证据' })).toBeVisible();
  await expect(drawer.locator('.session-classifications')).toContainText('父序列前缀已验证');
  await expect(drawer.locator('.session-classifications')).toContainText('继承边界待确认');
  await expect(drawer).toContainText('显示前 1 个子关系，共 3 个');
  await expect(drawer.getByRole('list', { name: '已解析子会话' }).getByRole('button', { name: 'Synthetic 会话 2' })).toBeVisible();
  await expect(page.getByRole('button', { name: '关闭会话详情' })).toBeFocused();
  expect(await page.locator('.workspace').evaluate(element => (element as HTMLElement).inert)).toBe(true);
  await page.screenshot({ path: 'test-results/session-drawer-top-1280.png' });
  await page.keyboard.press('Shift+Tab');
  await expect(drawer.getByRole('button', { name: '按所选起点固定到小窗' })).toBeFocused();
  await page.screenshot({ path: 'test-results/session-drawer-1280.png' });
  await page.keyboard.press('Escape');
  await expect(drawer).toHaveCount(0); await expect(trigger).toBeFocused();
  expect(await page.locator('.workspace').evaluate(element => (element as HTMLElement).inert)).toBe(false);
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(page.locator('.session-table tbody tr')).toHaveCount(5);
  await expect(page.getByRole('button', { name: '下一页', exact: true })).toBeDisabled();
  await expect(page.locator('.session-pagination')).toContainText('第 2 页 / 2 页');
  await page.getByRole('button', { name: '上一页', exact: true }).click();
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.setViewportSize({ width: 960, height: 680 });
  await page.getByRole('heading', { name: '会话', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/sessions-960.png' });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await trigger.click();
  await page.getByRole('dialog').getByRole('button', { name: 'Synthetic 会话 1', exact: true }).click();
  await expect(page.getByRole('combobox', { name: '会话', exact: true })).toContainText('Synthetic 会话 1');
  await expect(page.locator('.session-table tbody tr')).toHaveCount(1);
  await expect(page.locator('.session-table tbody tr')).toContainText('Synthetic 会话 1');
});

test('expired pages preserve the frozen page and explicit requery replaces it without mixing results', async ({ page }) => {
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.evaluate(() => (window as unknown as { __expireSyntheticSessions: () => void }).__expireSyntheticSessions());
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('查询快照已过期');
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await expect(page.getByRole('button', { name: '下一页', exact: true })).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __resetSyntheticSessions: () => void }).__resetSyntheticSessions());
  await page.getByRole('button', { name: '重新查询', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await expect(page.getByRole('button', { name: '下一页', exact: true })).toBeEnabled();
});

test('sort and scope changes serialize release, clear obsolete values and clean up late results on navigation', async ({ page }) => {
  type Calls = { command: string; args: { request?: { kind?: string; query: { sort: string; filter: { sources: unknown } }; request?: { query: { sort: string }; cursor: string }; cursor: string | null } } }[];
  type Bridge = { __syntheticSessionCalls: () => Calls; __deferSyntheticSessions: () => void; __releaseSyntheticSessions: () => void };
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.getByLabel('会话排序').selectOption('total_desc');
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  const calls = await page.evaluate(() => (window as unknown as Bridge).__syntheticSessionCalls());
  const closed = calls.findIndex(c => c.command === 'close_query_snapshot' && c.args.request?.kind === 'sessions');
  const sorted = calls.findIndex(c => c.command === 'query_sessions' && c.args.request?.query.sort === 'total_desc');
  expect(closed).toBeGreaterThanOrEqual(0); expect(sorted).toBeGreaterThan(closed);
  expect(calls[closed].args.request?.request?.query.sort).toBe('latest_desc');
  await page.getByLabel('来源', { exact: true }).selectOption('empty');
  await expect(page.getByRole('heading', { name: '当前筛选暂无消费会话' })).toBeVisible();
  await expect(page.locator('.session-table')).toHaveCount(0);
  await page.evaluate(() => (window as unknown as Bridge).__deferSyntheticSessions());
  await page.getByLabel('来源', { exact: true }).selectOption('');
  await expect(page.getByRole('heading', { name: '正在读取会话快照' })).toBeVisible();
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.evaluate(() => (window as unknown as Bridge).__releaseSyntheticSessions());
  await expect.poll(async () => (await page.evaluate(() => (window as unknown as Bridge).__syntheticSessionCalls())).filter(c => c.command === 'close_query_snapshot').length).toBeGreaterThanOrEqual(3);
  await expect(page.locator('.session-table')).toHaveCount(0);
});

test('detail refresh replaces its whole bundle, child navigation retains filters, and late responses stay closed', async ({ page }) => {
  type Bridge = { __reviseSyntheticDetail: () => void; __failSyntheticDetail: (fail: boolean) => void; __deferSyntheticDetail: () => void; __releaseSyntheticDetail: () => void };
  await page.getByRole('button', { name: 'Synthetic 会话 0', exact: true }).click();
  const drawer = page.getByRole('dialog');
  await expect(drawer.locator('.session-drawer-total')).toContainText('321');
  await page.evaluate(() => (window as unknown as Bridge).__reviseSyntheticDetail());
  await drawer.getByRole('button', { name: '刷新详情' }).click();
  await expect(drawer.locator('.session-drawer-total')).toContainText('654');
  await expect(drawer).toContainText('Synthetic Detail Model 1');
  await expect(drawer).toContainText('详情整体快照 · 数据 9 / 价格 4');
  await drawer.locator('.session-classifications').scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/session-classifications-1280.png' });
  await page.setViewportSize({ width: 960, height: 680 });
  await page.screenshot({ path: 'test-results/session-classifications-960.png' });
  expect(await drawer.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
  await drawer.getByRole('list', { name: '已解析子会话' }).getByRole('button', { name: 'Synthetic 会话 2', exact: true }).click();
  await expect(drawer).toHaveCount(0);
  await expect(page.getByRole('combobox', { name: '会话', exact: true })).toContainText('Synthetic 会话 2');
  await expect(page.locator('.session-table tbody tr')).toHaveCount(1);
  await page.evaluate(() => (window as unknown as Bridge).__failSyntheticDetail(true));
  await page.getByRole('button', { name: 'Synthetic 会话 2', exact: true }).click();
  await expect(drawer.getByRole('alert')).toBeVisible();
  await expect(drawer.locator('.session-drawer-total')).toHaveCount(0);
  await expect(drawer.getByRole('button', { name: '刷新详情' })).toBeEnabled();
  await page.evaluate(() => (window as unknown as Bridge).__failSyntheticDetail(false));
  await drawer.getByRole('button', { name: '刷新详情' }).click();
  await expect(drawer.locator('.session-drawer-total')).toContainText('654');
  await page.keyboard.press('Escape');
  await page.evaluate(() => (window as unknown as Bridge).__deferSyntheticDetail());
  const trigger = page.getByRole('button', { name: 'Synthetic 会话 2', exact: true });
  await trigger.click();
  await expect(drawer).toContainText('正在读取详情快照');
  await expect(drawer.locator('.session-drawer-total')).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(trigger).toBeFocused();
  await page.evaluate(() => (window as unknown as Bridge).__releaseSyntheticDetail());
  await expect(drawer).toHaveCount(0);
  expect(await page.locator('.workspace').evaluate(element => (element as HTMLElement).inert)).toBe(false);
});

test('reliable turns use their own stable pages, retain unknown event counts and release on collapse', async ({ page }) => {
  type Bridge = { __syntheticSessionCalls: () => { command: string; args: { request?: { kind?: string } } }[]; __expireSyntheticTurns: () => void; __resetSyntheticTurns: () => void };
  await page.getByRole('button', { name: 'Synthetic 会话 0', exact: true }).click();
  const drawer = page.getByRole('dialog');
  const trigger = drawer.getByRole('button', { name: '查看可靠回合' });
  await trigger.click();
  await expect(drawer.getByRole('list', { name: '已识别回合列表' }).locator('li')).toHaveCount(20);
  await expect(drawer.locator('.session-turn-list')).toContainText('已识别回合 23，未识别回合的用量事件 3 条');
  await expect(drawer.locator('.session-turn-list')).toContainText('回合识别不完整');
  await expect(drawer.locator('.session-turn-list')).toContainText('回合分页固定数据 10 / 价格 3');
  await expect(drawer.locator('.session-turn-cards [title="18,446,744,073,709,551,614"]')).toBeVisible();
  await drawer.locator('.session-turn-list').scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/session-turns-1280.png' });
  await drawer.getByRole('button', { name: '下一页回合' }).click();
  await expect(drawer.getByRole('list', { name: '已识别回合列表' }).locator('li')).toHaveCount(3);
  await expect(drawer.getByRole('button', { name: '下一页回合' })).toBeDisabled();
  await drawer.getByRole('button', { name: '上一页回合' }).click();
  await expect(drawer.getByRole('list', { name: '已识别回合列表' }).locator('li')).toHaveCount(20);
  await drawer.getByRole('button', { name: '重新读取回合' }).click();
  await page.evaluate(() => (window as unknown as Bridge).__expireSyntheticTurns());
  await drawer.getByRole('button', { name: '下一页回合' }).click();
  await expect(drawer.locator('.session-turn-list').getByRole('alert')).toContainText('查询快照已过期');
  await expect(drawer.getByRole('list', { name: '已识别回合列表' }).locator('li')).toHaveCount(20);
  await expect(drawer.getByRole('button', { name: '下一页回合' })).toBeDisabled();
  await page.evaluate(() => (window as unknown as Bridge).__resetSyntheticTurns());
  await drawer.getByRole('button', { name: '重新读取回合' }).click();
  await expect(drawer.locator('.session-turn-list').getByRole('alert')).toHaveCount(0);
  await page.setViewportSize({ width: 960, height: 680 });
  await drawer.locator('.session-turn-list').scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/session-turns-960.png' });
  expect(await drawer.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
  const beforeCollapse = await page.evaluate(() => (window as unknown as Bridge).__syntheticSessionCalls().filter(c => c.command === 'close_query_snapshot' && c.args.request?.kind === 'turns').length);
  await drawer.getByRole('button', { name: '收起回合列表' }).click();
  await expect(drawer.getByRole('list', { name: '已识别回合列表' })).toHaveCount(0);
  // Terminal pages release themselves on the server. Collapse closes exactly
  // the remaining nonterminal capability, rather than inventing extra closes.
  await expect.poll(async () => (await page.evaluate(() => (window as unknown as Bridge).__syntheticSessionCalls())).filter(c => c.command === 'close_query_snapshot' && c.args.request?.kind === 'turns').length).toBe(beforeCollapse + 1);
});

test('specified estimate time reaches session pages, detail and reliable turns with exact cursor scope', async ({ page }) => {
  await expect(page.locator('.session-table>tbody>tr')).toHaveCount(50);
  await page.getByLabel('计价依据').selectOption('specified_time'); await page.getByTitle('编辑明确估价时点（UTC）').click();
  await page.getByLabel('估价时点（UTC）').fill('2026-11-01T05:30:00.123'); await page.getByRole('button', { name: '应用估价时点' }).click();
  await expect(page.locator('.session-table>tbody>tr')).toHaveCount(50);
  await page.locator('.session-table>tbody>tr').first().getByRole('button').click();
  await expect(page.getByRole('dialog', { name: 'Synthetic 会话 0' })).toBeVisible();
  await page.getByRole('button', { name: '查看可靠回合' }).click(); await expect(page.getByRole('list', { name: '已识别回合列表' }).locator('li')).toHaveCount(20);
  type Bridge = { __syntheticSessionCalls: () => { command: string; args: { request: { query?: { price_basis: unknown }; price_basis?: unknown } } }[] };
  const calls = await page.evaluate(() => (window as unknown as Bridge).__syntheticSessionCalls());
  const expected = { mode: 'specified_time', specified_at_ms: Date.parse('2026-11-01T05:30:00.123Z') };
  for (const command of ['query_sessions', 'get_session_bundle', 'query_turns']) {
    const r = calls.filter(value => value.command === command).at(-1)!.args.request;
    expect(r.query?.price_basis ?? r.price_basis).toEqual(expected);
  }
});
type SessionQA = { __syntheticSessionCalls: () => { command: string; args: Record<string, unknown> }[]; __releaseSyntheticSessions: () => void };
type PrivacyQA = { __privacyQA: { external: (value: boolean) => void; reject: (value: boolean) => void; deferOld: () => void; listeners: () => number } };

test('external privacy closes the real drawer and clears retained pages, labels and path attributes', async ({ page }) => {
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.getByRole('button', { name: 'Synthetic 会话 0', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'Synthetic 会话 0', exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as PrivacyQA).__privacyQA.external(true));
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await page.locator('.workspace').evaluate(e => (e as HTMLElement).inert)).toBe(false);
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await expect(page.locator('.session-table')).toContainText('隐藏标签');
  await expect(page.locator('.session-table tbody tr').first().locator('td.numeric[title="9,007,199,254,740,993"]')).toBeVisible();
  expect(await page.locator('body').evaluate(e => e.innerHTML.includes('Synthetic 会话') || e.innerHTML.includes('Synthetic Project') || e.innerHTML.includes('Synthetic Empty Source'))).toBe(false);
  await page.getByRole('button', { name: '设置', exact: true }).click(); await page.getByRole('tab', { name: '显示与窗口' }).click();
  await expect(page.getByLabel('隐私模式')).toBeChecked();
  await page.screenshot({ path: 'test-results/privacy-display-1280.png', fullPage: true });
  await page.setViewportSize({ width: 960, height: 680 }); await page.screenshot({ path: 'test-results/privacy-display-960.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.getByRole('tab', { name: '价格规则' }).click(); await expect(page.getByRole('heading', { name: '价格规则已隐藏' })).toBeVisible();
  await page.getByRole('tab', { name: '数据来源' }).click(); await expect(page.getByRole('button', { name: '添加自定义目录' })).toHaveCount(0);
  await page.getByRole('tab', { name: '显示与窗口' }).click(); await page.getByLabel('隐私模式').uncheck();
  await expect(page.getByLabel('隐私模式')).not.toBeChecked();
  await page.getByRole('button', { name: '会话', exact: true }).click(); await expect(page.getByRole('button', { name: 'Synthetic 会话 0', exact: true })).toBeVisible();
  await expect.poll(async () => page.evaluate(() => (window as unknown as PrivacyQA).__privacyQA.listeners())).toBe(1);
});

test('late serialized unmasked page is dropped and releases its original query while new private page remains', async ({ page }) => {
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.evaluate(() => (window as unknown as PrivacyQA).__privacyQA.deferOld());
  await page.getByLabel('会话排序').selectOption('total_desc');
  await expect.poll(async () => page.evaluate(() => (window as unknown as SessionQA).__syntheticSessionCalls().filter(v => v.command === 'query_sessions').at(-1)?.args.request)).toMatchObject({ query: { sort: 'total_desc' } });
  await page.evaluate(() => (window as unknown as PrivacyQA).__privacyQA.external(true));
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.evaluate(() => (window as unknown as SessionQA).__releaseSyntheticSessions());
  await expect.poll(async () => page.evaluate(() => (window as unknown as SessionQA).__syntheticSessionCalls().filter(v => v.command === 'close_query_snapshot').some(v => {
    const request = v.args.request as { request: { query: { sort: string }; cursor: string } }; return request.request.query.sort === 'total_desc' && request.request.cursor.length === 151;
  }))).toBe(true);
  expect(await page.locator('body').evaluate(e => e.innerHTML.includes('Synthetic 会话') || e.innerHTML.includes('Synthetic Project'))).toBe(false);
});

test('failed privacy save keeps protection and explicit successful disable restores only new data', async ({ page }) => {
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.getByRole('button', { name: '设置', exact: true }).click(); await page.getByRole('tab', { name: '显示与窗口' }).click();
  await page.evaluate(() => (window as unknown as PrivacyQA).__privacyQA.reject(true));
  await page.getByLabel('隐私模式').check();
  await expect(page.locator('.privacy-status')).toContainText('保存失败');
  await expect(page.getByLabel('隐私模式')).toBeChecked();
  expect(await page.locator('body').evaluate(e => e.innerHTML.includes('Synthetic Empty Source'))).toBe(false);
  await page.evaluate(() => (window as unknown as PrivacyQA).__privacyQA.reject(false));
  await page.getByLabel('隐私模式').uncheck(); await expect(page.getByLabel('隐私模式')).not.toBeChecked();
  await page.getByRole('button', { name: '会话', exact: true }).click(); await expect(page.getByRole('button', { name: 'Synthetic 会话 0', exact: true })).toBeVisible();
});
test('privacy clears candidate searches and names while retaining selected stable session scope', async ({ page }) => {
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.getByRole('combobox', { name: '会话', exact: true }).click();
  await page.getByRole('option', { name: /Synthetic 会话 0/ }).click();
  await expect(page.locator('.session-table tbody tr')).toHaveCount(1);
  await page.getByRole('combobox', { name: '会话', exact: true }).click();
  await page.getByRole('textbox', { name: /搜索会话/ }).fill('Synthetic 会话');
  await page.evaluate(() => (window as unknown as PrivacyQA).__privacyQA.external(true));
  await expect(page.getByRole('textbox', { name: /搜索会话/ })).toHaveCount(0);
  await expect(page.locator('.session-table tbody tr')).toHaveCount(1);
  await expect(page.getByRole('combobox', { name: '会话', exact: true })).toContainText('已隐藏');
  const request = await page.evaluate(() => (window as unknown as SessionQA).__syntheticSessionCalls().filter(v => v.command === 'query_sessions').at(-1)?.args.request);
  expect(request).toMatchObject({ query: { filter: { sessions: { kind: 'ids', ids: ['synthetic-session-0'] } } } });
  expect(await page.locator('body').evaluate(e => e.innerHTML.includes('Synthetic 会话'))).toBe(false);
});


test('detail pins today or exact selected start to mini independently and preserves the current main query on conflict', async ({ page }) => {
  await expect(page.locator('.session-table tbody tr')).toHaveCount(50);
  await page.getByRole('button', { name: 'Synthetic 会话 0', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: 'Synthetic 会话 0', exact: true });
  await drawer.getByRole('button', { name: '固定到小窗（今日）' }).click();
  await expect(drawer.getByRole('status')).toContainText('每天从统计时区零点');
  const calls = await page.evaluate(() => (window as unknown as { __syntheticSessionCalls: () => { command: string; args: { request?: unknown; action?: string } }[] }).__syntheticSessionCalls());
  expect(calls.filter(c => c.command === 'set_mini_scope').at(-1)?.args.request).toMatchObject({ mini_scope: { kind: 'session', session_key: 'synthetic-session-0', start: { kind: 'today' } } });
  expect(calls.filter(c => c.command === 'perform_window_action').at(-1)?.args.action).toBe('show_mini');
  await drawer.getByRole('button', { name: '按所选起点固定到小窗' }).click();
  await expect(drawer.getByRole('status')).toContainText('精确起点');
  const after = await page.evaluate(() => (window as unknown as { __syntheticSessionCalls: () => { command: string; args: { request?: unknown } }[] }).__syntheticSessionCalls());
  const detail = after.filter(c => c.command === 'get_session_bundle').at(-1)?.args.request as { filter: { range: { start_ms: number } } };
  expect(after.filter(c => c.command === 'set_mini_scope').at(-1)?.args.request).toMatchObject({ mini_scope: { start: { kind: 'fixed', start_ms: detail.filter.range.start_ms } } });
  await page.evaluate(() => (window as unknown as { __miniPinQA: { reject(v: boolean): void } }).__miniPinQA.reject(true));
  await drawer.getByRole('button', { name: '固定到小窗（今日）' }).click(); await expect(drawer.getByRole('alert')).toContainText('已发生变化');
  await expect(drawer.getByRole('heading', { name: '所选范围累计消耗' })).toBeVisible();
  await expect(page.getByLabel('日期范围')).toHaveValue('today');
});
