import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { MiniUsageSnapshot, MiniScopeMutation, MiniScopeSnapshot, MiniWindowAction, MiniWindowState, MiniStatsRequest, MiniStatsOpenRequest, MiniSessionsRequest, MiniSessionsPage } from './generated/contracts';
import { displayPolicy } from './display-policy';
import type { DisplayPolicyStamp, DisplayPrivacyMutation, DisplayThemeMutation } from './generated/contracts';
import type { CloseQuerySnapshotRequest, FilterOptionsRequest, FilterOptionsPage } from './generated/contracts';

import type { AppStatus, Response, WindowAction, SourcesSnapshot, SourceDirectorySelection, SourceDirectoryKind, ManageSourceAction, Job, JobRequest, CancelJobResult, ContextSnapshot, PriceRuleMutation, PriceRulesSnapshot, DashboardRequest, DashboardBundle, GroupedUsageRequest, GroupedUsageBundle } from './generated/contracts';
import type { PriceChanged } from './generated/contracts';
import type { SessionsPage, SessionsRequest } from './generated/contracts';
import type { SessionBundle, SessionBundleRequest } from './generated/contracts';
import type { TurnsPage, TurnsRequest } from './generated/contracts';
import type { CalendarSelectionRequest, CalendarSelectionResult } from './generated/contracts';
import type { DisplaySettingsSnapshot, TimezoneMutation, SettingsChanged } from './generated/contracts';
import type { UsageEventsPage, UsageEventsRequest } from './generated/contracts';
export type { AppStatus } from './generated/contracts';

const plainCommands = new Set(['cancel_account_service_selection', 'get_mini_passthrough', 'set_mini_passthrough', 'get_mini_opacity', 'set_mini_opacity', 'get_recovery_shortcut', 'set_recovery_shortcut', 'resolve_calendar_selection', 'perform_window_action', 'mini_window_action', 'open_mini_stats', 'get_mini_stats_request']);
const controlCommands = new Set(['get_display_settings', 'set_display_timezone', 'set_display_theme', 'set_display_privacy', 'close_query_snapshot']);
const pageKinds: Record<string, CloseQuerySnapshotRequest['kind']> = { query_mini_sessions: 'mini_sessions', get_filter_options: 'filter_options', query_sessions: 'sessions', query_usage_events: 'usage_events', query_turns: 'turns' };
async function releaseRejectedPage(command: string, args: Record<string, unknown>, data: unknown) {
  if (command === 'choose_account_service' && typeof data === 'object' && data !== null && 'selection_handle' in data && typeof data.selection_handle === 'string') {
    await invoke('cancel_account_service_selection', { requestId: crypto.randomUUID(), selectionHandle: data.selection_handle }).catch(() => {});
  }
  const kind = pageKinds[command];
  if (!kind || typeof data !== 'object' || data === null || !('next_cursor' in data) || typeof data.next_cursor !== 'string') return;
  // Return only the original request and authenticated cursor. A snapshot ID is never a close capability.
  const original = args.request;
  if (typeof original !== 'object' || original === null) return;
  await invoke('close_query_snapshot', { requestId: crypto.randomUUID(), request: { kind, request: { ...original, cursor: data.next_cursor } } }).catch(() => {});
}
async function request<T>(command: string, args: Record<string, unknown> = {}, explicitDisable = false): Promise<T> {
  if (!isTauri()) throw new Error('请通过桌面应用打开。浏览器预览不提供本地采集与统计。');
  const epoch = displayPolicy.get().epoch;
  const requestId = crypto.randomUUID();
  const response = await invoke<Response<T>>(command, { ...args, requestId });
  if (response.api_version !== 1 || response.request_id !== requestId) throw new Error('桌面协议版本或响应身份不匹配。');
  if (!plainCommands.has(command)) {
    const stamp = response.display_policy;
    let accepted = false;
    try { accepted = stamp ? displayPolicy.accept(stamp, explicitDisable) : false; }
    catch (error) {
      displayPolicy.enable(); displayPolicy.failed('无法确认响应中的显示隐私策略。');
      await releaseRejectedPage(command, args, response.data);
      throw error;
    }
    if (command === 'set_display_privacy' && !accepted) throw new Error('显示隐私策略已变化，请刷新设置后重试。');
    if (!controlCommands.has(command) && (!accepted || !stamp || stamp.privacy !== displayPolicy.get().privacy || epoch !== displayPolicy.get().epoch)) {
      await releaseRejectedPage(command, args, response.data);
      throw new Error('显示隐私策略已变化，已丢弃旧响应，请重新读取。');
    }
    if (!stamp) throw new Error('桌面响应缺少显示隐私策略，已停止显示。');
  }
  return response.data;
}
export async function setDisplayPrivacy(mutation: DisplayPrivacyMutation): Promise<DisplaySettingsSnapshot> {
  if (mutation.privacy) displayPolicy.enable();
  try { return await request('set_display_privacy', { request: mutation }, !mutation.privacy); }
  catch (error) { if (mutation.privacy) displayPolicy.failed(runtimeError(error)); throw error; }
}
export async function onDisplayPolicyChanged(): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen<DisplayPolicyStamp>('display_policy_changed', event => { try { displayPolicy.accept(event.payload); } catch { displayPolicy.enable(); displayPolicy.failed('无法确认显示隐私策略，已隐藏敏感信息。'); } });
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}
export function getAppStatus(): Promise<AppStatus> { return request('get_app_status'); }
export function getDisplaySettings(): Promise<DisplaySettingsSnapshot> { return request('get_display_settings'); }
export function getMiniOpacity(): Promise<import('./generated/contracts').MiniOpacitySnapshot> { return request('get_mini_opacity'); }
export function getMiniPassthrough(): Promise<import('./generated/contracts').MiniPassthroughSnapshot> { return request('get_mini_passthrough'); }
export function setMiniPassthrough(mutation: import('./generated/contracts').MiniPassthroughMutation): Promise<import('./generated/contracts').MiniPassthroughSnapshot> { return request('set_mini_passthrough', { request: mutation }); }
export async function onMiniInteractionChanged(refresh: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen('mini_interaction_changed', refresh);
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}
export function setMiniOpacity(mutation: import('./generated/contracts').MiniOpacityMutation): Promise<import('./generated/contracts').MiniOpacitySnapshot> { return request('set_mini_opacity', { request: mutation }); }
export function getRecoveryShortcut(): Promise<import('./generated/contracts').RecoveryShortcutSnapshot> { return request('get_recovery_shortcut'); }
export function setRecoveryShortcut(mutation: import('./generated/contracts').RecoveryShortcutMutation): Promise<import('./generated/contracts').RecoveryShortcutSnapshot> { return request('set_recovery_shortcut', { request: mutation }); }
export function setDisplayTheme(mutation: DisplayThemeMutation): Promise<DisplaySettingsSnapshot> { return request('set_display_theme', { request: mutation }); }
export function setDisplayTimezone(mutation: TimezoneMutation): Promise<DisplaySettingsSnapshot> { return request('set_display_timezone', { request: mutation }); }
export async function onSettingsChanged(refresh: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen<SettingsChanged>('settings_changed', refresh);
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}
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
export function queryTurns(query: TurnsRequest): Promise<TurnsPage> { return request('query_turns', { request: query }); }
export function resolveCalendarSelection(query: CalendarSelectionRequest): Promise<CalendarSelectionResult> { return request('resolve_calendar_selection', { request: query }); }
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
    if (code === 'DB_WRITE_FAILED') return '数据库写入失败，设置未保存，请重试。';
    const descriptions: Record<string, string> = { QUOTA_DISCONNECTED: '账户服务未连接，请先连接已保存服务。', QUOTA_UNSUPPORTED: '当前连接不提供账户额度，本地统计继续可用。', QUOTA_AUTH_REQUIRED: '所选 Codex Home 的本地登录态不可用，请选择已登录账户使用的 Home 后重新连接。', QUOTA_TIMEOUT: '账户服务响应超时，保留已知旧快照。', QUOTA_PROTOCOL_ERROR: '账户服务响应无法验证，请检查服务版本或重新连接。', QUOTA_SERVICE_UNAVAILABLE: '账户服务程序不可用，请检查已选择程序和 Home。', SHORTCUT_CONFLICT: '恢复快捷键已被其他应用占用，旧组合保持生效，请更换组合。', SHORTCUT_UNAVAILABLE: '无法注册恢复快捷键，托盘恢复入口继续可用。', UNSUPPORTED_SETTINGS_VERSION: '配置版本高于或不同于当前应用支持的版本，已有配置已保留。', REVISION_CONFLICT: '配置或作业状态已发生变化，请刷新后重试。', PRICE_RULE_CONFLICT: '同一范围和优先级的价格规则有效期重叠，请调整日期或优先级。', SOURCE_UNREADABLE: '无法读取所选来源，请检查目录和访问权限。', INVALID_QUERY: '请求参数或当前数据范围无效，请检查后重试。', STALE_CONFIRMATION: '选择已过期或程序已变化，请重新选择。', PERMISSION_DENIED: '该窗口或目录不在允许范围内。', CANDIDATE_OBSOLETE: '重建输入已发生变化，旧统计已保留，请核对来源后重试。', JOB_INTERRUPTED: '作业已中断，旧统计已保留，可重新提交。', JOB_CANCELLED: '作业已安全取消。' };
    return descriptions[code] ?? `操作失败（${code}），请查看采集诊断。`;
  }
  return '桌面服务未能完成操作，请重试。';
}

export async function windowAction(action: WindowAction): Promise<void> {
  await request<null>('perform_window_action', { action });
}

export function getMiniUsage(): Promise<MiniUsageSnapshot> { return request('get_mini_usage'); }
export function getMiniScope(): Promise<MiniScopeSnapshot> { return request('get_mini_scope'); }
export function setMiniScope(mutation: MiniScopeMutation): Promise<MiniScopeSnapshot> { return request('set_mini_scope', { request: mutation }); }
export function miniWindowAction(action: MiniWindowAction): Promise<MiniWindowState> { return request('mini_window_action', { request: action }); }

export function openMiniStats(open: MiniStatsOpenRequest): Promise<MiniStatsRequest> { return request('open_mini_stats', { request: open }); }
export function getMiniStatsRequest(): Promise<MiniStatsRequest | null> { return request('get_mini_stats_request'); }
export async function onMiniStatsRequested(refresh: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen('mini_stats_requested', refresh);
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}

export function queryMiniSessions(query: MiniSessionsRequest): Promise<MiniSessionsPage> { return request('query_mini_sessions', { request: query }); }

export function getAccountServiceConfig(): Promise<import('./generated/contracts').AccountServiceConfigSnapshot> { return request('get_account_service_config'); }
export function chooseAccountService(selection: import('./generated/contracts').AccountServiceSelectionRequest): Promise<import('./generated/contracts').AccountServiceSelection | null> { return request('choose_account_service', { request: selection }); }
export function cancelAccountServiceSelection(selectionHandle: string): Promise<void> { return request('cancel_account_service_selection', { selectionHandle }); }
export function saveAccountServiceConfig(mutation: import('./generated/contracts').AccountServiceConfigMutation): Promise<import('./generated/contracts').AccountServiceConfigSnapshot> { return request('save_account_service_config', { request: mutation }); }
export function manageAccountConnection(connection: import('./generated/contracts').AccountConnectionRequest): Promise<import('./generated/contracts').QuotaSnapshot> { return request('manage_account_connection', { request: connection }); }
export function getAccountQuota(): Promise<import('./generated/contracts').QuotaSnapshot> { return request('get_account_quota'); }
export function refreshAccountQuota(): Promise<import('./generated/contracts').QuotaRefreshResult> { return request('refresh_account_quota'); }
export async function onAccountQuotaChanged(refresh: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen<import('./generated/contracts').QuotaChanged>('account_quota_changed', refresh);
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}
