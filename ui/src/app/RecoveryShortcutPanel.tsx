import { useEffect, useRef, useState } from 'react';
import type { RecoveryShortcut, RecoveryShortcutSnapshot } from '../shared/generated/contracts';
import { getRecoveryShortcut, onSettingsChanged, runtimeError, setRecoveryShortcut } from '../shared/runtime';

const labels = { ready: '恢复快捷键已注册', conflict: '快捷键冲突，请更换组合或重新注册', unsupported: '当前平台尚不支持恢复快捷键', unavailable: '恢复快捷键不可用，可重新注册' };
export function RecoveryShortcutPanel() {
  const [snapshot, setSnapshot] = useState<RecoveryShortcutSnapshot | null>(null);
  const [draft, setDraft] = useState<{ key: RecoveryShortcut; revision: string } | null>(null);
  const [error, setError] = useState<string | null>(null), [readError, setReadError] = useState<string | null>(null), [busy, setBusy] = useState(false);
  const active = useRef(false), serial = useRef(0), writing = useRef(false);
  const reload = async () => { const request = ++serial.current; try { const value = await getRecoveryShortcut(); if (active.current && request === serial.current) { setSnapshot(value); setReadError(null); } } catch (e) { if (active.current && request === serial.current) setReadError(runtimeError(e)); } };
  useEffect(() => {
    active.current = true; void reload(); let live = true, stop: (() => void) | null = null;
    void onSettingsChanged(() => { if (live) void reload(); }).then(value => { if (live) stop = value; else value(); }).catch(() => { if (live) setReadError('快捷键状态订阅失败，请手动刷新。'); });
    return () => { live = false; active.current = false; ++serial.current; stop?.(); };
  }, []);
  const change = (patch: Partial<RecoveryShortcut>) => { if (snapshot) setDraft(old => ({ revision: old?.revision ?? snapshot.settings_revision, key: { ...(old?.key ?? snapshot.shortcut), ...patch } })); };
  const value = draft?.key ?? snapshot?.shortcut;
  const save = async () => {
    if (!snapshot || !value || writing.current) return;
    if (!value.control && !value.alt) { setError('恢复快捷键至少需要 Ctrl 或 Alt 修饰键。'); return; }
    writing.current = true; setBusy(true); setError(null);
    try { const saved = await setRecoveryShortcut({ shortcut: value, expected_settings_revision: draft?.revision ?? snapshot.settings_revision }); if (active.current) { ++serial.current; setSnapshot(saved); setDraft(null); setReadError(null); } }
    catch (e) { if (active.current) setError(runtimeError(e)); }
    finally { writing.current = false; if (active.current) setBusy(false); }
  };
  return <section className="recovery-shortcut" aria-label="恢复快捷键设置"><h3>恢复小窗交互</h3>
    <p className="muted">按快捷键显示小窗并恢复鼠标交互。</p>
    <p role="status" aria-label="恢复快捷键注册状态">{readError ? '快捷键状态读取失败，保留上次状态：' : ''}{snapshot ? labels[snapshot.registration] : '尚未读取恢复快捷键'}{readError ? ` · ${readError}` : ''}</p>
    <div className="shortcut-fields">{(['control', 'alt', 'shift'] as const).map((field, index) => <label key={field}><input type="checkbox" aria-label={`恢复快捷键 ${['Ctrl', 'Alt', 'Shift'][index]}`} checked={value?.[field] ?? false} disabled={!snapshot || busy} onChange={e => change({ [field]: e.target.checked })} />{['Ctrl', 'Alt', 'Shift'][index]}</label>)}
      <label>按键<select aria-label="恢复快捷键按键" disabled={!snapshot || busy} value={value?.key ?? ''} onChange={e => change({ key: e.target.value })}>{!value && <option value="">等待配置</option>}{[...'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789', ...Array.from({ length: 11 }, (_, i) => `F${i + 1}`)].map(key => <option key={key} value={key}>{key}</option>)}</select></label></div>
    {error && <p className="notice" role="alert">{error}</p>}
    <div className="display-setting-actions"><button className="primary" disabled={!snapshot || busy || (!draft && snapshot.registration === 'ready') || snapshot.registration === 'unsupported'} onClick={() => void save()}>{busy ? '正在注册并保存…' : draft ? '保存恢复快捷键' : '重新注册恢复快捷键'}</button><button disabled={busy} onClick={() => void reload()}>刷新快捷键状态</button><button disabled={!draft || busy} onClick={() => { setDraft(null); setError(null); }}>重置快捷键草稿</button></div>
  </section>;
}
