import { useEffect, useRef, useState } from 'react';
import type { MiniOpacitySnapshot } from '../shared/generated/contracts';
import { getMiniOpacity, runtimeError, setMiniOpacity } from '../shared/runtime';

export function MiniOpacityPanel({ revision }: { revision: string | null }) {
  const [snapshot, setSnapshot] = useState<MiniOpacitySnapshot | null>(null);
  const [draft, setDraft] = useState<{ value: number; revision: string } | null>(null);
  const [error, setError] = useState<string | null>(null), [readError, setReadError] = useState<string | null>(null), [busy, setBusy] = useState(false);
  const active = useRef(false), serial = useRef(0), writing = useRef(false);
  const reload = async () => { const request = ++serial.current; try { const value = await getMiniOpacity(); if (active.current && request === serial.current) { setSnapshot(value); setReadError(null); } } catch (e) { if (active.current && request === serial.current) setReadError(runtimeError(e)); } };
  useEffect(() => { active.current = true; void reload(); return () => { active.current = false; ++serial.current; }; }, [revision]);
  const save = async () => {
    if (!draft || !snapshot?.supported || writing.current) return;
    writing.current = true; setBusy(true); setError(null);
    try { const value = await setMiniOpacity({ opacity_percent: draft.value, expected_settings_revision: draft.revision }); if (active.current) { ++serial.current; setSnapshot(value); setDraft(null); setReadError(null); } }
    catch (e) { if (active.current) setError(runtimeError(e)); }
    finally { writing.current = false; if (active.current) setBusy(false); }
  };
  return <section className="mini-opacity-settings" aria-label="小窗透明度设置"><h3>小窗透明度</h3>
    <p className="muted">0%–100%，数值越高越不透明。0% 为完全透明，可在这里恢复。</p>
    <label>悬浮窗透明度<input type="range" aria-label="悬浮窗透明度" min="0" max="100" step="1" value={draft?.value ?? snapshot?.opacity_percent ?? 100} disabled={!snapshot?.supported || busy} onChange={e => { if (snapshot) setDraft(old => ({ value: Number(e.target.value), revision: old?.revision ?? snapshot.settings_revision })); }} /></label>
    <p role="status" aria-label="小窗透明度状态">{snapshot ? `已保存 ${snapshot.opacity_percent}%${snapshot.supported ? '' : ' · 当前平台暂不支持原生透明度'}` : '尚未读取小窗透明度'}{draft ? ` · 待保存 ${draft.value}%` : ''}{readError ? ` · 读取失败：${readError}` : ''}</p>
    {error && <p className="notice" role="alert">{error}</p>}
    <div className="display-setting-actions"><button className="primary" disabled={!draft || !snapshot?.supported || busy} onClick={() => void save()}>{busy ? '正在应用并保存…' : '保存小窗透明度'}</button><button disabled={busy} onClick={() => void reload()}>刷新透明度</button><button disabled={!draft || busy} onClick={() => { setDraft(null); setError(null); }}>重置透明度草稿</button></div>
  </section>;
}
