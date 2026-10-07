import { useEffect, useSyncExternalStore } from 'react';
import type { TaskbarRuntimeSnapshot } from '../shared/generated/contracts';
import { getTaskbarStatus, onTaskbarStatusChanged, runtimeError } from '../shared/runtime';
let state: { snapshot: TaskbarRuntimeSnapshot | null; error: string | null } = { snapshot: null, error: null };
const listeners = new Set<() => void>();
let users = 0, generation = 0, flight: Promise<void> | null = null, pending = false;
let stop: (() => void) | null = null, timer: ReturnType<typeof setInterval> | null = null;
const publish = () => { for (const listener of listeners) listener(); };
const refresh = async (): Promise<void> => {
  if (flight) { pending = true; return flight; }
  const current = generation;
  flight = (async () => {
    try { const value = await getTaskbarStatus(); if (current === generation) { state = { snapshot: state.snapshot && BigInt(state.snapshot.revision) > BigInt(value.revision) ? state.snapshot : value, error: null }; publish(); } }
    catch (error) { if (current === generation) { state = { ...state, error: runtimeError(error) }; publish(); } }
  })();
  await flight; flight = null;
  if (pending && users) { pending = false; void refresh(); }
};
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
const get = () => state;
export function useTaskbarRuntime() {
  const view = useSyncExternalStore(subscribe, get);
  useEffect(() => {
    if (++users === 1) {
      const current = ++generation;
      void onTaskbarStatusChanged(() => void refresh()).then(unsubscribe => { if (current === generation) { stop = unsubscribe; void refresh(); } else unsubscribe(); }).catch(() => void refresh());
      timer = setInterval(() => { if (!document.hidden) void refresh(); }, 30_000);
    }
    return () => { if (--users === 0) { ++generation; stop?.(); stop = null; if (timer) clearInterval(timer); timer = null; } };
  }, []);
  return { ...view, refresh };
}
