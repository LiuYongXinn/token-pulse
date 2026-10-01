import { Fragment, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { displayPolicy } from '../shared/display-policy';
import { useAppTheme } from '../shared/useAppTheme';
import { getAppStatus, getSources, onDisplayPolicyChanged, runtimeError, windowAction } from '../shared/runtime';
import type { AppStatus } from '../shared/runtime';
import { SourcesPanel } from './SourcesPanel';
import { JobsPanel } from './JobsPanel';
import { PriceRulesPanel } from './PriceRulesPanel';
import { GroupedPage } from './GroupedPage';
import { OverviewPage } from './OverviewPage';
import { SessionsPage } from './SessionsPage';
import { EventsPage } from './EventsPage';
import { useMainCalendar } from './useMainCalendar';
import { DisplaySettingsPanel } from './DisplaySettingsPanel';
import { DateFilter } from './DateFilter';
import { PriceBasisFilter } from './PriceBasisFilter';
import { AdvancedFilters, type FilterChoices } from './AdvancedFilters';
import { mainRequestForCalendar } from '../shared/main-filter';
import type { PriceBasis, CalendarSelection, Grain, SourcesSnapshot } from '../shared/generated/contracts';

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
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const [page, setPage] = useState<Page>('overview');
  const [statusCache, setStatus] = useState<{ value: AppStatus; epoch: number } | null>(null);
  const status = statusCache?.epoch === policy.epoch ? statusCache.value : null;
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState('数据来源');
  const [sourceCache, setSources] = useState<{ value: SourcesSnapshot; epoch: number } | null>(null);
  const sources = sourceCache?.epoch === policy.epoch ? sourceCache.value : null;
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [selection, setSelection] = useState<CalendarSelection>({ kind: 'today' });
  const [source, setSource] = useState<string | null>(null);
  const [choices, setChoices] = useState<FilterChoices>({ models: null, projects: null, sessions: null });
  const [priceBasis, setPriceBasis] = useState<PriceBasis>({ mode: 'event_time' });
  const [grain, setGrain] = useState<Grain>('hour');
  const [clock, setClock] = useState(Date.now());
  const [refreshRevision, setRefreshRevision] = useState(0);
  const statusRequest = useRef(0);
  const mounted = useRef(false);
  const display = useMainCalendar(selection, status?.storage === 'ready', refreshRevision, clock);
  useAppTheme(display.settings?.preferences.theme);
  const query = useMemo(() => {
    if (!display.calendar) return null;
    const request = mainRequestForCalendar(display.calendar, source, grain);
    request.price_basis = priceBasis;
    for (const dimension of ['models', 'projects', 'sessions'] as const) {
      const choice = choices[dimension];
      request.filter[dimension] = choice === null ? { kind: 'all' } : { kind: 'ids', ids: choice.key === null ? [] : [choice.key], include_unknown: choice.key === null };
    }
    return request;
  }, [display.calendar, source, grain, choices, priceBasis]);
  const safeChoices = useMemo(() => {
    const safe = { ...choices };
    for (const dimension of ['projects', 'sessions'] as const) if (safe[dimension] !== null && policy.privacy !== false) safe[dimension] = { ...safe[dimension]!, display_name: dimension === 'projects' ? '所选项目（已隐藏）' : '所选会话（已隐藏）' };
    return safe;
  }, [choices, policy.privacy]);
  useEffect(() => {
    // Keep stable filter IDs, discard prior identifying labels on either transition.
    if (policy.epoch === 0) return;
    setStatus(null); setSources(null);
    setChoices(value => ({ ...value, projects: value.projects ? { ...value.projects, display_name: '所选项目' } : null, sessions: value.sessions ? { ...value.sessions, display_name: '所选会话' } : null }));
    setError(null); setSourceError(null);
  }, [policy.epoch]);
  const current = pages.find(p => p[0] === page)!;
  const refresh = async () => {
    const sequence = ++statusRequest.current;
    setLoading(true);
    try {
      const result = await getAppStatus();
      if (!mounted.current || sequence !== statusRequest.current) return;
      setStatus({ value: result, epoch: displayPolicy.get().epoch }); setError(null); setRefreshRevision(value => value + 1);
      try { const sourceResult = await getSources(); if (mounted.current && sequence === statusRequest.current) { setSources({ value: sourceResult, epoch: displayPolicy.get().epoch }); setSourceError(null); } }
      catch (e) { if (mounted.current && sequence === statusRequest.current) setSourceError(runtimeError(e)); }
    }
    catch (e) { if (mounted.current && sequence === statusRequest.current) setError(runtimeError(e)); }
    finally { if (mounted.current && sequence === statusRequest.current) setLoading(false); }
  };
  useEffect(() => { mounted.current = true; void refresh(); const timer = setInterval(() => { if (!document.hidden) setClock(Date.now()); }, 60_000); return () => { mounted.current = false; ++statusRequest.current; clearInterval(timer); }; }, [policy.epoch, policy.pending]);
  useEffect(() => {
    let active = true, stop: (() => void) | null = null;
    void onDisplayPolicyChanged().then(unsubscribe => { if (active) stop = unsubscribe; else unsubscribe(); }).catch(() => { if (active) { displayPolicy.enable(); displayPolicy.failed('无法订阅隐私同步，已隐藏敏感信息。'); } });
    return () => { active = false; stop?.(); };
  }, []);
  const openSources = () => { setPage('settings'); setTab('数据来源'); };
  return <div className="workspace">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark">▥</span>TokenPulse</div>
      <nav aria-label="主导航">{pages.map(([id, label]) => <button key={id} className={page === id ? 'active' : ''} aria-current={page === id ? 'page' : undefined} onClick={() => setPage(id)}>{label}</button>)}</nav>
      <div className="sidebar-bottom"><span className="status-dot" />{status?.collector === 'ready' ? '正在采集本地来源' : status?.collector === 'error' ? '采集需要处理' : status ? '尚未配置来源' : '桌面连接未就绪'}<small>本地日志只读 · 账户连接可选</small><button disabled={!status} onClick={() => void windowAction('show_mini').catch(e => setError(runtimeError(e)))}>显示悬浮窗</button></div>
    </aside>
    <main>
      <header className="heading"><div><h1>{current[1]}</h1><p>{current[2]}</p></div><div className="head-actions"><button onClick={() => void refresh()} disabled={loading}>刷新</button>{status && <button onClick={() => void windowAction('hide_main').catch(e => setError(String(e)))}>隐藏到托盘</button>}</div></header>
      <Fragment key={policy.epoch}>
      {!['settings', 'diagnostics'].includes(page) && <div className="filters" aria-label="统一筛选"><DateFilter selection={selection} calendar={display.calendar} timezone={display.settings?.preferences.display_timezone ?? null} disabled={!status} onChange={setSelection} /><select aria-label="来源" disabled={sources === null} value={source ?? ''} onChange={e => setSource(e.target.value || null)}><option value="">全部来源</option>{sources?.sources.map(source => <option value={source.source_id} key={source.source_id}>{source.root_path}{source.removed ? '（历史来源）' : ''}</option>)}</select>{query && <AdvancedFilters filter={query.filter} choices={safeChoices} disabled={status?.storage !== 'ready'} onChange={(dimension, choice) => setChoices(value => ({ ...value, [dimension]: choice }))} />}{(selection.kind !== 'today' || source !== null || Object.values(choices).some(choice => choice !== null) || priceBasis.mode !== 'event_time') && <button className="reset-filters" onClick={() => { setSelection({ kind: 'today' }); setPriceBasis({ mode: 'event_time' }); setSource(null); setChoices({ models: null, projects: null, sessions: null }); }}>重置筛选</button>}<PriceBasisFilter basis={priceBasis} disabled={!status} onChange={setPriceBasis} /><span>{display.settings?.preferences.display_timezone ?? '等待统计时区'}</span></div>}
      {error && <div role="alert" className="notice">{error}<button onClick={() => void refresh()}>重试连接</button></div>}
      {status?.storage_error && <div role="alert" className="notice">本地数据库无法使用（{status.storage_error}）。已保留数据库文件，采集尚未启动。请查看采集诊断。</div>}
      {sourceError && !['settings', 'diagnostics'].includes(page) && <div className="notice" role="alert">来源候选暂不可用：{sourceError}</div>}
      {display.settingsError && page !== 'settings' && <div className="notice" role="alert">显示设置读取失败：{display.settingsError}<button onClick={() => void display.reloadSettings()}>重试显示设置</button></div>}
      {display.calendarError && !['settings', 'diagnostics'].includes(page) && <div className="notice" role="alert">统计日期解析失败：{display.calendarError}</div>}
      {policy.privacy === null && !error && !['settings', 'diagnostics'].includes(page) ? <section className="empty panel"><h2>正在确认显示隐私策略</h2><p>确认后读取统计和来源。</p></section> : page === 'settings' ? <>
        <div className="tabs" role="tablist" aria-label="设置分类">{['数据来源', '显示与窗口', '任务栏显示', '价格规则', '数据与备份'].map(t => <button role="tab" aria-selected={tab === t} key={t} onClick={() => setTab(t)}>{t}</button>)}</div>
        {tab === '数据来源' ? <SourcesPanel onChanged={() => void refresh()} /> : tab === '显示与窗口' ? <DisplaySettingsPanel snapshot={display.settings} loadingError={display.settingsError} onRefresh={() => void display.reloadSettings()} onChanged={display.acceptSettings} /> : tab === '价格规则' ? policy.privacy !== false ? <section className="panel" role="tabpanel"><h2>价格规则已隐藏</h2><p className="muted">关闭隐私后重新读取价格规则与编辑器。</p></section> : <PriceRulesPanel onChanged={() => void refresh()} /> : <section className="panel" role="tabpanel"><h2>{tab}</h2><p className="muted">此模块正在实施，完成后可在此配置。</p>{tab === '数据与备份' && status && <dl><dt>数据目录</dt><dd>{status.data_directory}</dd><dt>运行版本</dt><dd>{status.version}{status.development ? ' · 开发版（数据隔离）' : ''}</dd></dl>}</section>}
      </> : page === 'diagnostics' ? <><section className="panel"><h2>运行状态</h2><dl><dt>桌面运行壳</dt><dd>{status ? '已连接' : loading ? '正在连接' : '未连接'}</dd><dt>采集服务</dt><dd>{status?.collector==='ready'?'正在采集':status?.collector==='error'?'采集需要处理':status?'尚未配置来源':'未连接'}</dd><dt>本地数据库</dt><dd>{status?.storage === 'ready' ? '已就绪' : status?.storage_error ?? '未连接'}</dd><dt>核算服务</dt><dd>已接入采集与必要观察重放</dd><dt>账户额度</dt><dd>未连接</dd><dt>任务栏显示</dt><dd>正在实施 · 默认关闭</dd></dl></section><JobsPanel /></> : page === 'overview' && status?.storage === 'ready' && query !== null ? <OverviewPage request={query} refreshRevision={refreshRevision} sources={sources} onSources={openSources} onPrices={() => { setPage('settings'); setTab('价格规则'); }} onSessions={() => setPage('sessions')} onGrain={setGrain} onDay={day => setSelection({ kind: 'custom', start_date: day, end_date_inclusive: day })} /> : (page === 'models' || page === 'projects') && status?.storage === 'ready' && query !== null ? <GroupedPage key={page} request={query} dimension={page} refreshRevision={refreshRevision} onPrices={() => { setPage('settings'); setTab('价格规则'); }} /> : page === 'sessions' && status?.storage === 'ready' && query !== null ? <SessionsPage request={query} refreshRevision={refreshRevision} onSessionScope={(key, name) => setChoices(value => ({ ...value, sessions: { key, display_name: name } }))} /> : page === 'events' && status?.storage === 'ready' && query !== null ? <EventsPage request={query} refreshRevision={refreshRevision} onSession={(key, name) => { setChoices(value => ({ ...value, sessions: { key, display_name: name } })); setPage('sessions'); }} /> : status?.storage === 'ready' && query === null ? <section className="empty panel"><h2>{display.settingsError || display.calendarError ? '统计日期尚未就绪' : '正在读取统计日期'}</h2><p>读取已保存时区并解析日历范围后显示统计。</p><button onClick={() => void refresh()}>重新读取</button></section> : <section className="empty panel"><div className="empty-symbol">▥</div><h2>{loading ? '正在连接桌面服务' : '开始记录本地用量'}</h2><p>添加 Codex 数据来源后，可信 Token、费用估算和来源覆盖会显示在这里。</p><p className="support-note">采集与核算已接入本地账本，统计查询与页面正在实施。</p><button className="primary" onClick={openSources}>查看数据来源</button></section>}
      </Fragment>
      <footer><span>{status?.development ? '开发版 · 独立数据目录' : 'TokenPulse'}</span><span>账户额度与本地消费分别计算</span></footer>
    </main>
  </div>;
}
