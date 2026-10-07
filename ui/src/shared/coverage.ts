import type { Coverage } from './generated/contracts';
import { fullTokens } from './format';

const sourceProblems = new Set(['source_unreadable', 'source_partially_readable', 'source_scan_incomplete']);
const scanning = new Set(['source_scanning', 'source_scan_pending', 'source_scan_verifying', 'source_scan_changed']);

export function coverageStatus(coverage: Coverage): string {
  if (coverage.pending_observation_count !== '0' || coverage.unattributed_observation_count !== '0' || coverage.format_issues.length > 0)
    return '已统计可信用量，部分记录待核对';
  if (coverage.source_issues.some(issue => sourceProblems.has(issue.code))) return '已统计可信用量，部分来源需检查';
  if (coverage.pending_file_count !== '0') return '正在补采日志';
  if ((coverage.verifying_file_count && coverage.verifying_file_count !== '0') || coverage.source_issues.some(issue => scanning.has(issue.code)))
    return '正在校验来源覆盖';
  if (coverage.source_issues.some(issue => issue.code === 'source_paused')) return '部分来源已暂停，已统计用量保留';
  return coverage.state === 'complete' ? '已配置来源覆盖完整' : '来源覆盖尚待核对';
}

/** Counts describe different work; a zero is not an unresolved item. */
export function coverageSummary(coverage: Coverage): string {
  const parts = [coverageStatus(coverage)];
  for (const [label, value] of [
    ['待核对用量记录', coverage.pending_observation_count],
    ['时间未归属记录', coverage.unattributed_observation_count],
    ['待采集文件', coverage.pending_file_count],
    ['待校验文件', coverage.verifying_file_count],
  ]) if (value && value !== '0') parts.push(`${label} ${fullTokens(value)}`);
  const formats = coverage.format_issues.reduce((n, issue) => n + BigInt(issue.count), 0n);
  if (formats > 0n) parts.push(`日志格式提示 ${fullTokens(formats.toString())}`);
  const problems = coverage.source_issues.filter(issue => sourceProblems.has(issue.code)).length;
  if (problems) parts.push(`来源需检查 ${problems}`);
  return parts.join(' · ');
}
