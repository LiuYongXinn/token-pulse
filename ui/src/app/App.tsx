import { flushSync } from 'react-dom';
import { Fragment, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { displayPolicy } from '../shared/display-policy';
import { timeNavigation } from '../shared/query-timing';
import { useUsageQueryController } from './usage-query-controller';
import { useAppTheme } from '../shared/useAppTheme';
import { getAppStatus, onDisplayPolicyChanged, runtimeError, windowAction } from '../shared/runtime';
import { useSourcesSnapshot } from './main-state-cache';
import { PendingOverview, PendingStatistics } from './PendingStatistics';
import type { AppStatus } from '../shared/runtime';
import { SourcesPanel } from './SourcesPanel';
import { JobsPanel } from './JobsPanel';
import { DiagnosticsIssues } from './DiagnosticsIssues';
import { PriceRulesPanel } from './PriceRulesPanel';
import { GroupedPage } from './GroupedPage';
import { OverviewPage } from './OverviewPage';
import { SessionsPage } from './SessionsPage';
import { EventsPage } from './EventsPage';
import { useMainCalendar } from './useMainCalendar';
import { DisplaySettingsPanel } from './DisplaySettingsPanel';
import { UpdateSettingsPanel } from './UpdateSettingsPanel';
import { TaskbarDiagnosticsPanel, TaskbarSettingsPanel } from './TaskbarSettingsPanel';
import { useMainNavigation } from './useMainNavigation';
import { whenFull } from './usage-display';
import type { MiniStatsRequest } from '../shared/generated/contracts';
import { DateFilter } from './DateFilter';
import { PriceBasisFilter } from './PriceBasisFilter';
import { AdvancedFilters, type FilterChoices } from './AdvancedFilters';
import { mainRequestForCalendar } from '../shared/main-filter';
import { Icon } from '../shared/Icon';
import { ActionButton } from '../shared/ActionButton';
import type { PriceBasis, CalendarSelection, Grain } from '../shared/generated/contracts';

const pages = [
  ['overview', '总览'],
  ['models', '模型'],
  ['projects', '项目'],
  ['sessions', '会话'],
  ['events', '明细'],
  ['diagnostics', '采集诊断'],
  ['settings', '设置'],
] as const;
type Page = typeof pages[number][0];

export function App() {
  useUsageQueryController();
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const [page, setPage] = useState<Page>('overview');
  const [statusCache, setStatus] = useState<{ value: AppStatus; epoch: number } | null>(null);
  const status = statusCache?.epoch === policy.epoch ? statusCache.value : null;
  const [error, setError] = useState<string | null>(null);
  const [statusPollError, setStatusPollError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState('数据来源');
  const visitedSettings = useRef(new Set<string>());
  const diagnosticVisited = useRef(false);
  if (page === 'settings') visitedSettings.current.add(tab);
  if (page === 'diagnostics') diagnosticVisited.current = true;
  const [miniStats, setMiniStats] = useState<MiniStatsRequest | null>(null);
  const [selection, setSelection] = useState<CalendarSelection>({ kind: 'today' });
  const [source, setSource] = useState<string | null>(null);
  const [choices, setChoices] = useState<FilterChoices>({ models: null, projects: null, sessions: null });
  const [priceBasis, setPriceBasis] = useState<PriceBasis>({ mode: 'event_time' });
  const [grain, setGrain] = useState<Grain>('hour');
  const [clock, setClock] = useState(Date.now());
  const [refreshRevision, setRefreshRevision] = useState(0);
  const sourceState = useSourcesSnapshot(refreshRevision);
  const sources = sourceState.bundle, sourceError = sourceState.error;
  const statusRequest = useRef(0);
  const statusRefreshBusy = useRef(false);
  const mounted = useRef(false);
  const historicalReady = useRef<number | null>(null);
  if (status?.storage === 'ready') historicalReady.current = policy.epoch;
  const ready = status?.storage === 'ready' || historicalReady.current === policy.epoch;
  const display = useMainCalendar(selection, ready, refreshRevision, clock);
  useAppTheme(display.settings?.preferences.theme);
  // A page/tab change starts at its heading, independent of the previous page's scroll.
  useEffect(() => { window.scrollTo(0, 0); }, [page, tab]);
  useMainNavigation(intent => {
    if (intent.kind === 'taskbar_settings') { setPage('settings'); setTab('任务栏显示'); return; }
    const request = intent.request;
    setMiniStats(request); setSource(null); setChoices({ models: null, projects: null, sessions: request.mini_scope.kind === 'session' ? { key: request.mini_scope.session_key, display_name: '固定会话（小窗）' } : null }); setPriceBasis({ mode: 'event_time' }); setPage('overview');
  }, message => setError(message));
  const query = useMemo(() => {
    const calendar = miniStats?.calendar ?? display.calendar;
    if (!calendar) return null;
    const request = mainRequestForCalendar(calendar, source, grain);
    request.price_basis = priceBasis;
    for (const dimension of ['models', 'projects', 'sessions'] as const) {
      const choice = choices[dimension];
      request.filter[dimension] = choice === null ? { kind: 'all' } : { kind: 'ids', ids: choice.key === null ? [] : [choice.key], include_unknown: choice.key === null };
    }
    return request;
  }, [miniStats, display.calendar, source, grain, choices, priceBasis]);
  const safeChoices = useMemo(() => {
    const safe = { ...choices };
    for (const dimension of ['projects', 'sessions'] as const) if (safe[dimension] !== null && policy.privacy !== false) safe[dimension] = { ...safe[dimension]!, display_name: dimension === 'projects' ? '所选项目（已隐藏）' : '所选会话（已隐藏）' };
    return safe;
  }, [choices, policy.privacy]);
  useEffect(() => {
    // Keep stable filter IDs, discard prior identifying labels on either transition.
    if (policy.epoch === 0) return;
    setStatus(null);
    setChoices(value => ({ ...value, projects: value.projects ? { ...value.projects, display_name: '所选项目' } : null, sessions: value.sessions ? { ...value.sessions, display_name: '所选会话' } : null }));
    setError(null); setStatusPollError(null);
  }, [policy.epoch]);
  const current = pages.find(p => p[0] === page)!;
  const refresh = async () => {
    const sequence = ++statusRequest.current;
    statusRefreshBusy.current = true;
    setLoading(true);
    try {
      const result = await getAppStatus();
      if (!mounted.current || sequence !== statusRequest.current) return;
      setStatus({ value: result, epoch: displayPolicy.get().epoch }); setError(null); setStatusPollError(null); setRefreshRevision(value => value + 1);
    }
    catch (e) { if (mounted.current && sequence === statusRequest.current) setError(runtimeError(e)); }
    finally { if (sequence === statusRequest.current) { statusRefreshBusy.current = false; if (mounted.current) setLoading(false); } }
  };
  useEffect(() => { mounted.current = true; void refresh(); const timer = setInterval(() => { if (!document.hidden) setClock(Date.now()); }, 60_000); return () => { mounted.current = false; ++statusRequest.current; clearInterval(timer); }; }, [policy.epoch, policy.pending]);
  useEffect(() => {
    let active = true, inFlight = false;
    const poll = async () => {
      if (document.hidden || inFlight || statusRefreshBusy.current || displayPolicy.get().pending) return;
      const sequence = statusRequest.current, epoch = displayPolicy.get().epoch;
      const current = () => active && mounted.current && sequence === statusRequest.current && epoch === displayPolicy.get().epoch;
      inFlight = true;
      try {
        const result = await getAppStatus();
        if (current()) { setStatus({ value: result, epoch }); setStatusPollError(null); }
      } catch (e) { if (current()) setStatusPollError(runtimeError(e)); }
      finally { inFlight = false; }
    };
    const timer = setInterval(() => void poll(), 2000);
    const visible = () => { if (!document.hidden) { setClock(Date.now()); void poll(); } };
    document.addEventListener('visibilitychange', visible);
    return () => { active = false; clearInterval(timer); document.removeEventListener('visibilitychange', visible); };
  }, [policy.epoch, policy.pending]);
  useEffect(() => {
    let active = true, stop: (() => void) | null = null;
    void onDisplayPolicyChanged().then(unsubscribe => { if (active) stop = unsubscribe; else unsubscribe(); }).catch(() => { if (active) { displayPolicy.enable(); displayPolicy.failed('无法订阅隐私同步，已隐藏敏感信息。'); } });
    return () => { active = false; stop?.(); };
  }, []);
  const openSources = () => { setPage('settings'); setTab('数据来源'); };
  const configuredSources = sources?.sources.filter(source => !source.removed);
  const collectorText = !status ? '桌面连接未就绪' : status.collector === 'error' ? '采集需要处理'
    : sources && !configuredSources?.some(source => source.enabled) ? configuredSources?.length ? '采集已暂停 · 历史保留' : sources.sources.length ? '采集已停止 · 历史保留' : '尚未配置来源'
    : status.collector === 'ready' ? '正在采集' : configuredSources?.some(source => source.enabled) ? '正在启动采集'
    : status.collector === 'not_implemented' ? '采集服务尚未启用' : '采集状态待确认';
  return <div className="workspace" data-page={page}>
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark"><Icon name="pulse" size={23} /></span>TokenPulse</div>
      <nav aria-label="主导航">{pages.map(([id, label]) => <button key={id} className={page === id ? 'active' : ''} aria-current={page === id ? 'page' : undefined} onClick={() => { timeNavigation(id); flushSync(() => setPage(id)); }}><Icon name={id} /><span>{label}</span></button>)}</nav>
      <div className="sidebar-bottom"><span className="status-dot" />{collectorText === '正在采集' ? '正在采集本地来源' : collectorText}<ActionButton icon="mini" disabled={!status} onClick={() => void windowAction('show_mini').catch(e => setError(runtimeError(e)))}>显示悬浮窗</ActionButton></div>
    </aside>
    <main>
      <header className="heading"><div><h1>{current[1]}</h1></div><div className="head-actions"><ActionButton icon="mini" variant="primary" disabled={!status} title="显示小窗" onClick={() => void windowAction('show_mini').catch(e => setError(runtimeError(e)))}>显示小窗</ActionButton><ActionButton icon="refresh" title="刷新" onClick={() => void refresh()} disabled={loading}>刷新</ActionButton>{status && <ActionButton icon="tray" variant="quiet" title="隐藏到托盘" onClick={() => void windowAction('hide_main').catch(e => setError(String(e)))}>隐藏到托盘</ActionButton>}</div></header>
      <Fragment key={policy.epoch}>
      {!['settings', 'diagnostics'].includes(page) && <div className="filters" aria-label="统一筛选">{miniStats ? <div className="mini-stat-scope" aria-label="从小窗带入的精确范围"><span>{whenFull(miniStats.calendar.range.start_ms, miniStats.calendar.range.timezone)} 至 {whenFull(miniStats.calendar.range.end_ms, miniStats.calendar.range.timezone)}（不含结束时刻）</span><button onClick={() => setMiniStats(null)}>改用主窗口日期</button></div> : <DateFilter selection={selection} calendar={display.calendar} timezone={display.settings?.preferences.display_timezone ?? null} disabled={!status} onChange={value => { setMiniStats(null); setSelection(value); }} />}<select aria-label="来源" disabled={sources === null} value={source ?? ''} onChange={e => setSource(e.target.value || null)}><option value="">全部来源</option>{sources?.sources.map(source => <option value={source.source_id} key={source.source_id}>{source.root_path}{source.removed ? '（历史来源）' : ''}</option>)}</select>{query && <AdvancedFilters filter={query.filter} choices={safeChoices} disabled={status?.storage !== 'ready'} onChange={(dimension, choice) => setChoices(value => ({ ...value, [dimension]: choice }))} />}{(miniStats !== null || selection.kind !== 'today' || source !== null || Object.values(choices).some(choice => choice !== null) || priceBasis.mode !== 'event_time') && <button className="reset-filters" onClick={() => { setMiniStats(null); setSelection({ kind: 'today' }); setPriceBasis({ mode: 'event_time' }); setSource(null); setChoices({ models: null, projects: null, sessions: null }); }}>重置筛选</button>}<PriceBasisFilter basis={priceBasis} disabled={!status} onChange={setPriceBasis} /><span>{miniStats?.calendar.range.timezone ?? display.settings?.preferences.display_timezone ?? '等待统计时区'}</span></div>}
      {(error || statusPollError) && <div role="alert" className="notice">{error || statusPollError}<button onClick={() => void refresh()}>重试连接</button></div>}
      {status?.storage_error && <div role="alert" className="notice">本地数据库无法使用（{status.storage_error}）。已保留数据库文件，采集尚未启动。请查看采集诊断。</div>}
      {sourceError && !['settings', 'diagnostics'].includes(page) && <div className="notice" role="alert">来源候选暂不可用：{sourceError}</div>}
      {display.settingsError && page !== 'settings' && <div className="notice" role="alert">显示设置读取失败：{display.settingsError}<button onClick={() => void display.reloadSettings()}>重试显示设置</button></div>}
      {display.calendarError && !['settings', 'diagnostics'].includes(page) && <div className="notice" role="alert">统计日期解析失败：{display.calendarError}</div>}
      <div hidden={page !== 'settings'}>
        <div className="tabs" role="tablist" aria-label="设置分类">{['数据来源', '显示与窗口', '任务栏显示', '价格规则', '软件更新'].map(t => <button role="tab" aria-selected={tab === t} key={t} onClick={() => setTab(t)}>{t}</button>)}</div>
        {visitedSettings.current.has('数据来源') && <div hidden={tab !== '数据来源'}><SourcesPanel onChanged={() => void refresh()} timezone={display.settings?.preferences.display_timezone ?? null} /></div>}
        {visitedSettings.current.has('显示与窗口') && <div hidden={tab !== '显示与窗口'}><DisplaySettingsPanel snapshot={display.settings} loadingError={display.settingsError} onRefresh={() => void display.reloadSettings()} onChanged={display.acceptSettings} /></div>}
        {visitedSettings.current.has('任务栏显示') && <div hidden={tab !== '任务栏显示'}><TaskbarSettingsPanel timezone={display.settings?.preferences.display_timezone ?? null} /></div>}
        {visitedSettings.current.has('软件更新') && <div hidden={tab !== '软件更新'}><UpdateSettingsPanel timezone={display.settings?.preferences.display_timezone ?? null} development={status?.development ?? null} /></div>}
        {visitedSettings.current.has('价格规则') && <div hidden={tab !== '价格规则'}>{policy.privacy !== false ? <section className="panel" role="tabpanel"><h2>价格规则已隐藏</h2></section> : <PriceRulesPanel onChanged={() => void refresh()} />}</div>}
      </div>
      {diagnosticVisited.current && <div hidden={page !== 'diagnostics'}><section className="panel"><h2>运行状态</h2><dl><dt>桌面运行壳</dt><dd>{statusPollError ? '连接异常，保留已知状态' : status ? '已连接' : loading ? '正在连接' : '未连接'}</dd><dt>采集服务</dt><dd>{collectorText}</dd><dt>本地数据库</dt><dd>{status?.storage === 'ready' ? '已就绪' : status?.storage_error ?? '未连接'}</dd><dt>核算服务</dt><dd>已接入采集与必要观察重放</dd></dl></section><SourcesPanel diagnostics onChanged={() => void refresh()} timezone={display.settings?.preferences.display_timezone ?? null} /><DiagnosticsIssues sources={sources} /><JobsPanel timezone={display.settings?.preferences.display_timezone ?? null} /><TaskbarDiagnosticsPanel timezone={display.settings?.preferences.display_timezone ?? null} /></div>}
      {!['settings', 'diagnostics'].includes(page) && (query === null || policy.privacy === null || !ready) && <>
        {page === 'overview' ? <PendingOverview grainControls={<span>统计日期确认中</span>} recentAction={<button onClick={() => setPage('sessions')}>查看全部</button>}><section className="panel"><h2>账户额度</h2><p>—</p></section></PendingOverview> : <PendingStatistics label={current[1]} columns={page === 'models' ? ['模型', 'Token 总量', '占总量', '缓存 / 输入', '估算费用', '价格覆盖'] : page === 'projects' ? ['项目', '会话', 'Token 总量', '估算费用'] : ['记录', '项目 / 模型', 'Token 总量', '估算费用']} />}
      </>}
      {/* Keep query owners mounted across navigation; inactive views render no DOM. */}
      {policy.privacy !== null && ready && query !== null && <>
        <OverviewPage active={page === 'overview'} request={query} refreshRevision={refreshRevision} sources={sources} accountTimezone={display.settings?.preferences.display_timezone ?? null} onSources={openSources} onDiagnostics={() => setPage('diagnostics')} onPrices={() => { setPage('settings'); setTab('价格规则'); }} onSessions={() => setPage('sessions')} onGrain={setGrain} onDay={day => { setMiniStats(null); setSelection({ kind: 'custom', start_date: day, end_date_inclusive: day }); }} />
        <GroupedPage active={page === 'models'} request={query} dimension="models" refreshRevision={refreshRevision} onPrices={() => { setPage('settings'); setTab('价格规则'); }} />
        <GroupedPage active={page === 'projects'} request={query} dimension="projects" refreshRevision={refreshRevision} onPrices={() => { setPage('settings'); setTab('价格规则'); }} />
        <SessionsPage active={page === 'sessions'} request={query} refreshRevision={refreshRevision} onSessionScope={(key, name) => setChoices(value => ({ ...value, sessions: { key, display_name: name } }))} />
        <EventsPage active={page === 'events'} request={query} refreshRevision={refreshRevision} onSession={(key, name) => { setChoices(value => ({ ...value, sessions: { key, display_name: name } })); setPage('sessions'); }} />
      </>}
      </Fragment>
      <footer><span>{status?.development ? '开发版 · 独立数据目录' : 'TokenPulse'}</span></footer>
    </main>
  </div>;
}
