import { expect, test } from '@playwright/test';

test('preview does not invent usage and preserves the seven-page navigation', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('navigation', { name: '主导航' }).getByRole('button')).toHaveCount(7);
  await expect(page.getByRole('alert')).toContainText('浏览器预览不提供本地采集与统计');
  await expect(page.getByText('当前版本提供桌面运行壳')).toBeVisible();
  await page.getByRole('button', { name: '查看数据来源' }).click();
  await expect(page.getByRole('tab')).toHaveCount(5);
  await page.getByRole('tab', { name: '价格规则' }).click();
  await expect(page.getByRole('tabpanel')).toContainText('价格规则');
  await page.getByRole('button', { name: '采集诊断', exact: true }).click();
  await expect(page.getByRole('heading', { name: '运行状态' })).toBeVisible();
});
