export type QueryTiming = Readonly<{ kind: string; stage: 'queue' | 'ipc' | 'paint'; milliseconds: number }>;
const samples: QueryTiming[] = [];
const kinds = new Set(['overview', 'models', 'projects', 'sessions', 'events', 'diagnostics', 'settings', 'snapshot', 'lease', 'get_dashboard_bundle', 'get_grouped_usage', 'query_sessions', 'query_usage_events', 'get_app_status', 'get_filter_options', 'get_session_bundle', 'query_turns', 'get_mini_usage']);
/** Bounded, anonymous measurements: never record request bodies, keys or labels. */
export function recordQueryTiming(kind: string, stage: QueryTiming['stage'], started: number) {
  if (!kinds.has(kind)) return;
  samples.push({ kind, stage, milliseconds: Math.max(0, performance.now() - started) });
  if (samples.length > 2048) samples.splice(0, samples.length - 2048);
}
export function queryTimings(): readonly QueryTiming[] { return samples.slice(); }
export function resetQueryTimings() { samples.length = 0; }
export function timeNavigation(kind: string) {
  const started = performance.now();
  // After React's discrete event commit; records the first frame opportunity.
  requestAnimationFrame(() => recordQueryTiming(kind, 'paint', started));
}
