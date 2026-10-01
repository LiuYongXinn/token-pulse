import { expect, test } from 'vitest';
import { compactTokens, fullTokens, money, percentage } from './format';

test('large tokens remain exact and unknown values remain distinct from zero', () => {
  expect(fullTokens('9007199254740993')).toBe('9,007,199,254,740,993');
  expect(fullTokens('0')).toBe('0'); expect(fullTokens(null)).toBe('—');
  expect(compactTokens('135')).toBe('135'); expect(compactTokens('1000500')).toBe('1.0M');
  expect(percentage('9007199254740993', '18014398509481986')).toBe(50);
  expect(percentage('0', '0')).toBeNull(); expect(percentage(null, '1')).toBeNull();
});
test('money is rounded once with carry, without a floating point conversion', () => {
  expect(money('9007199254740993.995')).toBe('9007199254740994.00');
  expect(money('0.004999999999999')).toBe('0.00'); expect(money('0.005')).toBe('0.01');
  expect(money('0')).toBe('0.00'); expect(money(null)).toBe('—');
  expect(money('1.500000000000001', 0)).toBe('2'); expect(money('1.000000000000001', 15)).toBe('1.000000000000001');
  for (const invalid of ['1e3', '-1', '01', 'NaN', '1.0000000000000001']) expect(() => money(invalid)).toThrow();
});
