// Compare the native popup renderer with Chromium using the actual project CSS.
// Only synthetic text is drawn; no running application or account data is read.
import { chromium } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const directory = resolve('.local/tmp/taskbar-font-reference');
mkdirSync(directory, { recursive: true });
execFileSync('cargo', ['test', '-p', 'token-pulse-taskbar', 'project_font_visual_reference', '--lib', '--', '--nocapture'], {
  stdio: 'inherit', env: { ...process.env, TOKENPULSE_FONT_VISUAL_DIR: directory },
});
const native = readFileSync(resolve(directory, 'native-font-reference.bmp')).toString('base64');
const css = readFileSync('ui/src/shared/silver-mist.css', 'utf8');
const samples = ['TokenPulse · 用量详情', '统计范围 已计价部分估算 输入', '全部来源 · 今日 Asia/Shanghai', '263.6M Token $55.93 89%'];
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1024, height: 208 }, deviceScaleFactor: 1.5 });
  await page.setContent(`<style>${css}
    body{margin:0;background:#fff;color:#292929}
    .comparison{display:flex;gap:16px;padding:4px}
    .label{font-size:12px;line-height:28px;margin:0}
    .sample{position:relative;width:500px;height:160px;background:#fafcff}
    .sample>span{position:absolute;left:12px;font-size:12px;font-weight:400;line-height:normal}
    #sample3{font-family:var(--dataFont);font-variant-numeric:tabular-nums;letter-spacing:-.5px}
    img{display:block;width:500px;height:160px}
    </style><div class="comparison">
    <div><p class="label">弹窗原生渲染 · DirectWrite</p><img src="data:image/bmp;base64,${native}"></div>
    <div><p class="label">项目 CSS · Chromium</p><div class="sample">${samples.map((text, index) => `<span id="sample${index}" style="top:${8 + index * 36}px">${text}</span>`).join('')}</div></div>
    </div>`);
  await page.evaluate(async () => { await document.fonts.ready; await Promise.all([...document.images].map(image => image.decode())); });
  const metrics = await page.locator('.sample>span').evaluateAll(nodes => nodes.map(node => ({
    text: node.textContent, width: node.getBoundingClientRect().width * 1.5,
    height: node.getBoundingClientRect().height * 1.5,
    family: getComputedStyle(node).fontFamily, spacing: getComputedStyle(node).letterSpacing,
  })));
  const session = await page.context().newCDPSession(page);
  await session.send('DOM.enable');
  await session.send('CSS.enable');
  const { root } = await session.send('DOM.getDocument');
  for (let index = 0; index < samples.length; index += 1) {
    const { nodeId } = await session.send('DOM.querySelector', { nodeId: root.nodeId, selector: `#sample${index}` });
    metrics[index].platformFonts = (await session.send('CSS.getPlatformFontsForNode', { nodeId })).fonts;
  }
  writeFileSync(resolve(directory, 'browser-metrics.json'), JSON.stringify(metrics, null, 2));
  await page.screenshot({ path: resolve(directory, 'font-comparison.png') });
  console.log(JSON.stringify(metrics, null, 2));
  console.log(`Font comparison: ${resolve(directory, 'font-comparison.png')}`);
} finally {
  await browser.close();
}
