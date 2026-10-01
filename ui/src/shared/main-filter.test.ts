import { expect, test } from 'vitest';
import { mainRequestForCalendar } from './main-filter';
test('dashboard forwards authoritative calendar and independent heatmap without local reinterpretation', () => {
  const range = { start_ms: Date.parse('2026-11-01T04:00:00Z'), end_ms: Date.parse('2026-11-02T05:00:00Z'), timezone: 'America/New_York' };
  const heatmap_range = { ...range, start_ms: Date.parse('2026-05-04T04:00:00Z') };
  const request = mainRequestForCalendar({ range, heatmap_range, local_today: '2026-11-01' }, 'synthetic-source', 'hour');
  expect(request.filter.range).toEqual(range); expect(request.heatmap_range).toEqual(heatmap_range);
  expect(request.filter.sources).toEqual({ kind: 'ids', ids: ['synthetic-source'], include_unknown: false });
  expect(request.price_basis).toEqual({ mode: 'event_time' });
  expect(mainRequestForCalendar({ range, heatmap_range, local_today: '2026-11-01' }, null, 'day').filter.sources).toEqual({ kind: 'all' });
});
