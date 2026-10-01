import { useCallback, useEffect, useRef, useState } from 'react';
import type { CalendarSelection, CalendarSelectionResult, DisplaySettingsSnapshot } from '../shared/generated/contracts';
import { getDisplaySettings, onSettingsChanged, resolveCalendarSelection, runtimeError, setDisplayTimezone } from '../shared/runtime';

/** Saved timezone is authoritative; every date range comes from Rust. */
export function useMainCalendar(selection: CalendarSelection, ready: boolean, refreshRevision: number, clock: number) {
  const [settings, setSettings] = useState<DisplaySettingsSnapshot | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [resolved, setResolved] = useState<{ key: string; value: CalendarSelectionResult } | null>(null);
  const [failure, setFailure] = useState<{ key: string; error: string } | null>(null);
  const currentSettings = useRef<DisplaySettingsSnapshot | null>(null);
  const mounted = useRef(false), settingsSequence = useRef(0);
  const acceptSettings = useCallback((snapshot: DisplaySettingsSnapshot) => {
    if (currentSettings.current && BigInt(snapshot.settings_revision) < BigInt(currentSettings.current.settings_revision)) return;
    currentSettings.current = snapshot; setSettings(snapshot); setSettingsError(null);
  }, []);
  const reloadSettings = useCallback(async () => {
    const serial = ++settingsSequence.current;
    try {
      let snapshot = await getDisplaySettings();
      if (snapshot.preferences.display_timezone === null) snapshot = await setDisplayTimezone({ kind: 'initialize', system_timezone: Intl.DateTimeFormat().resolvedOptions().timeZone });
      if (mounted.current && serial === settingsSequence.current) acceptSettings(snapshot);
    } catch (error) { if (mounted.current && serial === settingsSequence.current) setSettingsError(runtimeError(error)); }
  }, [acceptSettings]);
  useEffect(() => {
    if (!ready) return;
    mounted.current = true; void reloadSettings();
    let active = true, stop: (() => void) | null = null;
    void onSettingsChanged(() => { if (active) void reloadSettings(); }).then(unsubscribe => { if (active) stop = unsubscribe; else unsubscribe(); }).catch(() => {});
    const visible = () => { if (!document.hidden) void reloadSettings(); };
    document.addEventListener('visibilitychange', visible);
    return () => { active = false; mounted.current = false; ++settingsSequence.current; stop?.(); document.removeEventListener('visibilitychange', visible); };
  }, [ready, refreshRevision, reloadSettings]);
  const timezone = settings?.preferences.display_timezone ?? null;
  const key = JSON.stringify([selection, timezone]);
  const minute = Math.floor(clock / 60_000);
  useEffect(() => {
    if (!ready || timezone === null) return;
    let active = true;
    void resolveCalendarSelection({ timezone, selection }).then(value => {
      if (value.range.timezone !== timezone || value.heatmap_range.timezone !== timezone || !Number.isSafeInteger(value.range.start_ms) || !Number.isSafeInteger(value.range.end_ms) || value.range.start_ms >= value.range.end_ms || !Number.isSafeInteger(value.heatmap_range.start_ms) || !Number.isSafeInteger(value.heatmap_range.end_ms) || value.heatmap_range.start_ms >= value.heatmap_range.end_ms) throw new Error('后台日期范围与请求不一致，请重新查询。');
      if (active) { setResolved({ key, value }); setFailure(null); }
    }).catch(error => { if (active) setFailure({ key, error: runtimeError(error) }); });
    return () => { active = false; };
  }, [ready, key, minute, refreshRevision, timezone, selection]);
  return { settings, settingsError, reloadSettings, acceptSettings, calendar: ready && resolved?.key === key ? resolved.value : null, calendarError: failure?.key === key ? failure.error : null };
}
