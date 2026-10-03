import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { MiniUsageSnapshot, MiniScopeMutation, MiniScopeSnapshot, MiniWindowAction, MiniWindowState, MiniStatsRequest, MiniStatsOpenRequest, MiniSessionsRequest, MiniSessionsPage } from './generated/contracts';
import { displayPolicy } from './display-policy';
import type { UpdateSnapshot, UpdateActionRequest } from './generated/contracts';
import type { NotifyIntegrationsSnapshot, NotifyPrepareAction, NotifyConfigPreview, NotifyApplyResult } from './generated/contracts';
import type { DisplayPolicyStamp, DisplayPrivacyMutation, DisplayThemeMutation } from './generated/contracts';
import type { CloseQuerySnapshotRequest, FilterOptionsRequest, FilterOptionsPage } from './generated/contracts';

import type { AppStatus, Response, WindowAction, SourcesSnapshot, SourceDirectorySelection, SourceDirectoryKind, ManageSourceAction, Job, JobRequest, CancelJobResult, ContextSnapshot, PriceRuleMutation, PriceRulesSnapshot, DashboardRequest, DashboardBundle, GroupedUsageRequest, GroupedUsageBundle } from './generated/contracts';
import type { PriceChanged } from './generated/contracts';
import type { ModelAliasMutation } from './generated/contracts';
import type { OfflinePriceCatalogSnapshot } from './generated/contracts';
import type { SessionsPage, SessionsRequest } from './generated/contracts';
import type { SessionBundle, SessionBundleRequest } from './generated/contracts';
import type { TurnsPage, TurnsRequest } from './generated/contracts';
import type { CalendarSelectionRequest, CalendarSelectionResult } from './generated/contracts';
import type { DisplaySettingsSnapshot, TimezoneMutation, SettingsChanged } from './generated/contracts';
import type { UsageEventsPage, UsageEventsRequest } from './generated/contracts';
export type { AppStatus } from './generated/contracts';

const plainCommands = new Set(['get_taskbar_preferences', 'set_taskbar_preferences', 'get_taskbar_status', 'retry_taskbar_embed', 'cancel_account_service_selection', 'get_mini_passthrough', 'set_mini_passthrough', 'get_mini_opacity', 'set_mini_opacity', 'get_recovery_shortcut', 'set_recovery_shortcut', 'resolve_calendar_selection', 'perform_window_action', 'mini_window_action', 'open_mini_stats', 'get_mini_stats_request', 'get_main_navigation']);
const controlCommands = new Set(['get_display_settings', 'set_display_timezone', 'set_display_theme', 'set_display_privacy', 'close_query_snapshot']);
const pageKinds: Record<string, CloseQuerySnapshotRequest['kind']> = { query_mini_sessions: 'mini_sessions', get_filter_options: 'filter_options', query_sessions: 'sessions', query_usage_events: 'usage_events', query_turns: 'turns' };
async function releaseRejectedPage(command: string, args: Record<string, unknown>, data: unknown) {
  if (command === 'prepare_notify_integration' && typeof data === 'object' && data !== null && 'plan_id' in data && typeof data.plan_id === 'string') {
    await invoke('release_notify_preview', { requestId: crypto.randomUUID(), planId: data.plan_id }).catch(() => {});
  }
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
export function getUpdateStatus(): Promise<UpdateSnapshot> { return request('get_update_status'); }
export function checkForUpdates(): Promise<UpdateSnapshot> { return request('check_for_updates'); }
export function downloadUpdate(action: UpdateActionRequest): Promise<UpdateSnapshot> { return request('download_update', { request: action }); }
export function installUpdate(action: UpdateActionRequest): Promise<UpdateSnapshot> { return request('install_update', { request: action }); }
export async function onUpdatesChanged(refresh: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen('updates_changed', refresh);
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}
// These DTOs contain display switches and runtime state only, never usage values or paths.
export function getTaskbarPreferences(): Promise<import('./generated/contracts').TaskbarPreferencesSnapshot> { return request('get_taskbar_preferences'); }
export function setTaskbarPreferences(mutation: import('./generated/contracts').TaskbarPreferencesMutation): Promise<import('./generated/contracts').TaskbarPreferencesSnapshot> { return request('set_taskbar_preferences', { request: mutation }); }
export function getTaskbarStatus(): Promise<import('./generated/contracts').TaskbarRuntimeSnapshot> { return request('get_taskbar_status'); }
export async function retryTaskbarEmbed(): Promise<void> { await request<null>('retry_taskbar_embed'); }
/** Runtime events invalidate only; a fresh command supplies the complete authoritative status. */
export async function onTaskbarStatusChanged(refresh: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen('taskbar_status_changed', refresh);
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}
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
export function queryDiagnostics(sourceId: string | null): Promise<import('./generated/contracts').DiagnosticsSnapshot> { return request('query_diagnostics', { request: { source_id: sourceId } }); }
export function chooseSourceDirectory(kind: SourceDirectoryKind): Promise<SourceDirectorySelection | null> { return request('choose_source_directory', { kind }); }
export function manageSource(action: ManageSourceAction, expectedSettingsRevision: string): Promise<SourcesSnapshot> { return request('manage_source', { action, expectedSettingsRevision }); }
export function listJobs(): Promise<Job[]> { return request('list_jobs', { limit: 50 }); }
export function getRebuildStatus(): Promise<Job | null> { return request('get_rebuild_status'); }
export function startJob(jobRequest: JobRequest): Promise<Job> { return request('start_job', { request: jobRequest }); }
export function startSourceReread(jobRequest: JobRequest): Promise<Job> { return request('start_source_reread', { request: jobRequest }); }
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
export function getPriceRevalueStatus(): Promise<import('./generated/contracts').PriceRevalueStatus> { return request('get_price_revalue_status'); }
export function startPriceRevalue(job: import('./generated/contracts').PriceRevalueRequest): Promise<import('./generated/contracts').PriceRevalueJob> { return request('start_price_revalue', { request: job }); }
export function cancelPriceRevalue(jobId: string): Promise<CancelJobResult> { return request('cancel_price_revalue', { jobId }); }
export async function onPriceRevalueChanged(refresh: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen('price_revalue_changed', refresh);
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}
export function getOfflinePriceCatalog(revision: string | null = null): Promise<OfflinePriceCatalogSnapshot> { return request('get_offline_price_catalog', { revision }); }
export function mutateModelAlias(aliasRequest: ModelAliasMutation, expectedPriceRevision: string): Promise<PriceRulesSnapshot> { return request('mutate_model_alias', { request: aliasRequest, expectedPriceRevision }); }
export function savePriceRule(priceRequest: Exclude<PriceRuleMutation, { kind: 'retire' }>, expectedPriceRevision: string): Promise<PriceRulesSnapshot> { return request('save_price_rule', { request: priceRequest, expectedPriceRevision }); }
export function retirePriceRule(ruleId: string, expectedPriceRevision: string): Promise<PriceRulesSnapshot> { return request('retire_price_rule', { ruleId, expectedPriceRevision }); }
export function runtimeError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'object' && error !== null && 'code' in error) {
    const code = String(error.code);
    if (code === 'NOTIFY_INTEGRATION_FAILED') {
      const details = 'details' in error && typeof error.details === 'object' && error.details !== null ? error.details : null;
      const issue = details && 'notify_issue' in details ? String(details.notify_issue) : '';
      return notifyIssueText(issue);
    }
    if (code === 'SNAPSHOT_EXPIRED') return '查询快照已过期，请重新查询。';
    if (code === 'UPDATE_UNAVAILABLE') return '更新不可用，请确认正在使用已配置发布源的正式安装版。';
    if (code === 'UPDATE_BUSY') return '已有更新操作正在进行，请等待完成。';
    if (code === 'UPDATE_FAILED') return '更新未能完成，请重新检查后重试。';
    if (code === 'CURSOR_INVALID') return '分页条件或游标已失效，请重新查询。';
    if (code === 'DB_WRITE_FAILED') return '数据库写入失败，设置未保存，请重试。';
    const descriptions: Record<string, string> = { QUOTA_DISCONNECTED: '账户服务未连接，请先连接已保存服务。', QUOTA_UNSUPPORTED: '当前连接不提供账户额度，本地统计继续可用。', QUOTA_AUTH_REQUIRED: '所选 Codex Home 的本地登录态不可用，请选择已登录账户使用的 Home 后重新连接。', QUOTA_TIMEOUT: '账户服务响应超时，保留已知旧快照。', QUOTA_PROTOCOL_ERROR: '账户服务响应无法验证，请检查服务版本或重新连接。', QUOTA_SERVICE_UNAVAILABLE: '账户服务程序不可用，请检查已选择程序和 Home。', SHORTCUT_CONFLICT: '恢复快捷键已被其他应用占用，旧组合保持生效，请更换组合。', SHORTCUT_UNAVAILABLE: '无法注册恢复快捷键，托盘恢复入口继续可用。', UNSUPPORTED_SETTINGS_VERSION: '配置版本高于或不同于当前应用支持的版本，已有配置已保留。', REVISION_CONFLICT: '配置或作业状态已发生变化，请刷新后重试。', PRICE_RULE_CONFLICT: '同一范围和优先级的价格规则有效期重叠，请调整日期或优先级。', SOURCE_UNREADABLE: '无法读取所选来源，请检查目录和访问权限。', INVALID_QUERY: '请求参数或当前数据范围无效，请检查后重试。', STALE_CONFIRMATION: '选择已过期或程序已变化，请重新选择。', PERMISSION_DENIED: '该窗口或目录不在允许范围内。', CANDIDATE_OBSOLETE: '重建输入已发生变化，旧统计已保留，请核对来源后重试。', JOB_INTERRUPTED: '作业已中断，旧统计已保留，可重新提交。', JOB_CANCELLED: '作业已安全取消。' };
    return descriptions[code] ?? `操作失败（${code}），请查看采集诊断。`;
  }
  return '桌面服务未能完成操作，请重试。';
}

export function notifyIssueText(issue: string): string {
  const messages: Record<string, string> = {
    transaction_unavailable: '当前目录不支持安全修改通知配置。请使用本机 NTFS 目录；日志采集继续可用。',
    busy: '通知配置正在被其他程序使用，请稍后重试。',
    config_changed: '配置已发生变化，请关闭当前预览并重新预览。',
    ownership_changed: '通知命令已被修改，无法覆盖。请检查该 Home 的通知配置。',
    already_managed: '该 Home 已有 TokenPulse 通知命令，请先核对现有接入状态。',
    active_configuration: '通知仍在启用，请先预览并停用通知，再清理接入记录。',
    plan_not_found: '预览已关闭或失效，请重新预览。', plan_expired: '预览已过期，请重新预览。',
    plan_limit: '打开的预览过多，请关闭已有预览后重试。',
    no_original_command: '没有可保留的原通知命令，请重新选择通知处理方式。',
    invalid_config: '无法读取有效的 config.toml，请修正配置后重试。',
    invalid_notify: '原通知配置不是受支持的命令数组，请检查配置后重试。',
    unsafe_path: '仅支持本机磁盘上的 Codex Home，请重新选择目录。',
    unsafe_file: '配置文件类型不受支持，请检查链接或文件属性。',
    unsafe_permissions: '通知接入记录的访问权限不符合要求，请检查应用数据目录权限。',
    permission_denied: '没有修改通知配置的权限，请检查目录与文件权限。',
    invalid_registration: '接入记录无法验证，已保留配置，请检查此 Home 的通知设置。',
    invalid_marker: '通知标记无法验证，接入记录已保留。',
    limit_reached: '通知接入数量或配置大小超过限制，请检查已有接入。',
    wrong_executable: '通知指向之前的程序位置，请先停用旧接入再重新启用。',
    cleanup_failed: '配置操作未完成且接入记录仍待清理，请刷新接入状态后核对。',
    not_found: '接入记录已不存在，请刷新通知状态。',
    already_exists: '接入记录已存在，请刷新通知状态。',
    channel_unavailable: '通知监听暂时不可用，日志采集继续运行。',
    worker_unavailable: '通知服务暂时不可用，请重启应用后重试。',
    unavailable: '暂时无法读取或清理通知接入，请检查占用与访问权限后重试。',
  };
  return messages[issue] ?? '通知接入未能完成，请刷新状态后重试。';
}
export function getNotifyIntegrations(): Promise<NotifyIntegrationsSnapshot> { return request('get_notify_integrations'); }
export function prepareNotifyIntegration(action: NotifyPrepareAction): Promise<NotifyConfigPreview | null> { return request('prepare_notify_integration', { request: action }); }
export function applyNotifyIntegration(planId: string): Promise<NotifyApplyResult> { return request('apply_notify_integration', { planId }); }
export function retireNotifyIntegration(registrationId: string): Promise<NotifyApplyResult> { return request('retire_notify_integration', { registrationId }); }
export async function releaseNotifyPreview(planId: string): Promise<void> {
  if (!isTauri()) return;
  const requestId = crypto.randomUUID();
  const response = await invoke<Response<null>>('release_notify_preview', { requestId, planId });
  if (response.api_version !== 1 || response.request_id !== requestId) throw new Error('桌面协议版本或响应身份不匹配。');
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

export function queryMiniSessions(query: MiniSessionsRequest): Promise<MiniSessionsPage> { return request('query_mini_sessions', { request: query }); }

export function getAccountServiceConfig(): Promise<import('./generated/contracts').AccountServiceConfigSnapshot> { return request('get_account_service_config'); }
export function chooseAccountService(selection: import('./generated/contracts').AccountServiceSelectionRequest): Promise<import('./generated/contracts').AccountServiceSelection | null> { return request('choose_account_service', { request: selection }); }
export function cancelAccountServiceSelection(selectionHandle: string): Promise<void> { return request('cancel_account_service_selection', { selectionHandle }); }
export function saveAccountServiceConfig(mutation: import('./generated/contracts').AccountServiceConfigMutation): Promise<import('./generated/contracts').AccountServiceConfigSnapshot> { return request('save_account_service_config', { request: mutation }); }
export function manageAccountConnection(connection: import('./generated/contracts').AccountConnectionRequest): Promise<import('./generated/contracts').QuotaSnapshot> { return request('manage_account_connection', { request: connection }); }
export function getAccountQuota(): Promise<import('./generated/contracts').QuotaSnapshot> { return request('get_account_quota'); }
export function refreshAccountQuota(): Promise<import('./generated/contracts').QuotaRefreshResult> { return request('refresh_account_quota'); }
export async function onAccountQuotaChanged(refresh: (change: import('./generated/contracts').QuotaChanged) => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen<import('./generated/contracts').QuotaChanged>('account_quota_changed', event => refresh(event.payload));
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}

export function getMainNavigation(): Promise<import('./generated/contracts').MainNavigationSnapshot> { return request('get_main_navigation'); }
export async function onMainNavigationChanged(refresh: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const stop = await listen('main_navigation_changed', refresh);
  return () => { void Promise.resolve(stop()).catch(() => {}); };
}
