import { invoke, isTauri } from '@tauri-apps/api/core';

export type ServiceState = 'not_configured' | 'not_implemented' | 'ready';
export interface AppStatus {
  api_version: number;
  version: string;
  development: boolean;
  data_directory: string;
  collector: ServiceState;
  storage: ServiceState;
  quota: ServiceState;
  taskbar: ServiceState;
}

export async function getAppStatus(): Promise<AppStatus> {
  if (!isTauri()) throw new Error('请通过桌面应用打开。浏览器预览不提供本地采集与统计。');
  return invoke<AppStatus>('get_app_status');
}

export async function windowAction(action: 'open_stats' | 'hide_main' | 'quit'): Promise<void> {
  return invoke('window_action', { action });
}
