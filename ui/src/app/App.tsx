import { useEffect, useMemo, useRef, useState } from 'react';
import { getAppStatus, getSources, runtimeError, windowAction } from '../shared/runtime';
import type { AppStatus } from '../shared/runtime';
import { SourcesPanel } from './SourcesPanel';
import { JobsPanel } from './JobsPanel';
import { PriceRulesPanel } from './PriceRulesPanel';
import { GroupedPage } from './GroupedPage';
import { OverviewPage } from './OverviewPage';
import { AdvancedFilters, type FilterChoices } from './AdvancedFilters';
import { mainDayIdentity, mainRequest } from '../shared/main-filter';
import type { DatePreset } from '../shared/main-filter';
import type { Grain, SourcesSnapshot } from '../shared/generated/contracts';

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
  const [sources, setSources] = useState<SourcesSnapshot | null>(null);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [preset, setPreset] = useState<DatePreset>('today');
  const [source, setSource] = useState<string | null>(null);
  const [choices, setChoices] = useState<FilterChoices>({ models: null, projects: null, sessions: null });
  const [grain, setGrain] = useState<Grain>('hour');
  const [clock, setClock] = useState(Date.now());
  const [refreshRevision, setRefreshRevision] = useState(0);
  const statusRequest = useRef(0);
  const mounted = useRef(false);
  const dayIdentity = mainDayIdentity(clock);
  const query = useMemo(() => {
    const request = mainRequest(preset, source, grain, clock);
    for (const dimension of ['models', 'projects', 'sessions'] as const) {
      const choice = choices[dimension];
      request.filter[dimension] = choice === null ? { kind: 'all' } : { kind: 'ids', ids: choice.key === null ? [] : [choice.key], include_unknown: choice.key === null };
    }
    return request;
  }, [preset, source, grain, dayIdentity, choices]);
  const current = pages.find(p => p[0] === page)!;
  const refresh = async () => {
    const sequence = ++statusRequest.current;
    setLoading(true);
    try {
      const result = await getAppStatus();
      if (!mounted.current || sequence !== statusRequest.current) return;
      setStatus(result); setError(null); setRefreshRevision(value => value + 1);
      try { const sourceResult = await getSources(); if (mounted.current && sequence === statusRequest.current) { setSources(sourceResult); setSourceError(null); } }
      catch (e) { if (mounted.current && sequence === statusRequest.current) setSourceError(runtimeError(e)); }
    }
    catch (e) { if (mounted.current && sequence === statusRequest.current) setError(runtimeError(e)); }
    finally { if (mounted.current && sequence === statusRequest.current) setLoading(false); }
  };
  useEffect(() => { mounted.current = true; void refresh(); const timer = setInterval(() => { if (!document.hidden) setClock(Date.now()); }, 60_000); return () => { mounted.current = false; ++statusRequest.current; clearInterval(timer); }; }, []);
  const openSources = () => { setPage('settings'); setTab('数据来源'); };
  return <div className="workspace">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark">▥</span>TokenPulse</div>
      <nav aria-label="主导航">{pages.map(([id, label]) => <button key={id} className={page === id ? 'active' : ''} aria-current={page === id ? 'page' : undefined} onClick={() => setPage(id)}>{label}</button>)}</nav>
      <div className="sidebar-bottom"><span className="status-dot" />{status?.collector === 'ready' ? '正在采集本地来源' : status?.collector === 'error' ? '采集需要处理' : status ? '尚未配置来源' : '桌面连接未就绪'}<small>本地日志只读 · 账户连接可选</small></div>
    </aside>
    <main>
      <header className="heading"><div><h1>{current[1]}</h1><p>{current[2]}</p></div><div className="head-actions"><button onClick={() => void refresh()} disabled={loading}>刷新</button>{status && <button onClick={() => void windowAction('hide_main').catch(e => setError(String(e)))}>隐藏到托盘</button>}</div></header>
      {!['settings', 'diagnostics'].includes(page) && <div className="filters" aria-label="统一筛选"><select aria-label="日期范围" disabled={!status} value={preset} onChange={e => setPreset(e.target.value as DatePreset)}><option value="today">今日</option><option value="last7">近 7 日</option><option value="last30">近 30 日</option></select><select aria-label="来源" disabled={sources === null} value={source ?? ''} onChange={e => setSource(e.target.value || null)}><option value="">全部来源</option>{sources?.sources.map(source => <option value={source.source_id} key={source.source_id}>{source.root_path}{source.removed ? '（历史来源）' : ''}</option>)}</select><AdvancedFilters filter={query.filter} choices={choices} disabled={status?.storage !== 'ready'} onChange={(dimension, choice) => setChoices(value => ({ ...value, [dimension]: choice }))} />{(preset !== 'today' || source !== null || Object.values(choices).some(choice => choice !== null)) && <button className="reset-filters" onClick={() => { setPreset('today'); setSource(null); setChoices({ models: null, projects: null, sessions: null }); }}>重置筛选</button>}<span>{status ? query.filter.range.timezone : '等待接入统计服务'}</span></div>}
      {error && <div role="alert" className="notice">{error}<button onClick={() => void refresh()}>重试连接</button></div>}
      {status?.storage_error && <div role="alert" className="notice">本地数据库无法使用（{status.storage_error}）。已保留数据库文件，采集尚未启动。请查看采集诊断。</div>}
      {sourceError && !['settings', 'diagnostics'].includes(page) && <div className="notice" role="alert">来源候选暂不可用：{sourceError}</div>}
      {page === 'settings' ? <>
        <div className="tabs" role="tablist" aria-label="设置分类">{['数据来源', '显示与窗口', '任务栏显示', '价格规则', '数据与备份'].map(t => <button role="tab" aria-selected={tab === t} key={t} onClick={() => setTab(t)}>{t}</button>)}</div>
        {tab === '数据来源' ? <SourcesPanel onChanged={() => void refresh()} /> : tab === '价格规则' ? <PriceRulesPanel onChanged={() => void refresh()} /> : <section className="panel" role="tabpanel"><h2>{tab}</h2><p className="muted">此模块正在实施，完成后可在此配置。</p>{tab === '数据与备份' && status && <dl><dt>数据目录</dt><dd>{status.data_directory}</dd><dt>运行版本</dt><dd>{status.version}{status.development ? ' · 开发版（数据隔离）' : ''}</dd></dl>}</section>}
      </> : page === 'diagnostics' ? <><section className="panel"><h2>运行状态</h2><dl><dt>桌面运行壳</dt><dd>{status ? '已连接' : loading ? '正在连接' : '未连接'}</dd><dt>采集服务</dt><dd>{status?.collector==='ready'?'正在采集':status?.collector==='error'?'采集需要处理':status?'尚未配置来源':'未连接'}</dd><dt>本地数据库</dt><dd>{status?.storage === 'ready' ? '已就绪' : status?.storage_error ?? '未连接'}</dd><dt>核算服务</dt><dd>已接入采集与必要观察重放</dd><dt>账户额度</dt><dd>未连接</dd><dt>任务栏显示</dt><dd>正在实施 · 默认关闭</dd></dl></section><JobsPanel /></> : page === 'overview' && status?.storage === 'ready' ? <OverviewPage request={query} refreshRevision={refreshRevision} sources={sources} onSources={openSources} onPrices={() => { setPage('settings'); setTab('价格规则'); }} onSessions={() => setPage('sessions')} onGrain={setGrain} /> : (page === 'models' || page === 'projects') && status?.storage === 'ready' ? <GroupedPage key={page} request={query} dimension={page} refreshRevision={refreshRevision} onPrices={() => { setPage('settings'); setTab('价格规则'); }} /> : <section className="empty panel"><div className="empty-symbol">▥</div><h2>{loading ? '正在连接桌面服务' : '开始记录本地用量'}</h2><p>添加 Codex 数据来源后，可信 Token、费用估算和来源覆盖会显示在这里。</p><p className="support-note">采集与核算已接入本地账本，统计查询与页面正在实施。</p><button className="primary" onClick={openSources}>查看数据来源</button></section>}
      <footer><span>{status?.development ? '开发版 · 独立数据目录' : 'TokenPulse'}</span><span>账户额度与本地消费分别计算</span></footer>
    </main>
  </div>;
}
