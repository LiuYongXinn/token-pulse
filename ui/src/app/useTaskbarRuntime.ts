import { useCallback, useEffect, useRef, useState } from 'react';
import type { TaskbarRuntimeSnapshot } from '../shared/generated/contracts';
import { getTaskbarStatus, onTaskbarStatusChanged, runtimeError } from '../shared/runtime';

export function useTaskbarRuntime() {
  const [snapshot, setSnapshot] = useState<TaskbarRuntimeSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const active = useRef(false), serial = useRef(0);
  const refresh = useCallback(async () => {
    const request = ++serial.current;
    try {
      const value = await getTaskbarStatus();
      if (!active.current || request !== serial.current) return;
      setSnapshot(old => old && BigInt(old.revision) > BigInt(value.revision) ? old : value);
      setError(null);
    } catch (e) { if (active.current && request === serial.current) setError(runtimeError(e)); }
  }, []);
  useEffect(() => {
    active.current = true;
    let alive = true, stop: (() => void) | null = null;
    void onTaskbarStatusChanged(() => { if (alive) void refresh(); }).then(unsubscribe => {
      if (alive) { stop = unsubscribe; void refresh(); } else unsubscribe();
    }).catch(e => { if (alive) { setError(runtimeError(e)); void refresh(); } });
    const timer = setInterval(() => { if (!document.hidden) void refresh(); }, 5000);
    return () => { alive = false; active.current = false; ++serial.current; stop?.(); clearInterval(timer); };
  }, [refresh]);
  return { snapshot, error, refresh };
}
