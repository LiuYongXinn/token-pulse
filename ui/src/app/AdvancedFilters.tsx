import { useEffect, useRef, useState } from 'react';
import type { FilterOption, UsageFilter } from '../shared/generated/contracts';
import { fullTokens } from '../shared/format';
import { useFilterOptions } from './useFilterOptions';
import './filters.css';

export type AdvancedDimension = 'models' | 'projects' | 'sessions';
export type FilterChoice = Pick<FilterOption, 'key' | 'display_name'>;
export type FilterChoices = Record<AdvancedDimension, FilterChoice | null>;
const labels: Record<AdvancedDimension, string> = { models: '模型', projects: '项目', sessions: '会话' };
const dimensions: AdvancedDimension[] = ['models', 'projects', 'sessions'];

export function AdvancedFilters({ filter, choices, disabled, onChange }: { filter: UsageFilter; choices: FilterChoices; disabled: boolean; onChange: (dimension: AdvancedDimension, choice: FilterOption | null) => void }) {
  const [open, setOpen] = useState<AdvancedDimension | null>(null);
  const [search, setSearch] = useState('');
  const [debounced, setDebounced] = useState({ dimension: open, text: '' });
  const root = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const buttons = useRef<Partial<Record<AdvancedDimension, HTMLButtonElement | null>>>({});
  const searchReady = open === debounced.dimension && search === debounced.text;
  const searchValid = [...search].length <= 256 && !/[\u0000-\u001f\u007f-\u009f]/.test(search);
  const candidates = useFilterOptions(open && searchValid ? { filter, dimension: open, search: debounced.dimension === open ? debounced.text : '', page_size: 50 } : null);
  useEffect(() => { const timer = setTimeout(() => setDebounced({ dimension: open, text: search }), 250); return () => clearTimeout(timer); }, [search, open]);
  useEffect(() => {
    if (!open) return;
    input.current?.focus();
    const pointer = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(null); };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') { event.preventDefault(); setOpen(null); buttons.current[open]?.focus(); } };
    document.addEventListener('pointerdown', pointer); document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', pointer); document.removeEventListener('keydown', escape); };
  }, [open]);
  const select = (choice: FilterOption | null) => { if (!open) return; onChange(open, choice); setOpen(null); buttons.current[open]?.focus(); };
  return <div className="advanced-filters" ref={root}>
    {dimensions.map(dimension => <button key={dimension} ref={button => { buttons.current[dimension] = button; }} role="combobox" aria-label={labels[dimension]} aria-expanded={open === dimension} aria-controls={open === dimension ? 'filter-picker' : undefined} aria-haspopup="dialog" className={choices[dimension] ? 'facet-trigger selected' : 'facet-trigger'} disabled={disabled} title={choices[dimension]?.display_name} onClick={() => { setOpen(open === dimension ? null : dimension); setSearch(''); setDebounced({ dimension, text: '' }); }}>{choices[dimension] ? `${labels[dimension]}：${choices[dimension].display_name}` : `全部${labels[dimension]}`}<span aria-hidden="true">⌄</span></button>)}
    {open && <div className="facet-popup" id="filter-picker" role="dialog" aria-label={`选择${labels[open]}`} onKeyDown={event => {
      const options = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="option"]'));
      const index = options.indexOf(event.target as HTMLButtonElement);
      if (event.target === input.current && event.key === 'ArrowDown' && options.length) { event.preventDefault(); options[0].focus(); }
      else if (index >= 0) {
        const target = event.key === 'ArrowDown' ? options[Math.min(index + 1, options.length - 1)] : event.key === 'ArrowUp' ? options[Math.max(index - 1, 0)] : event.key === 'Home' ? options[0] : event.key === 'End' ? options.at(-1) : undefined;
        if (target) { event.preventDefault(); target.focus(); }
      }
    }}>
      <div className="facet-heading"><strong>{labels[open]}筛选</strong><button className="text-button" onClick={() => select(null)}>清除此筛选</button></div>
      <input ref={input} aria-label={`搜索${labels[open]}`} placeholder={`搜索${labels[open]}名称`} value={search} onChange={event => setSearch(event.target.value)} autoComplete="off" />
      {!searchValid ? <p role="alert" className="facet-error">搜索最多 256 个字符，不能包含控制字符。</p> : !searchReady ? <p className="facet-note" role="status">正在准备搜索…</p> : <>
        {candidates.error && <p className="facet-error" role="alert">{candidates.error}</p>}
        <div role="listbox" aria-label={`${labels[open]}候选`} id="filter-candidates" className="facet-options">{candidates.options.map(option => <button key={option.key ?? 'unknown'} role="option" aria-selected={choices[open]?.key === option.key} onClick={() => select(option)} className="facet-option"><span className="facet-label">{option.display_name}</span><span className="facet-count" title={`${fullTokens(option.count)} 条用量记录`}>{fullTokens(option.count)}</span></button>)}</div>
        {candidates.loading && <p className="facet-note" role="status">正在读取候选…</p>}
        {!candidates.loading && !candidates.error && candidates.options.length === 0 && <p className="facet-note">当前范围没有匹配候选。</p>}
        {candidates.limited && <p className="facet-note">已显示 1,000 个候选，请输入更具体的搜索。</p>}
        <div className="facet-actions">{candidates.more && <button disabled={candidates.loading} onClick={candidates.loadMore}>加载下一页</button>}<button disabled={candidates.loading} onClick={candidates.reload}>重新查询</button></div>
      </>}
    </div>}
  </div>;
}
