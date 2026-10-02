import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { displayPolicy } from '../shared/display-policy';
import { cancelAccountServiceSelection, chooseAccountService, getAccountQuota, getAccountServiceConfig, manageAccountConnection, onAccountQuotaChanged, onSettingsChanged, refreshAccountQuota, runtimeError, saveAccountServiceConfig } from '../shared/runtime';
import type { AccountConnectionRequest, AccountServiceConfigSnapshot, AccountServiceSelection, AccountServiceSelectionKind, QuotaSnapshot, QuotaState } from '../shared/generated/contracts';

const states: Record<QuotaState, string> = { disconnected: '未连接', connecting: '正在读取本地账户', authorization_required: '本地登录态不可用', unsupported: '当前连接不提供账户额度', ready: '额度已更新', stale: '旧额度快照', error: '额度读取失败' };
type Cached<T> = { epoch: number; value: T };
type Draft = { selection: AccountServiceSelection; auto: boolean };

function date(value: number | null, timezone: string | null): string {
  if (value === null) return '—';
  if (!timezone) return '等待设置显示时区';
  try { return new Intl.DateTimeFormat('zh-CN', { timeZone: timezone, dateStyle: 'medium', timeStyle: 'short' }).format(value); }
  catch { return '显示时区无效'; }
}

export function AccountServicePanel({ timezone }: { timezone: string | null }) {
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const [configCache, setConfig] = useState<Cached<AccountServiceConfigSnapshot> | null>(null);
  const [quotaCache, setQuota] = useState<Cached<QuotaSnapshot> | null>(null);
  const [draftCache, setDraft] = useState<Cached<Draft> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [clock, setClock] = useState(Date.now());
  const life = useRef(0), configSequence = useRef(0), quotaSequence = useRef(0), busyRef = useRef(false);
  const lease = useRef<string | null>(null);
  const config = configCache?.epoch === policy.epoch ? configCache.value : null;
  const quota = quotaCache?.epoch === policy.epoch ? quotaCache.value : null;
  const draft = draftCache?.epoch === policy.epoch ? draftCache.value : null;
  const preview = draft?.selection.preview ?? config;
  const unhidden = policy.privacy === false;
  const current = (generation: number, epoch: number) => life.current === generation && displayPolicy.get().epoch === epoch;
  const readConfig = async () => {
    const generation = life.current, epoch = displayPolicy.get().epoch, sequence = ++configSequence.current;
    try { const value = await getAccountServiceConfig(); if (current(generation, epoch) && sequence === configSequence.current) setConfig({ value, epoch }); }
    catch (e) { if (current(generation, epoch) && sequence === configSequence.current) setError(runtimeError(e)); }
  };
  const readQuota = async () => {
    const generation = life.current, epoch = displayPolicy.get().epoch, sequence = ++quotaSequence.current;
    try { const value = await getAccountQuota(); if (current(generation, epoch) && sequence === quotaSequence.current) setQuota({ value, epoch }); }
    catch (e) { if (current(generation, epoch) && sequence === quotaSequence.current) setError(runtimeError(e)); }
  };
  useEffect(() => {
    ++life.current;
    let alive = true;
    const stops: (() => void)[] = [];
    setError(null); setNotice(null);
    void Promise.all([
      () => onSettingsChanged(() => { if (alive) void readConfig(); }),
      () => onAccountQuotaChanged(() => { if (alive) void readQuota(); }),
    ].map(subscribe => subscribe().then(stop => { if (alive) stops.push(stop); else stop(); }).catch(e => { if (alive) setError(runtimeError(e)); })))
      .then(() => { if (alive) { void readConfig(); void readQuota(); } });
    const timer = setInterval(() => { setClock(Date.now()); if (!busyRef.current) void readQuota(); }, 10_000);
    return () => {
      alive = false; ++life.current; ++configSequence.current; ++quotaSequence.current;
      clearInterval(timer); stops.forEach(stop => stop());
      if (lease.current) { void cancelAccountServiceSelection(lease.current).catch(() => {}); lease.current = null; }
    };
    // Every privacy epoch discards retained private views and native selection capabilities.
  }, [policy.epoch]);

  const run = async (action: (generation: number, epoch: number) => Promise<void>) => {
    if (busyRef.current) return;
    const generation = life.current, epoch = displayPolicy.get().epoch;
    busyRef.current = true; setBusy(true); setError(null); setNotice(null);
    try { await action(generation, epoch); }
    catch (e) { if (current(generation, epoch)) setError(runtimeError(e)); }
    finally { busyRef.current = false; setBusy(false); }
  };
  const choose = (kind: AccountServiceSelectionKind, auto?: boolean) => run(async (generation, epoch) => {
    if (!config || !unhidden) return;
    const selected = await chooseAccountService({ kind, base_selection_handle: draft?.selection.selection_handle ?? null, expected_settings_revision: draft?.selection.preview.settings_revision ?? config.settings_revision }).catch(error => {
      if (kind === 'detect_local' && typeof error === 'object' && error !== null && 'code' in error && error.code === 'QUOTA_SERVICE_UNAVAILABLE') throw new Error('未检测到可用的本地原生 Codex 程序或现有 Home。请手动选择程序和已登录的 Home。');
      throw error;
    });
    if (!selected) return;
    if (!current(generation, epoch)) { await cancelAccountServiceSelection(selected.selection_handle).catch(() => {}); return; }
    lease.current = selected.selection_handle;
    setDraft({ epoch, value: { selection: selected, auto: auto ?? draft?.auto ?? config.auto_connect } });
    if (kind === 'detect_local') setNotice('已检测到本地程序，请核对程序和 Home 后保存。检测不会建立账户连接。');
  });
  const discard = async () => {
    const handle = lease.current; lease.current = null; setDraft(null);
    if (handle) await cancelAccountServiceSelection(handle).catch(() => {});
  };
  const save = () => run(async (generation, epoch) => {
    if (!draft || !unhidden) return;
    const value = await saveAccountServiceConfig({ selection_handle: draft.selection.selection_handle, auto_connect: draft.auto, expected_settings_revision: draft.selection.preview.settings_revision });
    lease.current = null;
    if (current(generation, epoch)) { ++configSequence.current; setConfig({ value, epoch }); setDraft(null); setNotice('连接配置已保存；用于下次连接或启动，当前连接保持原状。'); }
  });
  const connection = (request: AccountConnectionRequest) => run(async (generation, epoch) => {
    ++quotaSequence.current;
    const value = await manageAccountConnection(request);
    if (current(generation, epoch)) { ++quotaSequence.current; setQuota({ value, epoch }); }
  });
  const refresh = () => run(async (generation, epoch) => {
    ++quotaSequence.current;
    const result = await refreshAccountQuota();
    if (current(generation, epoch)) {
      ++quotaSequence.current; setQuota({ value: result.quota, epoch });
      setNotice(result.status === 'started' ? '已提交额度读取，等待账户服务响应。' : result.status === 'in_flight' ? '额度读取正在进行。' : result.status === 'rate_limited' ? `请在 ${result.retry_after_ms === null ? '稍后' : `${Math.ceil(result.retry_after_ms / 1000)} 秒后`}刷新额度。` : '额度尚未到刷新时间。');
    }
  });
  const selectable = unhidden && !busy && !!config;
  return <section className="account-service" aria-label="账户额度连接">
    <div className="source-heading"><div><h2>账户额度（可选）</h2><p className="muted">账户额度与本地 Token、费用估算分别计算，不随会话或日期筛选变化。</p></div><button disabled={busy} onClick={() => { setError(null); void readConfig(); void readQuota(); }}>刷新连接状态</button></div>
    {!unhidden && <p className="notice">隐私模式已隐藏账户服务路径和额度。关闭后可选择程序或建立连接。</p>}
    {error && <p role="alert" className="notice">{error}</p>}
    {notice && <p role="status" className="notice">{notice}</p>}
    <article className="source-card">
      <h3>{draft ? '待保存的连接配置' : '已保存的连接配置'}</h3>
      <p className="muted">选择本机原生 codex.exe 和已登录的 Codex Home，复用现有登录状态读取额度，无需在 TokenPulse 重新登录。选择和保存不会启动服务；点击连接后读取。</p>
      {!preview ? <p className="muted">尚未读取连接配置</p> : !preview.configured ? <p className="muted">尚未配置账户服务</p> : <dl><dt>服务程序</dt><dd className="source-path">{unhidden ? preview.executable_display_path ?? '—' : '已隐藏'}</dd><dt>Codex Home</dt><dd className="source-path">{unhidden ? preview.home_display_path ?? '账户服务默认目录' : '已隐藏'}</dd><dt>程序指纹（SHA-256）</dt><dd className="source-path">{unhidden ? preview.executable_sha256 ?? '—' : '已隐藏'}</dd></dl>}
      <div className="source-actions"><button className="primary" disabled={!selectable} onClick={() => void choose('detect_local')}>检测本地 Codex</button><button disabled={!selectable} onClick={() => void choose('executable')}>选择账户服务程序</button><button disabled={!selectable || !preview?.configured} onClick={() => void choose('home')}>选择账户服务 Home</button><button disabled={!selectable || !preview?.configured} onClick={() => void choose('default_home')}>使用服务默认 Home</button></div>
      <label className="account-auto"><input type="checkbox" checked={draft?.auto ?? config?.auto_connect ?? false} disabled={!selectable || !preview?.configured} onChange={e => { const auto = e.target.checked; if (draft) setDraft({ epoch: policy.epoch, value: { ...draft, auto } }); else void choose('current', auto); }} />启动 TokenPulse 时自动连接此服务</label>
      <div className="source-actions"><button className="primary" disabled={busy || !unhidden || !draft} onClick={() => void save()}>保存账户连接配置</button><button disabled={busy || !draft} onClick={() => void discard()}>放弃账户配置草稿</button></div>
    </article>
    <article className="source-card">
      <h3>当前连接账户</h3><p role="status">{quota ? states[quota.state] : '尚未读取连接状态'}</p>
      {quota?.state === 'authorization_required' && <p className="muted">所选 Codex Home 没有可用的 ChatGPT 登录状态。请选择本地已登录账户使用的 Home，再重新连接；本地用量统计继续可用。</p>}
      {quota?.state === 'stale' && <p className="notice">保留上次成功额度。以下读数可能已过期，等待服务更新。</p>}
      {quota?.error_code && <p className="muted">服务状态：{quota.error_code}</p>}
      <div className="source-actions"><button className="primary" disabled={busy || !unhidden || !!draft || !config?.configured || !config.executable_sha256 || !quota || quota.state === 'connecting'} onClick={() => { if (config?.executable_sha256 && quota) void connection({ kind: 'connect', expected_settings_revision: config.settings_revision, expected_connection_epoch: quota.connection_epoch, acknowledged_executable_sha256: config.executable_sha256 }); }}>连接已保存服务</button><button disabled={busy || !quota || quota.state === 'disconnected'} onClick={() => { if (quota) void connection({ kind: 'disconnect', expected_connection_epoch: quota.connection_epoch }); }}>断开本次连接</button><button disabled={busy || !unhidden || !quota || !['ready', 'stale', 'error'].includes(quota.state)} onClick={() => void refresh()}>刷新账户额度</button></div>
      <p className="muted">断开本次连接保留已保存的启动连接偏好。更换程序或 Home 后，请点击连接已保存服务。</p>
      {unhidden && quota && <>
        {quota.available_limits.length > 0 && <label>额度桶 <select aria-label="账户额度桶" value={quota.selected_limit_id ?? ''} disabled={busy} onChange={e => void connection({ kind: 'select_limit', expected_connection_epoch: quota.connection_epoch, expected_quota_revision: quota.quota_revision, limit_id: e.target.value })}><option value="" disabled>请选择额度桶</option>{quota.available_limits.map(limit => <option key={limit.limit_id} value={limit.limit_id}>{limit.display_name ?? limit.limit_id}</option>)}</select></label>}
        <p className="muted">最近成功读取：{date(quota.fetched_at_ms, timezone)}</p>
        <div className="account-windows">{quota.windows.map(window => <div key={window.window_id}><strong>{window.duration_mins === 10080 ? '周额度' : window.duration_mins === null ? '未知周期' : `${window.duration_mins} 分钟周期`}</strong><span>剩余 {window.remaining_percent === null ? '—' : `${window.remaining_percent.toLocaleString('zh-CN', { maximumFractionDigits: 2 })}%`}</span><small>重置：{window.resets_at_ms !== null && window.resets_at_ms <= Math.max(clock, Date.now()) ? '等待额度更新' : date(window.resets_at_ms, timezone)}</small></div>)}</div>
      </>}
    </article>
  </section>;
}
