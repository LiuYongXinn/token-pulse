import { expect, test } from 'vitest';
import { getAppStatus, notifyIssueText, runtimeError } from './runtime';

test('browser preview cannot supply a fabricated desktop status', async () => {
  await expect(getAppStatus()).rejects.toThrow('浏览器预览不提供本地采集与统计');
});

test('notify failures explain action without echoing unknown diagnostics', () => {
  expect(runtimeError({ code: 'NOTIFY_INTEGRATION_FAILED', details: { notify_issue: 'config_changed' } })).toContain('重新预览');
  expect(notifyIssueText('transaction_unavailable')).toContain('日志采集继续可用');
  expect(runtimeError({ code: 'NOTIFY_INTEGRATION_FAILED', details: { notify_issue: 'synthetic private command' } })).not.toContain('synthetic');
});

test('update failures explain retry without exposing raw provider details', () => {
  expect(runtimeError({ code: 'UPDATE_UNAVAILABLE', details: { raw: 'private-provider-text' } })).toContain('正式安装版');
  expect(runtimeError({ code: 'UPDATE_BUSY' })).toContain('等待完成');
  expect(runtimeError({ code: 'UPDATE_FAILED', details: { raw: 'private-provider-text' } })).not.toContain('private-provider-text');
});
