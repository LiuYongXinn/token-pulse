import type { DashboardRequest, Grain, CalendarSelectionResult } from './generated/contracts';
export function mainRequestForCalendar(calendar: CalendarSelectionResult, source: string | null, grain: Grain): DashboardRequest {
  return { filter: { range: calendar.range, sources: source === null ? { kind: 'all' } : { kind: 'ids', ids: [source], include_unknown: false }, models: { kind: 'all' }, projects: { kind: 'all' }, sessions: { kind: 'all' } }, price_basis: { mode: 'event_time' }, grain, heatmap_range: calendar.heatmap_range };
}

export type DatePreset = 'today' | 'last7' | 'last30';
