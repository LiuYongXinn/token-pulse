import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import type { DateRange, SessionRow, SnapshotMeta, TokenMeasure } from '../shared/generated/contracts';
import { fullTokens } from '../shared/format';
import { Cost, coverageNames, when } from './usage-display';

function Measure({ label, value }: { label: string; value: TokenMeasure }) { return <div><dt>{label}</dt><dd>{fullTokens(value.value)}<small>{value.value === null ? '未知' : value.complete ? '分项已知' : `部分已知 · 覆盖 ${fullTokens(value.covered_total_tokens)} Token`}</small></dd></div>; }
const qualityNames: Record<string, string> = { confirmed: '已确认', pending: '待确认', inherited: '继承快照', duplicate: '重复证据', unattributed: '未归属', unknown: '未知' };
export function SessionDrawer({ row, meta, range, onClose, onSessionScope }: { row: SessionRow; meta: SnapshotMeta; range: DateRange; onClose: () => void; onSessionScope: (key: string, name: string) => void }) {
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
  const ctx = row.latest_context;
  return createPortal(<div className="session-drawer-backdrop" onPointerDown={event => { if (event.target === event.currentTarget) onClose(); }}><section className="session-drawer" role="dialog" aria-modal="true" aria-labelledby="session-drawer-title" ref={dialog}>
    <header><div><p className="metric-label">会话详情</p><h2 id="session-drawer-title">{row.display_name}</h2><p className="muted">{row.latest_project_name ?? '未知项目'}</p></div><button aria-label="关闭会话详情" onClick={onClose}>×</button></header>
    <section className="session-range"><h3>所选范围累计消耗</h3><p className="chart-caption">{when(range.start_ms, meta.display_timezone)} 至 {when(range.end_ms, meta.display_timezone)}（不含结束时刻）</p><strong className="session-drawer-total" title={fullTokens(row.summary.total_tokens)}>{fullTokens(row.summary.total_tokens)}<small>Token</small></strong><Cost pricing={row.pricing} /><p className="chart-caption">未计价 {fullTokens(row.pricing.unpriced_total_tokens)} Token · {coverageNames[row.coverage.state]}</p><dl className="session-measures"><Measure label="输入总数" value={row.summary.input_total} /><Measure label="缓存输入" value={row.summary.cached_input} /><Measure label="非缓存输入" value={row.summary.noncached_input} /><Measure label="输出总数" value={row.summary.output_total} /><Measure label="其中推理" value={row.summary.reasoning_output} /></dl><p className="chart-caption">缓存包含在输入内，推理包含在输出内。</p></section>
    <section className="session-context"><h3>最近请求上下文</h3><strong title={ctx.context_tokens === null ? '未知' : fullTokens(ctx.context_tokens)}>{fullTokens(ctx.context_tokens)}<small>Token</small></strong><p>{ctx.model_context_window === null ? '窗口容量未知' : `窗口容量 ${fullTokens(ctx.model_context_window)} Token`}</p>{ctx.percentage !== null && <p>占窗口 {ctx.percentage.toLocaleString('zh-CN', { maximumFractionDigits: 2 })}%</p>}<p className="chart-caption">{ctx.observed_at_ms === null ? '尚无可信上下文快照' : `快照 ${when(ctx.observed_at_ms, meta.display_timezone)} · ${qualityNames[ctx.quality] ?? '未知质量'}`}。最近上下文不表示所选日期内累计消费。</p></section>
    <section className="session-relations"><h3>事件与关系</h3><dl><dt>最新事件模型</dt><dd>{row.latest_model ?? '未知模型'}</dd><dt>最新消费时间</dt><dd>{when(row.latest_at_ms, meta.display_timezone)}</dd><dt>用量事件</dt><dd>{fullTokens(row.summary.usage_event_count)}</dd><dt>已识别回合</dt><dd>{fullTokens(row.summary.reliable_turn_count)}<small>{row.summary.reliable_turns_complete ? '回合身份已完整识别' : '不能视为完整用户回合总数'}</small></dd><dt>父会话</dt><dd>{row.parent_key ? <button className="text-button" onClick={() => onSessionScope(row.parent_key!, row.parent_display_name ?? row.parent_key!)}>{row.parent_display_name ?? row.parent_key}</button> : row.parent_provider_id ? `${row.parent_provider_id}（尚未解析）` : '无已知父会话'}</dd><dt>已解析子会话</dt><dd>{fullTokens(row.child_count)}<small>跨日期已登记关系</small></dd></dl><p className="chart-caption">父子关系不代表继承扣除已确认；消费只含已发布可信事件。</p></section>
    <button className="primary session-scope-button" onClick={() => onSessionScope(row.session_key, row.display_name)}>在主窗口筛选此会话</button>
    <p className="chart-caption">来自当前列表同一快照 · 数据 {meta.data_revision} / 价格 {meta.price_revision}</p>
  </section></div>, document.body);
}
