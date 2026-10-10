import { Fragment, useState } from 'react';
import type { DashboardRequest, MatchedPrice, RawUsageVector, RequestInputEvidence, UsageEventRow, UsageEventSort, UsageEventsPage, UsageEventsQuery } from '../shared/generated/contracts';
import { compactTokens, fullTokens, money, rawTokens } from '../shared/format';
import { closeQuerySnapshot, queryUsageEvents } from '../shared/runtime';
import { Cost, reasonNames, whenExact } from './usage-display';
import { usePagedUsage, type PageAdapter } from './usePagedUsage';
import './events.css';
import { PendingStatistics } from './PendingStatistics';

const adapter: PageAdapter<UsageEventsQuery, UsageEventsPage> = { label: '明细', read: queryUsageEvents, close: request => closeQuerySnapshot({ kind: 'usage_events', request }), keys: page => page.events.map(event => event.event_id) };
const methodNames: Record<string, string> = { last_new_stream: '新计数流的最后用量', last_with_baseline: '最后用量（累计基线已核对）', cumulative_delta: '累计差值', last_rebased: '最后用量（后续累计基线已恢复）', unchanged_cumulative: '累计未变的上下文刷新', last_only: '最后用量', episode_reset: '新计数阶段或重置', physical_duplicate: '相同物理记录', verified_duplicate: '已证实重复', inherited: '已证实继承', lineage_pending: '继承待确认', repeated_snapshot: '累计快照未增加', unattributed_anchor: '缺少归属依据的基线', unattributed_usage: '未归属用量', ambiguous_usage: '存在歧义的用量', invalid_usage: '无效用量', stream_capacity_exceeded: '计数流超出上限' };
const qualityNames: Record<string, string> = { confirmed: '已确认', pending: '待确认', inherited: '继承', duplicate: '重复证据', unattributed: '未归属' };
const vectors: [keyof RawUsageVector, string][] = [['input_total', '输入总数'], ['cached_input', '缓存输入'], ['cache_write_input', '缓存写入（输入包含项）'], ['output_total', '输出总数'], ['reasoning_output', '其中推理'], ['reported_total', '报告总量']];
function EventCost({ row }: { row: UsageEventRow }) {
  const price = row.price;
  if (price.status === 'redacted') return <span className="event-unpriced">已隐藏</span>;
  return price.status === 'unpriced' ? <span className="event-unpriced" title={reasonNames[price.reason] ?? price.reason}>未计价</span> : <strong className="event-cost" title={`${price.currency} ${price.estimated_cost} · 规则 ${price.rule_id}`}>{price.currency === 'USD' ? '$' : `${price.currency} `}{money(price.estimated_cost)}</strong>;
}
function MatchedPriceExplanation({ row }: { row: UsageEventRow }) {
  if (row.price.status === 'redacted') return <><dt>价格来源</dt><dd>已隐藏</dd></>;
  const matched = row.matched_price;
  if (!matched) return <><dt>价格来源</dt><dd>尚无可确认的匹配依据</dd></>;
  const basis = matched.basis;
  let label: string, note: string;
  switch (basis.kind) {
    case 'custom_rule':
      label = basis.source_specific ? '来源专用自定义规则' : '自定义规则 · 全部来源';
      note = '按自定义单价估算；该规则不证明请求实际模式或地区条件。'; break;
    case 'offline_standard_reference':
      label = '离线 Standard 平价参考';
      note = '请求实际模式未知。使用全球 API 文本参考价，不代表账户订阅账单或独立工具费用。'; break;
    case 'offline_assumed_reference': {
      const bands = { all: '不分上下文档位', short: '短上下文', long: '长上下文' };
      label = `Standard 参考估算 · ${bands[basis.context]}${basis.context_assumed ? '（假设）' : ''}`;
      note = `实际处理模式未知，按 Standard 全球 API 文本价格估算。${basis.context_assumed ? '缺少可靠单次请求输入，暂用短上下文档；累计用量不作为请求长度。' : ''}${basis.cache_write_assumed_zero ? '缓存写入量未知，估算暂按 0；原始用量仍保留未知。' : ''}地区、订阅账单及独立工具费用未包含。`; break;
    }
    case 'offline_request_reference': {
      const tiers = { standard: 'Standard', fast: 'Fast', batch: 'Batch', flex: 'Flex', ultrafast: 'Ultrafast' };
      const bands = { all: '不分上下文档位', short: '短上下文', long: '长上下文' };
      label = `响应确认 ${tiers[basis.actual_tier]} · ${bands[basis.context]}`;
      note = '按已关联请求选择全球 API 文本参考价；地区及独立工具费用未包含。'; break;
    }
    case 'offline_rule':
      label = '离线价格规则'; note = '没有足够依据确认请求实际模式。'; break;
  }
  return <><dt>价格来源</dt><dd>{label}<small className="event-price-assumption">{note}</small></dd>
    {('catalog_id' in basis) && <><dt>目录版本</dt><dd>{basis.catalog_id}</dd></>}
    <dt>计价模型</dt><dd>{matched.model_exact}</dd><dt>规则发布版本</dt><dd>{matched.introduced_revision}</dd></>;
}
function Evidence({ row }: { row: UsageEventRow }) {
  const price = row.price;
  return <div className="event-evidence" aria-label={`${row.event_id} 核算依据`}><div className="event-evidence-heading"><h3>核算与价格依据</h3><span>{row.quality_flags.map(flag => qualityNames[flag] ?? flag).join(' · ') || '质量标记未知'}</span></div><div className="event-evidence-grid"><div><table className="event-vector-table"><caption>缓存包含在输入中，推理包含在输出中。</caption><thead><tr><th>分项</th><th>已计入增量</th><th>原始 last</th><th>原始累计</th></tr></thead><tbody>{vectors.map(([field,label]) => <tr key={field}><th>{label}</th>{[row.usage,row.raw_last,row.raw_cumulative].map((vector,index) => <td className={vector?.[field]?.startsWith('-') ? 'raw-negative' : ''} key={index}>{rawTokens(vector?.[field] ?? null)}</td>)}</tr>)}</tbody></table><p className="chart-caption">Token 用量 {fullTokens(row.total_tokens)} Token</p></div><dl><RequestInput evidence={row.request_input} matched={row.matched_price} hidden={price.status === 'redacted'} /><dt>核算方法</dt><dd title={row.calculation_method}>{methodNames[row.calculation_method] ?? row.calculation_method}</dd><dt>价格依据</dt><dd>{price.status === 'redacted' ? '已隐藏' : price.status === 'priced' ? `${price.currency} ${price.estimated_cost}` : `未计价 · ${reasonNames[price.reason] ?? price.reason}`}</dd><MatchedPriceExplanation row={row} /><dt>匹配规则</dt><dd>{price.status === 'redacted' ? '已隐藏' : row.matched_price?.rule_id ?? (price.status === 'priced' ? price.rule_id : '—')}</dd><dt>解析 / 核算版本</dt><dd>{row.parser_version} / {row.accounting_version}</dd><dt>回合标识</dt><dd>{row.turn_id ?? '未知'}</dd><dt>来源标识</dt><dd>{row.source_ids.length ? row.source_ids.join('、') : '未知'}</dd></dl></div></div>;
}
function RequestInput({ evidence, matched, hidden }: { evidence: RequestInputEvidence | null; matched: MatchedPrice | null; hidden: boolean }) {
  const actual = matched?.basis.kind === 'offline_request_reference' ? matched.basis.actual_tier : null;
  const tiers = { standard: 'Standard', fast: 'Fast', batch: 'Batch', flex: 'Flex', ultrafast: 'Ultrafast' };
  return <><dt>单次请求输入</dt><dd>{evidence ? `${fullTokens(evidence.input_tokens)} Token` : '未知（缺少可核对的响应记录）'}</dd><dt>请求与消费关联</dt><dd>{evidence ? evidence.binding === 'full_request' ? '对应完整请求用量' : '与本笔增量不同，不能据此选档' : '未知'}</dd><dt>实际模式 / 地区</dt><dd>{hidden ? '已隐藏' : actual ? `响应模式 ${tiers[actual]}；地区尚未确认` : '尚未采集，不能据此确认完整计费'}</dd></>;
}
export function EventsPage({ request, refreshRevision, onSession, active = true }: { request: DashboardRequest; refreshRevision: number; onSession: (key: string, name: string) => void; active?: boolean }) {
  const [sort, setSort] = useState<UsageEventSort>('time_desc');
  const [size, setSize] = useState(50);
  const pager = usePagedUsage({ filter: request.filter, price_basis: request.price_basis, sort, page_size: size }, refreshRevision, adapter, !active);
  const [expanded, setExpanded] = useState<string | null>(null);
  const page = pager.page;
  const scope = JSON.stringify([page?.meta.snapshot_id, pager.pageNumber]);
  const timezone = page?.meta.display_timezone ?? request.filter.range.timezone;
  const pages = page ? (BigInt(page.summary.usage_event_count) + BigInt(size) - 1n) / BigInt(size) : 0n;
  if (!active) return null;
  return <>
    <div className="session-toolbar"><div><label>排序 <select aria-label="明细排序" value={sort} onChange={e => setSort(e.target.value as UsageEventSort)}><option value="time_desc">最近发生</option><option value="total_desc">消耗最多</option></select></label><label>每页 <select aria-label="明细每页数量" value={size} onChange={e => setSize(Number(e.target.value))}>{[50,100,200].map(n => <option key={n} value={n}>{n} 条</option>)}</select></label></div></div>
    {pager.error && <div className="notice" role="alert">{pager.error}{page && <span>保留上次读取的明细。</span>}</div>}
    {!pager.loading && (pager.renewal || pager.updateAvailable) && page && <p className="notice" role="status">{pager.updateAvailable ? '有新记录，点击页面顶部“刷新”更新列表。' : '点击页面顶部“刷新”后可继续查看记录。'}</p>}
    {!page ? <PendingStatistics tableClass="event-table" label="明细" columns={['记录时间 / 会话', '项目 / 模型', '输入 / 缓存', '输出 / 推理', '总量', '估算', '依据']} /> : <>
      <section className="group-stat-strip" aria-label="明细统计汇总"><div><p className="metric-label">范围内 Token</p><strong className="group-total" aria-label={`${fullTokens(page.summary.total_tokens)} Token`} title={fullTokens(page.summary.total_tokens)}>{compactTokens(page.summary.total_tokens)}</strong></div><div><p className="metric-label">估算费用</p><Cost pricing={page.pricing} /></div><div><p className="metric-label">用量记录</p><strong className="group-total">{fullTokens(page.summary.usage_event_count)}</strong></div></section>
      {page.events.length === 0 ? <section className="panel group-empty"><h2>当前筛选暂无用量事件</h2></section> : <div className="event-table-wrap"><table className="event-table" aria-label="用量明细"><thead><tr><th>记录时间 / 会话</th><th>项目 / 模型</th><th>输入 / 缓存</th><th>输出 / 推理</th><th>总量</th><th>估算</th><th>依据</th></tr></thead><tbody>{page.events.map(row => {
        const expandedKey = JSON.stringify([scope,row.event_id]); const open = expanded === expandedKey;
        return <Fragment key={row.event_id}><tr><td><time dateTime={new Date(row.occurred_at_ms).toISOString()} title={new Date(row.occurred_at_ms).toISOString()}>{whenExact(row.occurred_at_ms, timezone)}</time><button className="text-button" onClick={() => onSession(row.session_key, row.session_display_name)}>{row.session_display_name}</button></td><td><strong>{row.project_display_name ?? '未知项目'}</strong><small>{row.model ?? '未知模型'}{row.provider ? ` · ${row.provider}` : ''}</small></td><td className="numeric">{rawTokens(row.usage.input_total)}<small>缓存 {rawTokens(row.usage.cached_input)}</small><small>写入 {rawTokens(row.usage.cache_write_input)}</small></td><td className="numeric">{rawTokens(row.usage.output_total)}<small>推理 {rawTokens(row.usage.reasoning_output)}</small></td><td className="numeric" title={fullTokens(row.total_tokens)}>{compactTokens(row.total_tokens)}</td><td><EventCost row={row} /></td><td><button className="text-button" aria-expanded={open} aria-label={`查看 ${row.event_id} 核算依据`} onClick={() => setExpanded(open ? null : expandedKey)}>{open ? '收起' : '查看依据'}</button></td></tr>{open && <tr className="event-evidence-row"><td colSpan={7}><Evidence row={row} /></td></tr>}</Fragment>;
      })}</tbody></table></div>}
      <div className="session-pagination" aria-label="明细分页"><span>第 {pager.pageNumber} 页{pages === 0n ? '' : ` / ${fullTokens(pages.toString())} 页`} · 本页 {page.events.length} 条{pager.loading ? ' · 正在读取…' : ''}</span><div><button disabled={!pager.hasPrevious || pager.loading} onClick={pager.previous}>上一页</button><button disabled={!pager.hasNext || pager.loading} onClick={pager.next}>下一页</button></div></div>
      {pager.trimmed && <p className="group-footnote">查看更早页面请点击页面顶部“刷新”。</p>}
    </>}
  </>;
}
