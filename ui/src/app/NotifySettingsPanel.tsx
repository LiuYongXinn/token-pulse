import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { displayPolicy } from '../shared/display-policy';
import { applyNotifyIntegration, getNotifyIntegrations, notifyIssueText, prepareNotifyIntegration, releaseNotifyPreview, retireNotifyIntegration, runtimeError } from '../shared/runtime';
import { useSnapshotQuery } from './useSnapshotQuery';
import type { NotifyConfigPreview, NotifyPrepareAction, SourcesSnapshot } from '../shared/generated/contracts';
import './notify-settings.css';

type Cached<T> = { epoch: number; value: T };
export function NotifySettingsPanel({ sources }: { sources: SourcesSnapshot | null }) {
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const shared = useSnapshotQuery(undefined, 0, getNotifyIntegrations);
  const [draft, setDraft] = useState<Cached<NotifyConfigPreview> | null>(null);
  const [sourceId, setSourceId] = useState('');
  const [mode, setMode] = useState<'preserve' | 'replace'>('preserve');
  const [busy, setBusy] = useState(false), [error, setError] = useState<string | null>(null), [notice, setNotice] = useState<string | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [remaining, setRemaining] = useState(0);
  const life = useRef(0), sequence = useRef(0), working = useRef(false), lease = useRef<string | null>(null), deadline = useRef(0);
  const mounted = useRef(false);
  const snapshot = shared.bundle;
  const preview = draft?.epoch === policy.epoch && policy.privacy === false ? draft.value : null;
  const visible = policy.privacy === false && !policy.pending;
  const current = (generation: number, epoch: number) => life.current === generation && displayPolicy.get().epoch === epoch;
  const clear = () => {
    const id = lease.current; lease.current = null; deadline.current = 0; setDraft(null);
    if (id) void releaseNotifyPreview(id).catch(() => {});
  };
  const reload = async () => {
    const generation = life.current, epoch = displayPolicy.get().epoch, serial = ++sequence.current;
    try { const value = await getNotifyIntegrations(); if (current(generation, epoch) && serial === sequence.current) { shared.accept(value); setReadError(null); } }
    catch (e) { if (current(generation, epoch) && serial === sequence.current) setReadError(runtimeError(e)); }
  };
  useEffect(() => {
    mounted.current = true; ++life.current;  setDraft(null); setError(null); setReadError(null); setNotice(null); setSourceId('');

    const timer = setInterval(() => {
      setRemaining(Math.max(0, Math.ceil((deadline.current - performance.now()) / 1000)));

    }, 2000);
    return () => { mounted.current = false; ++life.current; ++sequence.current; clearInterval(timer); const id = lease.current; lease.current = null; deadline.current = 0; if (id) void releaseNotifyPreview(id).catch(() => {}); };
  }, [policy.epoch]);
  const run = async (action: (generation: number, epoch: number) => Promise<void>) => {
    if (working.current || !visible) return;
    const generation = life.current, epoch = policy.epoch;
    working.current = true; setBusy(true); setError(null); setNotice(null); ++sequence.current;
    try { await action(generation, epoch); } catch (e) { if (current(generation, epoch)) setError(runtimeError(e)); }
    finally { working.current = false; if (mounted.current) setBusy(false); }
  };
  const prepare = (action: NotifyPrepareAction) => run(async (generation, epoch) => {
    clear(); const value = await prepareNotifyIntegration(action);
    if (!value) { if (current(generation, epoch)) setNotice('已取消目录选择。'); return; }
    if (!current(generation, epoch) || value.redacted) { await releaseNotifyPreview(value.plan_id).catch(() => {}); return; }
    lease.current = value.plan_id; deadline.current = performance.now() + value.expires_in_seconds * 1000;
    setRemaining(value.expires_in_seconds); setDraft({ epoch, value });
  });
  const apply = () => run(async (generation, epoch) => {
    if (!preview) return;
    if (performance.now() >= deadline.current) { setRemaining(0); setError('预览已过期，请关闭后重新预览。'); return; }
    const result = await applyNotifyIntegration(preview.plan_id);
    lease.current = null; deadline.current = 0;
    if (current(generation, epoch)) {
      setDraft(null); setNotice(result.configured ? '通知已启用，接入状态正在更新。' : result.cleanup_issue ? `通知已停用；接入记录待清理。${notifyIssueText(result.cleanup_issue)}` : '通知已停用，原通知配置已恢复。');
      await reload();
    }
  });
  const retire = (id: string) => run(async (generation, epoch) => {
    await retireNotifyIntegration(id);
    if (current(generation, epoch)) { setNotice('已清理停用记录。'); await reload(); }
  });
  const choices = sources?.sources.filter(s => !s.removed && s.origin !== 'wsl') ?? [];
  return <section className="notify-settings" aria-label="Codex 通知接入">
    <div className="source-heading"><div><h2>Codex 通知（可选）</h2><p className="muted">回合完成后核对用量日志。</p></div><button disabled={busy} onClick={() => void reload()}>刷新通知状态</button></div>
    {!visible && <p className="notice">隐私模式已隐藏通知路径与配置。关闭后可预览接入或停用。</p>}
    {(error ?? readError) && <p className="notice" role="alert">{error ?? readError}</p>}{notice && <p className="notice" role="status">{notice}</p>}
    <p className="notify-health">{!snapshot ? '正在读取通知状态…' : snapshot.service_issue ? notifyIssueText(snapshot.service_issue) : snapshot.listener_count === null ? '正在检查通知监听…' : snapshot.listener_count === 0 ? '暂无正在监听的通知接入' : `${snapshot.listener_count} 个通知入口监听中`}</p>
    {snapshot?.registry_issue && <p className="notice">{notifyIssueText(snapshot.registry_issue)}</p>}
    {snapshot?.registrations?.map(row => <article className="source-card notify-registration" key={row.registration_id}>
      <h3>{row.configured === true ? '通知已启用' : row.configured === false ? '通知已停用' : '通知状态未知'}</h3>
      <p className="source-path">{visible ? row.home_path ?? '目录无法读取' : '路径已隐藏'}</p>
      {row.current_executable === false && <p className="notice">{notifyIssueText('wrong_executable')}</p>}
      {row.issue && <p className="notice">{notifyIssueText(row.issue)}</p>}
      <p className="muted">{row.chain_original === null ? '原通知处理方式未知' : row.chain_original ? '保留并调用原通知命令' : '使用 TokenPulse 通知；原配置保留供停用时恢复'}</p>
      <div className="source-actions"><button disabled={!visible || busy || row.configured !== true} onClick={() => void prepare({ kind: 'disable', registration_id: row.registration_id })}>预览停用通知</button><button disabled={!visible || busy || row.configured !== false || !!preview} onClick={() => void retire(row.registration_id)}>清理停用记录</button></div>
    </article>)}
    {visible && <div className="notify-setup">
      <label>接入的本地来源<select aria-label="通知接入来源" value={sourceId} disabled={busy || !!preview} onChange={e => setSourceId(e.target.value)}><option value="">选择已登记来源</option>{choices.map(source => <option key={source.source_id} value={source.source_id}>{source.root_path}</option>)}</select></label>
      <label>已有通知命令<select aria-label="原通知处理方式" value={mode} disabled={busy || !!preview} onChange={e => setMode(e.target.value as 'preserve' | 'replace')}><option value="preserve">保留并调用原通知（默认）</option><option value="replace">改用 TokenPulse，停用时恢复原配置</option></select></label>
      <div className="source-actions"><button className="primary" disabled={busy || !!preview || !choices.some(s => s.source_id === sourceId)} onClick={() => void prepare({ kind: 'enable_source', source_id: sourceId, chain_original: mode === 'preserve' ? null : false })}>预览启用通知</button><button disabled={busy || !!preview} onClick={() => void prepare({ kind: 'choose_home', chain_original: mode === 'preserve' ? null : false })}>选择其他 Home 并预览</button></div>
    </div>}
    {preview && <section className="notify-preview" aria-label="通知配置预览">
      <h3>{preview.operation === 'enable' ? '启用通知前核对配置' : '停用通知前核对恢复内容'}</h3>
      <p className="source-path">{preview.home_path}</p>
      <div className="notify-diff"><div><h4>当前 notify</h4><pre>{preview.before_notify ?? '未设置根级 notify'}</pre></div><div><h4>{preview.operation === 'enable' ? '启用后 notify' : '恢复后 notify'}</h4><pre>{preview.after_notify ?? '移除 TokenPulse 添加的根级 notify'}</pre></div></div>
      <p className="muted">{preview.creates_config ? '将创建 config.toml；' : '仅修改此 Home 的根级 notify；'}其他配置保持原样。{preview.operation === 'enable' && (preview.chain_original ? '继续调用原通知命令。' : '原通知配置保留，停用时恢复。')}</p>
      <p className="muted">{remaining > 0 ? `预览将在 ${remaining} 秒后过期；配置变化后需重新预览。` : '预览已过期，请关闭后重新预览。'}</p>
      <div className="source-actions"><button className="primary" disabled={busy || remaining <= 0} onClick={() => void apply()}>{busy ? '正在应用…' : preview.operation === 'enable' ? '确认启用通知' : '确认停用通知'}</button><button disabled={busy} onClick={clear}>关闭预览</button></div>
    </section>}
  </section>;
}
