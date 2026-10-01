import { expect, test } from 'vitest';
import { parsePriceInstant, utcPriceInput } from './price-basis';

test('UTC price instants are explicit, exact to millisecond and independent of browser zone', () => {
  const at = Date.parse('2026-11-01T05:30:12.345Z');
  expect(parsePriceInstant(utcPriceInput(at))).toBe(at);
  expect(parsePriceInstant('2026-11-01T05:30')).toBe(Date.parse('2026-11-01T05:30:00.000Z'));
  expect(parsePriceInstant('2024-02-29T00:00:00.1')).toBe(Date.parse('2024-02-29T00:00:00.100Z'));
  expect(parsePriceInstant('1970-01-01T00:00')).toBe(0);
  expect(parsePriceInstant('1969-12-31T23:59:59')).toBe(-1000);
});
test('invalid calendar and clock values are rejected instead of normalized into another instant', () => {
  for (const value of ['2026-02-29T00:00', '2026-01-01T24:00', '2026-01-01T00:60', '2026-01-01T00:00:60', '2026-01-01T00:00Z', '2026-01-01T00:00:00.0000', '2026-1-01T00:00', '']) expect(parsePriceInstant(value)).toBeNull();
});
