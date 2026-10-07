import { useEffect, useRef, useState } from 'react';
import type { CalendarSelection, CalendarSelectionResult } from '../shared/generated/contracts';
import { calendarDateLabel } from '../shared/main-filter';
import './date-filter.css';

export function DateFilter({ selection, calendar, timezone, disabled, onChange }: { selection: CalendarSelection; calendar: CalendarSelectionResult | null; timezone: string | null; disabled: boolean; onChange: (value: CalendarSelection) => void }) {
  const [draft, setDraft] = useState<{ start: string; end: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null), startInput = useRef<HTMLInputElement>(null), select = useRef<HTMLSelectElement>(null);
  const open = () => {
    if (!calendar && selection.kind !== 'custom') return;
    setDraft(selection.kind === 'custom' ? { start: selection.start_date, end: selection.end_date_inclusive } : { start: calendarDateLabel(calendar!.range.start_ms, calendar!.range.timezone), end: calendarDateLabel(calendar!.range.end_ms - 1, calendar!.range.timezone) });
    setError(null);
  };
  const close = () => { setDraft(null); setError(null); select.current?.focus(); };
  useEffect(() => {
    if (!draft) return;
    startInput.current?.focus();
    const dismiss = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) { setDraft(null); setError(null); } };
    document.addEventListener('pointerdown', dismiss);
    return () => document.removeEventListener('pointerdown', dismiss);
  }, [draft !== null]);
  // A newly applied heatmap day, preset, or timezone closes an obsolete editor.
  const appliedKey = JSON.stringify([selection, timezone]);
  useEffect(() => { setDraft(null); setError(null); }, [appliedKey]);
  return <div className="date-filter" ref={root}>
    <div className="date-presets" role="group" aria-label="快捷日期">{([['today', '今天'], ['last7', '近 7 天'], ['last30', '近 30 天']] as const).map(([kind, label]) => <button key={kind} type="button" aria-pressed={selection.kind === kind} disabled={disabled} onClick={() => { setDraft(null); setError(null); onChange({ kind }); }}>{label}</button>)}</div>
    <select aria-label="日期范围" ref={select} value={draft ? 'custom' : selection.kind} disabled={disabled} onChange={event => { const value = event.target.value; if (value === 'custom') open(); else { setDraft(null); setError(null); onChange({ kind: value as 'today' | 'last7' | 'last30' }); } }}>
      <option value="today">今日</option><option value="last7">近 7 日</option><option value="last30">近 30 日</option><option value="custom" disabled={!calendar && selection.kind !== 'custom'}>自定义日期</option>
    </select>
    {selection.kind === 'custom' && !draft && <button className="date-range-label" title="编辑已应用日期" disabled={disabled || !timezone} onClick={open}>{selection.start_date} — {selection.end_date_inclusive}</button>}
    {draft && <form className="date-popup" role="dialog" aria-label="自定义日期" onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); close(); } }} onSubmit={event => {
      event.preventDefault();
      if (!/^\d{4}-\d{2}-\d{2}$/.test(draft.start) || !/^\d{4}-\d{2}-\d{2}$/.test(draft.end) || draft.start > draft.end) { setError('请选择有效日期，结束日期不能早于开始日期。'); return; }
      onChange({ kind: 'custom', start_date: draft.start, end_date_inclusive: draft.end }); close();
    }}>
      <strong>自定义日期</strong>
      <label>开始日期<input ref={startInput} type="date" aria-label="开始日期" value={draft.start} required onChange={event => { const start = event.target.value; setDraft(value => value && { ...value, start }); }} /></label>
      <label>结束日期（包含当天）<input type="date" aria-label="结束日期（包含当天）" value={draft.end} required onChange={event => { const end = event.target.value; setDraft(value => value && { ...value, end }); }} /></label>
      {error && <p className="date-error" role="alert">{error}</p>}
      <div className="date-actions"><button className="primary" type="submit">应用日期</button><button type="button" onClick={close}>取消</button></div>
    </form>}
  </div>;
}
