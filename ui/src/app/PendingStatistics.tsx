import type { ReactNode } from 'react';

export function PendingStatistics({ label, columns, children }: { label: string; columns: string[]; children?: ReactNode }) {
  return <>
    <div className="overview-coverage"><span>正在加载统计…</span><span role="status"></span></div>
    <section className="group-stat-strip" aria-label={`${label}统计汇总`}>
      {['Token 总量', '估算费用', '记录数量'].map(title => <div key={title}><p className="metric-label">{title}</p><strong className="group-total">—</strong></div>)}
    </section>
    {children}
    <div className="group-table-wrap"><table className="group-table" aria-label={`${label}用量`}><thead><tr>{columns.map(column => <th key={column}>{column}</th>)}</tr></thead><tbody><tr><td colSpan={columns.length}>—</td></tr></tbody></table></div>
    <div className="session-pagination"><span>第 — 页 · 本页 — 条</span><div><button disabled>上一页</button><button disabled>下一页</button></div></div>
  </>;
}
export function PendingOverview({ children, grainControls, recentAction }: { children: ReactNode; grainControls: ReactNode; recentAction: ReactNode }) {
  return <><div className="overview-coverage" role="status">正在加载统计…</div><div className="overview-grid"><div className="overview-left">
    <section className="overview-summary panel"><div><p className="metric-label">所选范围 Token 总量</p><div className="total-number">—<span>Token</span></div><p className="metric-foot">— 个会话 · — 条用量事件</p></div><div className="overview-cost"><p className="metric-label">估算费用</p><strong>—</strong></div></section>
    <section className="panel breakdown-panel"><h2>Token 分解</h2><dl className="breakdown-measures">{['非缓存输入', '缓存输入', '输出（含推理）', '输入', '缓存写入（输入包含项）', '推理输出（包含项）', '缓存 / 输入'].map(title => <div className="measure" key={title}><dt>{title}</dt><dd>—</dd></div>)}</dl></section>
    <section className="panel trend-panel"><div className="panel-heading"><h2>用量趋势</h2>{grainControls}</div><div className="trend-chart" aria-label="用量趋势尚未就绪">—</div></section>
    <section className="panel activity-panel"><div className="panel-heading"><h2>近期活动</h2><span>近 26 周</span></div><div className="activity-grid">—</div></section>
  </div><div className="overview-right"><section className="panel recent-panel"><div className="panel-heading"><h2>最近会话</h2>{recentAction}</div><p>—</p></section><section className="panel overview-details"><h2>统计口径</h2><dl>{['回合', '未计价 Token'].map(title => <div key={title}><dt>{title}</dt><dd>—</dd></div>)}</dl></section>{children}</div></div></>;
}
