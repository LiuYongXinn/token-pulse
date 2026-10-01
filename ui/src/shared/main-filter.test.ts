import { expect, test } from 'vitest';
import { mainDayIdentity, mainRequest } from './main-filter';

test('system-local presets use calendar days across DST and independent heatmap range', () => {
  const previous = process.env.TZ;
  try {
    process.env.TZ = 'America/New_York';
    const fall = Date.parse('2026-11-01T12:00:00Z');
    const r = mainRequest('today', 'synthetic-source', 'hour', fall);
    expect(r.filter.range).toEqual({ start_ms: Date.parse('2026-11-01T04:00:00Z'), end_ms: Date.parse('2026-11-02T05:00:00Z'), timezone: 'America/New_York' });
    expect(r.filter.sources).toEqual({ kind: 'ids', ids: ['synthetic-source'], include_unknown: false });
    expect(r.heatmap_range.start_ms).toBeLessThan(r.filter.range.start_ms);
    const spring = mainRequest('today', null, 'day', Date.parse('2026-03-08T12:00:00Z'));
    expect(spring.filter.range.start_ms).toBe(Date.parse('2026-03-08T05:00:00Z'));
    expect(spring.filter.range.end_ms).toBe(Date.parse('2026-03-09T04:00:00Z'));
    expect(mainDayIdentity(fall)).toBe(mainDayIdentity(fall + 3_600_000));
    expect(mainDayIdentity(fall)).not.toBe(mainDayIdentity(fall + 86_400_000));
    const seven = mainRequest('last7', null, 'day', fall);
    expect(seven.filter.range.start_ms).toBe(Date.parse('2026-10-26T04:00:00Z'));
    expect(seven.filter.range.end_ms).toBe(Date.parse('2026-11-02T05:00:00Z'));
  } finally { if (previous === undefined) delete process.env.TZ; else process.env.TZ = previous; }
});
