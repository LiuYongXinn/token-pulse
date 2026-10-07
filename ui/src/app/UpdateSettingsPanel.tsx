import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import type { UpdateSnapshot, UpdatePhase, UpdateIssue } from '../shared/generated/contracts';
import { checkForUpdates, downloadUpdate, getUpdateStatus, installUpdate, onUpdatesChanged, runtimeError } from '../shared/runtime';
import { displayPolicy } from '../shared/display-policy';
import './update-settings.css';

const phaseText: Record<UpdatePhase, string> = { unavailable: '更新不可用', idle: '尚未检查更新', checking: '正在检查更新', current: '当前已是最新版本', available: '发现新版本', downloading: '正在下载安装包', verifying: '正在验证签名', ready_to_install: '更新已验证，可以安装', installing: '正在启动安装器', error: '更新未完成' };
const issueText: Record<UpdateIssue, string> = { publication_not_configured: '此安装包未配置有效的更新签名公钥，请使用官方发布的安装包。', unsupported_platform: '当前系统不支持此更新渠道。', network: '无法取得更新，请检查网络后重试。', invalid_release: '发布信息无法验证，请稍后重新检查。', download_failed: '安装包未能完整下载，请重新检查后重试。', signature_invalid: '安装包签名或版本校验失败，请重新检查更新。', installer_unavailable: '安装包无法用于此安装渠道，请重新检查。', install_failed: '安装器未能启动，应用继续运行，请重新检查后重试。' };
function time(value: number | null, timezone: string | null) {
  if (value === null) return '尚未提供';
  try { if (!Number.isSafeInteger(value) || Math.abs(value) > 8640000000000000) return '时间超出可显示范围'; return new Intl.DateTimeFormat('zh-CN', { timeZone: timezone ?? undefined, dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value)); }
  catch { return '时间无法显示'; }
}
const working = (phase: UpdatePhase | undefined) => phase !== undefined && ['checking', 'downloading', 'verifying', 'installing'].includes(phase);

export function UpdateSettingsPanel({ timezone, development }: { timezone: string | null; development: boolean | null }) {
  const [snapshot, setSnapshot] = useState<UpdateSnapshot | null>(null), [error, setError] = useState<string | null>(null), [busy, setBusy] = useState(false);
  const [review, setReview] = useState<{ revision: string; version: string } | null>(null);
  const active = useRef(false), serial = useRef(0), writing = useRef(false), latest = useRef<UpdateSnapshot | null>(null);
  const gate = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const accept = useCallback((value: UpdateSnapshot) => {
    if (!active.current || (latest.current && BigInt(latest.current.update_revision) > BigInt(value.update_revision))) return;
    latest.current = value; setSnapshot(value);
  }, []);
  const refresh = useCallback(async () => {
    const query = ++serial.current;
    try { const value = await getUpdateStatus(); if (active.current && query === serial.current) { accept(value); setError(null); } }
    catch (e) { if (active.current && query === serial.current) setError(runtimeError(e)); }
  }, [accept]);
  useEffect(() => {
    active.current = true; let alive = true, stop: (() => void) | null = null;
    void onUpdatesChanged(() => { if (alive) void refresh(); }).then(unsubscribe => { if (alive) { stop = unsubscribe; void refresh(); } else unsubscribe(); }).catch(e => { if (alive) { setError(runtimeError(e)); void refresh(); } });
    const visible = () => { if (!document.hidden) void refresh(); };
    document.addEventListener('visibilitychange', visible); const timer = setInterval(visible, 5000);
    return () => { alive = false; active.current = false; ++serial.current; stop?.(); clearInterval(timer); document.removeEventListener('visibilitychange', visible); };
  }, [refresh]);
  useEffect(() => { if (active.current) void refresh(); }, [gate.epoch, refresh]);
  const action = async (kind: 'check' | 'download' | 'install') => {
    if (writing.current || !snapshot || error || working(snapshot.phase)) return;
    const expected = kind === 'install' ? review?.revision : snapshot.update_revision;
    if (kind !== 'check' && !expected) return;
    writing.current = true; setBusy(true); setError(null); ++serial.current;
    try { const value = await (kind === 'check' ? checkForUpdates() : kind === 'download' ? downloadUpdate({ expected_update_revision: expected! }) : installUpdate({ expected_update_revision: expected! })); if (active.current) { accept(value); setReview(null); } }
    catch (e) { if (active.current) { setError(runtimeError(e)); setReview(null); } }
    finally { writing.current = false; if (active.current) { setBusy(false); } }
  };
  const blocked = !snapshot || busy || working(snapshot.phase) || error !== null;
  const sameReview = review && snapshot?.phase === 'ready_to_install' && snapshot.update_revision === review.revision && snapshot.release?.version === review.version;
  const downloaded = snapshot?.downloaded_bytes ?? null, total = snapshot?.total_bytes ?? null;
  const percent = downloaded !== null && total !== null && BigInt(total) > 0n ? Number((BigInt(downloaded) * 100n) / BigInt(total)) : null;
  return <section className="panel update-panel" role="tabpanel" aria-label="软件更新">
    <div className="panel-heading"><div><h2>软件更新</h2></div><button onClick={() => void refresh()}>刷新更新状态</button></div>
    <div className="update-summary"><span className="update-phase" role="status">{snapshot ? phaseText[snapshot.phase] : '正在读取更新状态'}</span><p>当前版本 <strong>{snapshot?.current_version ?? '尚未读取'}</strong>{snapshot?.release && <> <span className="muted">→</span> 可用版本 <strong>{snapshot.release.version}</strong></>}</p>
      <dl><dt>最近成功检查</dt><dd>{time(snapshot?.last_checked_at_ms ?? null, timezone)}</dd>{snapshot?.release && <><dt>版本发布时间</dt><dd>{time(snapshot.release.published_at_ms, timezone)}</dd></>}</dl>
    </div>
    {snapshot?.issue && <p className="notice" role="alert">{issueText[snapshot.issue]}</p>}
    {downloaded !== null && <div className="update-progress"><p>{downloaded} 字节已下载 / {total === null ? '总大小未知' : `${total} 字节`}</p>{percent !== null && <progress aria-label="更新下载进度" max={100} value={percent} />}</div>}
    {snapshot?.release?.notes && <section className="update-notes" aria-label="版本说明"><h3>版本说明</h3><p>{snapshot.release.notes}</p></section>}
    {error && <p className="notice" role="alert">{error}</p>}
    <div className="update-actions"><button className="primary" disabled={blocked || snapshot?.phase === 'unavailable'} onClick={() => void action('check')}>检查更新</button>{snapshot?.phase === 'available' && <button className="primary" disabled={blocked} onClick={() => void action('download')}>下载更新</button>}{snapshot?.phase === 'ready_to_install' && <button className="primary" disabled={blocked || development !== false} onClick={() => { if (snapshot.release) setReview({ revision: snapshot.update_revision, version: snapshot.release.version }); }}>安装更新</button>}</div>
    {development === true && snapshot?.phase === 'ready_to_install' && <p className="muted">请在正式安装版中安装更新。</p>}
    {review && <section className="update-review" aria-label="确认安装更新"><h3>安装 TokenPulse {review.version}</h3><p>确认后应用将关闭，安装完成后重新打开。</p>{!sameReview && <p role="alert">更新状态已变化，请关闭后重新确认版本。</p>}<div className="update-actions"><button className="primary" disabled={blocked || !sameReview || development !== false} onClick={() => void action('install')}>确认安装并关闭应用</button><button disabled={busy} onClick={() => setReview(null)}>取消安装</button></div></section>}
  </section>;
}
