import { useEffect, useState, useSyncExternalStore } from 'react';
import type { DashboardRequest } from '../shared/generated/contracts';
import { availableGrain } from '../shared/main-filter';
import { displayPolicy } from '../shared/display-policy';
import { prepareMainCalendar } from './useMainCalendar';
import { useDashboard } from './useDashboard';

function PreparedOverview({ request, revision }: { request: DashboardRequest; revision: number }) {
  useDashboard(request, revision, false);
  return null;
}

/** Prepare only three bounded overview presets after the current overview succeeds. */
export function DatePresetCache({ request, refreshRevision }: { request: DashboardRequest; refreshRevision: number }) {
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const current = useDashboard(request, refreshRevision, false);
  const key = JSON.stringify([request, policy.epoch, refreshRevision]);
  const ready = current.bundle !== null && !current.loading && !current.stale && !current.error && !policy.pending;
  const [prepared, setPrepared] = useState<{ key: string; requests: DashboardRequest[] } | null>(null);
  useEffect(() => {
    if (!ready) return;
    let active = true, started = false;
    const prepare = () => {
      if (!active || started || document.hidden) return;
      started = true;
      void Promise.all((['today', 'last7', 'last30'] as const).map(kind => prepareMainCalendar({ kind }, request.filter.range.timezone, Date.now()))).then(calendars => {
        if (!active || policy.epoch !== displayPolicy.get().epoch || displayPolicy.get().pending) return;
        const requests = calendars.map(calendar => ({ ...request, filter: { ...request.filter, range: calendar.range }, grain: availableGrain(request.grain, calendar.range), heatmap_range: calendar.heatmap_range }));
        setPrepared({ key, requests: requests.filter(value => JSON.stringify(value) !== JSON.stringify(request)) });
      }).catch(() => { /* Optional preparation never replaces the active date's result. */ });
    };
    const timer = setTimeout(prepare, 300);
    document.addEventListener('visibilitychange', prepare);
    return () => { active = false; clearTimeout(timer); document.removeEventListener('visibilitychange', prepare); };
  }, [key, ready]);
  return ready && prepared?.key === key ? prepared.requests.map(value => <PreparedOverview key={JSON.stringify(value)} request={value} revision={refreshRevision} />) : null;
}
