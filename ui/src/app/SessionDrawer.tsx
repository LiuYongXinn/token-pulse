import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { SessionBundleRequest, TokenMeasure } from '../shared/generated/contracts';
import { fullTokens } from '../shared/format';
import { getSessionBundle } from '../shared/runtime';
import { useSnapshotQuery } from './useSnapshotQuery';
import { TurnList } from './TurnList';
import { Cost, when } from './usage-display';
import { PinMiniScope } from './PinMiniScope';

function Measure({ label, value }: { label: string; value: TokenMeasure }) { return <div><dt>{label}</dt><dd>{fullTokens(value.value)}</dd></div>; }
const kindNames = { pending: '待确认', inherited: '已证实继承', duplicate: '重复证据', unattributed: '未归属' };
const classificationNames: Record<string, string> = { inherited: '父序列继承已验证', inherited_prefix: '父序列前缀已验证', lineage_pending: '继承边界待确认', verified_mirror: '镜像序列已验证', sequence_identity_conflict: '序列身份冲突', sequence_incomplete: '序列证据不完整', sequence_unproven: '序列尚未证实', zero_usage_excluded: '零增量未计为消费', repeated_snapshot: '累计快照未增加', unchanged_cumulative: '累计未变的上下文刷新', unattributed_anchor: '累计基线缺少归属依据', unattributed_usage: '用量缺少归属依据', ambiguous_usage: '用量存在歧义', invalid_usage: '用量无效', stream_capacity_exceeded: '计数流超出上限', verified_duplicate: '重复已验证', physical_duplicate: '相同物理记录' };
export function SessionDrawer({ request, displayName, onClose, onSessionScope }: { request: SessionBundleRequest; displayName: string; onClose: () => void; onSessionScope: (key: string, name: string) => void }) {
  const [revision, setRevision] = useState(0);
  const [showTurns, setShowTurns] = useState(false);
  const { bundle, error, loading } = useSnapshotQuery(request, revision, getSessionBundle);
  const dialog = useRef<HTMLElement>(null);
  const close = useRef(onClose); close.current = onClose;
  useEffect(() => {
    const original = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const workspace = document.querySelector<HTMLElement>('.workspace'); const priorInert = workspace?.inert ?? false;
    const overflow = document.body.style.overflow; document.body.style.overflow = 'hidden';
    if (workspace) workspace.inert = true;
    dialog.current?.querySelector<HTMLElement>('button')?.focus();
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); close.current(); }
      if (event.key === 'Tab') {
        const buttons = Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled),[tabindex="0"]') ?? []);
        const first = buttons[0], last = buttons.at(-1);
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }
    };
    document.addEventListener('keydown', keyboard);
    return () => { document.removeEventListener('keydown', keyboard); if (workspace) workspace.inert = priorInert; document.body.style.overflow = overflow; if (original?.isConnected) original.focus(); else document.querySelector<HTMLElement>('button[role="combobox"][aria-label="会话"]')?.focus(); };
  }, []);
  const identity = bundle?.identity;
  const ctx = bundle?.latest_context;
  const activity = bundle?.latest_selected_activity;
  const timezone = bundle?.meta.display_timezone ?? request.filter.range.timezone;
  return createPortal(<div className="session-drawer-backdrop" onPointerDown={event => { if (event.target === event.currentTarget) onClose(); }}><section className="session-drawer" role="dialog" aria-modal="true" aria-labelledby="session-drawer-title" ref={dialog}>
    <header><div><p className="metric-label">会话详情</p><h2 id="session-drawer-title">{identity?.display_name ?? displayName}</h2><p className="muted">{activity?.project_display_name ?? '未知项目'}</p></div><button aria-label="关闭会话详情" onClick={onClose}>×</button></header>
    {error && <div className="notice" role="alert">{error}</div>}
    <div className="session-detail-refresh"><span>{loading && !bundle ? '正在读取详情…' : null}</span><button onClick={() => setRevision(value => value + 1)} disabled={loading}>刷新详情</button></div>
    {!bundle || !identity || !ctx ? <p className="chart-caption">{loading ? '正在读取会话详情…' : '详情暂不可用，可重新查询。'}</p> : <>
      <section className="session-range"><h3>所选范围累计消耗</h3><p className="chart-caption">{when(request.filter.range.start_ms, timezone)} 至 {when(request.filter.range.end_ms, timezone)}（不含结束时刻）</p><strong className="session-drawer-total" title={fullTokens(bundle.summary.total_tokens)}>{fullTokens(bundle.summary.total_tokens)}<small>Token</small></strong><Cost pricing={bundle.pricing} /><dl className="session-measures"><Measure label="输入" value={bundle.summary.input_total} /><Measure label="缓存输入" value={bundle.summary.cached_input} /><Measure label="非缓存输入" value={bundle.summary.noncached_input} /><Measure label="输出" value={bundle.summary.output_total} /><Measure label="其中推理" value={bundle.summary.reasoning_output} /><Measure label="缓存写入（输入包含项）" value={bundle.summary.cache_write_input} /></dl></section>
      <section className="session-context"><h3>最近请求上下文</h3><strong title={ctx.context_tokens === null ? '未知' : fullTokens(ctx.context_tokens)}>{fullTokens(ctx.context_tokens)}<small>Token</small></strong><p>{ctx.model_context_window === null ? '窗口容量未知' : `窗口容量 ${fullTokens(ctx.model_context_window)} Token`}</p>{ctx.percentage !== null && <p>占窗口 {ctx.percentage.toLocaleString('zh-CN', { maximumFractionDigits: 2 })}%</p>}<p className="chart-caption">{ctx.observed_at_ms === null ? '暂无上下文数据' : `更新于 ${when(ctx.observed_at_ms, timezone)}`}</p></section>
      <section className="session-relations"><h3>事件与关系</h3><dl><dt>最新事件模型</dt><dd>{activity?.model ?? '未知模型'}</dd><dt>最新消费时间</dt><dd>{activity ? when(activity.occurred_at_ms, timezone) : '所选范围无消费事件'}</dd><dt>用量事件</dt><dd>{fullTokens(bundle.summary.usage_event_count)}</dd><dt>已识别回合</dt><dd>{fullTokens(bundle.summary.reliable_turn_count)}</dd><dt>父会话</dt><dd>{identity.parent_key ? <button className="text-button" onClick={() => onSessionScope(identity.parent_key!, identity.parent_display_name ?? identity.parent_key!)}>{identity.parent_display_name ?? identity.parent_key}</button> : identity.parent_provider_id ? `${identity.parent_provider_id}（尚未解析）` : '无已知父会话'}</dd><dt>已解析子会话</dt><dd>{fullTokens(bundle.child_count)}</dd></dl>{bundle.children.length > 0 && <ul className="session-children" aria-label="已解析子会话">{bundle.children.map(child => <li key={child.session_key}><button className="text-button" title={child.session_key} onClick={() => onSessionScope(child.session_key, child.display_name)}>{child.display_name}</button></li>)}</ul>}{bundle.children_truncated && <p className="chart-caption">显示前 {bundle.children.length} 个子关系，共 {fullTokens(bundle.child_count)} 个。</p>}</section>
      <details className="session-classifications"><summary>高级采集信息</summary><p className="chart-caption">各类观察可重叠，数量不可相加。</p>{bundle.classifications.length === 0 ? <p className="muted">当前账本没有已保存的分类证据。</p> : <ul>{bundle.classifications.map(item => <li key={`${item.kind}:${item.reason_code}`}><div><strong>{kindNames[item.kind]}</strong><small title={item.reason_code}>{classificationNames[item.reason_code] ?? item.reason_code}</small></div><span>{fullTokens(item.observation_count)}<small>条记录</small></span></li>)}</ul>}</details>
      <section className="session-turns"><div className="session-turn-heading"><h3>回合</h3><button aria-expanded={showTurns} aria-controls="session-turn-list" onClick={() => setShowTurns(value => !value)}>{showTurns ? '收起回合列表' : '查看回合'}</button></div>{showTurns && <TurnList request={request} />}</section>
      <button className="primary session-scope-button" onClick={() => onSessionScope(identity.session_key, identity.display_name)}>在主窗口筛选此会话</button>
      <PinMiniScope key={`${identity.session_key}:${request.filter.range.start_ms}`} sessionKey={identity.session_key} startMs={request.filter.range.start_ms} />
    </>}
  </section></div>, document.body);
}
