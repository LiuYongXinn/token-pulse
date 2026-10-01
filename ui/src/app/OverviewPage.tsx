import { useState } from 'react';
import type { CSSProperties } from 'react';
import type { DashboardRequest, Grain, PricingSummary, SourcesSnapshot, TokenMeasure, UsageSeriesBucket } from '../shared/generated/contracts';
import { compactTokens, fullTokens, money, percentage } from '../shared/format';
import { useDashboard } from './useDashboard';

const coverageNames = { complete: '已配置来源覆盖完整', partial: '存在采集或解释缺口', unknown: '来源覆盖尚未确认' };
const reasonNames: Record<string, string> = { unknown_model: '模型或提供方未知', missing_rule: '无匹配价格', ambiguous_rule: '价格规则存在歧义', insufficient_usage: '必要分项不足', overflow: '精确计算溢出' };
function when(time: number, timezone: string) { return new Intl.DateTimeFormat('zh-CN', { timeZone: timezone, month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }).format(time); }
function Cost({ pricing }: { pricing: PricingSummary }) {
  if (pricing.redacted) return <strong className="cost-number">已隐藏</strong>;
  if (!pricing.currencies.length) return <strong className="cost-number unavailable">未计价</strong>;
  return <div className="currency-list">{pricing.currencies.map(currency => <strong className="cost-number" key={currency.currency} title={currency.estimated_cost === null ? '金额未知' : `${currency.currency} ${currency.estimated_cost}（精确估算）`}>{currency.estimated_cost === null ? '—' : `${pricing.currencies.length === 1 && currency.currency === 'USD' ? '$' : `${currency.currency} `}${money(currency.estimated_cost)}`}</strong>)}</div>;
}
function Measure({ label, measure }: { label: string; measure: TokenMeasure }) { return <div className="measure"><dt>{label}</dt><dd>{fullTokens(measure.value)}<small>{measure.value === null ? '未知' : measure.complete ? '分项已知' : `部分已知 · 覆盖 ${fullTokens(measure.covered_total_tokens)} Token`}</small></dd></div>; }

function Trend({ buckets }: { buckets: UsageSeriesBucket[] }) {
  const [selected, setSelected] = useState<number | null>(null);
  let maximum = 0n;
  for (const bucket of buckets) { const value = BigInt(bucket.totals.total_tokens); if (value > maximum) maximum = value; }
  const bucket = selected === null ? null : buckets[selected];
  return <><div className="trend-bars" role="group" aria-label="各时间桶可信 Token"><div className="trend-grid" aria-hidden="true" />{buckets.map((bucket, index) => {
    const value = BigInt(bucket.totals.total_tokens);
    const height = maximum === 0n ? 0 : Number(value * 10000n / maximum) / 100;
    const label = `${bucket.display_label} ${bucket.utc_offset} · ${fullTokens(bucket.totals.total_tokens)} Token · ${coverageNames[bucket.coverage.state]}`;
    return <button key={bucket.start_ms} className={`trend-bin ${selected === index ? 'selected' : ''}`} aria-label={label} title={label} onClick={() => setSelected(index)} onFocus={() => setSelected(index)} style={{ '--bar-height': `${height}%` } as CSSProperties}><span /></button>;
  })}</div><div className="trend-axis"><span>{buckets[0]?.display_label}</span><span>{buckets.at(-1)?.display_label}</span></div><p className="chart-caption" aria-live="polite">{bucket ? `${bucket.display_label} ${bucket.utc_offset} · ${fullTokens(bucket.totals.total_tokens)} Token · ${coverageNames[bucket.coverage.state]}` : maximum === 0n ? '当前范围暂无可信消费；来源覆盖以采集结果为准。' : '选择时间桶查看完整整数与覆盖状态。'}</p></>;
}
function Heatmap({ buckets }: { buckets: UsageSeriesBucket[] }) {
  const [selected, setSelected] = useState<number | null>(null);
  let max = 0n; for (const b of buckets) { const n = BigInt(b.totals.total_tokens); if (n > max) max = n; }
  const chosen = selected === null ? null : buckets[selected];
  return <><div className="activity-grid" role="group" aria-label="近 26 周每日活动">{buckets.map((bucket, index) => {
    const amount = BigInt(bucket.totals.total_tokens);
    const level = amount === 0n ? 0 : Number((amount * 3n + max - 1n) / max);
    const label = `${bucket.display_label} · ${fullTokens(bucket.totals.total_tokens)} Token · ${coverageNames[bucket.coverage.state]}`;
    return <button key={bucket.start_ms} className={`activity-day level-${level}`} title={label} aria-label={label} onClick={() => setSelected(index)} onFocus={() => setSelected(index)} onMouseEnter={() => setSelected(index)} />;
  })}</div><p className="chart-caption" aria-live="polite">{chosen ? `${chosen.display_label} · ${fullTokens(chosen.totals.total_tokens)} Token · ${coverageNames[chosen.coverage.state]}` : '零读数保留实际覆盖状态；悬停或聚焦查看完整消费。'}</p></>;
}

export function OverviewPage({ request, refreshRevision, sources, onSources, onPrices, onSessions, onGrain }: { request: DashboardRequest; refreshRevision: number; sources: SourcesSnapshot | null; onSources: () => void; onPrices: () => void; onSessions: () => void; onGrain: (grain: Grain) => void }) {
  const { bundle, error, loading } = useDashboard(request, refreshRevision);
  if (sources?.sources.length === 0) return <section className="empty panel"><div className="empty-symbol">▥</div><h2>添加 Codex 数据来源</h2><p>检测 Windows 本地 Codex Home 或选择自定义目录，开始导入历史用量。</p><p className="support-note">原始日志保持只读；账户额度连接可选。</p><button className="primary" onClick={onSources}>查看数据来源</button></section>;
  if (!bundle) return <section className="empty panel"><h2>{loading ? '正在读取统计快照' : '统计暂不可用'}</h2>{error && <p role="alert">{error}</p>}<p>可信消费、费用估算与覆盖将从同一读取事务返回。</p></section>;
  const totals = bundle.summary, pricing = bundle.pricing;
  const fullBreakdown = totals.noncached_input.complete && totals.cached_input.complete && totals.output_total.complete;
  const cacheRate = totals.cached_input.complete && totals.input_total.complete ? percentage(totals.cached_input.value, totals.input_total.value) : null;
  const priceCoverage = percentage(pricing.priced_total_tokens, totals.total_tokens);
  const parts = [['普通输入', totals.noncached_input, 'input'], ['缓存输入', totals.cached_input, 'cached'], ['完整输出', totals.output_total, 'output']] as const;
  return <>
    {error && <div className="notice" role="alert">{error}<span>保留上次快照 · {when(bundle.meta.generated_at_ms, bundle.meta.display_timezone)}</span></div>}
    <div className={`overview-coverage ${bundle.coverage.state}`}><span>{coverageNames[bundle.coverage.state]} · 待确认观察 {fullTokens(bundle.coverage.pending_observation_count)} · 待读文件 {fullTokens(bundle.coverage.pending_file_count)}</span><span role="status">{loading ? '正在刷新…' : `快照 ${when(bundle.meta.generated_at_ms, bundle.meta.display_timezone)}`}</span></div>
    <div className="overview-grid"><div className="overview-left">
      <section className="overview-summary panel"><div><div className="metric-label">所选范围可信 Token 总量</div><div className="total-number" title={fullTokens(totals.total_tokens)} aria-label={`${fullTokens(totals.total_tokens)} Token`}>{compactTokens(totals.total_tokens)}<span>Token</span></div><p className="metric-foot">{fullTokens(totals.session_count)} 个会话 · {fullTokens(totals.usage_event_count)} 条用量事件</p></div><div className="overview-cost"><div className="metric-label">已计价部分估算</div><Cost pricing={pricing} /><button className="text-button" onClick={onPrices}>覆盖 {priceCoverage === null ? '—' : `${priceCoverage}%`} · 查看依据</button><p className="metric-foot">等价价格估算 · 与账户额度分别计算</p></div></section>
      <section className="panel breakdown-panel"><div className="panel-heading"><h2>Token 分解</h2><span className="metric-foot">缓存包含在输入中 · 推理包含在输出中</span></div>
        {fullBreakdown && totals.total_tokens !== '0' && <div className="breakdown-bar" aria-label="完整 Token 分解">{parts.map(([label, measure, kind]) => <span className={kind} key={kind} style={{ width: `${percentage(measure.value, totals.total_tokens) ?? 0}%` }} title={`${label} ${fullTokens(measure.value)}`} />)}</div>}
        <dl className="breakdown-measures">{parts.map(([label, measure]) => <Measure key={label} label={label} measure={measure} />)}<Measure label="输入总数" measure={totals.input_total} /><Measure label="推理输出（包含项）" measure={totals.reasoning_output} /><div className="measure"><dt>缓存 / 输入</dt><dd>{cacheRate === null ? '—' : `${cacheRate}%`}<small>{cacheRate === null ? '分项覆盖不足或无输入' : '缓存占输入总数'}</small></dd></div></dl>
      </section>
      <section className="panel trend-panel"><div className="panel-heading"><h2>用量趋势</h2><div className="grain-switch" role="group" aria-label="时间粒度">{([['hour', '小时'], ['day', '日'], ['month', '月']] as const).map(([grain, label]) => <button key={grain} aria-pressed={request.grain === grain} onClick={() => onGrain(grain)}>{label}</button>)}</div></div><Trend buckets={bundle.series} /></section>
      <section className="panel activity-panel"><div className="panel-heading"><h2>近期活动</h2><span className="metric-foot">当前维度筛选 · 近 26 周</span></div><Heatmap buckets={bundle.heatmap} /></section>
    </div><div className="overview-right">
      <section className="panel recent-panel"><div className="panel-heading"><h2>最近会话</h2><button className="text-button" onClick={onSessions}>查看全部</button></div>{bundle.recent_sessions.length === 0 ? <p className="muted">当前范围暂无可信消费会话。</p> : <div className="recent-list">{bundle.recent_sessions.map(session => <article key={session.session_key}><strong title={session.display_name}>{session.display_name}</strong><p>{session.latest_project_name ?? '未知项目'} · {session.latest_model ?? '未知模型'}</p><div><span title={fullTokens(session.summary.total_tokens)}>{compactTokens(session.summary.total_tokens)} Token</span><time>{when(session.latest_at_ms, bundle.meta.display_timezone)}</time></div></article>)}</div>}</section>
      <section className="panel overview-details"><h2>统计口径</h2><dl><dt>可靠回合</dt><dd>{fullTokens(totals.reliable_turn_count)}<small>{totals.reliable_turns_complete ? '回合身份已完整识别' : '仅显示已识别回合'}</small></dd><dt>未计价 Token</dt><dd>{fullTokens(pricing.unpriced_total_tokens)}</dd><dt>未归属 Token</dt><dd>{fullTokens(bundle.coverage.unattributed_total_tokens)}</dd><dt>数据 / 价格修订</dt><dd>{bundle.meta.data_revision} / {bundle.meta.price_revision}</dd></dl>{pricing.reasons.length > 0 && <ul className="pricing-reasons">{pricing.reasons.map(reason => <li key={reason.code}>{reasonNames[reason.code] ?? reason.code}<span>{fullTokens(reason.total_tokens)} Token</span></li>)}</ul>}</section>
      <section className="panel quota-overview"><h2>账户额度</h2><p className="muted">尚未连接账户服务</p><p className="chart-caption">剩余百分比与周期重置时间来自可选账户连接，本地消费无法推算。</p></section>
    </div></div>
  </>;
}
