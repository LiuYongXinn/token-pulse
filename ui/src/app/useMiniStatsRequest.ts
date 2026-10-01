import { useEffect, useRef } from 'react';
import type { MiniStatsRequest } from '../shared/generated/contracts';
import { getMiniStatsRequest, onMiniStatsRequested, runtimeError } from '../shared/runtime';

/** Startup/visibility read recovers a request emitted before the main React tree was ready. */
export function useMiniStatsRequest(accept: (request: MiniStatsRequest) => void, onError: (error: string) => void) {
  const latest = useRef(accept); latest.current = accept;
  const reportError = useRef(onError); reportError.current = onError;
  const accepted = useRef<string | null>(null);
  useEffect(() => {
    let active = true, serial = 0, stop: (() => void) | null = null;
    const read = async (report = false) => {
      const current = ++serial;
      try {
        const request = await getMiniStatsRequest();
        if (active && serial === current && request !== null && request.request_id !== accepted.current) {
          const { range, heatmap_range } = request.calendar;
          if (!Number.isSafeInteger(range.start_ms) || !Number.isSafeInteger(range.end_ms) || range.start_ms >= range.end_ms || heatmap_range.timezone !== range.timezone) throw new Error('小窗统计范围无效。');
          accepted.current = request.request_id; latest.current(request);
        }
      } catch (error) { if (active && report) reportError.current(runtimeError(error)); }
    };
    void onMiniStatsRequested(() => { if (active) void read(true); }).then(unsubscribe => { if (active) { stop = unsubscribe; void read(); } else unsubscribe(); }).catch(() => {});
    const visible = () => { if (!document.hidden) void read(); };
    document.addEventListener('visibilitychange', visible);
    return () => { active = false; ++serial; stop?.(); document.removeEventListener('visibilitychange', visible); };
  }, []);
}
