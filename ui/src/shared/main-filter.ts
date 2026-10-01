import type { DashboardRequest, DateRange, Grain } from './generated/contracts';

export type DatePreset = 'today' | 'last7' | 'last30';
const lengths: Record<DatePreset, number> = { today: 1, last7: 7, last30: 30 };
function localCalendarRange(days: number, at: number, timezone: string): DateRange {
  const start = new Date(at); start.setHours(0, 0, 0, 0); start.setDate(start.getDate() - days + 1);
  const end = new Date(at); end.setHours(0, 0, 0, 0); end.setDate(end.getDate() + 1);
  // Native local calendar operations preserve 23/25-hour days. No fixed-day ms.
  return { start_ms: start.getTime(), end_ms: end.getTime(), timezone };
}
export function mainDayIdentity(at: number): string {
  const midnight = new Date(at); midnight.setHours(0, 0, 0, 0);
  return `${midnight.getTime()}:${Intl.DateTimeFormat().resolvedOptions().timeZone}`;
}
export function mainRequest(preset: DatePreset, source: string | null, grain: Grain, at: number): DashboardRequest {
  const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  return { filter: { range: localCalendarRange(lengths[preset], at, timezone), sources: source === null ? { kind: 'all' } : { kind: 'ids', ids: [source], include_unknown: false }, models: { kind: 'all' }, projects: { kind: 'all' }, sessions: { kind: 'all' } }, price_basis: { mode: 'event_time' }, grain, heatmap_range: localCalendarRange(182, at, timezone) };
}
