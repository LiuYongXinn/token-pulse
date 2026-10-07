import type { QuotaSnapshot, QuotaState, QuotaWindow } from './generated/contracts';

export const quotaStates: Record<QuotaState, string> = { disconnected: '未连接', connecting: '正在读取本地账户', authorization_required: '本地登录态不可用', unsupported: '当前连接不提供账户额度', ready: '额度已更新', stale: '更新失败 · 显示上次结果', error: '额度读取失败' };
export function quotaPeriod(window: QuotaWindow): string {
  const minutes = window.duration_mins;
  return minutes === null ? '未知周期' : minutes === 10080 ? '周额度' : minutes > 0 && minutes % 60 === 0 ? `${minutes / 60} 小时额度` : `${minutes} 分钟额度`;
}
export function quotaPercent(value: number | null): string {
  return value === null ? '—' : `${value.toLocaleString('zh-CN', { maximumFractionDigits: 2 })}%`;
}
/** Roles depend only on actual duration, never primary/secondary position or window id. */
export function quotaRoles(windows: QuotaWindow[]): { short: QuotaWindow | null; weekly: QuotaWindow | null } {
  const weeks = windows.filter(w => w.duration_mins === 10080);
  const shorter = windows.filter(w => w.duration_mins !== null && w.duration_mins > 0 && w.duration_mins < 10080);
  const length = Math.min(...shorter.map(w => w.duration_mins!));
  const shortest = shorter.filter(w => w.duration_mins === length);
  return { short: shortest.length === 1 ? shortest[0] : null, weekly: weeks.length === 1 ? weeks[0] : null };
}
export function quotaWindows(quota: QuotaSnapshot | null): QuotaWindow[] {
  return quota && ['ready', 'stale', 'error'].includes(quota.state) ? quota.windows : [];
}
export function quotaDate(value: number | null, timezone: string | null, compact = false): string {
  if (value === null) return '—';
  if (!timezone) return '等待设置显示时区';
  try { return new Intl.DateTimeFormat('zh-CN', { timeZone: timezone, ...(compact ? { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' } as const : { dateStyle: 'medium', timeStyle: 'short' } as const) }).format(value); }
  catch { return '显示时区无效'; }
}
export function quotaCountdown(reset: number | null, now: number): string {
  if (reset === null) return '时间未提供';
  if (reset <= now) return '等待额度更新';
  const minutes = Math.ceil((reset - now) / 60_000);
  const days = Math.floor(minutes / 1440), hours = Math.floor(minutes % 1440 / 60), rest = minutes % 60;
  return days > 0 ? `${days}天 ${hours}小时` : hours > 0 ? `${hours}小时 ${rest}分钟` : `${rest}分钟`;
}
