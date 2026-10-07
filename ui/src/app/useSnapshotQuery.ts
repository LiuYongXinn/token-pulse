import { useCallback, useEffect, useRef, useSyncExternalStore } from 'react';
import { runtimeError } from '../shared/runtime';
import { displayPolicy } from '../shared/display-policy';
import { promoteUsageQueries, scheduleUsageQuery } from './usage-query-scheduler';
import { usageQueryKey } from './usage-query-key';
import { usageQueryCache } from './usage-query-cache';

const readers = new WeakMap<object, string>();
let serial = 0, epoch = displayPolicy.get().epoch;
displayPolicy.subscribe(() => {
  if (epoch !== displayPolicy.get().epoch) { epoch = displayPolicy.get().epoch; usageQueryCache.clear(); }
});

export function useSnapshotQuery<Query, Bundle>(request: Query, refreshRevision: number, read: (request: Query) => Promise<Bundle>, foreground = true) {
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  if (!readers.has(read)) readers.set(read, `read-${++serial}`);
  const key = usageQueryKey(readers.get(read)!, request, policy.epoch);
  const visible = useRef(foreground); visible.current = foreground;
  const subscribe = useCallback((listener: () => void) => usageQueryCache.subscribe(key, listener), [key]);
  const get = useCallback(() => usageQueryCache.get<Bundle>(key), [key]);
  const result = useSyncExternalStore(subscribe, get);
  const refresh = useCallback(() => usageQueryCache.read(key, async () => {
    let value!: Bundle;
    await scheduleUsageQuery(async () => { value = await read(request); }, () => visible.current, () => policy.epoch === displayPolicy.get().epoch);
    return value;
  }, runtimeError), [key, read]);
  const previousRefresh = useRef(refreshRevision);
  useEffect(() => { promoteUsageQueries(); }, [foreground]);
  useEffect(() => {
    if (previousRefresh.current !== refreshRevision) { previousRefresh.current = refreshRevision; usageQueryCache.invalidate(key); }
    if (!policy.pending) void refresh();
  }, [key, refreshRevision, policy.pending, refresh]);
  useEffect(() => { if (result.stale && !result.loading && !result.error && !policy.pending) void refresh(); }, [result, refresh, policy.pending]);
  return { bundle: result.value, error: result.error, loading: result.loading || result.value === null, reload: () => { usageQueryCache.invalidate(key); void refresh(); }, accept: (value: Bundle) => usageQueryCache.accept(key, value) };
}
