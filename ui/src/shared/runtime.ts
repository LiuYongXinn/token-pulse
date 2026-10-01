import { invoke, isTauri } from '@tauri-apps/api/core';

import type { AppStatus, Response, WindowAction } from './generated/contracts';
export type { AppStatus } from './generated/contracts';

export async function getAppStatus(): Promise<AppStatus> {
  if (!isTauri()) throw new Error('请通过桌面应用打开。浏览器预览不提供本地采集与统计。');
  const requestId = crypto.randomUUID();
  const response = await invoke<Response<AppStatus>>('get_app_status', { requestId });
  if (response.api_version !== 1 || response.request_id !== requestId) throw new Error('桌面协议版本或响应身份不匹配。');
  return response.data;
}

export async function windowAction(action: WindowAction): Promise<void> {
  await invoke<Response<null>>('perform_window_action', { action, requestId: crypto.randomUUID() });
}
