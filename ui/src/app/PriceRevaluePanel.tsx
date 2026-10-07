import { useEffect, useRef, useState } from 'react';
import type { PriceBasis, PriceRevalueRequest, PriceRevalueState, PriceRevalueStatus } from '../shared/generated/contracts';
import { cancelPriceRevalue, getPriceRevalueStatus, onPriceRevalueChanged, runtimeError, startPriceRevalue } from '../shared/runtime';
import { fullTokens, percentage } from '../shared/format';
import { PriceBasisFilter } from './PriceBasisFilter';
import './price-revalue.css';

const states: Record<PriceRevalueState, string> = { queued: '等待重估', running: '正在重估', cancelling: '正在取消', succeeded: '重估完成', cancelled: '已取消重估', failed: '重估失败', interrupted: '重估已中断' };
function revalueError(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'code' in error) {
    if (error.code === 'DB_WRITE_FAILED') return '费用重估失败，原有费用结果已保留，请重试。';
    if (error.code === 'CANDIDATE_OBSOLETE') return '用量已发生变化，原有费用结果已保留，请重新提交。';
  }
  return runtimeError(error);
}
export function PriceRevaluePanel({ revision, historical }: { revision: string; historical: boolean }) {
  const [status, setStatus] = useState<PriceRevalueStatus | null>(null);
  const [readError, setReadError] = useState<string | null>(null), [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null), [busy, setBusy] = useState(false);
  const [basis, setBasis] = useState<PriceBasis>({ mode: 'event_time' });
  const mounted = useRef(false), actionBusy = useRef(false), refresh = useRef<() => void>(() => {});
  const pendingStart = useRef<PriceRevalueRequest | null>(null);
  useEffect(() => {
    let active = true, reading = false, queued = false, stop: (() => void) | undefined;
    mounted.current = true;
    const read = async () => {
      if (!active) return;
      queued = true; if (reading) return; reading = true;
      do {
        queued = false;
        try { const data = await getPriceRevalueStatus(); if (active) { setStatus(data); setReadError(null); } }
        catch (error) { if (active) { setStatus(null); setReadError(revalueError(error)); } }
      } while (active && queued);
      reading = false;
    };
    refresh.current = () => { void read(); };
    void onPriceRevalueChanged(refresh.current).then(unsubscribe => { if (active) stop = unsubscribe; else unsubscribe(); }).catch(() => {});
    void read();
    const timer = window.setInterval(() => { void read(); }, 2000);
    return () => { active = false; mounted.current = false; refresh.current = () => {}; window.clearInterval(timer); stop?.(); };
  }, []);
  const run = async (action: () => Promise<unknown>, message: string) => {
    if (actionBusy.current) return;
    actionBusy.current = true; setBusy(true); setActionError(null); setNotice(null);
    try { await action(); if (mounted.current) setNotice(message); }
    catch (error) { if (mounted.current) setActionError(revalueError(error)); }
    finally { actionBusy.current = false; if (mounted.current) { setBusy(false); refresh.current(); } }
  };
  const job = status?.active_job ?? status?.latest_job;
  const percent = job ? percentage(job.processed_events, job.total_events) : null;
  const stale = status !== null && status.current_price_revision !== revision;
  const start = async () => {
    pendingStart.current ??= { scope: { kind: 'all' }, basis, expected_price_revision: revision, request_key: crypto.randomUUID() };
    await startPriceRevalue(pendingStart.current);
    pendingStart.current = null;
  };
  return <section className="price-revalue" aria-label="后台费用重估">
    <div className="panel-heading"><div><h3>后台费用重估</h3><p className="muted">按所选计价依据重新估算全部用量。</p></div><button disabled={busy} onClick={() => refresh.current()}>刷新重估状态</button></div>
    {readError && <p role="alert" className="notice">重估状态读取失败：{readError}</p>}
    {actionError && <p role="alert" className="notice">{actionError}</p>}
    {notice && <p role="status" className="muted">{notice}</p>}
    {!status && !readError && <p className="muted" role="status">正在读取重估状态…</p>}
    {status && <>
      {job ? <div className="revalue-progress" role="status" aria-live="polite">
        <div className="revalue-progress-heading"><strong>{states[job.state]}</strong></div>
        <p>已处理记录 {fullTokens(job.processed_events)} / {fullTokens(job.total_events)} · 完成会话 {fullTokens(job.completed_ledgers)} / {fullTokens(job.total_ledgers)}</p>
        {percent !== null && <progress aria-label="费用重估进度" value={percent} max={100}>{percent}%</progress>}
        <p className="muted">{job.basis.mode === 'event_time' ? '按每条事件发生时的有效单价' : `指定估价时点：${new Date(job.basis.specified_at_ms).toISOString()}`}</p>
        {job.error && <p className="notice">{revalueError({ code: job.error })}</p>}
        {job.can_cancel && <button disabled={busy || job.state === 'cancelling'} onClick={() => void run(() => cancelPriceRevalue(job.job_id), '已请求取消重估。')}>{job.state === 'cancelling' ? '等待取消完成…' : '取消当前重估'}</button>}
      </div> : <p className="muted">暂无重估任务。</p>}
    </>}
    {historical ? <p className="muted">历史价格版本只读；回到当前版本后可创建重估任务。</p> : <>
      {stale && <p className="notice">当前规则已变化，请刷新当前版本后重估。</p>}
      <div className="revalue-actions"><PriceBasisFilter basis={basis} disabled={busy} onChange={next => { pendingStart.current = null; setBasis(next); }} /><button className="primary" disabled={busy || !status || stale || status.active_job !== null} onClick={() => void run(start, '重估请求已接受。')}>重估全部用量</button></div>
    </>}
  </section>;
}
