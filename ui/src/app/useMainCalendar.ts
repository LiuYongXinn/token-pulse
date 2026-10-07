import { useCallback, useEffect, useRef, useState } from 'react';
import type { CalendarSelection, CalendarSelectionResult, DisplaySettingsSnapshot } from '../shared/generated/contracts';
import { getDisplaySettings, onSettingsChanged, resolveCalendarSelection, runtimeError, setDisplayTimezone } from '../shared/runtime';

const ranges = new Map<string, { value: CalendarSelectionResult; resolvedAt: number }>();
const flights = new Map<string, Promise<CalendarSelectionResult>>();

/** Shared Rust-resolved ranges let prepared dates render on their first frame. */
export async function prepareMainCalendar(selection: CalendarSelection, timezone: string, clock: number): Promise<CalendarSelectionResult> {
  const key = JSON.stringify([selection, timezone]);
  const cached = ranges.get(key);
  if (cached && clock >= cached.resolvedAt && clock < cached.value.heatmap_range.end_ms) return cached.value;
  const existing = flights.get(key);
  if (existing) return existing;
  const flight = resolveCalendarSelection({ timezone, selection }).then(value => {
    if (value.range.timezone !== timezone || value.heatmap_range.timezone !== timezone || !Number.isSafeInteger(value.range.start_ms) || !Number.isSafeInteger(value.range.end_ms) || value.range.start_ms >= value.range.end_ms || !Number.isSafeInteger(value.heatmap_range.start_ms) || !Number.isSafeInteger(value.heatmap_range.end_ms) || value.heatmap_range.start_ms >= value.heatmap_range.end_ms) throw new Error('日期读取失败，请重新查询。');
    ranges.delete(key); ranges.set(key, { value, resolvedAt: clock });
    while (ranges.size > 20) ranges.delete(ranges.keys().next().value!);
    return value;
  });
  flights.set(key, flight);
  try { return await flight; } finally { if (flights.get(key) === flight) flights.delete(key); }
}
/** Saved timezone is authoritative; every date range comes from Rust. */
export function useMainCalendar(selection: CalendarSelection, ready: boolean, refreshRevision: number, clock: number) {
  const [settings, setSettings] = useState<DisplaySettingsSnapshot | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [, setResolved] = useState<{ key: string; value: CalendarSelectionResult } | null>(null);
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
  const minute = Math.floor(clock / 60_000);
  const key = JSON.stringify([selection, timezone]);
  const cached = ranges.get(key);
  // A backend-resolved end boundary authorizes reuse only within that same calendar day.
  const reusable = cached && clock >= cached.resolvedAt && clock < cached.value.heatmap_range.end_ms ? cached.value : null;
  useEffect(() => {
    if (!ready || timezone === null) return;
    let active = true;
    void prepareMainCalendar(selection, timezone, clock).then(value => {
      if (active) { setResolved({ key, value }); setFailure(null); }
    }).catch(error => { if (active) setFailure({ key, error: runtimeError(error) }); });
    return () => { active = false; };
  }, [ready, key, minute, refreshRevision, timezone, selection]);
  return { settings, settingsError, reloadSettings, acceptSettings, calendar: ready ? reusable : null, calendarError: failure?.key === key ? failure.error : null };
}
