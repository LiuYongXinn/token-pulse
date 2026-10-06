import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { displayPolicy } from '../shared/display-policy';
import { useAppTheme } from '../shared/useAppTheme';
import { compactTokens, fullTokens, money, percentage } from '../shared/format';
import { getDisplaySettings, getMiniUsage, openMiniStats, miniWindowAction, onDisplayPolicyChanged, onMiniInteractionChanged, onPriceRulesChanged, onSettingsChanged, runtimeError, setDisplayPrivacy, setMiniScope } from '../shared/runtime';
import type { DisplaySettingsSnapshot, MiniUsageSnapshot, MiniWindowAction, MiniWindowState } from '../shared/generated/contracts';
import { coverageNames, whenExact } from '../app/usage-display';
import './mini.css';
import { MiniScopeEditor } from './MiniScopeEditor';
import { MiniAccountQuota } from '../shared/AccountQuota';
import { Icon } from '../shared/Icon';

/** Separate WebView; shared Rust scope and policy, independent from all main-window filters. */
export function MiniApp() {
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const [editing, setEditing] = useState(false);
  const [settings, setSettings] = useState<DisplaySettingsSnapshot | null>(null);
  const [cache, setCache] = useState<{ epoch: number; value: MiniUsageSnapshot } | null>(null);
  const usage = cache?.epoch === policy.epoch ? cache.value : null;
  const [interaction, setInteraction] = useState<MiniWindowState>({ expanded: false, pinned: true });
  const [error, setError] = useState<string | null>(null), [busy, setBusy] = useState(false), [details, setDetails] = useState(false);
  const mounted = useRef(false), sequence = useRef(0), interactionSequence = useRef(0), acting = useRef(false);
  useAppTheme(settings?.preferences.theme);
  useEffect(() => { setCache(null); setDetails(false); setEditing(false); setError(null); }, [policy.epoch]);
  const refresh = async () => {
    const serial = ++sequence.current;
    try {
      const display = await getDisplaySettings();
      if (!mounted.current || serial !== sequence.current) return;
      setSettings(old => !old || BigInt(display.settings_revision) >= BigInt(old.settings_revision) ? display : old);
      const data = await getMiniUsage();
      if (!mounted.current || serial !== sequence.current) return;
      // A settings invalidation overtakes prior reads. Never combine scope/name from another revision.
      if (BigInt(data.settings_revision) < BigInt(display.settings_revision)) throw new Error('小窗范围已变化，请重新读取。');
      setCache({ epoch: displayPolicy.get().epoch, value: data }); setError(null);
    } catch (e) { if (mounted.current && serial === sequence.current) setError(runtimeError(e)); }
  };
  useEffect(() => {
    mounted.current = true; void refresh();
    const visible = () => { if (!document.hidden) void refresh(); };
    const timer = setInterval(visible, 10_000);
    document.addEventListener('visibilitychange', visible);
    const stops: (() => void)[] = [];
    let active = true;
    const subscribe = (promise: Promise<() => void>, privacy = false) => {
      void promise.then(stop => { if (active) stops.push(stop); else stop(); }).catch(() => {
        if (active && privacy) { displayPolicy.enable(); displayPolicy.failed('无法订阅隐私同步，已隐藏敏感信息。'); }
        else if (active) setError('更新订阅失败，请手动刷新。');
      });
    };
    const readInteraction = async () => {
      const serial = ++interactionSequence.current;
      try { const value = await miniWindowAction({ kind: 'read' }); if (active && serial === interactionSequence.current) { setInteraction(value); if (!value.expanded) setEditing(false); } }
      catch (e) { if (active && serial === interactionSequence.current) setError(runtimeError(e)); }
    };
    subscribe(onSettingsChanged(() => { void refresh(); void readInteraction(); })); subscribe(onPriceRulesChanged(() => void refresh()));
    subscribe(onMiniInteractionChanged(() => void readInteraction()));
    subscribe(onDisplayPolicyChanged(), true);
    void readInteraction();
    return () => { active = false; mounted.current = false; ++sequence.current; ++interactionSequence.current; clearInterval(timer); document.removeEventListener('visibilitychange', visible); stops.forEach(stop => stop()); };
  }, [policy.epoch, policy.pending]);
  const nativeAction = async (request: MiniWindowAction) => {
    if (acting.current) return;
    acting.current = true; setBusy(true);
    const serial = ++interactionSequence.current;
    try { const value = await miniWindowAction(request); if (mounted.current) { if (serial === interactionSequence.current) { setInteraction(value); if (!value.expanded) setEditing(false); } setError(null); if (request.kind === 'set_expanded' || request.kind === 'set_pinned') await refresh(); } }
    catch (e) { if (mounted.current) setError(runtimeError(e)); }
    finally { acting.current = false; if (mounted.current) setBusy(false); }
  };
  const privacy = async () => {
    if (!settings || acting.current) return;
    acting.current = true; setBusy(true);
    try { const saved = await setDisplayPrivacy({ privacy: policy.privacy !== true, expected_settings_revision: settings.settings_revision }); if (mounted.current) { setSettings(saved); await refresh(); } }
    catch (e) { if (mounted.current) { setError(runtimeError(e)); void refresh(); } }
    finally { acting.current = false; if (mounted.current) setBusy(false); }
  };
  const openStats = async () => {
    if (!usage || acting.current) return;
    acting.current = true; setBusy(true);
    try { await openMiniStats({ expected_settings_revision: usage.settings_revision }); if (mounted.current) setError(null); }
    catch (e) { if (mounted.current) setError(runtimeError(e)); }
    finally { acting.current = false; if (mounted.current) setBusy(false); }
  };
  const resetScope = async () => {
    if (!usage || acting.current) return;
    acting.current = true; setBusy(true);
    // Hide the old scope as soon as a scope change is requested. Keep tokens only on refresh failures.
    const revision = usage.settings_revision; setCache(null); setDetails(false); ++sequence.current;
    try { await setMiniScope({ mini_scope: { kind: 'today_all_sources' }, expected_settings_revision: revision }); if (mounted.current) await refresh(); }
    catch (e) { if (mounted.current) { setError(runtimeError(e)); void refresh(); } }
    finally { acting.current = false; if (mounted.current) setBusy(false); }
  };
  const known = usage !== null && (usage.usage.usage_event_count !== '0' || usage.coverage.state === 'complete');
  const cached = usage?.usage.cached_input, input = usage?.usage.input_total;
  const ratio = cached?.complete && input?.complete ? percentage(cached.value, input.value) : null;
  const scopeName = usage?.mini_scope.kind === 'session' && policy.privacy !== false ? '固定会话（已隐藏）' : usage?.scope_display_name ?? (usage?.mini_scope.kind === 'session' ? '固定会话' : '全部来源 · 今日');
  const currencies = usage?.pricing.currencies ?? [];
  const cost = policy.privacy !== false ? '已隐藏' : !known ? '—' : currencies.length === 0 ? usage?.usage.total_tokens === '0' && usage.coverage.state === 'complete' ? '$0.00' : '未计价' : currencies.length > 1 ? '多币种' : currencies[0].estimated_cost === null ? '—' : `${currencies[0].currency === 'USD' ? '$' : currencies[0].currency + ' '}${money(currencies[0].estimated_cost)}`;
  const parts = usage ? [usage.usage.noncached_input, usage.usage.cached_input, usage.usage.output_total] : [];
  const completeParts = known && usage && BigInt(usage.usage.total_tokens) > 0n && parts.every(part => part.complete && part.value !== null) && parts.reduce((sum, part) => sum + BigInt(part.value ?? '0'), 0n) === BigInt(usage.usage.total_tokens);
  return <section className={`mini-window ${interaction.expanded ? 'expanded' : 'compact'}`} aria-label="TokenPulse 悬浮窗">
    <header className="mini-title"><div className="mini-drag" onPointerDown={e => { if (e.button === 0) void nativeAction({ kind: 'drag' }); }}><span className="brand-mark"><Icon name="pulse" size={14} /></span>TokenPulse</div><div className="mini-title-actions"><button className="icon-button" aria-label="小窗置顶" title="小窗置顶" aria-pressed={interaction.pinned} disabled={busy} onClick={() => void nativeAction({ kind: 'set_pinned', pinned: !interaction.pinned })}><Icon name="pin" size={13} /></button><button className="icon-button" aria-label={interaction.expanded ? '收起小窗' : '展开小窗'} title={interaction.expanded ? '收起小窗' : '展开小窗'} disabled={busy} onClick={() => void nativeAction({ kind: 'set_expanded', expanded: !interaction.expanded })}><Icon name={interaction.expanded ? 'collapse' : 'expand'} size={13} /></button><button className="icon-button" aria-label="隐藏小窗" title="隐藏小窗" disabled={busy} onClick={() => void nativeAction({ kind: 'hide' })}><Icon name="close" size={13} /></button></div></header>
    <div className="mini-display" inert={editing}>
    <div className="mini-metric"><button className="mini-tokens" aria-label="可信 Token 分解" onClick={() => void nativeAction({ kind: 'set_expanded', expanded: true })} title={known && usage ? fullTokens(usage.usage.total_tokens) + ' Token' : '尚无已确认用量'}>{known && usage ? compactTokens(usage.usage.total_tokens) : '—'}</button><button className="mini-cost" aria-label="费用估算详情" disabled={!usage || policy.privacy !== false} onClick={() => { setDetails(value => !value); void nativeAction({ kind: 'set_expanded', expanded: true }); }}><strong>{cost}</strong><small>已计价估算</small></button></div>
    <div className="mini-scope" title={scopeName}>{scopeName} · 可信 Token</div>
    {completeParts && usage && <div className="mini-composition" aria-label="完整小窗 Token 分解">{parts.map((part, i) => <span key={i} className={['input', 'cached', 'output'][i]} style={{ width: `${percentage(part.value, usage.usage.total_tokens) ?? 0}%` }} />)}</div>}
    <div className="mini-meta"><span>输入缓存 {ratio === null ? '—' : `${ratio}%`}</span><button aria-label="刷新小窗" title={usage ? `最近成功统计快照 ${whenExact(usage.meta.generated_at_ms, usage.range.timezone)} · ${usage.range.timezone}；不表示最后消费时间` : '读取真实统计'} onClick={() => void refresh()}>{usage ? new Intl.DateTimeFormat('zh-CN', { timeZone: usage.range.timezone, hour: '2-digit', minute: '2-digit' }).format(usage.meta.generated_at_ms) : '刷新'}</button><button aria-label="打开小窗范围统计" disabled={!usage || busy} onClick={() => void openStats()}>打开统计</button><button aria-label="小窗隐私模式" aria-pressed={policy.privacy === true} disabled={!settings || busy || policy.pending} onClick={() => void privacy()}>{policy.privacy === true ? '隐私开' : '隐私'}</button></div>
    <MiniAccountQuota timezone={settings?.preferences.display_timezone ?? null} onExpand={() => { setEditing(false); void nativeAction({ kind: 'set_expanded', expanded: true }); }} />
    <div className="mini-health" role={error ? 'alert' : 'status'} title={error ?? (usage ? coverageNames[usage.coverage.state] : undefined)}>{error ? usage ? '更新失败 · 保留上次快照，点击时间重试' : '读取失败 · 点击刷新重试' : usage ? known ? coverageNames[usage.coverage.state] : '尚无已确认用量' : '正在读取用量…'}</div>
    {interaction.expanded && <div className="mini-expanded-content">
      {details && policy.privacy === false && usage ? <section className="mini-price-details" aria-label="小窗价格覆盖"><h2>当前范围费用</h2>{currencies.map(currency => <p key={currency.currency}>{currency.currency} {money(currency.estimated_cost, 6)}</p>)}<p>已计价 {compactTokens(usage.pricing.priced_total_tokens)} · 未计价 {compactTokens(usage.pricing.unpriced_total_tokens)} Token</p><small>事件时点价格 · 修订 {usage.meta.price_revision}</small></section> : <dl className="mini-breakdown"><div><dt>非缓存输入</dt><dd>{compactTokens(usage?.usage.noncached_input.value ?? null)}</dd></div><div><dt>缓存输入</dt><dd>{compactTokens(usage?.usage.cached_input.value ?? null)}</dd></div><div><dt>输出（含推理）</dt><dd>{compactTokens(usage?.usage.output_total.value ?? null)}</dd></div></dl>}
      <div className="mini-range"><button aria-label="选择小窗会话与起点" disabled={busy || !usage} onClick={() => setEditing(true)}>选择会话 / 起点</button><span>{usage ? `${whenExact(usage.range.start_ms, usage.range.timezone)} 起 · ${usage.mini_scope.kind === 'session' ? '固定会话' : '全部来源'}` : '等待范围'}</span>{usage?.mini_scope.kind === 'session' && <button disabled={busy} onClick={() => void resetScope()}>返回今日全部</button>}</div>
      <p className="mini-footnote">账户额度独立于本地消费。已知分项保留，未知显示 —。</p>
    </div>}
    </div>
    {editing && interaction.expanded && usage && <MiniScopeEditor key={policy.epoch} usage={usage} onClose={() => setEditing(false)} onSaved={() => { setEditing(false); setCache(null); setDetails(false); ++sequence.current; void refresh(); }} />}
  </section>;
}
