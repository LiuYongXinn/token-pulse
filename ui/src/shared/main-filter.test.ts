import { expect, test } from 'vitest';
import { availableGrain, calendarDateLabel, mainRequestForCalendar } from './main-filter';
test('dashboard forwards authoritative calendar and independent heatmap without local reinterpretation', () => {
  const range = { start_ms: Date.parse('2026-11-01T04:00:00Z'), end_ms: Date.parse('2026-11-02T05:00:00Z'), timezone: 'America/New_York' };
  const heatmap_range = { ...range, start_ms: Date.parse('2026-05-04T04:00:00Z') };
  const request = mainRequestForCalendar({ range, heatmap_range, local_today: '2026-11-01' }, 'synthetic-source', 'hour');
  expect(request.filter.range).toEqual(range); expect(request.heatmap_range).toEqual(heatmap_range);
  expect(request.filter.sources).toEqual({ kind: 'ids', ids: ['synthetic-source'], include_unknown: false });
  expect(request.price_basis).toEqual({ mode: 'event_time' });
  expect(mainRequestForCalendar({ range, heatmap_range, local_today: '2026-11-01' }, null, 'day').filter.sources).toEqual({ kind: 'all' });
});

test('date labels use the chosen timezone, including DST and inclusive last instant', () => {
  expect(calendarDateLabel(Date.parse('2026-11-01T03:59:59Z'), 'America/New_York')).toBe('2026-10-31');
  expect(calendarDateLabel(Date.parse('2026-11-01T04:00:00Z'), 'America/New_York')).toBe('2026-11-01');
  expect(calendarDateLabel(Date.parse('2026-11-02T04:59:59.999Z'), 'America/New_York')).toBe('2026-11-01');
  expect(calendarDateLabel(Date.parse('2026-11-01T04:00:00Z'), 'Asia/Tokyo')).toBe('2026-11-01');
});

test('long ranges use wider trend buckets without altering dates or token scope', () => {
  const range = { start_ms: 0, end_ms: 365 * 86_400_000, timezone: 'UTC' };
  expect(availableGrain('hour', range)).toBe('day'); expect(availableGrain('day', range)).toBe('day');
  expect(availableGrain('hour', { ...range, end_ms: 10 * 365 * 86_400_000 })).toBe('month');
  expect(availableGrain('month', range)).toBe('month');
  expect(availableGrain('hour', { ...range, end_ms: 25 * 3_600_000 })).toBe('hour');
  expect(mainRequestForCalendar({ range, heatmap_range: range, local_today: '1970-01-01' }, null, 'hour').filter.range).toEqual(range);
});
