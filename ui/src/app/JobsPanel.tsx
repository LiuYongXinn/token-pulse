import { useEffect, useRef, useState } from 'react';
import { listJobs, startJob, cancelJob, runtimeError } from '../shared/runtime';
import type { Job, JobState } from '../shared/generated/contracts';
const states:Record<JobState,string>={queued:'等待执行',running:'正在执行',validating:'正在验证',publishing:'正在发布',cancelling:'正在安全取消',succeeded:'已完成',cancelled:'已取消',failed:'失败',interrupted:'已中断'};
const kinds:Record<Job['kind'],string>={rebuild:'重建账本',import:'历史导入',reconcile:'核对来源',export:'导出',backup:'备份',restore:'恢复',price_revalue:'重新计价',clear:'清除数据'};
const phases:Record<string,string>={queued:'排队',planning:'规划依赖',replaying_observations:'重放必要观察',validating:'检查候选',publishing:'切换账本',complete:'完成'};
const full=(n:string)=>BigInt(n).toLocaleString('zh-CN');
export function JobsPanel() {
  const [jobs,setJobs]=useState<Job[]|null>(null);const [error,setError]=useState<string|null>(null);const [actionError,setActionError]=useState<string|null>(null);const [notice,setNotice]=useState<string|null>(null);const [busy,setBusy]=useState(false);
  const mounted=useRef(false);const serial=useRef(0);const pendingRequest=useRef<string|null>(null);
  const refresh=async()=>{const revision=++serial.current;try{const result=await listJobs();if(mounted.current && revision===serial.current){setJobs(result);setError(null);}}catch(e){if(mounted.current && revision===serial.current)setError(runtimeError(e));}};
  useEffect(()=>{mounted.current=true;void refresh();return()=>{mounted.current=false;serial.current++;};},[]);
  useEffect(()=>{const interval=setInterval(()=>{if(!busy)void refresh();},1000);return()=>clearInterval(interval);},[busy]);
  const run=async(action:()=>Promise<void>)=>{if(busy || jobs===null)return;setBusy(true);setNotice(null);setActionError(null);try{await action();if(mounted.current)await refresh();}catch(e){if(mounted.current)setActionError(runtimeError(e));}finally{if(mounted.current)setBusy(false);}};
  const rebuild=()=>run(async()=>{pendingRequest.current??=crypto.randomUUID();const created=await startJob({kind:'rebuild',scope:{kind:'all'},request_key:pendingRequest.current});pendingRequest.current=null;if(mounted.current)setNotice(`重建已提交，${states[created.state]}。`);});
  const cancel=(id:string)=>run(async()=>{const result=await cancelJob(id);if(mounted.current)setNotice(result==='too_late'?'作业已进入发布，结果将完整提交。':result==='already_finished'?'作业已结束。':'已请求安全取消，等待当前批次结束。');});
  return <section className="panel jobs-panel"><div className="panel-heading"><div><h2>恢复作业</h2><p className="muted">重放已采集的必要观察，通过验证后切换账本。执行过程中保留当前统计。</p></div><div className="source-actions"><button onClick={()=>void refresh()} disabled={busy}>刷新作业</button><button className="primary" onClick={()=>void rebuild()} disabled={busy || jobs===null}>重建全部账本</button></div></div>
    {(actionError??error) && <div className="notice" role="alert">{actionError??error}</div>}{notice && <p role="status" className="job-notice">{notice}</p>}
    {jobs===null?<p className="muted">{error?'作业状态暂不可用。':'正在读取作业状态…'}</p>:jobs.length===0?<p className="muted">暂无作业。配置来源并采集历史后，可在此进行重建。</p>:<div className="job-list">{jobs.map(job=><article key={job.job_id} className="job-card"><div className="job-title"><strong>{kinds[job.kind]}</strong><span className={`job-state ${job.state}`}>{states[job.state]}</span></div><p className="muted">{phases[job.phase]??job.phase} · {new Date(job.updated_at_ms).toLocaleString('zh-CN')}</p><div className="job-metrics"><span>文件 {full(job.processed_files)} / {job.discovery_complete?full(job.discovered_files):'发现中'}</span><span>已处理 {full(job.processed_bytes)} 字节</span><span>可信事件 {full(job.accepted_events)}</span><span>待确认观察 {full(job.pending_observations)}</span></div>{job.error && <p className="job-error">{runtimeError(job.error)}</p>}<div className="job-bottom"><small>作业 {job.job_id}</small>{job.can_cancel && <button disabled={busy || job.state==='cancelling'} onClick={()=>void cancel(job.job_id)}>{job.state==='cancelling'?'等待安全取消':'取消作业'}</button>}</div></article>)}</div>}
  </section>;
}
