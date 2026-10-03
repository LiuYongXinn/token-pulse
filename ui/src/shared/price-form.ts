import type { PriceRule, PriceRuleDraft } from './generated/contracts';

const atomScale = 1_000_000_000n;
const maxAtoms = 1_000_000_000_000_000n;
export function priceAtoms(value: string): string {
  if (!/^(0|[1-9][0-9]*)(\.[0-9]{1,9})?$/.test(value)) throw new Error('单价须为非负十进制，最多 9 位小数。');
  const [whole, fraction = ''] = value.split('.');
  const atoms = BigInt(whole) * atomScale + BigInt(fraction.padEnd(9, '0'));
  if (atoms > maxAtoms) throw new Error('每百万 Token 单价不能超过 1000000。');
  return atoms.toString();
}
export function pricePerMillion(atoms: string | null): string {
  if (atoms === null) return '未知';
  if (!/^(0|[1-9][0-9]*)$/.test(atoms)) throw new Error('无效的单价原子值。');
  const value = BigInt(atoms);
  const fraction = (value % atomScale).toString().padStart(9, '0').replace(/0+$/, '');
  return `${value / atomScale}${fraction ? `.${fraction}` : ''}`;
}
export function utcInput(epoch: number): string { return new Date(epoch).toISOString().slice(0, -1); }
export function utcEpoch(value: string): number {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d{1,3})?)?$/.test(value)) throw new Error('请填写有效的 UTC 日期和时间。');
  const epoch = Date.parse(`${value}Z`);
  // Date.parse normalizes invalid calendar days; reject them before submitting.
  const canonical = value.length === 16 ? `${value}:00.000` : value.split('.')[0] + '.' + (value.split('.')[1] ?? '').padEnd(3, '0');
  if (!Number.isSafeInteger(epoch) || utcInput(epoch) !== canonical) throw new Error('请填写有效的 UTC 日期和时间。');
  return epoch;
}
export type PriceForm = { provider: string; model: string; source: string; currency: string; from: string; to: string; priority: string; input: string; cached: string; write: string; output: string; reference: string };
export function blankPriceForm(): PriceForm { return { provider: '', model: '', source: '', currency: 'USD', from: '1970-01-01T00:00', to: '', priority: '0', input: '', cached: '', write: '', output: '', reference: '' }; }
export function editPriceForm(rule: PriceRule): PriceForm {
  return { provider: rule.provider, model: rule.model_exact, source: rule.source_id ?? '', currency: rule.currency, from: utcInput(rule.effective_from_ms), to: rule.effective_to_ms === null ? '' : utcInput(rule.effective_to_ms), priority: String(rule.priority), input: pricePerMillion(rule.input_rate_atoms), cached: rule.cached_rate_atoms === null ? '' : pricePerMillion(rule.cached_rate_atoms), write: rule.cache_write_rate_atoms === null ? '' : pricePerMillion(rule.cache_write_rate_atoms), output: pricePerMillion(rule.output_rate_atoms), reference: rule.origin_reference ?? '' };
}
export function priceDraft(form: PriceForm): PriceRuleDraft {
  const key = (value: string, label: string, max: number) => {
    if (!value || new TextEncoder().encode(value).length > max || /[\p{Cc}]/u.test(value)) throw new Error(`${label}不能为空、包含控制字符或超过 ${max} 字节。`);
    return value;
  };
  if (!/^[A-Z]{3}$/.test(form.currency)) throw new Error('货币须为 3 位大写代码。');
  if (!/^(0|[1-9][0-9]*)$/.test(form.priority) || BigInt(form.priority) > 10_000n) throw new Error('优先级须为 0 至 10000 的整数。');
  const from = utcEpoch(form.from), to = form.to === '' ? null : utcEpoch(form.to);
  if (to !== null && to <= from) throw new Error('截止时间须晚于开始时间；截止时刻不包含在有效期内。');
  return { provider: key(form.provider, '提供方', 256), model_exact: key(form.model, '模型标识', 256), source_id: form.source === '' ? null : key(form.source, '来源', 256), currency: form.currency, effective_from_ms: from, effective_to_ms: to, priority: Number(form.priority), input_rate_atoms: priceAtoms(form.input), cached_rate_atoms: form.cached === '' ? null : priceAtoms(form.cached), cache_write_rate_atoms: form.write === '' ? null : priceAtoms(form.write), output_rate_atoms: priceAtoms(form.output), origin_reference: form.reference === '' ? null : key(form.reference, '价格依据', 2048) };
}
