import { useEffect, useRef } from 'react';
import type { MainNavigationIntent } from '../shared/generated/contracts';
import { getMainNavigation, onMainNavigationChanged, runtimeError } from '../shared/runtime';

/** One retained revision orders stats and settings across startup, visibility and late responses. */
export function useMainNavigation(accept: (intent: MainNavigationIntent) => void, onError: (error: string) => void) {
  const latest = useRef(accept); latest.current = accept;
  const report = useRef(onError); report.current = onError;
  const accepted = useRef(0n);
  useEffect(() => {
    let active = true, serial = 0, stop: (() => void) | null = null;
    const read = async (reportFailure = false) => {
      const current = ++serial;
      try {
        const snapshot = await getMainNavigation();
        if (!active || current !== serial || BigInt(snapshot.revision) <= accepted.current) return;
        if (snapshot.intent?.kind === 'mini_stats') {
          const { range, heatmap_range } = snapshot.intent.request.calendar;
          if (!Number.isSafeInteger(range.start_ms) || !Number.isSafeInteger(range.end_ms) || range.start_ms >= range.end_ms || heatmap_range.timezone !== range.timezone) throw new Error('小窗统计范围无效。');
        }
        accepted.current = BigInt(snapshot.revision);
        if (snapshot.intent) latest.current(snapshot.intent);
      } catch (error) { if (active && current === serial && reportFailure) report.current(runtimeError(error)); }
    };
    void onMainNavigationChanged(() => { if (active) void read(true); }).then(unsubscribe => { if (active) { stop = unsubscribe; void read(); } else unsubscribe(); }).catch(() => { if (active) report.current('无法订阅窗口导航，请通过主窗口导航打开。'); });
    const visible = () => { if (!document.hidden) void read(); };
    document.addEventListener('visibilitychange', visible);
    return () => { active = false; ++serial; stop?.(); document.removeEventListener('visibilitychange', visible); };
  }, []);
}
