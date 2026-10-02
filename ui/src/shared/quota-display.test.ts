import { expect, test } from 'vitest';
import type { QuotaWindow } from './generated/contracts';
import { quotaCountdown, quotaDate, quotaPercent, quotaPeriod, quotaRoles } from './quota-display';

const window = (duration: number | null, id = 'secondary'): QuotaWindow => ({ window_id: id, duration_mins: duration, used_percent: null, remaining_percent: null, resets_at_ms: null });
test('quota labels use actual durations and preserve only-week, unknown and zero', () => {
  const weekly = window(10080, 'primary'), short = window(120);
  expect(quotaRoles([weekly, short])).toEqual({ weekly, short });
  expect(quotaPeriod(short)).toBe('2 小时额度');
  expect(quotaRoles([weekly])).toEqual({ weekly, short: null });
  expect(quotaRoles([window(null)])).toEqual({ weekly: null, short: null });
  expect(quotaPeriod(window(90))).toBe('90 分钟额度');
  expect(quotaPeriod(window(20160))).toBe('336 小时额度');
  expect(quotaPercent(null)).toBe('—'); expect(quotaPercent(0)).toBe('0%');
  expect(quotaRoles([weekly, window(10080, 'extra')]).weekly).toBeNull();
  expect(quotaRoles([short, window(120, 'extra')]).short).toBeNull();
});
test('quota countdown crosses a real reset without refilling and uses explicit timezones', () => {
  expect(quotaCountdown(null, 0)).toBe('时间未提供');
  expect(quotaCountdown(1000, 1000)).toBe('等待额度更新');
  expect(quotaCountdown(1000, 999)).toBe('1分钟');
  expect(quotaCountdown(90 * 60_000, 0)).toBe('1小时 30分钟');
  expect(quotaCountdown(2 * 86400_000 + 3 * 3600_000, 0)).toBe('2天 3小时');
  expect(quotaDate(null, 'UTC')).toBe('—');
  expect(quotaDate(0, null)).toBe('等待设置显示时区');
  expect(quotaDate(0, 'invalid')).toBe('显示时区无效');
  expect(quotaDate(0, 'Asia/Shanghai')).toContain('08:00');
  expect(quotaDate(0, 'UTC')).toContain('00:00');
});
