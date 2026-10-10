import type { SessionBundleRequest, TurnsPage, TurnsQuery } from '../shared/generated/contracts';
import { compactTokens, fullTokens, formatDuration } from '../shared/format';
import { closeQuerySnapshot, queryTurns } from '../shared/runtime';
import { usePagedUsage, type PageAdapter } from './usePagedUsage';
import { Cost, whenExact } from './usage-display';

const adapter: PageAdapter<TurnsQuery, TurnsPage> = { label: '回合', read: queryTurns, close: request => closeQuerySnapshot({ kind: 'turns', request }), keys: page => page.turns.map(turn => turn.turn_id) };
export function TurnList({ request, refreshRevision }: { request: SessionBundleRequest; refreshRevision: number }) {
  const pager = usePagedUsage({ ...request, page_size: 20 }, refreshRevision, adapter);
  const page = pager.page;
  const timezone = page?.meta.display_timezone ?? request.filter.range.timezone;
  return <div id="session-turn-list" className="session-turn-list">
    <p className="chart-caption">仅统计当前筛选范围内的回合用量。</p>
    <p className="chart-caption">耗时来自整轮任务的完成记录，包含模型调用与工具执行；首 Token 等待为回合开始到首个 Token 的时间。时间不随用量筛选截断，缺少完成记录或时间字段时显示未知。</p>
    {pager.error && <div className="notice" role="alert">{pager.error}</div>}
    <p className="chart-caption">{pager.loading ? '正在读取回合…' : '每页 20 个已识别回合'}</p>
    {!pager.loading && (pager.renewal || pager.updateAvailable) && page && <p className="notice" role="status">{pager.updateAvailable ? '有新记录，点击“刷新详情”更新回合列表。' : '点击“刷新详情”后可继续查看回合。'}</p>}
    {page && <>
      <p className="chart-caption">会话消费 {fullTokens(page.summary.total_tokens)} Token · {fullTokens(page.summary.reliable_turn_count)} 个回合。</p>
      {page.turns.length === 0 ? <p className="muted">当前范围没有已识别回合。</p> : <ol aria-label="已识别回合列表" className="session-turn-cards">{page.turns.map(turn => <li key={turn.turn_id}><header><strong title={turn.turn_id}>{turn.turn_id}</strong><span title={fullTokens(turn.summary.total_tokens)}>{compactTokens(turn.summary.total_tokens)}<small>Token</small></span></header><p className="chart-caption">{whenExact(turn.first_at_ms, timezone)} 至 {whenExact(turn.last_at_ms, timezone)} · {fullTokens(turn.summary.usage_event_count)} 条用量事件</p><dl className="session-turn-timing" aria-label="回合时间"><div><dt>回合耗时</dt><dd title={turn.duration_ms === null ? undefined : `${fullTokens(turn.duration_ms)} 毫秒`}>{formatDuration(turn.duration_ms)}</dd></div><div><dt>首 Token 等待</dt><dd title={turn.time_to_first_token_ms === null ? undefined : `${fullTokens(turn.time_to_first_token_ms)} 毫秒`}>{formatDuration(turn.time_to_first_token_ms)}</dd></div></dl><Cost pricing={turn.pricing} /><p className="chart-caption">未计价 {fullTokens(turn.pricing.unpriced_total_tokens)} Token</p></li>)}</ol>}
      <div className="session-pagination" aria-label="回合分页"><span>第 {pager.pageNumber} 页 · 本页 {page.turns.length} 个</span><div><button disabled={!pager.hasPrevious || pager.loading} onClick={pager.previous}>上一页回合</button><button disabled={!pager.hasNext || pager.loading} onClick={pager.next}>下一页回合</button></div></div>
      {pager.trimmed && <p className="chart-caption">查看更早回合请点击“刷新详情”。</p>}
    </>}
  </div>;
}
