import type { PricingSummary } from '../shared/generated/contracts';
import { money } from '../shared/format';

export const coverageNames = { complete: '已配置来源覆盖完整', partial: '存在采集或解释缺口', unknown: '来源覆盖尚未确认' };
export const reasonNames: Record<string, string> = { unknown_model: '模型或提供方未知', missing_rule: '无匹配价格', ambiguous_rule: '价格规则存在歧义', insufficient_usage: '必要分项不足', overflow: '精确计算溢出' };
export function when(time: number, timezone: string) { return new Intl.DateTimeFormat('zh-CN', { timeZone: timezone, month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }).format(time); }
export function whenExact(time: number, timezone: string) { return new Intl.DateTimeFormat('zh-CN', { timeZone: timezone, month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3 }).format(time); }
export function whenFull(time: number, timezone: string) { return new Intl.DateTimeFormat('zh-CN', { timeZone: timezone, year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3 }).format(time); }
export function Cost({ pricing }: { pricing: PricingSummary }) {
  if (pricing.redacted) return <strong className="cost-number">已隐藏</strong>;
  if (!pricing.currencies.length) return <strong className="cost-number unavailable">未计价</strong>;
  return <div className="currency-list">{pricing.currencies.map(currency => <strong className="cost-number" key={currency.currency} title={currency.estimated_cost === null ? '金额未知' : `${currency.currency} ${currency.estimated_cost}（精确估算）`}>{currency.estimated_cost === null ? '—' : `${pricing.currencies.length === 1 && currency.currency === 'USD' ? '$' : `${currency.currency} `}${money(currency.estimated_cost)}`}</strong>)}</div>;
}
