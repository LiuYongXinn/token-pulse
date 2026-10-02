import { useEffect, useRef, useState } from 'react';
import { queryDiagnostics, runtimeError } from '../shared/runtime';
import type { DiagnosticIssue, DiagnosticsSnapshot, SourcesSnapshot } from '../shared/generated/contracts';
import './diagnostics.css';

function reason(issue: DiagnosticIssue) {
  switch (issue.kind) {
    case 'unconfirmed_usage': return '用量尚无法确认，已与可信消费隔离。';
    case 'unattributed_usage': return '累计基线尚未确定，该段用量未计入可信消费。';
    case 'missing_file': return '已采集的文件当前未找到，保存的历史消费仍保留。';
    case 'directory_scan': return issue.code === 'SOURCE_UNREADABLE' ? '目录未能完整读取，请检查目录与访问权限。' : issue.code ? `目录核对未完成（${issue.code}）。` : '目录核对尚未完成。';
    case 'log_record': {
      const descriptions = { UNSUPPORTED_FORMAT: '当前日志记录格式尚不支持，该记录未计入可信消费。', INVALID_USAGE: '日志用量字段无法验证，该记录未计入可信消费。', AMBIGUOUS_USAGE: '日志用量存在歧义，该记录已与可信消费隔离。', NUMERIC_OVERFLOW: '用量超过支持的精确范围，该记录未计入可信消费。' };
      return issue.code !== null && issue.code in descriptions ? descriptions[issue.code as keyof typeof descriptions] : issue.code ? `日志记录处理失败（${issue.code}）。` : '日志记录尚无法解释。';
    }
  }
}
export function DiagnosticsIssues({ sources }: { sources: SourcesSnapshot | null }) {
  const [source, setSource] = useState<string | null>(null);
  const [cache, setSnapshot] = useState<{ source: string | null; value: DiagnosticsSnapshot } | null>(null);
  const [readError, setError] = useState<{ source: string | null; message: string } | null>(null);
  const snapshot = cache?.source === source ? cache.value : null;
  const error = readError?.source === source ? readError.message : null;
  const [loading, setLoading] = useState(true);
  const serial = useRef(0), mounted = useRef(false), reading = useRef(false);
  const refresh = async () => {
    const sequence = ++serial.current;
    reading.current = true;
    setLoading(true);
    try {
      const value = await queryDiagnostics(source);
      if (mounted.current && sequence === serial.current) { setSnapshot({ source, value }); setError(null); }
    } catch (e) { if (mounted.current && sequence === serial.current) setError({ source, message: runtimeError(e) }); }
    finally { if (mounted.current && sequence === serial.current) { reading.current = false; setLoading(false); } }
  };
  useEffect(() => {
    mounted.current = true; setSnapshot(null); setError(null); void refresh();
    const interval = setInterval(() => { if (!document.hidden && !reading.current) void refresh(); }, 2000);
    return () => { mounted.current = false; ++serial.current; clearInterval(interval); };
  }, [source]);
  const names = new Map(sources?.sources.map(s => [s.source_id, s.root_path]));
  return <section className="panel diagnostics-issues" aria-label="采集问题与必要位置">
    <div className="panel-heading"><div><h2>采集与核算问题</h2><p className="muted">每个位置的同类问题显示一处代表偏移。旧代次和已解决的问题不在此显示。</p></div><button disabled={loading} onClick={() => void refresh()}>刷新问题</button></div>
    <label className="diagnostics-source">查看来源<select aria-label="诊断来源" value={source ?? ''} onChange={e => setSource(e.target.value || null)}><option value="">全部来源</option>{sources?.sources.map(s => <option key={s.source_id} value={s.source_id}>{s.root_path}{s.removed ? '（历史保留）' : ''}</option>)}</select></label>
    {error && <p className="notice" role="alert">{error}{snapshot ? ' 显示上次读取的问题。' : ' 问题状态暂不可用。'}</p>}
    {snapshot === null ? !error && <p className="muted" role="status">正在读取采集问题…</p> : <>
      {snapshot.issues.length === 0 ? <p className="muted">暂无已保存的问题；这不表示来源已完整扫描或所有用量均已确认。</p> : <ul className="diagnostic-issue-list">{snapshot.issues.map(issue => <li key={issue.issue_id}>
        <strong>{reason(issue)}</strong>
        <p className="muted">来源：{issue.source_id === null ? '尚未关联' : names.get(issue.source_id) ?? '来源名称暂不可用'}</p>
        <p className="diagnostic-position">位置：{issue.path ?? '尚无文件位置'}{issue.byte_offset !== null && <span> · 字节偏移 {BigInt(issue.byte_offset).toLocaleString('zh-CN')}</span>}</p>
      </li>)}</ul>}
      {snapshot.has_more && <p className="notice">仅显示前 20 处问题；可选择单个来源查看其他问题。</p>}
    </>}
  </section>;
}
