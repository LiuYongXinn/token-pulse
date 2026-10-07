import { expect, test } from 'vitest';
import type { Coverage } from './generated/contracts';
import { coverageStatus, coverageSummary } from './coverage';

const complete: Coverage = { state: 'complete', pending_observation_count: '0', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '0', verifying_file_count: '0', source_issues: [], format_issues: [], breakdown_complete: true };

test('routine verification describes progress and hides zero pending counts', () => {
  const coverage: Coverage = { ...complete, state: 'unknown', verifying_file_count: '89', source_issues: [{ source_id: 'local', code: 'source_scan_verifying', last_success_ms: 1 }] };
  expect(coverageSummary(coverage)).toBe('正在校验来源覆盖 · 待校验文件 89');
  expect(coverageSummary(complete)).toBe('已配置来源覆盖完整');
});

test('pending records, unread files and format issues remain explicit', () => {
  expect(coverageSummary({ ...complete, state: 'partial', pending_observation_count: '15' })).toBe('已统计可信用量，部分记录待核对 · 待核对用量记录 15');
  expect(coverageSummary({ ...complete, state: 'partial', pending_file_count: '88' })).toBe('正在补采日志 · 待采集文件 88');
  expect(coverageSummary({ ...complete, state: 'partial', format_issues: [{ format: 'test', count: '9007199254740993' }, { format: 'other', count: '2' }] })).toContain('9,007,199,254,740,995');
});

test('unreadable sources and old snapshots cannot claim complete coverage', () => {
  expect(coverageStatus({ ...complete, state: 'partial', source_issues: [{ source_id: 'local', code: 'source_unreadable', last_success_ms: null }] })).toBe('已统计可信用量，部分来源需检查');
  expect(coverageStatus({ ...complete, state: 'unknown', verifying_file_count: undefined })).toBe('来源覆盖尚待核对');
});
