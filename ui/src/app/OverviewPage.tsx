import { useState } from 'react';
import type { CSSProperties } from 'react';
import type { DashboardRequest, Grain, SourcesSnapshot, TokenMeasure, UsageSeriesBucket } from '../shared/generated/contracts';
import { compactTokens, fullTokens, percentage } from '../shared/format';
import { availableGrain, calendarDateLabel } from '../shared/main-filter';
import { useDashboard } from './useDashboard';
import { Cost, reasonNames, when } from './usage-display';
import { AccountQuotaOverview } from '../shared/AccountQuota';
import { PendingOverview } from './PendingStatistics';

function Measure({ label, measure, secondary = false }: { label: string; measure: TokenMeasure; secondary?: boolean }) { return <div className={`measure ${secondary ? 'breakdown-support' : ''}`}><dt>{label}</dt><dd>{fullTokens(measure.value)}</dd></div>; }

function Trend({ buckets }: { buckets: UsageSeriesBucket[] }) {
  const [selected, setSelected] = useState<number | null>(null);
  let maximum = 0n;
  for (const bucket of buckets) { const value = BigInt(bucket.totals.total_tokens); if (value > maximum) maximum = value; }
  const bucket = selected === null ? null : buckets[selected];
  return <><div className="trend-bars" role="group" aria-label="各时段 Token"><div className="trend-grid" aria-hidden="true" />{buckets.map((bucket, index) => {
    const value = BigInt(bucket.totals.total_tokens);
    const height = maximum === 0n ? 0 : Number(value * 10000n / maximum) / 100;
    const parts = [bucket.totals.output_total, bucket.totals.cached_input, bucket.totals.noncached_input];
    const stacked = value > 0n && parts.every(part => part.complete && part.value !== null) && parts.reduce((sum, part) => sum + BigInt(part.value ?? '0'), 0n) === value;
    const label = `${bucket.display_label} ${bucket.utc_offset} · ${fullTokens(bucket.totals.total_tokens)} Token`;
    return <button key={bucket.start_ms} className={`trend-bin ${selected === index ? 'selected' : ''}`} aria-label={label} title={label} onClick={() => setSelected(index)} onFocus={() => setSelected(index)} style={{ '--bar-height': `${height}%` } as CSSProperties}>{stacked ? <span className="trend-stack" aria-hidden="true">{parts.map((part, i) => <i key={i} className={['output', 'cached', 'input'][i]} style={{ height: `${Number(BigInt(part.value!) * 10000n / value) / 100}%` }} />)}</span> : <span />}</button>;
  })}</div><div className="trend-axis"><span>{buckets[0]?.display_label}</span><span>{buckets.at(-1)?.display_label}</span></div><div className="trend-legend" aria-hidden="true"><span><i />非缓存输入</span><span><i className="cached" />缓存输入</span><span><i className="output" />输出</span></div>{(bucket || maximum === 0n) && <p className="chart-caption" aria-live="polite">{bucket ? `${bucket.display_label} ${bucket.utc_offset} · ${fullTokens(bucket.totals.total_tokens)} Token` : maximum === 0n ? '当前范围暂无用量' : null}</p>}</>;
}
function Heatmap({ buckets, timezone, onDay }: { buckets: UsageSeriesBucket[]; timezone: string; onDay: (date: string) => void }) {
  const [selected, setSelected] = useState<number | null>(null);
  let max = 0n; for (const b of buckets) { const n = BigInt(b.totals.total_tokens); if (n > max) max = n; }
  const chosen = selected === null ? null : buckets[selected];
  return <><div className="activity-grid" role="group" aria-label="近 26 周每日活动">{buckets.map((bucket, index) => {
    const amount = BigInt(bucket.totals.total_tokens);
    const level = amount === 0n ? 0 : Number((amount * 3n + max - 1n) / max);
    const label = `${bucket.display_label} · ${fullTokens(bucket.totals.total_tokens)} Token`;
    return <button key={bucket.start_ms} className={`activity-day level-${level}`} title={label} aria-label={label} onClick={() => { setSelected(index); onDay(calendarDateLabel(bucket.start_ms, timezone)); }} onFocus={() => setSelected(index)} onMouseEnter={() => setSelected(index)} />;
  })}</div>{chosen && <p className="chart-caption" aria-live="polite">{`${chosen.display_label} · ${fullTokens(chosen.totals.total_tokens)} Token`}</p>}</>;
}

export function OverviewPage({ request, refreshRevision, sources, accountTimezone, onSources, onSessions, onGrain, onDay, active = true }: { request: DashboardRequest; refreshRevision: number; sources: SourcesSnapshot | null; accountTimezone: string | null; onSources: () => void; onSessions: () => void; onGrain: (grain: Grain) => void; onDay: (date: string) => void; active?: boolean }) {
  const { bundle, error } = useDashboard(request, refreshRevision, active);
  if (!active) return null;
  if (sources?.sources.length === 0) return <div className="overview-grid"><div className="overview-left"><section className="empty panel"><div className="empty-symbol">▥</div><h2>添加 Codex 数据来源</h2><p>选择 Codex Home，导入历史用量。</p><button className="primary" onClick={onSources}>查看数据来源</button></section></div><div className="overview-right"><AccountQuotaOverview timezone={accountTimezone} onSettings={onSources} /></div></div>;
  if (!bundle) return <>{error && <p className="notice" role="alert">{error}</p>}<PendingOverview grainControls={<div className="grain-switch" role="group" aria-label="时间粒度">{([['hour', '小时'], ['day', '日'], ['month', '月']] as const).map(([grain, label]) => <button key={grain} aria-pressed={request.grain === grain} disabled={availableGrain(grain, request.filter.range) !== grain} onClick={() => onGrain(grain)}>{label}</button>)}</div>} recentAction={<button className="text-button" onClick={onSessions}>查看全部</button>}><AccountQuotaOverview timezone={accountTimezone} onSettings={onSources} /></PendingOverview></>;
  const totals = bundle.summary, pricing = bundle.pricing;
  const fullBreakdown = totals.noncached_input.complete && totals.cached_input.complete && totals.output_total.complete;
  const cacheRate = totals.cached_input.complete && totals.input_total.complete ? percentage(totals.cached_input.value, totals.input_total.value) : null;
  const parts = [['非缓存输入', totals.noncached_input, 'input'], ['缓存输入', totals.cached_input, 'cached'], ['输出（含推理）', totals.output_total, 'output']] as const;
  return <>
    {error && <div className="notice" role="alert">{error}<span>显示上次结果 · {when(bundle.meta.generated_at_ms, bundle.meta.display_timezone)}</span></div>}
    <div className="overview-grid"><div className="overview-left">
      <section className="overview-summary panel" data-snapshot-id={bundle.meta.snapshot_id}><div><div className="metric-label">所选范围 Token 总量</div><div className="total-number" title={fullTokens(totals.total_tokens)} aria-label={`${fullTokens(totals.total_tokens)} Token`}>{compactTokens(totals.total_tokens)}<span>Token</span></div><p className="metric-foot">{fullTokens(totals.session_count)} 个会话 · {fullTokens(totals.usage_event_count)} 条用量事件</p></div><div className="overview-cost"><div className="metric-label">估算费用</div><Cost pricing={pricing} /></div></section>
      <section className="panel breakdown-panel"><div className="panel-heading"><h2>Token 分解</h2></div>
        {fullBreakdown && totals.total_tokens !== '0' && <div className="breakdown-bar" aria-label="完整 Token 分解">{parts.map(([label, measure, kind]) => <span className={kind} key={kind} style={{ width: `${percentage(measure.value, totals.total_tokens) ?? 0}%` }} title={`${label} ${fullTokens(measure.value)}`} />)}</div>}
        <dl className="breakdown-measures">{parts.map(([label, measure]) => <Measure key={label} label={label} measure={measure} />)}<Measure label="输入" measure={totals.input_total} secondary /><Measure label="缓存写入（输入包含项）" measure={totals.cache_write_input} secondary /><Measure label="推理输出（包含项）" measure={totals.reasoning_output} secondary /><div className="measure cache-rate"><dt>缓存 / 输入</dt><dd>{cacheRate === null ? '—' : `${cacheRate}%`}</dd></div></dl>
      </section>
      <section className="panel trend-panel"><div className="panel-heading"><h2>用量趋势</h2><div className="grain-switch" role="group" aria-label="时间粒度">{([['hour', '小时'], ['day', '日'], ['month', '月']] as const).map(([grain, label]) => <button key={grain} aria-pressed={request.grain === grain} disabled={availableGrain(grain, request.filter.range) !== grain} title={availableGrain(grain, request.filter.range) !== grain ? '范围较长，请使用日或月粒度' : undefined} onClick={() => onGrain(grain)}>{label}</button>)}</div></div><Trend buckets={bundle.series} /></section>
      <section className="panel activity-panel"><div className="panel-heading"><h2>近期活动</h2><span className="metric-foot">当前维度筛选 · 近 26 周</span></div><Heatmap buckets={bundle.heatmap} timezone={bundle.meta.display_timezone} onDay={onDay} /></section>
    </div><div className="overview-right">
      <section className="panel recent-panel"><div className="panel-heading"><h2>最近会话</h2><button className="text-button" onClick={onSessions}>查看全部</button></div>{bundle.recent_sessions.length === 0 ? <p className="muted">当前范围暂无用量会话。</p> : <div className="recent-list">{bundle.recent_sessions.map(session => <article key={session.session_key}><strong title={session.display_name}>{session.display_name}</strong><p>{session.latest_project_name ?? '未知项目'} · {session.latest_model ?? '未知模型'}</p><div><span title={fullTokens(session.summary.total_tokens)}>{compactTokens(session.summary.total_tokens)} Token</span><time>{when(session.latest_at_ms, bundle.meta.display_timezone)}</time></div></article>)}</div>}</section>
      <section className="panel overview-details"><h2>更多统计</h2><dl><dt>回合</dt><dd>{fullTokens(totals.reliable_turn_count)}</dd><dt>未计价 Token</dt><dd>{fullTokens(pricing.unpriced_total_tokens)}</dd></dl>{pricing.reasons.length > 0 && <ul className="pricing-reasons">{pricing.reasons.map(reason => <li key={reason.code}>{reasonNames[reason.code] ?? reason.code}<span>{fullTokens(reason.total_tokens)} Token</span></li>)}</ul>}</section>
      <AccountQuotaOverview timezone={accountTimezone} onSettings={onSources} />
    </div></div>
  </>;
}
