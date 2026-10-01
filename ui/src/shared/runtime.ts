import { invoke, isTauri } from '@tauri-apps/api/core';

import type { AppStatus, Response, WindowAction, SourcesSnapshot, SourceDirectorySelection, SourceDirectoryKind, ManageSourceAction } from './generated/contracts';
export type { AppStatus } from './generated/contracts';

async function request<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!isTauri()) throw new Error('请通过桌面应用打开。浏览器预览不提供本地采集与统计。');
  const requestId = crypto.randomUUID();
  const response = await invoke<Response<T>>(command, { ...args, requestId });
  if (response.api_version !== 1 || response.request_id !== requestId) throw new Error('桌面协议版本或响应身份不匹配。');
  return response.data;
}
export function getAppStatus(): Promise<AppStatus> { return request('get_app_status'); }
export function getSources(): Promise<SourcesSnapshot> { return request('get_sources'); }
export function chooseSourceDirectory(kind: SourceDirectoryKind): Promise<SourceDirectorySelection | null> { return request('choose_source_directory', { kind }); }
export function manageSource(action: ManageSourceAction, expectedSettingsRevision: string): Promise<SourcesSnapshot> { return request('manage_source', { action, expectedSettingsRevision }); }
export function runtimeError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'object' && error !== null && 'code' in error) {
    const code = String(error.code);
    const descriptions: Record<string, string> = { REVISION_CONFLICT: '配置已发生变化，请刷新后重试。', SOURCE_UNREADABLE: '无法读取所选来源，请检查目录和访问权限。', INVALID_QUERY: '所选目录或操作无效，请重新选择 Codex Home。', STALE_CONFIRMATION: '目录选择已过期，请重新选择。', PERMISSION_DENIED: '该窗口或目录不在允许范围内。' };
    return descriptions[code] ?? `操作失败（${code}），请查看采集诊断。`;
  }
  return '桌面服务未能完成操作，请重试。';
}

export async function windowAction(action: WindowAction): Promise<void> {
  await request<null>('perform_window_action', { action });
}
