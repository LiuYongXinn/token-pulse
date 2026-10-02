import type { TaskbarCleanupOutcome, TaskbarRuntimeIssue, TaskbarRuntimeSnapshot, TaskbarRuntimeState } from '../shared/generated/contracts';
import { runtimeError } from '../shared/runtime';
const states: Record<TaskbarRuntimeState, string> = { disabled: '已关闭', probing: '正在检测任务栏', waiting_snapshot: '等待统计快照', embedded: '已嵌入任务栏', unavailable: '任务栏显示不可用', recovering: '正在重新应用显示', suspended: '休眠期间已暂停' };
const issues: Record<TaskbarRuntimeIssue, string> = {
  unsupported_version: '当前 Windows 版本尚未支持任务栏嵌入。', missing_taskbar: '未找到系统任务栏。', unexpected_structure: '系统任务栏结构无法确认。', unsafe_geometry: '任务栏布局无法安全预留空间。', insufficient_space: '任务栏剩余空间不足。', background_unavailable: '无法读取任务栏背景。', host_unavailable: '任务栏显示程序无法启动，请检查应用安装。', host_timeout: '任务栏显示程序响应超时。', protocol_error: '任务栏显示程序的响应无法验证。', unsupported_position: '当前版本尚未支持所选显示位置。', input_unavailable: '统计输入读取失败，请检查来源和显示时区。', cleanup_uncertain: '任务栏恢复结果尚无法确认，请检查系统任务栏。', cleanup_failed: '任务栏原布局恢复失败，请检查系统任务栏。', external_layout_change: '系统任务栏布局已被其他程序修改，已保留外部变化。',
};
const cleanups: Record<TaskbarCleanupOutcome, string> = { no_record: '没有本次布局记录', restored: '已恢复原布局', already_restored: '原布局已恢复', external_change: '已保留外部布局变化', identity_lost: '原任务栏已更换', failed: '恢复失败', uncertain: '恢复结果待确认', timeout: '恢复超时', unavailable: '无法读取恢复结果' };
export function TaskbarRuntimeDetails({ snapshot, error, timezone }: { snapshot: TaskbarRuntimeSnapshot | null; error: string | null; timezone: string | null }) {
  let time = '尚无成功发布的快照';
  if (snapshot?.last_snapshot_at_ms !== null && snapshot?.last_snapshot_at_ms !== undefined) {
    time = timezone ? new Intl.DateTimeFormat('zh-CN', { timeZone: timezone, dateStyle: 'short', timeStyle: 'medium' }).format(snapshot.last_snapshot_at_ms) : '等待显示时区';
  }
  return <div className="taskbar-runtime" aria-label="任务栏实际运行状态">
    <p role="status" className={snapshot?.state === 'embedded' && !error ? 'taskbar-ready' : ''}>{error ? '状态读取失败，下面保留上次已知结果。' : snapshot ? states[snapshot.state] : '正在读取任务栏状态…'}</p>
    {error && <p className="notice" role="alert">{error}</p>}
    {snapshot?.issue && <p className="taskbar-issue">{issues[snapshot.issue]}{snapshot.error ? `（${snapshot.error}）` : ''}</p>}
    {snapshot?.fallback_error && <p className="notice" role="alert">小窗回退失败：{runtimeError({ code: snapshot.fallback_error })} 可通过托盘或显示悬浮窗按钮重试。</p>}
    <dl><dt>已应用设置修订</dt><dd>{snapshot?.applied_settings_revision ?? '尚未确认'}</dd>
      <dt>实际显示密度</dt><dd>{snapshot?.compact === null || !snapshot ? '尚未确认' : snapshot.compact ? '精简显示' : '完整显示'}</dd>
      <dt>小窗自动回退</dt><dd>{snapshot?.fallback_visible === null || !snapshot ? '尚未确认' : snapshot.fallback_visible ? '回退小窗已显示' : '未显示回退小窗'}</dd>
      <dt>上次布局恢复</dt><dd>{snapshot?.last_cleanup ? cleanups[snapshot.last_cleanup] : '尚无恢复结果'}</dd>
      <dt>最近成功发布</dt><dd>{time}</dd></dl>
  </div>;
}
