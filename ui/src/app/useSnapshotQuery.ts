import { useEffect, useRef, useState } from 'react';
import { onPriceRulesChanged, runtimeError } from '../shared/runtime';

/** Keeps a complete response together and never relabels it with another scope. */
export function useSnapshotQuery<Query, Bundle>(request: Query, refreshRevision: number, read: (request: Query) => Promise<Bundle>) {
  const [result, setResult] = useState<{ key: string; bundle: Bundle } | null>(null);
  const [failure, setFailure] = useState<{ key: string; message: string } | null>(null);
  const [loading, setLoading] = useState(true);
  const sequence = useRef(0);
  const filterKey = JSON.stringify(request);
  const lastFilter = useRef<string | null>(null);
  useEffect(() => {
    let active = true;
    let busy = false;
    const refresh = async () => {
      if (busy) return;
      busy = true;
      const serial = ++sequence.current;
      if (lastFilter.current !== filterKey) { setResult(null); setFailure(null); lastFilter.current = filterKey; }
      setLoading(true);
      try { const bundle = await read(request); if (active && serial === sequence.current) { setResult({ key: filterKey, bundle }); setFailure(null); } }
      catch (e) { if (active && serial === sequence.current) setFailure({ key: filterKey, message: runtimeError(e) }); }
      finally { busy = false; if (active && serial === sequence.current) setLoading(false); }
    };
    void refresh();
    const interval = setInterval(() => { if (!document.hidden) void refresh(); }, 10_000);
    const visible = () => { if (!document.hidden) void refresh(); };
    document.addEventListener('visibilitychange', visible);
    let stopPriceListener: (() => void) | null = null;
    // A missing event channel still has polling and visibility recovery. StrictMode
    // can dispose this effect before listen resolves, so release that late handle.
    void onPriceRulesChanged(() => { if (active && !document.hidden) void refresh(); })
      .then(stop => { if (active) stopPriceListener = stop; else stop(); })
      .catch(() => {});
    return () => { active = false; ++sequence.current; stopPriceListener?.(); clearInterval(interval); document.removeEventListener('visibilitychange', visible); };
  }, [filterKey, refreshRevision, read]);
  // Also guard the frame before the new filter's effect starts its request.
  return { bundle: result?.key === filterKey ? result.bundle : null, error: failure?.key === filterKey ? failure.message : null, loading: loading || lastFilter.current !== filterKey };
}
