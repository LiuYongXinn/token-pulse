import { Fragment, useState } from 'react';
import type { DashboardRequest, RawUsageVector, UsageEventRow, UsageEventSort, UsageEventsPage, UsageEventsQuery } from '../shared/generated/contracts';
import { compactTokens, fullTokens, money, rawTokens } from '../shared/format';
import { closeQuerySnapshot, queryUsageEvents } from '../shared/runtime';
import { Cost, coverageNames, reasonNames, when, whenExact } from './usage-display';
import { usePagedUsage, type PageAdapter } from './usePagedUsage';
import './events.css';

const adapter: PageAdapter<UsageEventsQuery, UsageEventsPage> = { label: '明细', read: queryUsageEvents, close: request => closeQuerySnapshot({ kind: 'usage_events', request }), keys: page => page.events.map(event => event.event_id) };
const methodNames: Record<string, string> = { last_new_stream: '新计数流的最后用量', last_with_baseline: '最后用量（累计基线已核对）', cumulative_delta: '累计差值', last_only: '最后用量', episode_reset: '新计数阶段或重置', physical_duplicate: '相同物理记录', verified_duplicate: '已证实重复', inherited: '已证实继承', lineage_pending: '继承待确认', repeated_snapshot: '累计快照未增加', unattributed_anchor: '缺少归属依据的基线', unattributed_usage: '未归属用量', ambiguous_usage: '存在歧义的用量', invalid_usage: '无效用量', stream_capacity_exceeded: '计数流超出上限' };
const qualityNames: Record<string, string> = { confirmed: '已确认', pending: '待确认', inherited: '继承', duplicate: '重复证据', unattributed: '未归属' };
const vectors: [keyof RawUsageVector, string][] = [['input_total', '输入总数'], ['cached_input', '缓存输入'], ['output_total', '输出总数'], ['reasoning_output', '其中推理'], ['reported_total', '报告总量']];
function EventCost({ row }: { row: UsageEventRow }) {
  const price = row.price;
  if (price.status === 'redacted') return <span className="event-unpriced">已隐藏</span>;
  return price.status === 'unpriced' ? <span className="event-unpriced" title={reasonNames[price.reason] ?? price.reason}>未计价</span> : <strong className="event-cost" title={`${price.currency} ${price.estimated_cost} · 规则 ${price.rule_id}`}>{price.currency === 'USD' ? '$' : `${price.currency} `}{money(price.estimated_cost)}</strong>;
}
function Evidence({ row }: { row: UsageEventRow }) {
  const price = row.price;
  return <div className="event-evidence" aria-label={`${row.event_id} 核算依据`}><div className="event-evidence-heading"><h3>核算与价格依据</h3><span>{row.quality_flags.map(flag => qualityNames[flag] ?? flag).join(' · ') || '质量标记未知'}</span></div><div className="event-evidence-grid"><div><table className="event-vector-table"><caption>缓存包含在输入内，推理包含在输出内。原始向量可能含未采用的无效值。</caption><thead><tr><th>分项</th><th>已计入增量</th><th>原始 last</th><th>原始累计</th></tr></thead><tbody>{vectors.map(([field,label]) => <tr key={field}><th>{label}</th>{[row.usage,row.raw_last,row.raw_cumulative].map((vector,index) => <td className={vector?.[field]?.startsWith('-') ? 'raw-negative' : ''} key={index}>{rawTokens(vector?.[field] ?? null)}</td>)}</tr>)}</tbody></table><p className="chart-caption">可信总量 {fullTokens(row.total_tokens)} Token。缺失分项保留未知；原始 last / 累计向量不作为额外消费相加。</p></div><dl><dt>核算方法</dt><dd title={row.calculation_method}>{methodNames[row.calculation_method] ?? row.calculation_method}</dd><dt>价格依据</dt><dd>{price.status === 'redacted' ? '已隐藏' : price.status === 'priced' ? `${price.currency} ${price.estimated_cost}` : `未计价 · ${reasonNames[price.reason] ?? price.reason}`}</dd><dt>匹配规则</dt><dd>{price.status === 'priced' ? price.rule_id : '—'}</dd><dt>计价原子</dt><dd>{price.status === 'priced' ? `${price.cost_atoms}（10⁻¹⁵）` : '—'}</dd><dt>解析 / 核算版本</dt><dd>{row.parser_version} / {row.accounting_version}</dd><dt>回合标识</dt><dd>{row.turn_id ?? '未知'}</dd><dt>来源标识</dt><dd>{row.source_ids.length ? row.source_ids.join('、') : '未知'}</dd></dl></div></div>;
}
export function EventsPage({ request, refreshRevision, onSession }: { request: DashboardRequest; refreshRevision: number; onSession: (key: string, name: string) => void }) {
  const [sort, setSort] = useState<UsageEventSort>('time_desc');
  const [size, setSize] = useState(50);
  const pager = usePagedUsage({ filter: request.filter, price_basis: request.price_basis, sort, page_size: size }, refreshRevision, adapter);
  const [expanded, setExpanded] = useState<string | null>(null);
  const page = pager.page;
  const scope = JSON.stringify([page?.meta.snapshot_id, pager.pageNumber]);
  const timezone = page?.meta.display_timezone ?? request.filter.range.timezone;
  const pages = page ? (BigInt(page.summary.usage_event_count) + BigInt(size) - 1n) / BigInt(size) : 0n;
  return <>
    <div className="session-toolbar"><div><label>排序 <select aria-label="明细排序" value={sort} onChange={e => setSort(e.target.value as UsageEventSort)}><option value="time_desc">最近发生</option><option value="total_desc">消耗最多</option></select></label><label>每页 <select aria-label="明细每页数量" value={size} onChange={e => setSize(Number(e.target.value))}>{[50,100,200].map(n => <option key={n} value={n}>{n} 条</option>)}</select></label></div><button disabled={pager.loading} onClick={pager.reload}>重新查询</button></div>
    {pager.error && <div className="notice" role="alert">{pager.error}{page && <span>保留已读取的同一快照；重新查询将替换全部页面。</span>}</div>}
    {!page ? <section className="empty panel"><h2>{pager.loading ? '正在读取明细快照' : '用量明细暂不可用'}</h2><p>每页消费、原始向量和价格依据从同一只读快照返回。</p></section> : <>
      <div className={`overview-coverage ${page.coverage.state}`}><span>{coverageNames[page.coverage.state]} · 待确认观察 {fullTokens(page.coverage.pending_observation_count)}</span><span>快照 {when(page.meta.generated_at_ms, timezone)}</span></div>
      <section className="group-stat-strip" aria-label="明细统计汇总"><div><p className="metric-label">范围内可信 Token</p><strong className="group-total" aria-label={`${fullTokens(page.summary.total_tokens)} Token`} title={fullTokens(page.summary.total_tokens)}>{compactTokens(page.summary.total_tokens)}</strong></div><div><p className="metric-label">已计价部分估算</p><Cost pricing={page.pricing} /></div><div><p className="metric-label">可信用量事件</p><strong className="group-total">{fullTokens(page.summary.usage_event_count)}</strong></div></section>
      {page.events.length === 0 ? <section className="panel group-empty"><h2>当前筛选暂无可信事件</h2><p className="muted">消费事件与待确认观察分别处理；可查看采集诊断了解缺口。</p></section> : <div className="event-table-wrap"><table className="event-table"><caption>缓存是输入子项，推理是输出子项；未知分项保留“—”。事件数不等同调用次数或完整用户回合数。</caption><thead><tr><th>记录时间 / 会话</th><th>项目 / 模型</th><th>输入 / 缓存</th><th>输出 / 推理</th><th>总量</th><th>估算</th><th>依据</th></tr></thead><tbody>{page.events.map(row => {
        const expandedKey = JSON.stringify([scope,row.event_id]); const open = expanded === expandedKey;
        return <Fragment key={row.event_id}><tr><td><time dateTime={new Date(row.occurred_at_ms).toISOString()} title={new Date(row.occurred_at_ms).toISOString()}>{whenExact(row.occurred_at_ms, timezone)}</time><button className="text-button" onClick={() => onSession(row.session_key, row.session_display_name)}>{row.session_display_name}</button></td><td><strong>{row.project_display_name ?? '未知项目'}</strong><small>{row.model ?? '未知模型'}{row.provider ? ` · ${row.provider}` : ''}</small></td><td className="numeric">{rawTokens(row.usage.input_total)}<small>缓存 {rawTokens(row.usage.cached_input)}</small></td><td className="numeric">{rawTokens(row.usage.output_total)}<small>推理 {rawTokens(row.usage.reasoning_output)}</small></td><td className="numeric" title={fullTokens(row.total_tokens)}>{compactTokens(row.total_tokens)}</td><td><EventCost row={row} /></td><td><button className="text-button" aria-expanded={open} aria-label={`查看 ${row.event_id} 核算依据`} onClick={() => setExpanded(open ? null : expandedKey)}>{open ? '收起' : '查看依据'}</button><small>{row.quality_flags.map(q => qualityNames[q] ?? q).join(' · ') || '质量未知'}</small></td></tr>{open && <tr className="event-evidence-row"><td colSpan={7}><Evidence row={row} /></td></tr>}</Fragment>;
      })}</tbody></table></div>}
      <div className="session-pagination" aria-label="明细分页"><span>第 {pager.pageNumber} 页{pages === 0n ? '' : ` / ${fullTokens(pages.toString())} 页`} · 本页 {page.events.length} 条{pager.loading ? ' · 正在读取…' : ''}</span><div><button disabled={!pager.hasPrevious || pager.loading} onClick={pager.previous}>上一页</button><button disabled={!pager.hasNext || pager.loading} onClick={pager.next}>下一页</button></div></div>
      {pager.trimmed && <p className="group-footnote">只缓存最近 10 页，查看更早页面请重新查询。</p>}
      <p className="group-footnote">汇总覆盖整个筛选范围；分页固定数据和价格，超过查询有效期请重新查询。数据修订 {page.meta.data_revision} · 价格修订 {page.meta.price_revision}</p>
    </>}
  </>;
}
