import { useEffect, useRef, useState } from 'react';
import type { MiniScope, MiniUsageSnapshot } from '../shared/generated/contracts';
import { runtimeError, setMiniScope } from '../shared/runtime';
import { parsePriceInstant, utcPriceInput } from '../shared/price-basis';
import { whenFull } from '../app/usage-display';
import { useMiniSessionOptions } from './useMiniSessionOptions';

export function MiniScopeEditor({ usage, onSaved, onClose }: { usage: MiniUsageSnapshot; onSaved: () => void; onClose: () => void }) {
  // Opening establishes a CAS base. Background refresh never silently rebases an unsaved selection.
  const [base] = useState(() => ({ revision: usage.settings_revision, scope: usage.mini_scope, range: usage.range }));
  const [session, setSession] = useState<string | null>(() => base.scope.kind === 'session' ? base.scope.session_key : null);
  const [name, setName] = useState(() => base.scope.kind === 'session' ? usage.scope_display_name ?? '固定会话' : '');
  const [startMode, setStartMode] = useState<'today' | 'fixed'>(() => base.scope.kind === 'session' && base.scope.start.kind === 'fixed' ? 'fixed' : 'today');
  const [start, setStart] = useState(() => utcPriceInput(base.scope.kind === 'session' && base.scope.start.kind === 'fixed' ? base.scope.start.start_ms : base.range.start_ms));
  const [search, setSearch] = useState(''), [debounced, setDebounced] = useState('');
  const [busy, setBusy] = useState(false), [error, setError] = useState<string | null>(null);
  const validSearch = [...search].length <= 256 && !/[\u0000-\u001f\u007f-\u009f]/.test(search);
  const candidates = useMiniSessionOptions(validSearch ? { search: debounced, page_size: 25 } : null);
  const ready = search === debounced;
  const dialog = useRef<HTMLDivElement>(null), mounted = useRef(false), writing = useRef(false);
  const close = useRef(onClose); close.current = onClose;
  useEffect(() => { const timer = setTimeout(() => setDebounced(search), 250); return () => clearTimeout(timer); }, [search]);
  useEffect(() => { if (error) dialog.current?.querySelector('[role="alert"]')?.scrollIntoView({ block: 'nearest' }); }, [error]);
  useEffect(() => {
    mounted.current = true;
    const previous = document.querySelector<HTMLElement>('button[aria-label="选择小窗会话与起点"]');
    dialog.current?.querySelector<HTMLElement>('button')?.focus();
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !writing.current) { event.preventDefault(); close.current(); }
      if (event.key === 'Tab') {
        const elements = Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),select:not(:disabled)') ?? []);
        const first = elements[0], last = elements.at(-1);
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }
    };
    document.addEventListener('keydown', keyboard);
    return () => { mounted.current = false; document.removeEventListener('keydown', keyboard); queueMicrotask(() => { if (previous?.isConnected) previous.focus(); }); };
  }, []);
  const save = async () => {
    if (writing.current) return;
    let scope: MiniScope = { kind: 'today_all_sources' };
    if (session !== null) {
      const at = startMode === 'fixed' ? parsePriceInstant(start) : null;
      if (startMode === 'fixed' && (at === null || at > Date.now())) { setError('请输入不晚于当前时刻的有效 UTC 起点。'); return; }
      scope = { kind: 'session', session_key: session, start: startMode === 'today' ? { kind: 'today' } : { kind: 'fixed', start_ms: at! } };
    }
    writing.current = true; setBusy(true); setError(null);
    try { await setMiniScope({ mini_scope: scope, expected_settings_revision: base.revision }); if (mounted.current) onSaved(); }
    catch (e) { if (mounted.current) setError(runtimeError(e)); }
    finally { writing.current = false; if (mounted.current) setBusy(false); }
  };
  return <div className="mini-scope-editor" role="dialog" aria-modal="true" aria-labelledby="mini-scope-heading" ref={dialog}>
    <header><h2 id="mini-scope-heading">小窗统计范围</h2><button aria-label="关闭小窗范围选择" disabled={busy} onClick={onClose}>×</button></header>
    <div className="mini-scope-scroll">
      <button className={session === null ? 'mini-option selected' : 'mini-option'} aria-pressed={session === null} disabled={busy} onClick={() => { setSession(null); setName(''); }}>全部来源 · 今日</button>
      <label className="mini-search-label">固定会话<input aria-label="搜索小窗会话" value={search} disabled={busy} autoComplete="off" placeholder="搜索已登记会话" onChange={e => setSearch(e.target.value)} /></label>
      {session !== null && <p className="mini-selected">当前选择：{name}</p>}
      {!validSearch ? <p className="mini-editor-error" role="alert">搜索最多 256 个字符，不能含控制字符。</p> : !ready ? <p className="mini-editor-note">正在准备搜索…</p> : <>
        {candidates.error && <p className="mini-editor-error" role="alert">{candidates.error}</p>}
        <div className="mini-options" role="listbox" aria-label="小窗会话候选">{candidates.options.map(option => <button key={option.session_key} role="option" className={session === option.session_key ? 'mini-option selected' : 'mini-option'} aria-selected={session === option.session_key} disabled={busy} onClick={() => { setSession(option.session_key); setName(option.display_name); }}><span>{option.display_name}</span><small>{option.session_key.slice(0, 12)}</small></button>)}</div>
        {candidates.loading && <p className="mini-editor-note" role="status">正在读取会话…</p>}
        {!candidates.loading && !candidates.error && candidates.options.length === 0 && <p className="mini-editor-note">没有匹配的已登记会话。</p>}
        {candidates.limited && <p className="mini-editor-note">已显示 1,000 项，请缩小搜索。</p>}
        <div className="mini-picker-actions">{candidates.more && <button disabled={busy || candidates.loading} onClick={candidates.loadMore}>加载更多会话</button>}<button disabled={busy || candidates.loading} onClick={candidates.reload}>重新查询会话</button></div>
      </>}
      {session !== null && <div className="mini-start-editor"><label>消耗起点<select aria-label="小窗消耗起点" value={startMode} disabled={busy} onChange={e => setStartMode(e.target.value as 'today' | 'fixed')}><option value="today">今日 00:00（统计时区）</option><option value="fixed">自选固定起点（UTC）</option></select></label>{startMode === 'fixed' && <label>固定起点（UTC）<input type="datetime-local" step="0.001" aria-label="小窗固定起点（UTC）" value={start} disabled={busy} onChange={e => setStart(e.target.value)} /></label>}<p className="mini-editor-note">{startMode === 'today' ? `每天按 ${base.range.timezone} 零点更新。` : '固定起点跨午夜保留。'}</p></div>}
      <p className="mini-editor-note">当前范围起点 {whenFull(base.range.start_ms, base.range.timezone)}</p>
      {error && <p className="mini-editor-error" role="alert">{error} 可取消后重新打开以读取最新范围。</p>}
    </div>
    <div className="mini-editor-footer"><button className="primary" disabled={busy} onClick={() => void save()}>{busy ? '正在保存…' : '应用小窗范围'}</button><button disabled={busy} onClick={onClose}>取消</button></div>
  </div>;
}
