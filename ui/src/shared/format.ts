import type { DecimalInt, DecimalMoney } from './generated/contracts';

function integer(value: DecimalInt): bigint {
  if (!/^(0|[1-9][0-9]*)$/.test(value)) throw new Error('INVALID_DECIMAL');
  return BigInt(value);
}
export function fullTokens(value: DecimalInt | null): string {
  return value === null ? '—' : integer(value).toLocaleString('zh-CN');
}
/** Source durations stay exact integers until rounding for display. */
export function formatDuration(value: DecimalInt | null): string {
  if (value === null) return '未知';
  const ms = integer(value);
  if (ms < 1000n) return `${ms} ms`;
  const hundredths = (ms + 5n) / 10n;
  const hours = hundredths / 360000n, minutes = hundredths / 6000n % 60n;
  const seconds = hundredths % 6000n;
  const fraction = (seconds % 100n).toString().padStart(2, '0').replace(/0+$/, '');
  const secondText = `${seconds / 100n}${fraction ? `.${fraction}` : ''} 秒`;
  return `${hours ? `${hours} 小时 ` : ''}${hours || minutes ? `${minutes} 分 ` : ''}${secondText}`;
}
/** Original diagnostic vectors can retain invalid negative i64 counters. */
export function rawTokens(value: string | null): string {
  if (value === null) return '—';
  if (!/^(0|-?[1-9][0-9]*)$/.test(value)) throw new Error('INVALID_RAW_COUNTER');
  const count = BigInt(value);
  if (count < -9223372036854775808n || count > 9223372036854775807n) throw new Error('INVALID_RAW_COUNTER');
  return count.toLocaleString('zh-CN');
}
export function compactTokens(value: DecimalInt | null): string {
  if (value === null) return '—';
  const n = integer(value);
  for (const [scale, label] of [[1_000_000_000n, 'B'], [1_000_000n, 'M'], [1000n, 'K']] as const) {
    if (n >= scale) { const rounded = (n * 10n + scale / 2n) / scale; return `${rounded / 10n}.${rounded % 10n}${label}`; }
  }
  return n.toString();
}
export function percentage(numerator: DecimalInt | null, denominator: DecimalInt | null): number | null {
  if (numerator === null || denominator === null) return null;
  const n = integer(numerator), d = integer(denominator);
  if (d === 0n || n > d) return null;
  // Only the bounded final display percentage crosses into Number.
  return Number((n * 10000n + d / 2n) / d) / 100;
}
export function money(value: DecimalMoney | null, decimals = 2): string {
  if (value === null) return '—';
  if (!/^(0|[1-9][0-9]*)(\.[0-9]{1,15})?$/.test(value) || decimals < 0 || decimals > 15 || !Number.isInteger(decimals)) throw new Error('INVALID_MONEY');
  const [whole, fraction = ''] = value.split('.');
  const atoms = BigInt(whole + fraction.padEnd(15, '0'));
  const divisor = 10n ** BigInt(15 - decimals);
  const rounded = decimals === 15 ? atoms : (atoms + divisor / 2n) / divisor;
  const unit = 10n ** BigInt(decimals);
  return decimals === 0 ? rounded.toString() : `${rounded / unit}.${(rounded % unit).toString().padStart(decimals, '0')}`;
}
