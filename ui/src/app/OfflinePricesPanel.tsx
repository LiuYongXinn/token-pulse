import { useEffect, useState } from 'react';
import type { OfflinePriceCatalog, OfflinePriceEntry, OfflinePriceTier } from '../shared/generated/contracts';
import { getOfflinePriceCatalog, runtimeError } from '../shared/runtime';
import './offline-prices.css';

const tiers: OfflinePriceTier[] = ['standard', 'batch', 'flex', 'fast', 'ultrafast'];
const title = (value: string) => value[0].toUpperCase() + value.slice(1);
const band = { all: '统一单价', short: '输入 ≤ 272K', long: '输入 > 272K' };
const applicability = (entry: OfflinePriceEntry) => entry.tier !== 'standard' ? '非默认参考模式'
  : entry.context !== 'all' && entry.cache_write_per_million !== null ? '需请求档位与缓存写入'
  : entry.context !== 'all' ? '需每次请求输入量'
  : entry.cache_write_per_million !== null ? '需缓存写入量' : '已接自动参考计价';

export function OfflinePricesPanel({ revision }: { revision: string }) {
  const [catalog, setCatalog] = useState<OfflinePriceCatalog | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [tier, setTier] = useState<OfflinePriceTier | 'all'>('standard');
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let active = true;
    setCatalog(null); setError(null); setLoading(true);
    void getOfflinePriceCatalog(revision).then(result => {
      if (result.price_revision !== revision) throw new Error('离线目录的价格版本不一致，请重新读取。');
      if (active) setCatalog(result.catalog);
    }).catch(reason => { if (active) setError(runtimeError(reason)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [revision, retry]);
  const rows = catalog?.entries.filter(entry => (tier === 'all' || entry.tier === tier)
    && entry.model_exact.includes(search.trim().toLowerCase())) ?? [];
  return <section className="offline-prices" aria-label="离线价格目录">
    <div className="panel-heading"><div><h3>离线价格目录</h3><p className="muted">全球 API 参考单价。</p></div><button disabled={loading} onClick={() => setRetry(value => value + 1)}>重新读取目录</button></div>
    {loading && <p role="status" className="muted">正在读取版本 {revision} 的内置目录…</p>}
    {error && <p role="alert" className="notice">目录读取失败：{error}</p>}
    {!loading && !error && !catalog && <p className="muted">此价格版本尚无内置目录。</p>}
    {catalog && <>
      <p className="muted">核实于 {new Date(catalog.verified_at_ms).toISOString().slice(0, 10)}（UTC） · {catalog.catalog_id} · 价格版本 {revision} · {new Set(catalog.entries.map(entry => entry.model_exact)).size} 个模型 / {catalog.entries.length} 条单价</p>
      <p className="offline-explanation">模式未知时按 Standard 估算；请求输入未知时使用短上下文档，缓存写入未知时按 0 估算。具体假设见明细价格依据。</p>
      <div className="offline-filters"><label>查找目录模型<input value={search} maxLength={128} onChange={event => setSearch(event.target.value)} placeholder="确切模型标识" /></label><label>目录处理模式<select value={tier} onChange={event => setTier(event.target.value as OfflinePriceTier | 'all')}><option value="all">全部模式</option>{tiers.map(value => <option key={value} value={value}>{title(value)}</option>)}</select></label><span className="muted" role="status">{rows.length} 条匹配价格</span></div>
      {rows.length === 0 ? <p className="muted">没有匹配的目录条目。</p> : <div className="offline-table-wrap" tabIndex={0} role="region" aria-label="离线价格表，可横向滚动"><table className="offline-price-table"><caption>USD / 百万 Token</caption><thead><tr><th>模型 / 条件</th><th>输入</th><th>缓存输入</th><th>缓存写入</th><th>输出</th><th>参考适用情况</th></tr></thead><tbody>{rows.map(entry => <tr key={`${entry.model_exact}/${entry.tier}/${entry.context}`}><td><strong>{entry.model_exact}</strong><small>{title(entry.tier)} · {band[entry.context]}</small><a href={entry.reference} target="_blank" rel="noreferrer">官方来源</a></td><td>{entry.input_per_million}</td><td>{entry.cached_per_million ?? '未公布'}</td><td>{entry.cache_write_per_million ?? '未公布'}</td><td>{entry.output_per_million}</td><td>{applicability(entry)}</td></tr>)}</tbody></table></div>}
    </>}
  </section>;
}
