import { useEffect, useState } from 'react';
import { getAppStatus, windowAction } from '../shared/runtime';
import type { AppStatus } from '../shared/runtime';

const pages = [
  ['overview', '总览', '在同一统计快照中查看本地消费'],
  ['models', '模型', '按实际模型查看用量与价格覆盖'],
  ['projects', '项目', '按事件发生时的项目查看用量'],
  ['sessions', '会话', '查看会话消费、上下文与继承关系'],
  ['events', '明细', '复核可信事件与核算依据'],
  ['diagnostics', '采集诊断', '查看来源状态、缺口与恢复作业'],
  ['settings', '设置', '管理数据来源、显示、价格与备份'],
] as const;
type Page = typeof pages[number][0];

export function App() {
  const [page, setPage] = useState<Page>('overview');
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState('数据来源');
  const current = pages.find(p => p[0] === page)!;
  const refresh = async () => {
    setLoading(true);
    try { setStatus(await getAppStatus()); setError(null); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setLoading(false); }
  };
  useEffect(() => { void refresh(); }, []);
  return <div className="workspace">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark">▥</span>TokenPulse</div>
      <nav aria-label="主导航">{pages.map(([id, label]) => <button key={id} className={page === id ? 'active' : ''} aria-current={page === id ? 'page' : undefined} onClick={() => setPage(id)}>{label}</button>)}</nav>
      <div className="sidebar-bottom"><span className="status-dot" />{status ? '尚未配置来源' : '桌面连接未就绪'}<small>本地日志只读 · 账户连接可选</small></div>
    </aside>
    <main>
      <header className="heading"><div><h1>{current[1]}</h1><p>{current[2]}</p></div><div className="head-actions"><button onClick={() => void refresh()} disabled={loading}>刷新</button>{status && <button onClick={() => void windowAction('hide_main').catch(e => setError(String(e)))}>隐藏到托盘</button>}</div></header>
      {!['settings', 'diagnostics'].includes(page) && <div className="filters" aria-label="统一筛选"><select aria-label="日期范围" disabled><option>今日</option></select>{['来源', '模型', '项目', '会话'].map(label => <select key={label} aria-label={label} disabled><option>全部{label}</option></select>)}<span>等待接入统计服务</span></div>}
      {error && <div role="alert" className="notice">{error}<button onClick={() => void refresh()}>重试连接</button></div>}
      {page === 'settings' ? <>
        <div className="tabs" role="tablist" aria-label="设置分类">{['数据来源', '显示与窗口', '任务栏显示', '价格规则', '数据与备份'].map(t => <button role="tab" aria-selected={tab === t} key={t} onClick={() => setTab(t)}>{t}</button>)}</div>
        <section className="panel" role="tabpanel"><h2>{tab}</h2><p className="muted">此模块正在实施，完成后可在此配置。</p>{tab === '数据与备份' && status && <dl><dt>数据目录</dt><dd>{status.data_directory}</dd><dt>运行版本</dt><dd>{status.version}{status.development ? ' · 开发版（数据隔离）' : ''}</dd></dl>}</section>
      </> : page === 'diagnostics' ? <section className="panel"><h2>运行状态</h2><dl><dt>桌面运行壳</dt><dd>{status ? '已连接' : loading ? '正在连接' : '未连接'}</dd><dt>采集服务</dt><dd>尚未配置来源</dd><dt>存储与核算</dt><dd>正在实施</dd><dt>账户额度</dt><dd>未连接</dd><dt>任务栏显示</dt><dd>正在实施 · 默认关闭</dd></dl></section> : <section className="empty panel"><div className="empty-symbol">▥</div><h2>{loading ? '正在连接桌面服务' : '开始记录本地用量'}</h2><p>添加 Codex 数据来源后，可信 Token、费用估算和来源覆盖会显示在这里。</p><p className="support-note">采集与统计服务正在实施。当前版本提供桌面运行壳，不展示演示消费。</p><button className="primary" onClick={() => { setPage('settings'); setTab('数据来源'); }}>查看数据来源</button></section>}
      <footer><span>{status?.development ? '开发版 · 独立数据目录' : 'TokenPulse'}</span><span>账户额度与本地消费分别计算</span></footer>
    </main>
  </div>;
}
