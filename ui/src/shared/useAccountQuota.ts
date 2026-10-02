import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { displayPolicy } from './display-policy';
import type { QuotaSnapshot } from './generated/contracts';
import { getAccountQuota, onAccountQuotaChanged, refreshAccountQuota, runtimeError } from './runtime';

/** One account snapshot, independent of all usage filters. Notifications only invalidate. */
export function useAccountQuota() {
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const [cache, setCache] = useState<{ epoch: number; value: QuotaSnapshot } | null>(null);
  const [error, setError] = useState<string | null>(null), [notice, setNotice] = useState<string | null>(null);
  const [clock, setClock] = useState(Date.now()), [refreshing, setRefreshing] = useState(false);
  const life = useRef(0), sequence = useRef(0), refreshBusy = useRef(false);
  const latest = useRef<QuotaSnapshot | null>(null);
  const readCurrent = useRef<(() => void) | null>(null);
  const quota = policy.privacy === false && cache?.epoch === policy.epoch ? cache.value : null;
  useEffect(() => {
    const generation = ++life.current, epoch = policy.epoch;
    let active = true, stop: (() => void) | undefined;
    const current = (serial: number) => active && sequence.current === serial && displayPolicy.get().epoch === epoch && displayPolicy.get().privacy === false;
    latest.current = null; readCurrent.current = null; setCache(null); setError(null); setNotice(null); setRefreshing(false);
    if (policy.privacy !== false) return () => { active = false; ++life.current; ++sequence.current; };
    const read = async () => {
      const serial = ++sequence.current;
      try {
        const value = await getAccountQuota();
        if (!current(serial)) return;
        // A later transport response cannot downgrade a known revision in the same connection.
        if (latest.current?.connection_epoch === value.connection_epoch && BigInt(latest.current.quota_revision) > BigInt(value.quota_revision)) return;
        latest.current = value; setCache({ epoch, value }); setError(null);
      } catch (e) { if (current(serial)) setError(runtimeError(e)); }
    };
    const visible = () => { if (!document.hidden) { setClock(Date.now()); void read(); } };
    readCurrent.current = () => { if (active) void read(); };
    void onAccountQuotaChanged(change => {
      if (!active || life.current !== generation) return;
      if (!latest.current || change.connection_epoch !== latest.current.connection_epoch || ['disconnected', 'connecting', 'authorization_required', 'unsupported'].includes(change.state)) {
        latest.current = null; setCache(null); setNotice(null);
      }
      void read();
    }).then(unlisten => { if (active) { stop = unlisten; void read(); } else unlisten(); }).catch(e => { if (active) { setError(runtimeError(e)); void read(); } });
    const poll = setInterval(visible, 10_000); // Reads the cached DTO; does not force a network request.
    const timer = setInterval(() => { if (active) setClock(Date.now()); }, 60_000);
    document.addEventListener('visibilitychange', visible);
    return () => { active = false; ++life.current; ++sequence.current; stop?.(); clearInterval(poll); clearInterval(timer); document.removeEventListener('visibilitychange', visible); };
  }, [policy.epoch, policy.privacy]);
  const refresh = useCallback(async () => {
    if (refreshBusy.current || displayPolicy.get().privacy !== false) return;
    const generation = life.current, epoch = displayPolicy.get().epoch;
    refreshBusy.current = true; setRefreshing(true); setNotice(null); setError(null);
    try {
      const result = await refreshAccountQuota();
      if (generation === life.current && epoch === displayPolicy.get().epoch) {
        // Re-read through the normal query path; receipts never overwrite newer notifications.
        readCurrent.current?.();
        setNotice(result.status === 'started' ? '已请求更新额度。' : result.status === 'in_flight' ? '额度读取正在进行。' : result.status === 'rate_limited' ? `请在 ${result.retry_after_ms === null ? '稍后' : `${Math.ceil(result.retry_after_ms / 1000)} 秒后`}刷新。` : '额度尚未到刷新时间。');
      }
    } catch (e) { if (generation === life.current && epoch === displayPolicy.get().epoch) setError(runtimeError(e)); }
    finally { refreshBusy.current = false; if (generation === life.current) setRefreshing(false); }
  }, []);
  return { quota, error: policy.privacy === false ? error : null, notice: policy.privacy === false ? notice : null, clock: Math.max(clock, Date.now()), refreshing, refresh, hidden: policy.privacy !== false };
}
