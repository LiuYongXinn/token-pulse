import { useRef, useState } from 'react';
import type { PriceBasis } from '../shared/generated/contracts';
import { parsePriceInstant, utcPriceInput } from '../shared/price-basis';
import './price-basis-filter.css';

export function PriceBasisFilter({ basis, disabled, onChange }: { basis: PriceBasis; disabled: boolean; onChange: (basis: PriceBasis) => void }) {
  const [draft, setDraft] = useState<string | null>(null), [error, setError] = useState<string | null>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const close = () => { setDraft(null); setError(null); trigger.current?.focus(); };
  return <div className="price-basis-filter">
    <select aria-label="计价依据" value={basis.mode} disabled={disabled} onChange={event => {
      setDraft(null); setError(null);
      onChange(event.target.value === 'event_time' ? { mode: 'event_time' } : { mode: 'specified_time', specified_at_ms: Date.now() });
    }}><option value="event_time">按事件发生时价格</option><option value="specified_time">按指定时刻价格重估</option></select>
    {basis.mode === 'specified_time' && <>
      <button ref={trigger} className="price-instant-label" disabled={disabled} title="编辑明确估价时点（UTC）" onClick={() => { setDraft(utcPriceInput(basis.specified_at_ms)); setError(null); }}>{new Date(basis.specified_at_ms).toISOString()}</button>
      <button disabled={disabled} title="将估价时点更新为本机当前时间" onClick={() => { onChange({ mode: 'specified_time', specified_at_ms: Date.now() }); setDraft(null); setError(null); }}>取当前时刻</button>
    </>}
    {draft !== null && <form className="price-instant-editor" aria-label="指定估价时点" onSubmit={event => {
      event.preventDefault(); const instant = parsePriceInstant(draft);
      if (instant === null) { setError('请输入有效的 UTC 日期和时刻。'); return; }
      onChange({ mode: 'specified_time', specified_at_ms: instant }); close();
    }} onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); close(); } }}>
      <label>估价时点（UTC）<input type="datetime-local" autoFocus step="0.001" aria-label="估价时点（UTC）" value={draft} required onChange={event => setDraft(event.target.value)} /></label>
      {error && <p role="alert">{error}</p>}
      <div><button type="submit" className="primary">应用估价时点</button><button type="button" onClick={close}>取消</button></div>
    </form>}
  </div>;
}
