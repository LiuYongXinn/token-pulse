import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { CloseQuerySnapshotRequest, FilterOptionsRequest, FilterOptionsPage } from './generated/contracts';

import type { AppStatus, Response, WindowAction, SourcesSnapshot, SourceDirectorySelection, SourceDirectoryKind, ManageSourceAction, Job, JobRequest, CancelJobResult, ContextSnapshot, PriceRuleMutation, PriceRulesSnapshot, DashboardRequest, DashboardBundle, GroupedUsageRequest, GroupedUsageBundle } from './generated/contracts';
import type { PriceChanged } from './generated/contracts';
import type { SessionsPage, SessionsRequest } from './generated/contracts';
import type { SessionBundle, SessionBundleRequest } from './generated/contracts';
import type { UsageEventsPage, UsageEventsRequest } from './generated/contracts';
export type { AppStatus } from './generated/contracts';

async function request<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!isTauri()) throw new Error('请通过桌面应用打开。浏览器预览不提供本地采集与统计。');
  const requestId = crypto.randomUUID();
  const response = await invoke<Response<T>>(command, { ...args, requestId });
  if (response.api_version !== 1 || response.request_id !== requestId) throw new Error('桌面协议版本或响应身份不匹配。');
  return response.data;
}
export function getAppStatus(): Promise<AppStatus> { return request('get_app_status'); }
/** An invalidation only; values always come from a new complete query response. */
export async function onPriceRulesChanged(refresh: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen<PriceChanged>('price_rules_changed', refresh);
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}
export function getSources(): Promise<SourcesSnapshot> { return request('get_sources'); }
export function chooseSourceDirectory(kind: SourceDirectoryKind): Promise<SourceDirectorySelection | null> { return request('choose_source_directory', { kind }); }
export function manageSource(action: ManageSourceAction, expectedSettingsRevision: string): Promise<SourcesSnapshot> { return request('manage_source', { action, expectedSettingsRevision }); }
export function listJobs(): Promise<Job[]> { return request('list_jobs', { limit: 50 }); }
export function startJob(jobRequest: JobRequest): Promise<Job> { return request('start_job', { request: jobRequest }); }
export function cancelJob(jobId: string): Promise<CancelJobResult> { return request('cancel_job', { jobId }); }
export function getContextSnapshot(sessionKey: string): Promise<ContextSnapshot> { return request('get_context_snapshot', { sessionKey }); }
export function getDashboardBundle(query: DashboardRequest): Promise<DashboardBundle> { return request('get_dashboard_bundle', { request: query }); }
export function getGroupedUsage(query: GroupedUsageRequest): Promise<GroupedUsageBundle> { return request('get_grouped_usage', { request: query }); }
export function getFilterOptions(query: FilterOptionsRequest): Promise<FilterOptionsPage> { return request('get_filter_options', { request: query }); }
export function querySessions(query: SessionsRequest): Promise<SessionsPage> { return request('query_sessions', { request: query }); }
export function getSessionBundle(query: SessionBundleRequest): Promise<SessionBundle> { return request('get_session_bundle', { request: query }); }
export function queryUsageEvents(query: UsageEventsRequest): Promise<UsageEventsPage> { return request('query_usage_events', { request: query }); }
export async function closeQuerySnapshot(query: CloseQuerySnapshotRequest): Promise<void> { await request<null>('close_query_snapshot', { request: query }); }
export function getPriceRules(revision: string | null = null): Promise<PriceRulesSnapshot> { return request('get_price_rules', { revision }); }
export function savePriceRule(priceRequest: Exclude<PriceRuleMutation, { kind: 'retire' }>, expectedPriceRevision: string): Promise<PriceRulesSnapshot> { return request('save_price_rule', { request: priceRequest, expectedPriceRevision }); }
export function retirePriceRule(ruleId: string, expectedPriceRevision: string): Promise<PriceRulesSnapshot> { return request('retire_price_rule', { ruleId, expectedPriceRevision }); }
export function runtimeError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'object' && error !== null && 'code' in error) {
    const code = String(error.code);
    if (code === 'SNAPSHOT_EXPIRED') return '查询快照已过期，请重新查询。';
    if (code === 'CURSOR_INVALID') return '分页条件或游标已失效，请重新查询。';
    const descriptions: Record<string, string> = { REVISION_CONFLICT: '配置或作业状态已发生变化，请刷新后重试。', PRICE_RULE_CONFLICT: '同一范围和优先级的价格规则有效期重叠，请调整日期或优先级。', SOURCE_UNREADABLE: '无法读取所选来源，请检查目录和访问权限。', INVALID_QUERY: '请求参数或当前数据范围无效，请检查后重试。', STALE_CONFIRMATION: '目录选择已过期，请重新选择。', PERMISSION_DENIED: '该窗口或目录不在允许范围内。', CANDIDATE_OBSOLETE: '重建输入已发生变化，旧统计已保留，请核对来源后重试。', JOB_INTERRUPTED: '作业已中断，旧统计已保留，可重新提交。', JOB_CANCELLED: '作业已安全取消。' };
    return descriptions[code] ?? `操作失败（${code}），请查看采集诊断。`;
  }
  return '桌面服务未能完成操作，请重试。';
}

export async function windowAction(action: WindowAction): Promise<void> {
  await request<null>('perform_window_action', { action });
}
