import type { SessionBundleRequest, TurnsPage, TurnsQuery } from '../shared/generated/contracts';
import { compactTokens, fullTokens } from '../shared/format';
import { closeQuerySnapshot, queryTurns } from '../shared/runtime';
import { usePagedUsage, type PageAdapter } from './usePagedUsage';
import { Cost, whenExact } from './usage-display';

const adapter: PageAdapter<TurnsQuery, TurnsPage> = { label: '回合', read: queryTurns, close: request => closeQuerySnapshot({ kind: 'turns', request }), keys: page => page.turns.map(turn => turn.turn_id) };
export function TurnList({ request }: { request: SessionBundleRequest }) {
  const pager = usePagedUsage({ ...request, page_size: 20 }, 0, adapter);
  const page = pager.page;
  const timezone = page?.meta.display_timezone ?? request.filter.range.timezone;
  return <div id="session-turn-list" className="session-turn-list">
    <p className="chart-caption">按明确回合标识聚合，仅包含当前日期、来源、模型和项目筛选中的事件；不表示每个回合的全部历史消耗。</p>
    {pager.error && <div className="notice" role="alert">{pager.error}</div>}
    <div className="session-detail-refresh"><span>{pager.loading ? '正在读取回合快照…' : '每页 20 个已识别回合'}</span><button disabled={pager.loading} onClick={pager.reload}>重新读取回合</button></div>
    {page && <>
      <p className="chart-caption">此回合列表快照：所选会话消费 {fullTokens(page.summary.total_tokens)} Token；已识别回合 {fullTokens(page.summary.reliable_turn_count)}，未识别回合的用量事件 {fullTokens(page.unidentified_usage_event_count)} 条。{!page.summary.reliable_turns_complete && '回合识别不完整。'}</p>
      {page.turns.length === 0 ? <p className="muted">当前范围没有已识别回合；缺少回合标识的消费仍保留在范围汇总中。</p> : <ol aria-label="已识别回合列表" className="session-turn-cards">{page.turns.map(turn => <li key={turn.turn_id}><header><strong title={turn.turn_id}>{turn.turn_id}</strong><span title={fullTokens(turn.summary.total_tokens)}>{compactTokens(turn.summary.total_tokens)}<small>Token</small></span></header><p className="chart-caption">{whenExact(turn.first_at_ms, timezone)} 至 {whenExact(turn.last_at_ms, timezone)} · {fullTokens(turn.summary.usage_event_count)} 条用量事件</p><Cost pricing={turn.pricing} /><p className="chart-caption">未计价 {fullTokens(turn.pricing.unpriced_total_tokens)} Token</p></li>)}</ol>}
      <div className="session-pagination" aria-label="回合分页"><span>第 {pager.pageNumber} 页 · 本页 {page.turns.length} 个</span><div><button disabled={!pager.hasPrevious || pager.loading} onClick={pager.previous}>上一页回合</button><button disabled={!pager.hasNext || pager.loading} onClick={pager.next}>下一页回合</button></div></div>
      {pager.trimmed && <p className="chart-caption">仅缓存最近 10 页，更早回合请重新读取。</p>}
      <p className="chart-caption">回合分页固定数据 {page.meta.data_revision} / 价格 {page.meta.price_revision}，与详情快照独立。租约过期后请重新读取。</p>
    </>}
  </div>;
}
