import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import type { FormEvent } from 'react';
import type { AppTheme, DisplaySettingsSnapshot } from '../shared/generated/contracts';
import { runtimeError, setDisplayPrivacy, setDisplayTheme, setDisplayTimezone } from '../shared/runtime';
import './display-settings.css';
import { displayPolicy } from '../shared/display-policy';
import { RecoveryShortcutPanel } from './RecoveryShortcutPanel';
import { MiniOpacityPanel } from './MiniOpacityPanel';
import { MiniPassthroughPanel } from './MiniPassthroughPanel';
import { Icon } from '../shared/Icon';

export function DisplaySettingsPanel({ snapshot, loadingError, onRefresh, onChanged }: { snapshot: DisplaySettingsSnapshot | null; loadingError: string | null; onRefresh: () => void; onChanged: (value: DisplaySettingsSnapshot) => void }) {
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const [privacyBusy, setPrivacyBusy] = useState(false);
  const [draft, setDraft] = useState<{ value: string; revision: string } | null>(null);
  const [busy, setBusy] = useState(false), [error, setError] = useState<string | null>(null), [notice, setNotice] = useState<string | null>(null);
  const mounted = useRef(false), busyRef = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const changePrivacy = async (privacy: boolean) => {
    if (!snapshot || privacyBusy || policy.pending) return;
    setPrivacyBusy(true); setError(null);
    try { const saved = await setDisplayPrivacy({ privacy, expected_settings_revision: snapshot.settings_revision }); onChanged(saved); }
    catch (e) { if (mounted.current) setError(runtimeError(e)); }
    finally { if (mounted.current) setPrivacyBusy(false); }
  };
  const changeTheme = async (theme: AppTheme) => {
    if (!snapshot || privacyBusy || busyRef.current) return;
    busyRef.current = true; setBusy(true); setError(null);
    try { const saved = await setDisplayTheme({ theme, expected_settings_revision: snapshot.settings_revision }); onChanged(saved); if (mounted.current) setNotice('已保存应用主题。'); }
    catch (e) { if (mounted.current) setError(runtimeError(e)); }
    finally { busyRef.current = false; if (mounted.current) setBusy(false); }
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault(); if (!snapshot || !draft || busyRef.current) return;
    const timezone = draft.value.trim(); if (!timezone) { setError('请填写有效的 IANA 时区。'); return; }
    busyRef.current = true; setBusy(true); setError(null); setNotice(null);
    try {
      const saved = await setDisplayTimezone({ kind: 'set', display_timezone: timezone, expected_settings_revision: draft.revision });
      onChanged(saved);
      if (mounted.current) { setDraft(null); setNotice(`已保存时区 ${saved.preferences.display_timezone}，统计日期按此时区重新读取。`); }
    } catch (e) { if (mounted.current) setError(runtimeError(e)); }
    finally { busyRef.current = false; if (mounted.current) setBusy(false); }
  };
  return <section className="panel display-settings-panel" role="tabpanel" aria-label="显示与窗口设置">
    <div className="panel-heading"><div><h2>显示与窗口</h2><p className="muted">统计日期、日趋势和今日范围使用已保存时区。</p></div><button disabled={busy} onClick={onRefresh}>刷新显示设置</button></div>
    {(error ?? loadingError) && <div className="notice" role="alert">{error ?? loadingError}</div>}{notice && <p className="display-notice" role="status">{notice}</p>}
    <div className="theme-settings"><div className="theme-preview-options" role="group" aria-label="银雾主题预览">{([['light', '银雾浅色'], ['dark', '银雾深色']] as const).map(([theme, label]) => <button className={`theme-preview-option ${theme}`} key={theme} aria-pressed={snapshot?.preferences.theme === theme} disabled={!snapshot || busy || privacyBusy || policy.pending} onClick={() => void changeTheme(theme)}><span className="theme-preview-surface" aria-hidden="true"><i /><span><b /><b /><b /></span></span><span className="theme-preview-caption">{label}{snapshot?.preferences.theme === theme && <Icon name="check" size={15} />}</span></button>)}</div><label>应用主题<select aria-label="应用主题" value={snapshot?.preferences.theme ?? 'light'} disabled={!snapshot || busy || privacyBusy || policy.pending} onChange={event => void changeTheme(event.target.value as AppTheme)}><option value="light">银雾浅色</option><option value="dark">银雾深色</option><option value="system">跟随系统</option></select></label><p className="muted">主窗口与小窗使用相同的黑白银灰主题。跟随系统会在系统外观变化时同步更新。</p></div>
    <div className="privacy-settings"><label className="privacy-switch"><input type="checkbox" aria-label="隐私模式" checked={policy.privacy === true} disabled={!snapshot || busy || privacyBusy || policy.pending || (policy.failure === null && policy.revision !== null && BigInt(snapshot.settings_revision) < BigInt(policy.revision))} onChange={event => void changePrivacy(event.target.checked)} /><span>隐私模式</span></label><p className="muted">隐藏费用、来源路径、会话与项目名称；Token 和未知状态继续显示。主窗口、小窗与任务栏共用此设置。</p><p className="privacy-status">{policy.pending ? '已隐藏敏感信息，正在保存…' : policy.failure ? '已保护当前显示，但保存失败：' + policy.failure + ' 刷新设置后可明确关闭。' : policy.privacy ? '隐私已开启，敏感信息保持隐藏。' : '隐私已关闭，数据按当前查询重新读取。'}</p></div>
    <form onSubmit={event => void submit(event)}>
      <label>统计时区<input aria-label="统计时区" list="display-timezone-options" value={draft?.value ?? snapshot?.preferences.display_timezone ?? ''} maxLength={128} disabled={busy || !snapshot} autoComplete="off" required onChange={event => { if (snapshot) setDraft(current => ({ value: event.target.value, revision: current?.revision ?? snapshot.settings_revision })); }} /></label>
      <datalist id="display-timezone-options">{['Asia/Shanghai', 'Asia/Tokyo', 'Asia/Singapore', 'Europe/London', 'Europe/Berlin', 'America/New_York', 'America/Los_Angeles', 'UTC'].map(zone => <option key={zone} value={zone} />)}</datalist>
      <p className="muted">支持有效 IANA 时区，例如 Asia/Shanghai 或 America/New_York。首次取系统有效时区，之后保留你的选择；更改时区不会修改 Token 核算与价格记录。</p>
      <div className="display-setting-actions"><button type="submit" className="primary" disabled={busy || !draft || !snapshot}>{busy ? '正在保存…' : '保存统计时区'}</button><button type="button" disabled={busy || !draft || !snapshot} onClick={() => { setDraft(null); setError(null); setNotice(null); }}>重置为当前值</button></div>
      <p className="chart-caption">{snapshot ? `当前配置版本 ${snapshot.settings_version} · 修订 ${snapshot.settings_revision}` : '正在读取显示设置…'}{draft ? `；本次编辑基于修订 ${draft.revision}，刷新不会覆盖未保存输入。` : ''}</p>
    </form>
    <RecoveryShortcutPanel />
    <MiniOpacityPanel revision={snapshot?.settings_revision ?? null} />
    <MiniPassthroughPanel revision={snapshot?.settings_revision ?? null} />
  </section>;
}
