const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
const names = ['模型', '项目', '会话', '明细', '总览'];
const nav = name => [...document.querySelectorAll('nav[aria-label="主导航"] button')].find(button => button.textContent.trim() === name);
const refresh = [...document.querySelectorAll('button')].find(button => button.textContent.trim() === '刷新');
refresh.click();
// Real statistic IPC calls are held by the debug native gate, while status/mini/detail stay free.
for (let n = 0; n < 100; ++n) {
  nav(names[n % names.length]).click(); await frame();
  if (!document.querySelector('.total-number[aria-label], .group-total[aria-label]')) throw new Error('BLOCKED_REFRESH_CLEARED_CONTENT');
}
