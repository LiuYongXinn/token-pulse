import { useEffect, useRef, useState } from 'react';
import { getMiniScope, runtimeError, setMiniScope, windowAction } from '../shared/runtime';

export function PinMiniScope({ sessionKey, startMs }: { sessionKey: string; startMs: number }) {
  const [busy, setBusy] = useState(false), [error, setError] = useState<string | null>(null), [notice, setNotice] = useState<string | null>(null);
  const active = useRef(false), writing = useRef(false);
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  useEffect(() => { setError(null); setNotice(null); }, [sessionKey, startMs]);
  const pin = async (fixed: boolean) => {
    if (writing.current) return;
    writing.current = true; setBusy(true); setError(null); setNotice(null);
    try {
      const current = await getMiniScope();
      await setMiniScope({ mini_scope: { kind: 'session', session_key: sessionKey, start: fixed ? { kind: 'fixed', start_ms: startMs } : { kind: 'today' } }, expected_settings_revision: current.settings_revision });
      if (active.current) setNotice(fixed ? '已固定到小窗，保留所选范围的精确起点。' : '已固定到小窗，每天从统计时区零点计算。');
      try { await windowAction('show_mini'); }
      catch (e) { if (active.current) setError(`范围已保存，显示小窗失败：${runtimeError(e)}`); }
    } catch (e) { if (active.current) setError(runtimeError(e)); }
    finally { writing.current = false; if (active.current) setBusy(false); }
  };
  return <section className="pin-mini-scope" aria-label="固定会话到小窗">
    <div><button disabled={busy} onClick={() => void pin(false)}>固定到小窗（今日）</button><button disabled={busy} onClick={() => void pin(true)}>按所选起点固定到小窗</button></div>
    <p className="chart-caption">仅修改小窗 / 任务栏共享范围，主窗口筛选与账户额度保持各自范围。</p>
    {notice && <p className="chart-caption" role="status">{notice}</p>}{error && <p className="facet-error" role="alert">{error}</p>}
  </section>;
}
