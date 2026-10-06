import { useEffect, useRef, useState } from 'react';
import type { TaskbarPreferences, TaskbarPreferencesSnapshot } from '../shared/generated/contracts';
import { getTaskbarPreferences, onSettingsChanged, retryTaskbarEmbed, runtimeError, setTaskbarPreferences, windowAction } from '../shared/runtime';
import { useTaskbarRuntime } from './useTaskbarRuntime';
import { TaskbarRuntimeDetails } from './TaskbarRuntimeDetails';
import './taskbar-settings.css';
import { Icon } from '../shared/Icon';

export function TaskbarSettingsPanel({ timezone }: { timezone: string | null }) {
  const [snapshot, setSnapshot] = useState<TaskbarPreferencesSnapshot | null>(null);
  const [draft, setDraft] = useState<{ preferences: TaskbarPreferences; revision: string } | null>(null);
  const [error, setError] = useState<string | null>(null), [readError, setReadError] = useState<string | null>(null), [busy, setBusy] = useState(false);
  const active = useRef(false), serial = useRef(0), writing = useRef(false);
  const runtime = useTaskbarRuntime();
  const reload = async () => {
    const request = ++serial.current;
    try { const value = await getTaskbarPreferences(); if (active.current && request === serial.current) { setSnapshot(old => old && BigInt(old.settings_revision) > BigInt(value.settings_revision) ? old : value); setReadError(null); } }
    catch (e) { if (active.current && request === serial.current) setReadError(runtimeError(e)); }
  };
  useEffect(() => {
    active.current = true; void reload(); let alive = true, stop: (() => void) | null = null;
    void onSettingsChanged(() => { if (alive) void reload(); }).then(unsubscribe => { if (alive) { stop = unsubscribe; void reload(); } else unsubscribe(); }).catch(e => { if (alive) setReadError(runtimeError(e)); });
    return () => { alive = false; active.current = false; ++serial.current; stop?.(); };
  }, []);
  const preferences = draft?.preferences ?? snapshot?.preferences;
  const edit = (change: (value: TaskbarPreferences) => TaskbarPreferences) => {
    if (!snapshot || writing.current) return;
    setDraft(old => ({ preferences: change(old?.preferences ?? snapshot.preferences), revision: old?.revision ?? snapshot.settings_revision }));
  };
  const valid = preferences && Object.entries(preferences.display).some(([key, value]) => key !== 'layout' && value === true);
  const save = async () => {
    if (!draft || !valid || writing.current) return;
    writing.current = true; setBusy(true); setError(null);
    try {
      const value = await setTaskbarPreferences({ preferences: draft.preferences, expected_settings_revision: draft.revision });
      if (active.current) { ++serial.current; setSnapshot(value); setDraft(null); setReadError(null); void runtime.refresh(); }
    } catch (e) { if (active.current) setError(runtimeError(e)); }
    finally { writing.current = false; if (active.current) setBusy(false); }
  };
  const action = async (work: () => Promise<void>) => { if (writing.current) return; writing.current = true; setBusy(true); setError(null); try { await work(); if (active.current) void runtime.refresh(); } catch (e) { if (active.current) setError(runtimeError(e)); } finally { writing.current = false; if (active.current) setBusy(false); } };
  return <section className="panel taskbar-settings-panel" role="tabpanel" aria-label="任务栏显示设置">
    <div className="panel-heading"><div><h2>任务栏显示</h2><p className="muted">常驻读数使用悬浮窗的统计范围。账户额度始终属于账户。</p></div><button disabled={busy} onClick={() => { void reload(); void runtime.refresh(); }}>刷新任务栏设置</button></div>
    {readError && <p className="notice" role="alert">偏好读取失败：{readError} 尚未确认最新配置。</p>}
    <div className="taskbar-settings-grid"><div className="taskbar-preferences">
      <label className="taskbar-enable"><input type="checkbox" checked={preferences?.enabled ?? false} disabled={!snapshot || busy} onChange={e => edit(v => ({ ...v, enabled: e.target.checked }))} />启用任务栏显示</label>
      <p className="muted">保存后检测系统任务栏。实际嵌入结果见运行状态；无法使用时可手动打开悬浮窗。</p>
      <fieldset disabled={!snapshot || busy}><legend>显示内容</legend>{([['show_tokens', 'Token 用量'], ['show_costs', '费用估算'], ['show_quota', '账户剩余额度'], ['show_weekly_reset', '周重置时间']] as const).map(([key, label]) => <label key={key}><input type="checkbox" checked={preferences?.display[key] ?? false} onChange={e => edit(v => ({ ...v, display: { ...v.display, [key]: e.target.checked } }))} />{label}</label>)}</fieldset>
      {preferences && !valid && <p className="notice" role="alert">请至少选择一项显示内容。</p>}
      <div className="taskbar-selects"><label>显示布局<select aria-label="任务栏显示布局" value={preferences?.display.layout ?? ''} disabled={!snapshot || busy} onChange={e => edit(v => ({ ...v, display: { ...v.display, layout: e.target.value as TaskbarPreferences['display']['layout'] } }))}>{!preferences && <option value="">尚未读取</option>}<option value="two_rows">两行</option><option value="single_row">单行</option></select></label>
        <label>显示位置<select aria-label="任务栏显示位置" value={preferences?.position ?? ''} disabled={!snapshot || busy} onChange={e => edit(v => ({ ...v, position: e.target.value as TaskbarPreferences['position'] }))}>{!preferences && <option value="">尚未读取</option>}<option value="notification_left">通知区域左侧</option><option value="application_right">应用图标右侧</option></select></label></div>
      <p className="muted">隐私模式与主窗口、小窗同步；账户未连接或价格未知时保留未知状态。空间不足时会按实际可用宽度精简。</p>
      <label><input type="checkbox" checked={preferences?.fallback_to_mini ?? false} disabled={!snapshot || busy} onChange={e => edit(v => ({ ...v, fallback_to_mini: e.target.checked }))} />任务栏不可用时显示悬浮窗</label>
      <p className="muted">自动回退不抢焦点；同一失败期间隐藏小窗后不会反复弹出。关闭回退或恢复任务栏不会自动关闭已显示的小窗。</p>
      <div className="display-setting-actions"><button className="primary" disabled={!draft || !valid || busy} onClick={() => void save()}>{busy ? '正在处理…' : '保存任务栏设置'}</button><button disabled={!draft || busy} onClick={() => { setDraft(null); setError(null); }}>重置任务栏草稿</button></div>
      <p className="chart-caption">{snapshot ? `已保存修订 ${snapshot.settings_revision}。保存成功不代表嵌入成功。` : '正在读取任务栏偏好…'}{draft ? ` 编辑基于修订 ${draft.revision}；刷新保留草稿。` : ''}</p>
      {error && <p className="notice" role="alert">{error}{draft ? ' 草稿已保留，重置草稿后可基于最新配置编辑。' : ''}</p>}
    </div><div className="taskbar-state-panel"><section className="taskbar-layout-preview" aria-label="任务栏布局预览"><h3>布局预览</h3><div className="taskbar-preview-rail"><span className="taskbar-preview-app"><Icon name="pulse" size={22} /></span><div className="taskbar-preview-readings" data-layout={preferences?.display.layout ?? 'two_rows'}>{([['show_tokens', 'Token'], ['show_costs', '估算'], ['show_quota', '剩余'], ['show_weekly_reset', '周重置']] as const).filter(([key]) => preferences?.display[key]).map(([key, label]) => <div key={key}><span>{label}</span><b>—</b></div>)}</div></div><p className="chart-caption">布局示意，横线是占位符。真实任务栏沿用系统外观，数值来自小窗范围与独立账户额度。</p></section><h3>实际运行状态</h3><TaskbarRuntimeDetails snapshot={runtime.snapshot} error={runtime.error} timezone={timezone} />
      <div className="display-setting-actions"><button disabled={busy || !snapshot?.preferences.enabled || runtime.snapshot?.state !== 'unavailable'} onClick={() => void action(retryTaskbarEmbed)}>重试任务栏嵌入</button><button disabled={busy} onClick={() => void action(() => windowAction('show_mini'))}>显示悬浮窗</button></div>
      <p className="chart-caption">在悬浮窗中调整统计范围。主窗口筛选不改变任务栏范围。</p>
    </div></div>
  </section>;
}

export function TaskbarDiagnosticsPanel({ timezone }: { timezone: string | null }) {
  const runtime = useTaskbarRuntime();
  return <section className="panel taskbar-diagnostics"><div className="panel-heading"><h2>任务栏显示</h2><button onClick={() => void runtime.refresh()}>刷新任务栏状态</button></div><TaskbarRuntimeDetails snapshot={runtime.snapshot} error={runtime.error} timezone={timezone} /></section>;
}
