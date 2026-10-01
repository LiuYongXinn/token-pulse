import { expect, test } from 'vitest';
import { blankPriceForm, priceAtoms, priceDraft, pricePerMillion, utcEpoch, utcInput } from './price-form';

test('rates preserve every atom and distinguish unknown from a known zero', () => {
  expect(priceAtoms('999999.123456789')).toBe('999999123456789');
  expect(pricePerMillion('999999123456789')).toBe('999999.123456789');
  expect(priceAtoms('0.000000001')).toBe('1');
  expect(pricePerMillion(null)).toBe('未知'); expect(pricePerMillion('0')).toBe('0');
  const max = '1000000000000000';
  expect(priceAtoms(pricePerMillion(max))).toBe(max);
  for (const invalid of ['01', '-1', '1e3', '1.', '.1', ' 1', '1.0000000001', '1000000.000000001', '1000001', '9007199254740993.000000001', '170141183460469231731687303716']) expect(() => priceAtoms(invalid)).toThrow();
});
test('explicit UTC times reject normalized invalid days and retain millisecond boundaries', () => {
  for (const value of ['2024-02-29T12:30', '2026-10-01T12:30:05.123', '1900-01-01T00:00:00.001']) {
    const epoch = utcEpoch(value); expect(utcEpoch(utcInput(epoch))).toBe(epoch);
  }
  for (const value of ['2026-02-29T00:00', '2026-02-30T00:00', '2026-01-01T24:00', '2026-01-01T00:00+08:00', '2026-01-01T00:00:00.0001']) expect(() => utcEpoch(value)).toThrow();
});
test('rule drafts require actual prices and keep optional unknown fields null', () => {
  const form = { ...blankPriceForm(), provider: 'synthetic', model: 'fixture-only', input: '0', output: '0.000000001' };
  expect(priceDraft(form)).toMatchObject({ cached_rate_atoms: null, source_id: null, effective_from_ms: 0, effective_to_ms: null, priority: 0, input_rate_atoms: '0', output_rate_atoms: '1', origin_reference: null });
  expect(priceDraft({ ...form, cached: '0', reference: 'Synthetic test fixture', priority: '10000' }).cached_rate_atoms).toBe('0');
  for (const change of [{ input: '' }, { model: '' }, { provider: 'x\n' }, { currency: 'usd' }, { priority: '10001' }, { priority: '1.1' }, { from: '2026-01-01T00:00', to: '2026-01-01T00:00' }]) expect(() => priceDraft({ ...form, ...change })).toThrow();
});
