import { useState } from 'react';
import type { DashboardRequest, GroupDimension, GroupSort, PricedUsageGroup, PricingSummary } from '../shared/generated/contracts';
import { compactTokens, fullTokens, percentage } from '../shared/format';
import { getGroupedUsage } from '../shared/runtime';
import { useSnapshotQuery } from './useSnapshotQuery';
import { Cost, coverageNames, reasonNames, when } from './usage-display';
import './grouped.css';
import { Icon } from '../shared/Icon';

function ratio(a: string | null, b: string | null) { const n = percentage(a, b); return n === null ? '—' : `${n}%`; }
function PriceCoverage({ pricing, total, onPrices }: { pricing: PricingSummary; total: string; onPrices: () => void }) {
  const reasons = pricing.reasons.map(reason => `${reasonNames[reason.code] ?? reason.code} · ${fullTokens(reason.total_tokens)} Token`).join('\n');
  return <div className="group-pricing"><button className="text-button" onClick={onPrices} title={reasons || '按当前快照价格规则估算'}>已计价 {ratio(pricing.priced_total_tokens, total)}</button><small>未计价 {fullTokens(pricing.unpriced_total_tokens)} Token</small></div>;
}
function Share({ total, overall }: { total: string; overall: string }) {
  const amount = percentage(total, overall);
  return <div className="group-share"><span>{amount === null ? '—' : `${amount}%`}</span><div aria-hidden="true"><i style={{ width: `${amount ?? 0}%` }} /></div></div>;
}
function cache(group: PricedUsageGroup) {
  return group.totals.cached_input.complete && group.totals.input_total.complete ? ratio(group.totals.cached_input.value, group.totals.input_total.value) : '—';
}
export function GroupedPage({ request, dimension, refreshRevision, onPrices, active = true }: { request: DashboardRequest; dimension: GroupDimension; refreshRevision: number; onPrices: () => void; active?: boolean }) {
  const [sort, setSort] = useState<GroupSort>('total_desc');
  const [limit, setLimit] = useState(200);
  const { bundle, error, loading } = useSnapshotQuery({ filter: request.filter, price_basis: request.price_basis, dimension, sort, limit }, refreshRevision, getGroupedUsage, active);
  if (!active) return null;
  if (!bundle) return <section className="empty panel"><h2>{loading ? '正在读取分组统计' : '分组统计暂不可用'}</h2>{error && <p role="alert">{error}</p>}</section>;
  const name = dimension === 'models' ? '模型' : '项目';
  return <>
    {error && <div className="notice" role="alert">{error}<span>保留上次快照 · {when(bundle.meta.generated_at_ms, bundle.meta.display_timezone)}</span></div>}
    <div className={`overview-coverage ${bundle.coverage.state}`}><span>{coverageNames[bundle.coverage.state]} · 待确认观察 {fullTokens(bundle.coverage.pending_observation_count)}</span><span role="status">{loading ? '正在刷新…' : `快照 ${when(bundle.meta.generated_at_ms, bundle.meta.display_timezone)}`}</span></div>
    <section className="group-stat-strip" aria-label={`${name}统计汇总`}>
      <div><p className="metric-label">可信 Token 总量</p><strong className="group-total" title={fullTokens(bundle.summary.total_tokens)} aria-label={`${fullTokens(bundle.summary.total_tokens)} Token`}>{compactTokens(bundle.summary.total_tokens)}</strong></div>
      <div><p className="metric-label">已计价部分估算</p><Cost pricing={bundle.pricing} /></div>
      <div><p className="metric-label">消费会话</p><strong className="group-total">{fullTokens(bundle.summary.session_count)}</strong></div>
    </section>
    <div className="group-toolbar"><span>{fullTokens(bundle.total_group_count)} 个{name}分类 · 已显示 {bundle.groups.length} 个</span><div><label>排序 <select aria-label={`${name}排序`} value={sort} onChange={e => setSort(e.target.value as GroupSort)}><option value="total_desc">消耗最多</option><option value="name_asc">名称顺序</option></select></label><label>显示 <select aria-label={`${name}显示数量`} value={limit} onChange={e => setLimit(Number(e.target.value))}>{[50,100,200].map(n => <option key={n} value={n}>{n} 个</option>)}</select></label></div></div>
    {bundle.truncated && <p className="group-truncated" role="status">还有未显示的{name}，请缩小筛选范围。</p>}
    {bundle.groups.length === 0 ? <section className="panel group-empty"><h2>当前筛选暂无可信消费</h2></section> : dimension === 'models' ?
      <div className="group-table-wrap"><table className="group-table" aria-label="模型用量"><thead><tr><th>模型</th><th className="numeric">Token 总量</th><th>占总量</th><th className="numeric">缓存 / 输入</th><th className="numeric">估算费用</th><th>价格覆盖</th></tr></thead><tbody>{bundle.groups.map(group => <tr key={group.key ?? 'unknown'}><td><strong>{group.display_name}</strong><small>{coverageNames[group.coverage.state]}</small></td><td className="numeric" title={fullTokens(group.totals.total_tokens)}>{compactTokens(group.totals.total_tokens)}</td><td><Share total={group.totals.total_tokens} overall={bundle.summary.total_tokens} /></td><td className="numeric" title={cache(group) === '—' ? '分项覆盖不足或无输入' : '缓存包含在输入总数中'}>{cache(group)}</td><td className="numeric group-cost"><Cost pricing={group.pricing} /></td><td><PriceCoverage pricing={group.pricing} total={group.totals.total_tokens} onPrices={onPrices} /></td></tr>)}</tbody></table></div> :
      <div className="project-list" aria-label="项目统计列表">{bundle.groups.map(group => <article className="project-row" key={group.key ?? 'unknown'}><span className="project-symbol"><Icon name="projects" size={23} /></span><div className="project-info"><h2>{group.display_name}</h2><p>{fullTokens(group.totals.session_count)} 个会话</p><Share total={group.totals.total_tokens} overall={bundle.summary.total_tokens} /><small>{coverageNames[group.coverage.state]}</small></div><div className="project-values"><strong title={fullTokens(group.totals.total_tokens)}>{compactTokens(group.totals.total_tokens)} Token</strong><div className="group-cost"><Cost pricing={group.pricing} /></div><PriceCoverage pricing={group.pricing} total={group.totals.total_tokens} onPrices={onPrices} /></div></article>)}</div>}
  </>;
}
