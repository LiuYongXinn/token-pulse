const invoke = window.__TAURI_INTERNALS__.invoke;
const dashboard = DASHBOARD_REQUEST;
const timing = [];
const call = async (command, args = {}) => { const started = performance.now(); let result; try { result = (await invoke(command, { requestId: crypto.randomUUID(), ...args })).data; } catch(error) { throw new Error('AUX_'+command+'_'+(error.code ?? 'IPC_REJECTED')); } timing.push({ kind: command, milliseconds: performance.now()-started }); return result; };
await call('get_app_status'); await call('get_sources');
const facetQuery = { filter: dashboard.filter, dimension: 'models', search: '', page_size: 50 };
const facets = await call('get_filter_options', { request: { query: facetQuery, cursor: null } });
if (facets.next_cursor) await call('close_query_snapshot', { request: { kind: 'filter_options', request: { query: facetQuery, cursor: facets.next_cursor } } });
await call('get_dashboard_bundle', { request: dashboard });
for (const dimension of ['models', 'projects']) await call('get_grouped_usage', { request: { filter: dashboard.filter, price_basis: dashboard.price_basis, dimension, sort: 'total_desc', limit: 200 } });
for (const [command, kind, sort] of [['query_sessions', 'sessions', 'latest_desc'], ['query_usage_events', 'usage_events', 'time_desc']]) {
  const query = { filter: dashboard.filter, price_basis: dashboard.price_basis, sort, page_size: 50 };
  const page = await call(command, { request: { query, cursor: null } });
  if (kind === 'sessions' && page.sessions.length) await call('get_session_bundle', { request: { session_key: page.sessions[0].session_key, filter: query.filter, price_basis: query.price_basis } });
  if (page.next_cursor) await call('close_query_snapshot', { request: { kind, request: { query, cursor: page.next_cursor } } });
}
await invoke('plugin:event|emit', { event: MEASUREMENT_EVENT, payload: { queries: timing } });
