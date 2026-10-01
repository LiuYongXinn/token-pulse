import type { DashboardRequest, Grain, DateRange, CalendarSelectionResult } from './generated/contracts';
export function mainRequestForCalendar(calendar: CalendarSelectionResult, source: string | null, grain: Grain): DashboardRequest {
  return { filter: { range: calendar.range, sources: source === null ? { kind: 'all' } : { kind: 'ids', ids: [source], include_unknown: false }, models: { kind: 'all' }, projects: { kind: 'all' }, sessions: { kind: 'all' } }, price_basis: { mode: 'event_time' }, grain: availableGrain(grain, calendar.range), heatmap_range: calendar.heatmap_range };
}

/** Labels alone; UTC range construction belongs to the Rust calendar resolver. */
export function calendarDateLabel(at: number, timezone: string): string {
  const parts = new Intl.DateTimeFormat('en-US', { timeZone: timezone, calendar: 'iso8601', numberingSystem: 'latn', year: 'numeric', month: '2-digit', day: '2-digit' }).formatToParts(at);
  const part = (type: string) => parts.find(value => value.type === type)!.value;
  return part('year').padStart(4, '0') + '-' + part('month') + '-' + part('day');
}

// Reserve boundary headroom below Rust's 2000-bin cap for timezone transitions.
export function availableGrain(grain: Grain, range: DateRange): Grain {
  const duration = range.end_ms - range.start_ms;
  if (grain === 'hour' && duration > 1990 * 3_600_000) grain = 'day';
  if (grain === 'day' && duration > 1990 * 86_400_000) grain = 'month';
  return grain;
}
