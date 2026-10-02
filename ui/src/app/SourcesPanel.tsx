import { displayPolicy } from '../shared/display-policy';
import { AccountServicePanel } from './AccountServicePanel';
import { whenFull } from './usage-display';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { chooseSourceDirectory, getSources, manageSource, runtimeError } from '../shared/runtime';
import type { CapabilityState, ManageSourceAction, SourceDirectoryKind, SourceReadability, SourcesSnapshot } from '../shared/generated/contracts';

const readable: Record<SourceReadability, string> = { awaiting_directory: '等待目录出现', readable: '可读取', partially_readable: '部分目录不可读取', unreadable: '读取失败', disabled: '已暂停' };
const capability: Record<CapabilityState, string> = { not_probed: '尚未探测', available: '可用', unavailable: '不可用' };
const origins = { windows_default: 'Windows 本地', environment: 'CODEX_HOME', custom: '自定义目录', wsl: 'WSL' };
function time(value: number | null, timezone: string | null): string { return value === null ? '尚无成功记录' : timezone === null ? '统计时区尚未配置' : whenFull(value, timezone); }

export function SourcesPanel({ onChanged, timezone, diagnostics = false }: { onChanged: () => void; timezone: string | null; diagnostics?: boolean }) {
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const [snapshot, setSnapshot] = useState<SourcesSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const requestSequence = useRef(0);
  const mounted = useRef(true);
  const refresh = async () => {
    const sequence = ++requestSequence.current;
    try { const result = await getSources(); if (mounted.current && sequence === requestSequence.current) { setSnapshot(result); setError(null); } }
    catch (error) { if (mounted.current && sequence === requestSequence.current) setError(runtimeError(error)); }
  };
  useEffect(() => {
    mounted.current = true;
    void refresh();
    const interval = setInterval(() => { if (!busyRef.current) void refresh(); }, 2000);
    return () => { mounted.current = false; clearInterval(interval); };
  }, []);
  const run = async (action: ManageSourceAction) => {
    if (!snapshot || busyRef.current) return;
    busyRef.current = true; setBusy(true); ++requestSequence.current;
    try { const result = await manageSource(action, snapshot.settings_revision); if (mounted.current) { setSnapshot(result); setError(null); onChanged(); } }
    catch (error) { if (mounted.current) setError(runtimeError(error)); }
    finally { busyRef.current = false; if (mounted.current) setBusy(false); }
  };
  const choose = async (kind: SourceDirectoryKind) => {
    if (!snapshot || busyRef.current) return;
    busyRef.current = true; setBusy(true); ++requestSequence.current;
    try {
      const selected = await chooseSourceDirectory(kind);
      if (selected) {
        const result = await manageSource({ kind: 'add', selection_handle: selected.selection_handle }, snapshot.settings_revision);
        if (mounted.current) { setSnapshot(result); setError(null); onChanged(); }
      }
    } catch (error) { if (mounted.current) setError(runtimeError(error)); }
    finally { busyRef.current = false; if (mounted.current) setBusy(false); }
  };
  return <section className="panel source-panel" role={diagnostics ? 'region' : 'tabpanel'} aria-label={diagnostics ? '来源采集状态' : '数据来源设置'}>
    <div className="source-heading"><div><h2>数据来源</h2><p className="muted">仅采集用量与必要元数据；原始日志保持只读。</p></div><button onClick={() => void refresh()} disabled={busy}>刷新来源</button></div>
    {policy.privacy !== false && <p className="notice">隐私模式已隐藏来源路径；关闭后可选择新目录。</p>}
    <div className="source-actions">{!diagnostics && policy.privacy === false && <button className="primary" disabled={busy || !snapshot} onClick={() => void choose('local')}>添加自定义目录</button>}<button disabled={busy || !snapshot} onClick={() => void run({ kind: 'detect' })}>{diagnostics ? '重新检测来源' : '检测 Windows 本地来源'}</button></div>
    {error && <div className="notice" role="alert">{error}{snapshot && <span>保留上次来源状态</span>}</div>}
    {!snapshot && !error && <p className="muted" role="status">正在读取来源配置…</p>}
    {snapshot?.sources.length === 0 && <div className="source-empty"><h3>尚未配置 Codex Home</h3><p>可检测当前 Windows 用户的默认目录与 CODEX_HOME，或选择包含 sessions 的目录。</p></div>}
    <div className="source-list">{snapshot?.sources.map(source => <article className="source-card" key={source.source_id}>
      <div className="source-card-title"><strong>{origins[source.origin]}</strong><span className={`source-state ${source.readability}`}>{source.removed ? '已移除 · 历史保留' : readable[source.readability]}</span></div>
      <p className="source-path">{source.root_path}</p>
      <dl>{!diagnostics && <><dt>文件监听能力</dt><dd>{capability[source.capabilities.watcher]} · {source.enabled ? '定期核对持续开启' : '采集已停止，历史保留'}</dd><dt>物理文件身份</dt><dd>{capability[source.capabilities.physical_identity]}</dd></>}<dt>最近目录核对</dt><dd>{source.last_scan_at_ms === null && diagnostics ? '尚无扫描记录' : time(source.last_scan_at_ms, timezone)}</dd><dt>最近成功采集</dt><dd>{time(source.last_success_at_ms, timezone)}</dd></dl>
      {source.error && <p className="notice" role="status">{runtimeError({ code: source.error })}</p>}
      <div className="source-actions"><button disabled={busy} onClick={() => void run({ kind: source.enabled ? 'pause' : 'resume', source_id: source.source_id })}>{source.enabled ? '暂停采集' : '恢复采集'}</button>{!diagnostics && !source.removed && <button disabled={busy} onClick={() => void run({ kind: 'retain_remove', source_id: source.source_id })}>移除来源并保留历史</button>}</div>
    </article>)}</div>
    {!diagnostics && <AccountServicePanel timezone={timezone} />}
  </section>;
}
