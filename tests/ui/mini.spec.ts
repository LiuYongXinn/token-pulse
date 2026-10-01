import { expect, test, type Page } from '@playwright/test';

// All prices, identities and counters in this bridge are explicitly synthetic browser QA data.
async function bridge(page: Page) {
  await page.addInitScript(() => {
    let revision = '9007199254740993', privacy = false, theme = 'dark', fail = false, hold = false, mode = 'nonempty';
    let rejectScope = false, expireCandidates = false, holdCandidates = false, candidateSerial = 0;
    const candidateWaits: (() => void)[] = [], candidateCursors = new Map<string, { query: string; offset: number; snapshot: string }>();
    let state = { expanded: false, pinned: false };
    let scope: { kind: 'today_all_sources' } | { kind: 'session'; session_key: string; start: { kind: 'fixed'; start_ms: number } | { kind: 'today' } } = { kind: 'session', session_key: 'synthetic-session', start: { kind: 'fixed', start_ms: 1709179200123 } };
    const calls: { command: string; request: unknown }[] = [];
    const waits: (() => void)[] = [];
    let callbackId = 0, eventId = 0;
    const callbacks = new Map<number, (event: unknown) => void>(), listeners = new Map<number, { event: string; handler: number }>();
    const emit = (event: string, payload: unknown) => { for (const [id, item] of listeners) if (item.event === event) callbacks.get(item.handler)?.({ event, id, payload }); };
    const changed = () => emit('settings_changed', { settings_revision: revision });
    const measure = (value: string | null) => ({ value, covered_total_tokens: value === null ? '0' : '683067', complete: value !== null });
    const snapshot = () => ({ meta: { snapshot_id: 'synthetic-mini', data_revision: '7', price_revision: '3', generated_at_ms: 1709203200456, parser_versions: ['synthetic-v1'], accounting_versions: ['synthetic-v1'], display_timezone: 'UTC' }, settings_revision: revision, mini_scope: structuredClone(scope), scope_display_name: scope.kind === 'session' ? privacy ? '会话 · 123456' : 'SYNTHETIC PRIVATE SESSION' : '全部来源 · 今日', range: { start_ms: scope.kind === 'session' && scope.start.kind === 'fixed' ? scope.start.start_ms : 1709164800000, end_ms: 1709203200457, timezone: 'UTC' }, usage: { total_tokens: '683067', input_total: measure('600000'), noncached_input: measure('180000'), cached_input: measure('420000'), output_total: measure('83067'), reasoning_output: measure(null), session_count: '1', usage_event_count: '5', reliable_turn_count: null, reliable_turns_complete: false }, coverage: { state: 'partial', pending_observation_count: '1', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '0', source_issues: [], format_issues: [], breakdown_complete: false }, pricing: { redacted: privacy, basis: { mode: 'event_time' }, currencies: privacy ? [] : [{ currency: 'USD', estimated_cost: '0.573123456789012', priced_total_tokens: '600000' }], priced_total_tokens: '600000', unpriced_total_tokens: '83067', reasons: [], calculating: false } });
    const response = (requestId: unknown, data: unknown) => ({ api_version: 1, request_id: requestId, display_policy: { settings_revision: revision, privacy }, data });
    Object.assign(window, { isTauri: true,
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: (_: string, id: number) => { const listener = listeners.get(id); if (listener) callbacks.delete(listener.handler); listeners.delete(id); } },
      __TAURI_INTERNALS__: { transformCallback: (fn: (event: unknown) => void) => { callbacks.set(++callbackId, fn); return callbackId; }, invoke: async (command: string, args: Record<string, unknown>) => {
        calls.push({ command, request: args.request });
        if (command === 'plugin:event|listen') { listeners.set(++eventId, { event: String(args.event), handler: Number(args.handler) }); return eventId; }
        if (command === 'plugin:event|unlisten') return null;
        if (command === 'get_display_settings') return response(args.requestId, { settings_version: 1, settings_revision: revision, preferences: { theme, privacy, display_timezone: 'UTC' } });
        if (command === 'get_mini_usage') {
          if (fail) throw { code: 'DB_READ_FAILED' };
          // Preserve a deliberately late, unredacted response to test the client epoch barrier.
          const data = snapshot();
          if (mode === 'unknown' || mode === 'zero') {
            data.usage.total_tokens = '0'; data.usage.usage_event_count = '0';
            for (const dimension of ['input_total', 'noncached_input', 'cached_input', 'output_total', 'reasoning_output'] as const) data.usage[dimension] = { value: mode === 'zero' ? '0' : null, covered_total_tokens: '0', complete: mode === 'zero' };
            data.coverage.state = mode === 'zero' ? 'complete' : 'unknown'; data.coverage.pending_observation_count = '0';
            data.pricing.currencies = []; data.pricing.priced_total_tokens = '0'; data.pricing.unpriced_total_tokens = '0';
          } else if (mode === 'unpriced') { data.pricing.currencies = []; data.pricing.priced_total_tokens = '0'; data.pricing.unpriced_total_tokens = '683067'; data.usage.cached_input.complete = false; }
          const stamp = { settings_revision: revision, privacy };
          if (hold) { hold = false; await new Promise<void>(resolve => waits.push(resolve)); }
          return { api_version: 1, request_id: args.requestId, display_policy: stamp, data };
        }
        if (command === 'open_mini_stats') { if ((args.request as { expected_settings_revision: string }).expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' }; const data = snapshot(); return { api_version: 1, request_id: args.requestId, data: { request_id: String(args.requestId), mini_scope: data.mini_scope, calendar: { range: data.range, heatmap_range: data.range, local_today: '2024-02-29' } } }; }
        if (command === 'mini_window_action') {
          const r = args.request as { kind: string; expanded?: boolean; pinned?: boolean };
          if (r.kind === 'set_expanded') state.expanded = r.expanded!;
          if (r.kind === 'set_pinned') state.pinned = r.pinned!;
          return { api_version: 1, request_id: args.requestId, data: { ...state } };
        }
        if (command === 'set_display_privacy') {
          const r = args.request as { privacy: boolean; expected_settings_revision: string };
          if (r.expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' };
          privacy = r.privacy; revision = String(BigInt(revision) + 1n);
          emit('display_policy_changed', { settings_revision: revision, privacy }); changed();
          return response(args.requestId, { settings_version: 1, settings_revision: revision, preferences: { theme, privacy, display_timezone: 'UTC' } });
        }
        if (command === 'query_mini_sessions') {
          const { query, cursor } = args.request as { query: { search: string; page_size: number }; cursor: string | null };
          if (cursor !== null && expireCandidates) throw { code: 'SNAPSHOT_EXPIRED' };
          const saved = cursor === null ? null : candidateCursors.get(cursor);
          if (cursor !== null && (!saved || saved.query !== JSON.stringify(query))) throw { code: 'CURSOR_INVALID' };
          const id = saved?.snapshot ?? 'synthetic-candidate-' + ++candidateSerial, offset = saved?.offset ?? 0;
          const options = Array.from({ length: 55 }, (_, i) => ({ session_key: 'synthetic-candidate-' + String(i).padStart(2, '0'), display_name: privacy ? '会话 · ' + i : 'SYNTHETIC PRIVATE CANDIDATE ' + i })).filter(o => o.display_name.toLowerCase().includes(query.search.toLowerCase()));
          const next = offset + query.page_size < options.length ? String(++candidateSerial).padStart(151, 's') : null;
          if (next !== null) candidateCursors.set(next, { query: JSON.stringify(query), offset: offset + query.page_size, snapshot: id });
          const reply = response(args.requestId, { meta: { snapshot_id: id, data_revision: '7', price_revision: '3', generated_at_ms: 1709203200456, parser_versions: [], accounting_versions: [], display_timezone: 'UTC' }, options: options.slice(offset, offset + query.page_size), next_cursor: next });
          if (holdCandidates) { holdCandidates = false; await new Promise<void>(r => candidateWaits.push(r)); }
          return reply;
        }
        if (command === 'close_query_snapshot') {
          const r = args.request as { kind: string; request: { query: unknown; cursor: string } };
          if (r.kind !== 'mini_sessions') throw { code: 'PERMISSION_DENIED' };
          const saved = candidateCursors.get(r.request.cursor);
          if (saved && saved.query !== JSON.stringify(r.request.query)) throw { code: 'CURSOR_INVALID' };
          candidateCursors.delete(r.request.cursor); return response(args.requestId, null);
        }
        if (command === 'set_mini_scope') {
          const r = args.request as { mini_scope: typeof scope; expected_settings_revision: string };
          if (rejectScope || r.expected_settings_revision !== revision) throw { code: 'REVISION_CONFLICT' };
          scope = r.mini_scope; revision = String(BigInt(revision) + 1n); changed();
          return response(args.requestId, { settings_revision: revision, mini_scope: scope });
        }
        throw new Error(`Unexpected synthetic command ${command}`);
      } }, __miniQA: { calls: () => calls, fail: (value: boolean) => { fail = value; }, theme: (value: string) => { theme = value; revision = String(BigInt(revision) + 1n); changed(); }, hold: () => { hold = true; }, release: () => waits.splice(0).forEach(fn => fn()), privacy: (value: boolean) => { privacy = value; revision = String(BigInt(revision) + 1n); emit('display_policy_changed', { settings_revision: revision, privacy }); changed(); }, listeners: () => [...listeners.values()].map(l => l.event), mode: (value: string) => { mode = value; changed(); }, rejectScope: (value: boolean) => { rejectScope = value; }, expireCandidates: (value: boolean) => { expireCandidates = value; }, holdCandidates: () => { holdCandidates = true; }, releaseCandidates: () => candidateWaits.splice(0).forEach(fn => fn()), candidateLeases: () => candidateCursors.size } });
  });
}
type QA = { __miniQA: { calls(): { command: string; request: unknown }[]; fail(v: boolean): void; theme(v: string): void; hold(): void; release(): void; privacy(v: boolean): void; listeners(): string[]; mode(v: string): void; rejectScope(v: boolean): void; expireCandidates(v: boolean): void; holdCandidates(): void; releaseCandidates(): void; candidateLeases(): number } };
test.beforeEach(async ({ page }) => { await bridge(page); await page.setViewportSize({ width: 280, height: 220 }); });

test('real DTO presentation has compact and expanded layouts, exact pricing and unknown account fields', async ({ page }) => {
  await page.goto('/?window=mini');
  await expect(page.getByLabel('可信 Token 分解')).toHaveText('683.1K');
  await expect(page.getByLabel('费用估算详情')).toContainText('$0.57');
  await expect(page.getByLabel('账户额度未连接')).toContainText('短周期剩余 —');
  await expect(page.getByLabel('账户额度未连接')).toContainText('周重置 —');
  await expect(page.locator('.mini-meta')).toContainText('输入缓存 70%');
  expect(await page.evaluate(() => ({ width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight }))).toEqual({ width: 280, height: 220 });
  await page.screenshot({ path: 'test-results/mini-compact-dark.png' });
  await page.getByLabel('展开小窗').click(); await expect(page.getByLabel('收起小窗')).toBeVisible();
  await page.setViewportSize({ width: 360, height: 380 });
  await expect(page.locator('.mini-breakdown')).toContainText('180.0K');
  await expect(page.locator('.mini-breakdown')).toContainText('输出（含推理）83.1K');
  await page.screenshot({ path: 'test-results/mini-expanded-dark.png' });
  await page.getByLabel('费用估算详情').click();
  await expect(page.getByLabel('小窗价格覆盖')).toContainText('USD 0.573123');
  await expect(page.getByLabel('小窗价格覆盖')).toContainText('未计价 83.1K');
  await page.getByLabel('打开小窗范围统计').click();
  expect(await page.evaluate(() => (window as unknown as QA).__miniQA.calls().filter(v => v.command === 'open_mini_stats').at(-1)?.request)).toEqual({ expected_settings_revision: '9007199254740993' });
  await page.getByLabel('小窗置顶').click(); await expect(page.getByLabel('小窗置顶')).toHaveAttribute('aria-pressed', 'true');
  await page.getByLabel('隐藏小窗').click();
  const actions = await page.evaluate(() => (window as unknown as QA).__miniQA.calls().filter(v => v.command === 'mini_window_action').map(v => v.request));
  expect(actions).toContainEqual({ kind: 'set_expanded', expanded: true }); expect(actions).toContainEqual({ kind: 'set_pinned', pinned: true }); expect(actions).toContainEqual({ kind: 'hide' });
});

test('shared theme and privacy clear retained names, money details and reject a late old response', async ({ page }) => {
  await page.goto('/?window=mini'); await expect(page.locator('.mini-scope')).toContainText('SYNTHETIC PRIVATE SESSION');
  await page.getByLabel('费用估算详情').click(); await page.setViewportSize({ width: 360, height: 380 });
  await page.evaluate(() => { const qa = (window as unknown as QA).__miniQA; qa.theme('light'); });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.screenshot({ path: 'test-results/mini-expanded-light.png' });
  await page.evaluate(() => (window as unknown as QA).__miniQA.hold()); await page.getByLabel('刷新小窗').click();
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__miniQA.calls().filter(v => v.command === 'get_mini_usage').length)).toBeGreaterThan(2);
  await page.evaluate(() => (window as unknown as QA).__miniQA.privacy(true));
  await expect(page.getByLabel('费用估算详情')).toContainText('已隐藏');
  await page.evaluate(() => (window as unknown as QA).__miniQA.release());
  await expect(page.locator('.mini-scope')).toHaveAttribute('title', '固定会话（已隐藏）');
  await expect(page.getByLabel('小窗价格覆盖')).toHaveCount(0);
  expect(await page.locator('body').innerHTML()).not.toContain('SYNTHETIC PRIVATE SESSION');
  expect(await page.locator('body').innerHTML()).not.toContain('0.573123');
  await page.getByLabel('小窗隐私模式').click();
  await expect(page.getByLabel('费用估算详情')).toContainText('$0.57');
  await expect(page.locator('.mini-scope')).toContainText('SYNTHETIC PRIVATE SESSION');
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__miniQA.listeners().sort())).toEqual(['display_policy_changed', 'price_rules_changed', 'settings_changed']);
});

test('failed refresh keeps an explicitly old success, scope reset carries precise revision and never queries main data', async ({ page }) => {
  await page.goto('/?window=mini'); await expect(page.getByLabel('可信 Token 分解')).toHaveText('683.1K');
  await page.evaluate(() => (window as unknown as QA).__miniQA.fail(true)); await page.getByLabel('刷新小窗').click();
  await expect(page.getByRole('alert')).toContainText('保留上次快照'); await expect(page.getByLabel('可信 Token 分解')).toHaveText('683.1K');
  await expect(page.getByLabel('刷新小窗')).toHaveAttribute('title', /最近成功统计快照.*不表示最后消费时间/);
  await page.evaluate(() => (window as unknown as QA).__miniQA.fail(false));
  await page.getByLabel('展开小窗').click(); await page.setViewportSize({ width: 360, height: 380 });
  await page.getByRole('button', { name: '返回今日全部' }).click(); await expect(page.locator('.mini-scope')).toContainText('全部来源 · 今日');
  const calls = await page.evaluate(() => (window as unknown as QA).__miniQA.calls());
  expect(calls.filter(c => c.command === 'set_mini_scope').at(-1)?.request).toEqual({ mini_scope: { kind: 'today_all_sources' }, expected_settings_revision: '9007199254740993' });
  expect(calls.some(c => ['get_app_status', 'get_sources', 'get_dashboard_bundle', 'query_sessions', 'get_price_rules'].includes(c.command))).toBe(false);
  expect(await page.evaluate(() => ({ width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight }))).toEqual({ width: 360, height: 380 });
});

test('ordinary browser preview exposes no invented usage or account percentages', async ({ page }) => {
  // Remove the test bridge on the next navigation: this route is also usable for honest visual previews.
  await page.addInitScript(() => { Object.assign(window, { isTauri: false }); delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__; });
  await page.goto('/?window=mini'); await expect(page.getByRole('alert')).toContainText('读取失败');
  await expect(page.getByLabel('可信 Token 分解')).toHaveText('—');
  await expect(page.getByLabel('费用估算详情')).toContainText('已隐藏');
  await expect(page.getByLabel('账户额度未连接')).toContainText('账户未连接');
});


test('unknown evidence stays unknown, confirmed zero and unpriced consumption remain distinct', async ({ page }) => {
  await page.goto('/?window=mini'); await expect(page.getByLabel('可信 Token 分解')).toHaveText('683.1K');
  await page.evaluate(() => (window as unknown as QA).__miniQA.mode('unknown'));
  await expect(page.getByLabel('可信 Token 分解')).toHaveText('—'); await expect(page.getByLabel('费用估算详情')).toContainText('—');
  await expect(page.getByRole('status')).toHaveText('尚无已确认用量');
  await page.evaluate(() => (window as unknown as QA).__miniQA.mode('zero'));
  await expect(page.getByLabel('可信 Token 分解')).toHaveText('0'); await expect(page.getByLabel('费用估算详情')).toContainText('$0.00');
  await page.evaluate(() => (window as unknown as QA).__miniQA.mode('unpriced'));
  await expect(page.getByLabel('可信 Token 分解')).toHaveText('683.1K'); await expect(page.getByLabel('费用估算详情')).toContainText('未计价');
  await expect(page.locator('.mini-meta')).toContainText('输入缓存 —');
});

async function editor(page: Page) {
  await page.goto('/?window=mini'); await expect(page.getByLabel('可信 Token 分解')).toHaveText('683.1K');
  await page.getByLabel('展开小窗').click(); await page.setViewportSize({ width: 360, height: 380 });
  await page.getByLabel('选择小窗会话与起点').click();
  await expect(page.getByRole('dialog', { name: '小窗统计范围' })).toBeVisible();
}
test('session picker uses stable pages and explicit millisecond start, saving no main filter or quota', async ({ page }) => {
  await editor(page);
  await expect(page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option')).toHaveCount(25);
  await page.getByRole('button', { name: '加载更多会话' }).click(); await expect(page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option')).toHaveCount(50);
  await page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option').nth(30).click();
  await page.getByLabel('小窗消耗起点').selectOption('fixed');
  await page.getByLabel('小窗固定起点（UTC）').fill('2024-02-29T01:02:03.123');
  await page.screenshot({ path: 'test-results/mini-scope-editor.png' });
  await page.getByRole('button', { name: '应用小窗范围' }).click(); await expect(page.getByRole('dialog')).toHaveCount(0);
  const call = await page.evaluate(() => (window as unknown as QA).__miniQA.calls().filter(c => c.command === 'set_mini_scope').at(-1));
  expect(call?.request).toEqual({ mini_scope: { kind: 'session', session_key: 'synthetic-candidate-30', start: { kind: 'fixed', start_ms: Date.parse('2024-02-29T01:02:03.123Z') } }, expected_settings_revision: '9007199254740993' });
  await expect(page.getByLabel('账户额度未连接')).toContainText('周重置 —');
  const close = await page.evaluate(() => (window as unknown as QA).__miniQA.calls().filter(c => c.command === 'close_query_snapshot').at(-1)?.request);
  expect(close).toMatchObject({ kind: 'mini_sessions', request: { query: { search: '', page_size: 25 } } });
  await page.getByLabel('选择小窗会话与起点').click(); await page.getByLabel('小窗消耗起点').selectOption('today');
  await page.getByRole('button', { name: '应用小窗范围' }).click(); await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as QA).__miniQA.calls().filter(c => c.command === 'set_mini_scope').at(-1)?.request)).toMatchObject({ mini_scope: { start: { kind: 'today' } } });
});

test('expiry does not mix snapshots; search and cancellation release original cursors and restore focus', async ({ page }) => {
  await editor(page); await expect(page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option')).toHaveCount(25);
  await page.evaluate(() => (window as unknown as QA).__miniQA.expireCandidates(true));
  await page.getByRole('button', { name: '加载更多会话' }).click();
  await expect(page.getByRole('alert')).toContainText('快照已过期'); await expect(page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option')).toHaveCount(25);
  await page.evaluate(() => (window as unknown as QA).__miniQA.expireCandidates(false));
  await page.getByLabel('搜索小窗会话').fill('CANDIDATE 54'); await expect(page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option')).toHaveCount(1);
  await page.getByLabel('搜索小窗会话').fill(''); await expect(page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option')).toHaveCount(25);
  await page.getByRole('button', { name: '取消', exact: true }).click(); await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByLabel('选择小窗会话与起点')).toBeFocused();
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__miniQA.candidateLeases())).toBe(0);
  await page.getByLabel('选择小窗会话与起点').click(); await expect(page.getByLabel('关闭小窗范围选择')).toBeFocused();
  await page.keyboard.press('Shift+Tab'); await expect(page.getByRole('button', { name: '取消', exact: true })).toBeFocused();
  await page.keyboard.press('Tab'); await expect(page.getByLabel('关闭小窗范围选择')).toBeFocused();
  await page.keyboard.press('Escape'); await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByLabel('选择小窗会话与起点')).toBeFocused();
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__miniQA.candidateLeases())).toBe(0);
});

test('scope conflict preserves selection and fixed draft, future start issues no mutation', async ({ page }) => {
  await editor(page); await expect(page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option')).toHaveCount(25); await page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option').first().click();
  await page.getByLabel('小窗消耗起点').selectOption('fixed'); await page.getByLabel('小窗固定起点（UTC）').fill('2099-01-01T00:00');
  await page.getByRole('button', { name: '应用小窗范围' }).click(); await expect(page.getByRole('alert')).toContainText('不晚于当前时刻');
  expect(await page.evaluate(() => (window as unknown as QA).__miniQA.calls().filter(c => c.command === 'set_mini_scope').length)).toBe(0);
  await page.getByLabel('小窗固定起点（UTC）').fill('2024-02-29T01:02:03.456');
  await page.evaluate(() => (window as unknown as QA).__miniQA.theme('light'));
  await page.getByRole('button', { name: '应用小窗范围' }).click(); await expect(page.getByRole('alert')).toContainText('已发生变化');
  await expect(page.getByRole('alert')).toBeInViewport();
  await expect(page.getByLabel('小窗固定起点（UTC）')).toHaveValue('2024-02-29T01:02:03.456');
  await expect(page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option').first()).toHaveAttribute('aria-selected', 'true');
  await page.screenshot({ path: 'test-results/mini-scope-editor-light-conflict.png' });
  expect(await page.evaluate(() => (window as unknown as QA).__miniQA.calls().filter(c => c.command === 'set_mini_scope').at(-1)?.request)).toMatchObject({ expected_settings_revision: '9007199254740993' });
});

test('external privacy closes unsaved picker and a late unmasked page closes its exact original capability', async ({ page }) => {
  await page.goto('/?window=mini'); await expect(page.getByLabel('可信 Token 分解')).toHaveText('683.1K');
  await page.getByLabel('展开小窗').click(); await page.setViewportSize({ width: 360, height: 380 });
  await page.evaluate(() => (window as unknown as QA).__miniQA.holdCandidates());
  await page.getByLabel('选择小窗会话与起点').click(); await expect(page.getByRole('dialog')).toBeVisible();
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__miniQA.calls().filter(c => c.command === 'query_mini_sessions').length)).toBeGreaterThan(0);
  await page.evaluate(() => (window as unknown as QA).__miniQA.privacy(true)); await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.evaluate(() => (window as unknown as QA).__miniQA.releaseCandidates());
  await expect.poll(() => page.evaluate(() => (window as unknown as QA).__miniQA.calls().filter(c => c.command === 'close_query_snapshot').length)).toBeGreaterThan(0);
  expect(await page.locator('body').innerHTML()).not.toContain('SYNTHETIC PRIVATE CANDIDATE');
  await page.getByLabel('选择小窗会话与起点').click(); await expect(page.getByRole('listbox', { name: '小窗会话候选' }).getByRole('option')).toHaveCount(25);
  expect(await page.getByRole('dialog').innerHTML()).not.toContain('SYNTHETIC PRIVATE CANDIDATE');
});
