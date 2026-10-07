const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
const until = async check => { for (let n = 0; n < 6000; ++n) { if (check()) return; await new Promise(resolve => setTimeout(resolve, 15)); } throw new Error('NAVIGATION_TIMEOUT'); };
const nav = name => [...document.querySelectorAll('nav[aria-label="主导航"] button')].find(button => button.textContent.trim() === name);
const hasContent = () => !!document.querySelector('.total-number[aria-label], .group-total[aria-label]');
const structure = () => !!document.querySelector('.overview-summary, .group-stat-strip');
const names = ['模型', '项目', '会话', '明细', '总览'];
await until(() => nav('总览'));
const original = window.__TAURI_INTERNALS__.invoke;
const cold = [];
for (const name of names) {
  const started = performance.now(); nav(name).click(); await frame();
  if (!structure() || [...document.querySelectorAll('h2')].some(node => /正在读取/.test(node.textContent))) throw new Error('COLD_STRUCTURE_FAILED');
  cold.push({ milliseconds: performance.now() - started, contentReady: hasContent() });
}
// Verify real content for every page independently, including queries that were still queued.
const initial = [];
for (const name of names) { const started = performance.now(); nav(name).click(); await frame(); await until(hasContent); const elapsed = performance.now() - started; if (RESTART_PHASE && (elapsed > 2000 || !document.querySelector('main').innerText.includes('已恢复上次成功快照'))) throw new Error('RESTART_NOT_RESTORED_BEFORE_FRESH_QUERY'); initial.push(elapsed); }
const warm = [];
const presented = [];
for (let n = 0; n < 100; ++n) { const started = performance.now(); const name = names[n % names.length]; nav(name).click(); await frame(); if (!hasContent() || document.querySelector('h1').textContent !== name) throw new Error(`CACHE_FIRST_FRAME_FAILED_${n}`); warm.push(performance.now() - started); await frame(); presented.push(performance.now() - started); }

// Commit a real display-only change in the isolated copy. Tokens must remain unchanged.
nav('模型').click(); await frame(); await until(hasContent);
const before = document.querySelector('.group-stat-strip').dataset.snapshotId;
const tokensBefore = document.querySelector('.group-total[aria-label]').getAttribute('aria-label');
const sources = (await original('get_sources', { requestId: crypto.randomUUID() })).data;
const source = sources.sources.find(value => !value.removed);
let update = null;
if (source && !RESTART_PHASE) {
  const started = performance.now();
  await original('manage_source', { requestId: crypto.randomUUID(), action: { kind: 'retain_remove', source_id: source.source_id }, expectedSettingsRevision: sources.settings_revision });
  await until(() => document.querySelector('.group-stat-strip')?.dataset.snapshotId !== before && hasContent());
  await frame();
  update = { milliseconds: performance.now() - started, tokensUnchanged: tokensBefore === document.querySelector('.group-total[aria-label]').getAttribute('aria-label') };
  if (!update.tokensUnchanged) throw new Error('DISPLAY_UPDATE_CHANGED_TOKENS');
}
await original('plugin:event|emit', { event: REPORT_EVENT, payload: { cold, initial_content_ms: initial, cached_click_ms: warm, following_frame_ms: presented, update, stages: window.__tokenPulseQueryTimings() } });
