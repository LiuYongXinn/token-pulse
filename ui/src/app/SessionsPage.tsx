import { useEffect, useState } from 'react';
import type { DashboardRequest, SessionRow, SessionSort } from '../shared/generated/contracts';
import { compactTokens, fullTokens } from '../shared/format';
import { Cost, when } from './usage-display';
import { useSessions } from './useSessions';
import { SessionDrawer } from './SessionDrawer';
import './sessions.css';
import { PendingStatistics } from './PendingStatistics';

export function SessionsPage({ request, refreshRevision, onSessionScope, active = true }: { request: DashboardRequest; refreshRevision: number; onSessionScope: (key: string, name: string) => void; active?: boolean }) {
  const [sort, setSort] = useState<SessionSort>('latest_desc');
  const [size, setSize] = useState(50);
  const query = { filter: request.filter, price_basis: request.price_basis, sort, page_size: size };
  const pager = useSessions(query, refreshRevision, !active);
  const [selected, setSelected] = useState<{ row: SessionRow; key: string } | null>(null);
  useEffect(() => { if (!active) setSelected(null); }, [active]);
  const scopeKey = JSON.stringify(query);
  const page = pager.page;
  const timezone = page?.meta.display_timezone ?? request.filter.range.timezone;
  const pages = page ? (BigInt(page.summary.session_count) + BigInt(size) - 1n) / BigInt(size) : null;
  if (!active) return null;
  return <>
    <div className="session-toolbar"><div><label>排序 <select aria-label="会话排序" value={sort} onChange={e => setSort(e.target.value as SessionSort)}><option value="latest_desc">最近活跃</option><option value="total_desc">消耗最多</option></select></label><label>每页 <select aria-label="会话每页数量" value={size} onChange={e => setSize(Number(e.target.value))}>{[50,100,200].map(n => <option key={n} value={n}>{n} 条</option>)}</select></label></div></div>
    {pager.error && <div className="notice" role="alert">{pager.error}{page && <span>已显示的记录仍可查看，请重试。</span>}</div>}
    {!pager.loading && (pager.renewal || pager.updateAvailable) && page && <p className="notice" role="status">{pager.updateAvailable ? '有新记录，点击页面顶部“刷新”更新列表。' : '点击页面顶部“刷新”后可继续查看记录。'}</p>}
    {!page ? <PendingStatistics label="会话" columns={['会话 / 最近活跃', '项目 / 模型', '累计消费', '估算费用', '事件 / 已识别回合', '最近上下文', '父会话']} /> : <>
      <section className="group-stat-strip" aria-label="会话统计汇总"><div><p className="metric-label">范围内 Token</p><strong className="group-total" title={fullTokens(page.summary.total_tokens)} aria-label={`${fullTokens(page.summary.total_tokens)} Token`}>{compactTokens(page.summary.total_tokens)}</strong></div><div><p className="metric-label">估算费用</p><Cost pricing={page.pricing} /></div><div><p className="metric-label">范围内消费会话</p><strong className="group-total">{fullTokens(page.summary.session_count)}</strong></div></section>
      {page.sessions.length === 0 ? <section className="panel group-empty"><h2>当前筛选暂无消费会话</h2></section> : <div className="session-table-wrap"><table className="session-table" aria-label="会话用量"><thead><tr><th>会话 / 最近活跃</th><th>项目 / 模型</th><th>累计消费</th><th>估算费用</th><th>事件 / 已识别回合</th><th>最近上下文</th><th>父会话</th></tr></thead><tbody>{page.sessions.map(row => <tr key={row.session_key}><td><button className="text-button session-name" title={row.session_key} onClick={() => setSelected({ row, key: scopeKey })}>{row.display_name}</button><small>{when(row.latest_at_ms, timezone)}</small></td><td><strong>{row.latest_project_name ?? '未知项目'}</strong><small>{row.latest_model ?? '未知模型'}</small></td><td className="numeric" title={fullTokens(row.summary.total_tokens)}>{compactTokens(row.summary.total_tokens)}</td><td className="session-cost"><Cost pricing={row.pricing} /><small>未计价 {compactTokens(row.pricing.unpriced_total_tokens)} Token</small></td><td className="numeric"><span>{fullTokens(row.summary.usage_event_count)} / {fullTokens(row.summary.reliable_turn_count)}</span></td><td title={row.latest_context.context_tokens === null ? '上下文未知' : fullTokens(row.latest_context.context_tokens)}>{compactTokens(row.latest_context.context_tokens)}<small>{row.latest_context.observed_at_ms === null ? '暂无记录' : when(row.latest_context.observed_at_ms, timezone)}</small></td><td><span>{row.parent_display_name ?? row.parent_provider_id ?? '—'}</span><small>{BigInt(row.child_count) > 0n ? `${fullTokens(row.child_count)} 个子会话` : ''}</small></td></tr>)}</tbody></table></div>}
      <div className="session-pagination" aria-label="会话分页"><span>第 {pager.pageNumber} 页{pages === null || pages === 0n ? '' : ` / ${fullTokens(pages.toString())} 页`} · 本页 {page.sessions.length} 条</span><div><button disabled={!pager.hasPrevious || pager.loading} onClick={pager.previous}>上一页</button><button disabled={!pager.hasNext || pager.loading} onClick={pager.next}>下一页</button></div></div>
      {pager.trimmed && <p className="group-footnote">查看更早页面请点击页面顶部“刷新”。</p>}
    </>}
    {selected?.key === scopeKey && page && <SessionDrawer request={{ session_key: selected.row.session_key, filter: request.filter, price_basis: request.price_basis }} displayName={selected.row.display_name} onClose={() => setSelected(null)} onSessionScope={(key, name) => { setSelected(null); onSessionScope(key, name); }} />}
  </>;
}
