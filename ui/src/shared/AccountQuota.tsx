import { useEffect, useRef, useState } from 'react';
import type { QuotaSnapshot, QuotaWindow } from './generated/contracts';
import { quotaCountdown, quotaDate, quotaPercent, quotaPeriod, quotaRoles, quotaStates, quotaWindows } from './quota-display';
import { useAccountQuota } from './useAccountQuota';

function Windows({ quota, clock, timezone }: { quota: QuotaSnapshot | null; clock: number; timezone: string | null }) {
  return <div className="quota-periods">{quotaWindows(quota).map(window => <div className={`quota-period ${window.remaining_percent !== null && window.remaining_percent <= 20 ? 'low' : ''}`} key={window.window_id}>
    <div><strong>{quotaPeriod(window)}</strong><span>剩余 {quotaPercent(window.remaining_percent)}</span></div>
    {window.remaining_percent !== null && <progress aria-label={`${quotaPeriod(window)}剩余`} value={window.remaining_percent} max={100} />}
    <p>重置：{quotaDate(window.resets_at_ms, timezone)}<br />{quotaCountdown(window.resets_at_ms, clock)}</p>
  </div>)}</div>;
}
export function AccountQuotaOverview({ timezone, onSettings }: { timezone: string | null; onSettings: () => void }) {
  const view = useAccountQuota();
  return <section className="panel quota-overview" aria-label="账户额度总览"><div className="panel-heading"><h2>账户额度</h2><button className="text-button" onClick={onSettings}>连接设置</button></div>
    {(view.hidden || view.error || view.quota?.state !== 'ready') && <p className="quota-state" role="status">{view.hidden ? '隐私模式已隐藏账户额度' : view.error && view.quota ? '更新失败 · 显示上次结果' : view.quota ? quotaStates[view.quota.state] : view.error ? '额度暂不可用' : '正在读取账户状态'}</p>}
    {!view.hidden && <><Windows quota={view.quota} clock={view.clock} timezone={timezone} />{view.error && <p className="notice" role="alert">{view.error}</p>}{view.notice && <p className="chart-caption" role="status">{view.notice}</p>}<button disabled={view.refreshing || !view.quota || !['ready', 'stale', 'error'].includes(view.quota.state)} onClick={() => void view.refresh()}>刷新账户额度</button></>}
  </section>;
}
export function MiniAccountQuota({ timezone, onExpand }: { timezone: string | null; onExpand: () => void }) {
  const view = useAccountQuota();
  const [details, setDetails] = useState(false);
  const dialog = useRef<HTMLElement | null>(null), entry = useRef<HTMLButtonElement | null>(null);
  useEffect(() => { if (view.hidden) setDetails(false); }, [view.hidden]);
  useEffect(() => {
    if (!details || view.hidden) return;
    const panel = dialog.current;
    panel?.querySelector<HTMLButtonElement>('button')?.focus();
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); setDetails(false); entry.current?.focus(); }
      if (event.key === 'Tab') {
        const buttons = [...(panel?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
        const first = buttons[0], last = buttons.at(-1);
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }
    };
    document.addEventListener('keydown', key);
    return () => document.removeEventListener('keydown', key);
  }, [details, view.hidden]);
  const { short, weekly } = quotaRoles(quotaWindows(view.quota));
  const shortLabel = short ? quotaPeriod(short).replace('额度', '剩余') : '短周期剩余';
  const state = view.hidden ? '已隐藏' : view.error && view.quota ? '更新失败 · 显示上次结果' : view.quota ? view.quota.state === 'ready' ? '' : quotaStates[view.quota.state] : view.error ? '读取失败' : '正在读取';
  const percent = (window: QuotaWindow | null) => view.hidden ? '已隐藏' : quotaPercent(window?.remaining_percent ?? null);
  return <>
    <button ref={entry} className="mini-quota" aria-label="查看账户额度详情" disabled={view.hidden} onClick={() => { setDetails(true); onExpand(); }}>
      <div><span>{shortLabel} <b className={short?.remaining_percent !== null && short?.remaining_percent !== undefined && short.remaining_percent <= 20 ? 'quota-low' : undefined}>{percent(short)}</b></span><span>周剩余 <b className={weekly?.remaining_percent !== null && weekly?.remaining_percent !== undefined && weekly.remaining_percent <= 20 ? 'quota-low' : undefined}>{percent(weekly)}</b></span></div>
      <p><span>周重置 {view.hidden ? '已隐藏' : quotaDate(weekly?.resets_at_ms ?? null, timezone, true)}</span><span>{state}</span></p>
      {weekly?.resets_at_ms !== null && weekly?.resets_at_ms !== undefined && !view.hidden && <small>{quotaCountdown(weekly.resets_at_ms, view.clock)}</small>}
    </button>
    {details && !view.hidden && <section ref={dialog} className="mini-quota-details" role="dialog" aria-modal="true" aria-label="账户额度详情"><header><h2>账户额度</h2><button aria-label="关闭账户额度详情" onClick={() => { setDetails(false); entry.current?.focus(); }}>×</button></header><div className="mini-quota-scroll">{state && <p className="quota-state" role="status">{state}</p>}<Windows quota={view.quota} clock={view.clock} timezone={timezone} />{view.error && <p role="alert">{view.error}</p>}{view.notice && <p role="status">{view.notice}</p>}</div><button disabled={view.refreshing || !view.quota || !['ready', 'stale', 'error'].includes(view.quota.state)} onClick={() => void view.refresh()}>刷新账户额度</button></section>}
  </>;
}
