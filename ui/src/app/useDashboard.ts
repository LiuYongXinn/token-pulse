import { useEffect, useRef, useState } from 'react';
import type { DashboardBundle, DashboardRequest } from '../shared/generated/contracts';
import { getDashboardBundle, runtimeError } from '../shared/runtime';

export function useDashboard(request: DashboardRequest, refreshRevision: number) {
  const [result, setResult] = useState<{ key: string; bundle: DashboardBundle } | null>(null);
  const [failure, setFailure] = useState<{ key: string; message: string } | null>(null);
  const [loading, setLoading] = useState(true);
  const sequence = useRef(0);
  const filterKey = JSON.stringify(request);
  const lastFilter = useRef<string | null>(null);
  useEffect(() => {
    let active = true;
    const refresh = async () => {
      const serial = ++sequence.current;
      if (lastFilter.current !== filterKey) { setResult(null); setFailure(null); lastFilter.current = filterKey; }
      setLoading(true);
      try { const bundle = await getDashboardBundle(request); if (active && serial === sequence.current) { setResult({ key: filterKey, bundle }); setFailure(null); } }
      catch (e) { if (active && serial === sequence.current) setFailure({ key: filterKey, message: runtimeError(e) }); }
      finally { if (active && serial === sequence.current) setLoading(false); }
    };
    void refresh();
    const interval = setInterval(() => { if (!document.hidden) void refresh(); }, 10_000);
    const visible = () => { if (!document.hidden) void refresh(); };
    document.addEventListener('visibilitychange', visible);
    return () => { active = false; ++sequence.current; clearInterval(interval); document.removeEventListener('visibilitychange', visible); };
  }, [filterKey, refreshRevision]);
  // Guard during render too: a new selector must never label an older scope,
  // even in the frame before its effect starts the next request.
  return { bundle: result?.key === filterKey ? result.bundle : null, error: failure?.key === filterKey ? failure.message : null, loading: loading || lastFilter.current !== filterKey };
}
