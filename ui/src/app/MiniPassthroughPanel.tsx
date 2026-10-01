import { useEffect, useRef, useState } from 'react';
import type { MiniPassthroughSnapshot, RecoveryShortcut } from '../shared/generated/contracts';
import { getMiniPassthrough, onMiniInteractionChanged, runtimeError, setMiniPassthrough, windowAction } from '../shared/runtime';
function keyName(key: RecoveryShortcut) { return [key.control ? 'Ctrl' : null, key.alt ? 'Alt' : null, key.shift ? 'Shift' : null, key.key].filter(Boolean).join('+'); }
export function MiniPassthroughPanel({ revision }: { revision: string | null }) {
  const [snapshot, setSnapshot] = useState<MiniPassthroughSnapshot | null>(null);
  const [ack, setAck] = useState<{ key: RecoveryShortcut; revision: string } | null>(null);
  const [error, setError] = useState<string | null>(null), [readError, setReadError] = useState<string | null>(null), [busy, setBusy] = useState(false);
  const active = useRef(false), serial = useRef(0), writing = useRef(false);
  const reload = async () => { const request = ++serial.current; try { const value = await getMiniPassthrough(); if (active.current && request === serial.current) { setSnapshot(value); setReadError(null); } } catch (e) { if (active.current && request === serial.current) setReadError(runtimeError(e)); } };
  useEffect(() => {
    active.current = true; let live = true, stop: (() => void) | null = null;
    void onMiniInteractionChanged(() => { if (live) void reload(); }).then(value => { if (live) stop = value; else value(); }).catch(() => { if (live) setReadError('交互状态订阅失败，请手动刷新。'); });
    return () => { live = false; active.current = false; ++serial.current; stop?.(); };
  }, []);
  useEffect(() => { void reload(); }, [revision]);
  const ready = snapshot?.supported && snapshot.window_present && snapshot.recovery_registration === 'ready' && !readError;
  const sameKey = ack && snapshot && keyName(ack.key) === keyName(snapshot.recovery_shortcut);
  const change = async (enabled: boolean) => {
    if (!snapshot || writing.current || (enabled && (!ready || !ack || !sameKey))) return;
    writing.current = true; setBusy(true); setError(null);
    try {
      const value = await setMiniPassthrough({ enabled, acknowledged_recovery: enabled ? ack!.key : null, expected_settings_revision: enabled ? ack!.revision : snapshot.settings_revision });
      if (active.current) { ++serial.current; setSnapshot(value); setAck(null); setReadError(null); }
    } catch (e) { if (active.current) { setError(runtimeError(e)); await reload(); } }
    finally { writing.current = false; if (active.current) setBusy(false); }
  };
  const recover = async () => {
    if (writing.current) return; writing.current = true; setBusy(true); setError(null);
    try { await windowAction('show_mini'); if (active.current) { setAck(null); await reload(); } }
    catch (e) { if (active.current) setError(runtimeError(e)); }
    finally { writing.current = false; if (active.current) setBusy(false); }
  };
  return <section className="mini-passthrough-settings" aria-label="鼠标穿透设置"><h3>鼠标穿透</h3>
    <p className="muted">开启后，小窗上的鼠标操作会传给下方窗口。使用恢复快捷键或托盘「显示悬浮窗 / 恢复交互」解除；重新显示小窗也会关闭穿透。</p>
    <p role="status" aria-label="鼠标穿透状态">{snapshot ? `${snapshot.enabled ? '穿透已开启' : '穿透已关闭'}${snapshot.supported ? '' : ' · 当前平台不支持'}${snapshot.window_present ? '' : ' · 小窗尚未创建'}${snapshot.persisted_enabled !== snapshot.enabled ? ' · 已恢复交互，但保存状态尚未同步，请重试关闭' : ''}` : '尚未读取穿透状态'}{readError ? ` · 读取失败，保留上次状态：${readError}` : ''}</p>
    <p className="muted">{snapshot ? `恢复快捷键 ${keyName(snapshot.recovery_shortcut)} · ${snapshot.recovery_registration === 'ready' ? '已注册' : '尚未有效注册，请先处理恢复快捷键'}` : '等待实际恢复键状态。'}</p>
    <label className="passthrough-ack"><input type="checkbox" aria-label="我已了解鼠标穿透与恢复方式" checked={ack !== null} disabled={!ready || busy || snapshot?.enabled === true} onChange={e => { setError(null); setAck(e.target.checked && snapshot ? { key: snapshot.recovery_shortcut, revision: snapshot.settings_revision } : null); }} /><span>我已了解鼠标会穿透，并记住当前恢复快捷键。</span></label>
    {ack && <p className="chart-caption">已确认 {keyName(ack.key)} · 修订 {ack.revision}{!sameKey ? '；恢复键已变化，请取消勾选后重新确认。' : ''}</p>}
    {error && <p className="notice" role="alert">{error}</p>}
    <div className="display-setting-actions"><button className="primary" disabled={!ready || !ack || !sameKey || snapshot?.enabled === true || busy} onClick={() => void change(true)}>开启鼠标穿透</button><button disabled={!snapshot || busy || (!snapshot.enabled && !snapshot.persisted_enabled)} onClick={() => void change(false)}>关闭鼠标穿透</button><button disabled={busy} onClick={() => void recover()}>显示小窗并恢复交互</button><button disabled={busy} onClick={() => void reload()}>刷新穿透状态</button></div>
  </section>;
}
