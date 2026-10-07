import { useEffect, useSyncExternalStore } from 'react';
import { displayPolicy } from './display-policy';
import type { QuotaSnapshot } from './generated/contracts';
import { getAccountQuota, onAccountQuotaChanged, refreshAccountQuota, runtimeError } from './runtime';
type State = { quota: QuotaSnapshot | null; error: string | null; notice: string | null; clock: number; refreshing: boolean };
let state: State = { quota: null, error: null, notice: null, clock: Date.now(), refreshing: false };
const listeners = new Set<() => void>();
let epoch = displayPolicy.get().epoch, generation = 0, readSerial = 0, flight: Promise<void> | null = null, pending = false;
let stop: (() => void) | null = null, timer: ReturnType<typeof setInterval> | null = null;
const publish = (update: Partial<State>) => { state = { ...state, ...update }; for (const listener of listeners) listener(); };
const get = () => state;
async function read() {
  if (displayPolicy.get().privacy !== false || displayPolicy.get().pending) return;
  if (flight) { pending = true; return flight; }
  const captured = epoch, life = generation, serial = ++readSerial;
  flight = (async () => {
    try {
      const quota = await getAccountQuota();
      if (captured !== displayPolicy.get().epoch || life !== generation || serial !== readSerial) return;
      if (state.quota?.connection_epoch === quota.connection_epoch && BigInt(state.quota.quota_revision) > BigInt(quota.quota_revision)) return;
      publish({ quota, error: null, clock: Date.now() });
    } catch (error) { if (captured === displayPolicy.get().epoch && life === generation && serial === readSerial) publish({ error: runtimeError(error) }); }
    finally { if (serial === readSerial) { flight = null; if (pending) { pending = false; void read(); } } }
  })();
  return flight;
}
const visible = () => { if (!document.hidden) { publish({ clock: Date.now() }); void read(); } };
function end() { ++generation; ++readSerial; flight = null; pending = false; stop?.(); stop = null; if (timer) clearInterval(timer); timer = null; document.removeEventListener('visibilitychange', visible); }
function start() {
  if (timer || !listeners.size || displayPolicy.get().privacy !== false || displayPolicy.get().pending) return;
  const life = ++generation;
  void onAccountQuotaChanged(change => {
    if (life !== generation) return;
    if (state.quota?.connection_epoch !== change.connection_epoch || ['disconnected', 'connecting', 'authorization_required', 'unsupported'].includes(change.state)) { ++readSerial; flight = null; pending = false; publish({ quota: null, notice: null }); }
    if (!document.hidden) void read();
  }).then(unlisten => { if (life === generation && listeners.size) { stop = unlisten; void read(); } else unlisten(); }).catch(() => { if (life === generation) void read(); });
  timer = setInterval(visible, 30_000);
  document.addEventListener('visibilitychange', visible);
}
displayPolicy.subscribe(() => {
  const policy = displayPolicy.get();
  if (epoch !== policy.epoch) { epoch = policy.epoch; end(); pending = false; publish({ quota: null, error: null, notice: null, refreshing: false }); }
  if (policy.privacy !== false || policy.pending) end(); else start();
});
function subscribe(listener: () => void) { listeners.add(listener); start(); return () => { listeners.delete(listener); if (!listeners.size) end(); }; }
async function refresh() {
  if (state.refreshing || displayPolicy.get().privacy !== false) return;
  const captured = epoch;
  publish({ refreshing: true, error: null, notice: null });
  try {
    const result = await refreshAccountQuota();
    if (captured !== displayPolicy.get().epoch) return;
    void read();
    publish({ notice: result.status === 'started' ? '已请求更新额度。' : result.status === 'in_flight' ? '额度读取正在进行。' : result.status === 'rate_limited' ? `请在 ${result.retry_after_ms === null ? '稍后' : `${Math.ceil(result.retry_after_ms / 1000)} 秒后`}刷新。` : '额度尚未到刷新时间。' });
  } catch (error) { if (captured === displayPolicy.get().epoch) publish({ error: runtimeError(error) }); }
  finally { if (captured === displayPolicy.get().epoch) publish({ refreshing: false }); }
}
/** All account views in a window share one read and one event subscription. */
export function useAccountQuota() {
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const value = useSyncExternalStore(subscribe, get);
  useEffect(start, [policy.epoch, policy.pending]);
  const hidden = policy.privacy !== false;
  return { ...value, quota: hidden ? null : value.quota, error: hidden ? null : value.error, notice: hidden ? null : value.notice, refresh, hidden };
}
