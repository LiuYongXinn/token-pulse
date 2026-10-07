// Design review captures of real React components with existing, explicitly synthetic test DTOs.
// This script is a development tool; no test fixture is imported into the production bundle.
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import ts from 'typescript';
import { chromium, expect } from '@playwright/test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.join(root, 'docs/images');
const base = process.env.SILVER_MIST_URL ?? 'http://127.0.0.1:1421';
await mkdir(output, { recursive: true });
async function fixture(name) {
  const filename = path.join(root, `tests/ui/${name}.ts`);
  const source = ts.createSourceFile(filename, await readFile(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  let callback;
  const visit = node => {
    if (!callback && ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'addInitScript') callback = node.arguments[0];
    ts.forEachChild(node, visit);
  };
  visit(source);
  if (!callback || !ts.isArrowFunction(callback)) throw new Error(`Missing self-contained browser fixture: ${name}`);
  return ts.transpileModule(`(${callback.getText(source)})();`, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } }).outputText;
}
const calendar = await fixture('calendar-bridge');
const browser = await chromium.launch();
const captures = [];
const errors = [];
async function screen(fixtureName, nav, tab, theme = 'light', viewport = { width: 1440, height: 1000 }) {
  const page = await browser.newPage({ viewport, deviceScaleFactor: 1 });
  page.on('pageerror', e => errors.push(`${fixtureName}: ${e.message}`));
  const setup = `${calendar}\n${await fixture(fixtureName)}\n(${function (theme) {
    const native = window.__TAURI_INTERNALS__;
    const invoke = native.invoke;
    native.invoke = async (command, args) => {
      let response;
      try { response = await invoke(command, args); }
      catch (error) {
        // Only these supplementary read-only states are absent from some existing fixtures.
        const extras = {
          get_notify_integrations: { ready: false, listener_count: 0, service_issue: null, registrations: [], registry_issue: null, redacted: false },
          get_account_service_config: { settings_revision: '1', executable_display_path: null, home_display_path: null, executable_sha256: null, configured: false, auto_connect: false },
          get_main_navigation: null,
        };
        if (!(command in extras)) throw error;
        response = { api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data: extras[command] };
      }
      if (command === 'get_display_settings' && response?.data?.preferences) response.data.preferences.theme = theme;
      return response;
    };
  }.toString()})(${JSON.stringify(theme)});`;
  await page.addInitScript({ content: setup });
  await page.goto(`${base}/${nav === 'mini' ? '?window=mini' : ''}`);
  await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
  if (nav !== 'mini' && nav !== '总览') await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: nav, exact: true }).click();
  if (tab) await page.getByRole('tab', { name: tab, exact: true }).click();
  return page;
}
async function capture(page, id, { fullPage = false, target } = {}) {
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.evaluate(() => document.fonts.ready);
  await page.locator('[aria-busy=true]').waitFor({ state: 'hidden', timeout: 5000 }).catch(() => {});
  // Wait a frame after layout; no screenshot contains loading fixture placeholders.
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const size = await page.evaluate(() => ({ width: innerWidth, height: innerHeight, documentWidth: document.documentElement.scrollWidth }));
  if (size.documentWidth > size.width) throw new Error(`${id} overflows viewport: ${JSON.stringify(size)}`);
  await (target ? page.locator(target) : page).screenshot({ path: path.join(output, `silver-mist-${id}.png`), fullPage: target ? undefined : fullPage, animations: 'disabled' });
  captures.push({ id, ...size });
  console.log(`Captured ${id}`);
}
try {
  let page = await screen('overview.spec', '总览');
  await expect(page.locator('.total-number')).toBeVisible();
  const now = Date.now();
  await page.evaluate(now => window.__setSyntheticQuota({ connection_epoch: 'synthetic-design-review', quota_revision: '1', state: 'ready', selected_limit_id: 'codex', available_limits: [{ limit_id: 'codex', display_name: '演示账户' }], fetched_at_ms: now, last_attempt_at_ms: now, error_code: null, windows: [{ window_id: 'primary', duration_mins: 300, used_percent: 28, remaining_percent: 72, resets_at_ms: now + 7200000 }, { window_id: 'secondary', duration_mins: 10080, used_percent: 46, remaining_percent: 54, resets_at_ms: now + 259200000 }] }), now);
  await expect(page.getByRole('region', { name: '账户额度总览' })).toContainText('72%');
  await capture(page, 'overview', { fullPage: true });
  await page.setViewportSize({ width: 960, height: 680 }); await capture(page, 'overview-960', { fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 }); await capture(page, 'overview-390', { fullPage: true });
  await page.close();
  page = await screen('overview.spec', '总览', null, 'dark'); await expect(page.locator('.total-number')).toBeVisible(); await capture(page, 'overview-dark', { fullPage: true }); await page.close();
  for (const [nav, id] of [['模型', 'models'], ['项目', 'projects']]) {
    page = await screen('groups.spec', nav); await expect(page.locator('.group-stat-strip')).toBeVisible(); await capture(page, id); await page.close();
  }
  page = await screen('sessions.spec', '会话'); await expect(page.locator('.session-table')).toBeVisible(); await capture(page, 'sessions');
  await page.getByRole('button', { name: 'Synthetic 会话 0', exact: true }).click(); await expect(page.getByRole('dialog')).toBeVisible(); await capture(page, 'session-detail'); await page.close();
  page = await screen('events.spec', '明细'); await expect(page.locator('.event-table')).toBeVisible(); await capture(page, 'events');
  await page.getByRole('button', { name: '查看 synthetic-event-0 核算依据', exact: true }).click(); await expect(page.locator('.event-evidence')).toBeVisible(); await capture(page, 'event-evidence'); await page.close();
  page = await screen('diagnostics.spec', '采集诊断'); await expect(page.getByRole('heading', { name: '运行状态' })).toBeVisible(); await capture(page, 'diagnostics', { fullPage: true }); await page.close();
  for (const [name, tab, id, wait] of [
    ['sources.spec', '数据来源', 'sources', '.source-card'],
    ['display-settings.spec', '显示与窗口', 'display', '.theme-preview-options'],
    ['taskbar.spec', '任务栏显示', 'taskbar', '.taskbar-layout-preview'],
    ['prices.spec', '价格规则', 'prices', '.price-version'],
    ['updates.spec', '软件更新', 'updates', '[role="tabpanel"] [role="status"]'],
  ]) {
    page = await screen(name, '设置', tab); await expect(page.locator(wait).first()).toBeVisible(); await capture(page, id, { fullPage: true });
    if (id === 'prices') {
      await page.getByRole('button', { name: '新增规则', exact: true }).click(); await expect(page.locator('.price-editor')).toBeVisible();
      await capture(page, 'price-editor');
    }
    await page.close();
  }
  page = await screen('display-settings.spec', '设置', '显示与窗口', 'dark'); await expect(page.locator('.theme-preview-options')).toBeVisible(); await capture(page, 'display-dark', { fullPage: true }); await page.close();
  page = await screen('mini.spec', 'mini', null, 'light', { width: 280, height: 220 });
  await expect(page.locator('.mini-tokens')).toBeVisible(); await capture(page, 'mini-compact');
  await page.getByRole('button', { name: '展开小窗', exact: true }).click(); await page.setViewportSize({ width: 360, height: 380 }); await expect(page.locator('.mini-window.expanded')).toBeVisible(); await capture(page, 'mini-expanded');
  await page.getByRole('button', { name: '选择小窗会话与起点', exact: true }).click(); await capture(page, 'mini-scope'); await page.close();
  page = await screen('mini.spec', 'mini', null, 'dark', { width: 280, height: 220 }); await expect(page.locator('.mini-tokens')).toBeVisible(); await capture(page, 'mini-dark'); await page.close();
  page = await browser.newPage({ viewport: { width: 1280, height: 860 } }); await page.goto(base); await expect(page.getByRole('heading', { name: '开始记录本地用量' })).toBeVisible(); await capture(page, 'empty'); await page.close();
  if (errors.length) throw new Error(errors.join('\n'));
  await writeFile(path.join(root, 'prototypes/silver-mist-capture-manifest.json'), JSON.stringify({ synthetic: true, captures }, null, 2) + '\n');
} finally { await browser.close(); }
