const invoke = window.__TAURI_INTERNALS__.invoke;
const dashboard = DASHBOARD_REQUEST;
const call = async (command, args = {}) => (await invoke(command, { requestId: crypto.randomUUID(), ...args })).data;
await call('get_dashboard_bundle', { request: dashboard });
for (const dimension of ['models', 'projects']) await call('get_grouped_usage', { request: { filter: dashboard.filter, price_basis: dashboard.price_basis, dimension, sort: 'total_desc', limit: 200 } });
for (const [command, kind, sort] of [['query_sessions', 'sessions', 'latest_desc'], ['query_usage_events', 'usage_events', 'time_desc']]) {
  const query = { filter: dashboard.filter, price_basis: dashboard.price_basis, sort, page_size: 50 };
  const page = await call(command, { request: { query, cursor: null } });
  if (page.next_cursor) await call('close_query_snapshot', { request: { kind, request: { query, cursor: page.next_cursor } } });
}
