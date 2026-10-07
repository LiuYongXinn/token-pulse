import { useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getMiniVisibility, onMiniVisibilityChanged, runtimeError, windowAction } from '../shared/runtime';

export function useMiniVisibility() {
  const [visible, setVisible] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(false), writing = useRef(false), serial = useRef(0);
  const reload = async (clearError = false) => {
    const request = ++serial.current;
    try {
      const value = await getMiniVisibility();
      if (mounted.current && request === serial.current) { setVisible(value); if (clearError) setError(null); }
    } catch (e) {
      if (mounted.current && request === serial.current) setError(`悬浮窗状态读取失败：${runtimeError(e)}`);
    }
  };
  useEffect(() => {
    if (!isTauri()) return;
    mounted.current = true;
    let active = true, stop: (() => void) | null = null;
    void reload();
    void onMiniVisibilityChanged(() => { if (active) void reload(); }).then(unsubscribe => {
      if (active) { stop = unsubscribe; void reload(); } else unsubscribe();
    }).catch(e => { if (active) setError(runtimeError(e)); });
    const resume = () => { if (!document.hidden && !writing.current) void reload(); };
    const timer = setInterval(resume, 5000);
    document.addEventListener('visibilitychange', resume);
    return () => {
      active = false; mounted.current = false; ++serial.current; stop?.(); clearInterval(timer);
      document.removeEventListener('visibilitychange', resume);
    };
  }, []);
  const toggle = async () => {
    if (visible === null || writing.current) return;
    writing.current = true; setBusy(true); setError(null); ++serial.current;
    try {
      await windowAction(visible ? 'hide_mini' : 'show_mini');
      if (mounted.current) await reload(true);
    } catch (e) {
      if (mounted.current) { ++serial.current; setError(`悬浮窗切换失败：${runtimeError(e)}`); }
    } finally {
      writing.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  return { visible, busy, error, toggle, reload };
}
