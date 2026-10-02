import { useEffect, useRef, useState } from 'react';
import { getRebuildStatus, startJob, cancelJob, runtimeError } from '../shared/runtime';
import type { Job, JobState } from '../shared/generated/contracts';
import { whenFull } from './usage-display';

const states: Record<JobState, string> = { queued: '等待执行', running: '正在执行', validating: '正在验证', publishing: '正在发布', cancelling: '正在安全取消', succeeded: '已完成', cancelled: '已取消', failed: '失败', interrupted: '已中断' };
const finished = new Set<JobState>(['succeeded', 'cancelled', 'failed', 'interrupted']);
const phases: Record<string, string> = { queued: '等待执行', planning: '正在准备', registering_inputs: '正在准备', replaying_observations: '正在核算', validating: '正在验证', publishing: '正在发布', complete: '完成' };
const full = (n: string) => BigInt(n).toLocaleString('zh-CN');

export function JobsPanel({ timezone }: { timezone: string | null }) {
  const [snapshot, setSnapshot] = useState<{ job: Job | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mounted = useRef(false), serial = useRef(0), writing = useRef(false);
  const pendingRequest = useRef<string | null>(null);
  const job = snapshot?.job ?? null;
  const activeJob = job !== null && !finished.has(job.state);
  const refresh = async () => {
    const revision = ++serial.current;
    try {
      const result = await getRebuildStatus();
      if (mounted.current && revision === serial.current) { setSnapshot({ job: result }); setError(null); }
    } catch (e) { if (mounted.current && revision === serial.current) setError(runtimeError(e)); }
  };
  useEffect(() => {
    mounted.current = true; void refresh();
    const interval = setInterval(() => { if (!writing.current) void refresh(); }, 1000);
    return () => { mounted.current = false; ++serial.current; clearInterval(interval); };
  }, []);
  const run = async (action: () => Promise<void>) => {
    if (writing.current || snapshot === null) return;
    writing.current = true; ++serial.current; setBusy(true); setNotice(null); setActionError(null);
    try { await action(); if (mounted.current) await refresh(); }
    catch (e) { if (mounted.current) setActionError(runtimeError(e)); }
    finally { writing.current = false; if (mounted.current) setBusy(false); }
  };
  const rebuild = () => run(async () => {
    pendingRequest.current ??= crypto.randomUUID();
    const created = await startJob({ kind: 'rebuild', scope: { kind: 'all' }, request_key: pendingRequest.current });
    pendingRequest.current = null;
    if (mounted.current) setNotice(`重建已提交，${states[created.state]}。`);
  });
  const cancel = (id: string) => run(async () => {
    const result = await cancelJob(id);
    if (mounted.current) setNotice(result === 'too_late' ? '重建已进入发布，结果将完整提交。' : result === 'already_finished' ? '重建已结束。' : '已请求安全取消，等待当前批次结束。');
  });
  return <section className="panel jobs-panel" aria-label="基本重建进度">
    <div className="panel-heading"><div><h2>账本重建</h2><p className="muted">重新核算已采集的用量，通过验证后更新统计。执行过程中保留当前结果。</p></div><div className="source-actions">
      <button onClick={() => void refresh()} disabled={busy}>刷新重建状态</button>
      <button className="primary" onClick={() => void rebuild()} disabled={busy || snapshot === null || error !== null || (activeJob && pendingRequest.current === null)}>重建全部账本</button>
    </div></div>
    {(actionError ?? error) && <div className="notice" role="alert">{actionError ?? error}{error && snapshot && '；显示上次读取的状态。'}</div>}
    {notice && <p role="status" className="job-notice">{notice}</p>}
    {snapshot === null ? <p className="muted">{error ? '重建状态暂不可用。' : '正在读取重建状态…'}</p> : job === null ? <p className="muted">尚无重建记录。配置来源并采集历史后，可在此重新核算。</p> :
      <article className="job-card"><div className="job-title"><strong>{activeJob ? '当前重建' : '最近重建结果'}</strong><span className={`job-state ${job.state}`}>{states[job.state]}</span></div>
        <p className="muted">{activeJob ? (phases[job.phase] ?? states[job.state]) : states[job.state]} · {timezone === null ? '统计时区尚未配置' : whenFull(job.updated_at_ms, timezone)}</p>
        <div className="job-metrics"><span>文件 {full(job.processed_files)} / {job.discovery_complete ? full(job.discovered_files) : '发现中'}</span><span>已处理 {full(job.processed_bytes)} 字节</span></div>
        {job.error && <p className="job-error">{runtimeError(job.error)}</p>}
        {job.can_cancel && <div className="job-bottom"><button disabled={busy || error !== null || job.state === 'cancelling'} onClick={() => void cancel(job.job_id)}>{job.state === 'cancelling' ? '等待安全取消' : '取消重建'}</button></div>}
      </article>}
  </section>;
}
