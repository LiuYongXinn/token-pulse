import { expect, test } from 'vitest';
import { getAppStatus } from './runtime';

test('browser preview cannot supply a fabricated desktop status', async () => {
  await expect(getAppStatus()).rejects.toThrow('浏览器预览不提供本地采集与统计');
});
