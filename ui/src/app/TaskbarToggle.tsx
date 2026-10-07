import { useEffect, useRef, useState } from 'react';
import { ActionButton } from '../shared/ActionButton';
import type { TaskbarPreferencesSnapshot } from '../shared/generated/contracts';
import { getTaskbarPreferences, onSettingsChanged, runtimeError, setTaskbarPreferences } from '../shared/runtime';

export function TaskbarToggle() {
  const [snapshot, setSnapshot] = useState<TaskbarPreferencesSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mounted = useRef(false), writing = useRef(false), serial = useRef(0);
  const accept = (value: TaskbarPreferencesSnapshot) => {
    setSnapshot(old => old && BigInt(old.settings_revision) > BigInt(value.settings_revision) ? old : value);
  };
  const reload = async () => {
    const request = ++serial.current;
    try {
      const value = await getTaskbarPreferences();
      if (mounted.current && request === serial.current) { accept(value); setError(null); }
    } catch (e) {
      if (mounted.current && request === serial.current) setError(`任务栏设置读取失败：${runtimeError(e)}`);
    }
  };
  useEffect(() => {
    mounted.current = true;
    let active = true, stop: (() => void) | null = null;
    void reload();
    void onSettingsChanged(() => { if (active) void reload(); }).then(unsubscribe => {
      if (active) { stop = unsubscribe; void reload(); } else unsubscribe();
    }).catch(e => { if (active) setError(runtimeError(e)); });
    const visible = () => { if (!document.hidden) void reload(); };
    document.addEventListener('visibilitychange', visible);
    return () => {
      active = false; mounted.current = false; ++serial.current; stop?.();
      document.removeEventListener('visibilitychange', visible);
    };
  }, []);
  const toggle = async () => {
    if (!snapshot || writing.current) return;
    const enabled = !snapshot.preferences.enabled;
    writing.current = true; setBusy(true); setError(null);
    try {
      // Read the latest complete preferences so the shortcut preserves all display options.
      const current = await getTaskbarPreferences();
      const value = await setTaskbarPreferences({
        preferences: { ...current.preferences, enabled },
        expected_settings_revision: current.settings_revision,
      });
      if (mounted.current) { ++serial.current; accept(value); setError(null); }
    } catch (e) {
      if (mounted.current) { ++serial.current; setError(`任务栏显示切换失败：${runtimeError(e)}`); }
    } finally {
      writing.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  return <div className="sidebar-taskbar">
    <ActionButton icon="taskbar" aria-pressed={snapshot?.preferences.enabled ?? false} disabled={!snapshot || busy} onClick={() => void toggle()}>
      {busy ? '正在处理…' : snapshot?.preferences.enabled ? '隐藏任务栏' : '显示任务栏'}
    </ActionButton>
    {error && <div className="sidebar-action-error" role="alert"><p>{error}</p><button disabled={busy} onClick={() => void reload()}>重试任务栏设置</button></div>}
  </div>;
}
