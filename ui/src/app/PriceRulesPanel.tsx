import { useEffect, useRef, useState } from 'react';
import type { FormEvent } from 'react';
import type { ModelAlias, ModelAliasDraft, PriceRule, PriceRulesSnapshot, SourceSummary } from '../shared/generated/contracts';
import { getPriceRules, getSources, mutateModelAlias, retirePriceRule, runtimeError, savePriceRule } from '../shared/runtime';
import { blankPriceForm, editPriceForm, priceDraft, pricePerMillion } from '../shared/price-form';
import type { PriceForm } from '../shared/price-form';
import './model-aliases.css';
import { OfflinePricesPanel } from './OfflinePricesPanel';
import { PriceRevaluePanel } from './PriceRevaluePanel';

type Editor = { ruleId: string | null; revision: string; form: PriceForm };
type AliasEditor = { aliasId: string | null; revision: string; draft: ModelAliasDraft };
const date = (value: number) => new Date(value).toISOString().replace('T', ' ').replace(/\.000Z$/, ' UTC').replace(/Z$/, ' UTC');

export function PriceRulesPanel({ onChanged }: { onChanged: () => void }) {
  const [snapshot, setSnapshot] = useState<PriceRulesSnapshot | null>(null);
  const [sources, setSources] = useState<SourceSummary[] | null>(null);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [editor, setEditor] = useState<Editor | null>(null);
  const [aliasEditor, setAliasEditor] = useState<AliasEditor | null>(null);
  const [revisionInput, setRevisionInput] = useState('');
  const [historical, setHistorical] = useState(false);
  const mounted = useRef(false), sequence = useRef(0), busyRef = useRef(false);
  const refresh = async (revision: string | null = null) => {
    if (busyRef.current) return;
    busyRef.current = true; setBusy(true);
    const request = ++sequence.current;
    try {
      if (revision !== null && !/^(0|[1-9][0-9]*)$/.test(revision)) throw new Error('价格版本须为非负整数。');
      const result = await getPriceRules(revision);
      if (mounted.current && request === sequence.current) { setSnapshot(result); setHistorical(revision !== null); setError(null); }
    } catch (e) { if (mounted.current && request === sequence.current) setError(runtimeError(e)); }
    finally { if (mounted.current && request === sequence.current) { busyRef.current = false; setBusy(false); } }
  };
  useEffect(() => {
    let active = true;
    mounted.current = true; void refresh();
    void getSources().then(result => { if (active) setSources(result.sources); }).catch(e => { if (active) setSourceError(runtimeError(e)); });
    return () => { active = false; mounted.current = false; busyRef.current = false; ++sequence.current; };
  }, []);
  const run = async (action: () => Promise<PriceRulesSnapshot>, message: string, aliasAction = false) => {
    if (busyRef.current) return;
    busyRef.current = true; setBusy(true); ++sequence.current;
    try {
      const result = await action();
      if (mounted.current) { setSnapshot(result); setEditor(null); setAliasEditor(null); setHistorical(false); setError(null); setNotice(message); onChanged(); }
    } catch (e) { if (mounted.current) { setError(aliasAction && typeof e === 'object' && e !== null && 'code' in e && e.code === 'PRICE_RULE_CONFLICT' ? '模型别名与现有映射冲突，请检查重复、链式或循环映射。' : runtimeError(e)); setNotice(null); } }
    finally { busyRef.current = false; if (mounted.current) setBusy(false); }
  };
  const edit = (rule: PriceRule | null) => {
    if (!snapshot || busyRef.current || historical || aliasEditor !== null) return;
    setEditor({ ruleId: rule?.rule_id ?? null, revision: snapshot.price_revision, form: rule ? editPriceForm(rule) : blankPriceForm() });
    setError(null); setNotice(null);
  };
  const editAlias = (alias: ModelAlias | null) => {
    if (!snapshot || busyRef.current || historical || editing) return;
    setAliasEditor({ aliasId: alias?.alias_id ?? null, revision: snapshot.price_revision, draft: {
      provider: alias?.provider ?? '', alias: alias?.alias ?? '', canonical_model: alias?.canonical_model ?? '',
    } });
    setError(null); setNotice(null);
  };
  const submitAlias = (event: FormEvent) => {
    event.preventDefault(); if (!aliasEditor || busyRef.current) return;
    const draft = Object.fromEntries(Object.entries(aliasEditor.draft).map(([key, value]) => [key, value.trim()])) as ModelAliasDraft;
    if (Object.values(draft).some(value => !value || new TextEncoder().encode(value).length > 256 || /[\x00-\x1f\x7f]/.test(value))) {
      setError('提供方与模型标识不能为空、包含控制字符或超过 256 字节。'); return;
    }
    if (draft.alias === draft.canonical_model) { setError('别名须指向不同的标准模型标识。'); return; }
    const request = aliasEditor.aliasId === null ? { kind: 'create' as const, draft } : { kind: 'replace' as const, alias_id: aliasEditor.aliasId, draft };
    void run(() => mutateModelAlias(request, aliasEditor.revision), '模型别名已保存。', true);
  };
  const editing = editor !== null || aliasEditor !== null;
  const change = (field: keyof PriceForm, value: string) => setEditor(current => current && { ...current, form: { ...current.form, [field]: value } });
  const submit = (event: FormEvent) => {
    event.preventDefault(); if (!editor || busyRef.current) return;
    try {
      const draft = priceDraft(editor.form);
      const request = editor.ruleId === null ? { kind: 'create' as const, draft } : { kind: 'replace' as const, rule_id: editor.ruleId, draft };
      void run(() => savePriceRule(request, editor.revision), '价格规则已保存。');
    } catch (e) { setError(runtimeError(e)); }
  };
  const form = editor?.form;
  const textField = (field: keyof PriceForm, label: string, optional = false) => <label className="price-field">{label}<input value={form?.[field] ?? ''} onChange={e => change(field, e.target.value)} required={!optional} maxLength={field === 'reference' ? 2048 : 256} autoComplete="off" /></label>;
  return <section className="panel price-panel" role="tabpanel" aria-label="价格规则设置">
    <div className="panel-heading"><div><h2>价格规则</h2></div><div className="price-actions"><button disabled={busy} onClick={() => void refresh()}>刷新当前版本</button><button className="primary" disabled={busy || !snapshot || historical || editing} onClick={() => edit(null)}>新增规则</button></div></div>
    <details className="price-history"><summary>价格历史</summary><div className="price-version"><span>{snapshot ? `${historical ? '历史' : '当前'}价格版本 ${snapshot.price_revision}` : '价格版本未就绪'}</span><form onSubmit={e => { e.preventDefault(); void refresh(revisionInput); }}><label>查看历史版本<input aria-label="历史价格版本" inputMode="numeric" value={revisionInput} onChange={e => setRevisionInput(e.target.value)} maxLength={19} disabled={busy || editing} /></label><button disabled={busy || editing || !revisionInput}>查看</button></form></div></details>
    {error && <div className="notice" role="alert">{error}</div>}{notice && <p role="status" className="price-notice">{notice}</p>}
    {historical && <p className="muted">历史版本仅供复核；刷新当前版本后可编辑。</p>}
    {!snapshot && !error && <p role="status" className="muted">正在读取价格规则…</p>}
    {editor && form && <form className="price-editor" aria-label={editor.ruleId ? '替换价格规则' : '新增价格规则'} onSubmit={submit}>
      <div className="panel-heading"><div><h3>{editor.ruleId ? '替换价格规则' : '新增价格规则'}</h3><p className="muted">模型与提供方须填写日志中的确切标识。</p></div><button type="button" disabled={busy} onClick={() => { setEditor(null); setError(null); }}>取消编辑</button></div>
      <fieldset disabled={busy} className="price-fields">
        {textField('provider', '提供方')}{textField('model', '确切模型标识')}
        <label className="price-field">来源范围<select value={form.source} onChange={e => change('source', e.target.value)}><option value="">全部来源</option>{sources?.map(source => <option key={source.source_id} value={source.source_id}>{source.root_path}{source.removed ? '（历史来源）' : ''}</option>)}{form.source && !sources?.some(source => source.source_id === form.source) && <option value={form.source}>{form.source}（来源详情未加载）</option>}</select></label>
        {textField('currency', '货币代码')}
        <label className="price-field">开始时间（UTC）<input type="datetime-local" step="0.001" required value={form.from} onChange={e => change('from', e.target.value)} /></label>
        <label className="price-field">截止时间（UTC，可留空）<input type="datetime-local" step="0.001" value={form.to} onChange={e => change('to', e.target.value)} /></label>
        {textField('input', '输入单价 / 百万 Token')}{textField('cached', '缓存输入单价 / 百万 Token（可留空）', true)}{textField('write', '缓存写入单价 / 百万 Token（可留空）', true)}{textField('output', '输出单价 / 百万 Token')}{textField('priority', '优先级（0–10000）')}
        <div className="price-wide">{textField('reference', '价格依据或链接（可留空）', true)}</div>
      </fieldset>
      {sourceError && <p className="muted">来源详情未加载：{sourceError}。可保存全局规则，已有来源标识保留。</p>}
      <p className="muted">单价最多 9 位小数；留空表示未知，0 表示免费。截止时刻不包含。来源专用规则优先，同范围按优先级匹配。</p>
      <div className="price-actions"><button className="primary" disabled={busy} type="submit">{busy ? '正在发布…' : '保存规则'}</button></div>
    </form>}
    {snapshot?.rules.length === 0 && <div className="price-empty"><h3>此版本暂无价格规则</h3></div>}
    {snapshot && snapshot.rules.length > 0 && <div className="price-table-wrap"><table className="price-table price-rule-table"><caption>单价单位：所列货币 / 百万 Token</caption><thead><tr><th>模型 / 提供方</th><th>普通输入</th><th>缓存命中</th><th>缓存写入</th><th>输出</th><th>适用范围 / 有效期</th><th>操作</th></tr></thead><tbody>{snapshot.rules.map(rule => <tr key={rule.rule_id}><td><strong>{rule.model_exact}</strong><small>{rule.provider} · {rule.currency}</small><small>{rule.origin === 'custom' ? '自定义' : '离线目录'} · 优先级 {rule.priority}</small>{rule.origin_reference && <small className="price-reference">{rule.origin_reference}</small>}</td><td className="price-value">{pricePerMillion(rule.input_rate_atoms)}</td><td className="price-value">{pricePerMillion(rule.cached_rate_atoms)}</td><td className="price-value">{pricePerMillion(rule.cache_write_rate_atoms)}</td><td className="price-value">{pricePerMillion(rule.output_rate_atoms)}</td><td><span>{rule.source_id === null ? '全部来源' : sources?.find(source => source.source_id === rule.source_id)?.root_path ?? rule.source_id}</span><small>{date(rule.effective_from_ms)} 起</small><small>{rule.effective_to_ms === null ? '无截止时间' : `${date(rule.effective_to_ms)} 止`}</small></td><td>{rule.origin === 'custom' && !historical ? <div className="price-row-actions"><button disabled={busy || editing} onClick={() => edit(rule)}>编辑</button><button disabled={busy || editing} onClick={() => void run(() => retirePriceRule(rule.rule_id, snapshot.price_revision), '规则已停用。')}>停用</button></div> : <span className="muted">只读</span>}</td></tr>)}</tbody></table></div>}
    {snapshot && <PriceRevaluePanel key={snapshot.price_revision} revision={snapshot.price_revision} historical={historical} />}
    {snapshot && <OfflinePricesPanel revision={snapshot.price_revision} />}
    {snapshot && <section className="model-aliases" aria-label="模型别名">
      <div className="panel-heading"><div><h3>模型别名</h3><p className="muted">同一提供方内，将日志模型映射到计价模型。</p></div><button disabled={busy || historical || editing} onClick={() => editAlias(null)}>新增别名</button></div>
      {aliasEditor && <form className="price-editor" aria-label={aliasEditor.aliasId ? '替换模型别名' : '新增模型别名'} onSubmit={submitAlias}>
        <div className="panel-heading"><div><h3>{aliasEditor.aliasId ? '替换模型别名' : '新增模型别名'}</h3></div><button type="button" disabled={busy} onClick={() => { setAliasEditor(null); setError(null); }}>取消别名编辑</button></div>
        <fieldset disabled={busy} className="price-fields">{([['provider', '提供方'], ['alias', '日志模型标识'], ['canonical_model', '标准模型标识']] as const).map(([field, label]) => <label className="price-field" key={field}>{label}<input required maxLength={256} autoComplete="off" value={aliasEditor.draft[field]} onChange={event => setAliasEditor(current => current && { ...current, draft: { ...current.draft, [field]: event.target.value } })} /></label>)}</fieldset>
        <p className="muted">不支持重复、链式或循环别名。</p>
        <button className="primary" type="submit" disabled={busy}>{busy ? '正在发布…' : '保存别名'}</button>
      </form>}
      {snapshot.aliases.length === 0 ? <p className="muted model-alias-empty">此版本暂无模型别名。</p> : <div className="price-table-wrap"><table className="price-table model-alias-table" aria-label="模型别名"><thead><tr><th>日志模型 / 提供方</th><th>标准模型</th><th>操作</th></tr></thead><tbody>{snapshot.aliases.map(alias => <tr key={alias.alias_id}><td><strong>{alias.alias}</strong><small>{alias.provider}</small></td><td>{alias.canonical_model}</td><td>{alias.alias_id.startsWith('alias-custom-') && !historical ? <div className="price-row-actions"><button disabled={busy || editing} onClick={() => editAlias(alias)}>编辑别名</button><button disabled={busy || editing} onClick={() => void run(() => mutateModelAlias({ kind: 'retire', alias_id: alias.alias_id }, snapshot.price_revision), '别名已停用。', true)}>停用别名</button></div> : <span className="muted">只读</span>}</td></tr>)}</tbody></table></div>}
    </section>}
  </section>;
}
