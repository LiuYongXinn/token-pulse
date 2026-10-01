import type { SessionsPage, SessionsQuery } from '../shared/generated/contracts';
import { closeQuerySnapshot, querySessions } from '../shared/runtime';
import { usePagedUsage, type PageAdapter } from './usePagedUsage';

const adapter: PageAdapter<SessionsQuery, SessionsPage> = {
  label: '会话', read: querySessions,
  close: request => closeQuerySnapshot({ kind: 'sessions', request }),
  keys: page => page.sessions.map(session => session.session_key),
};
export function useSessions(query: SessionsQuery, refreshRevision: number) { return usePagedUsage(query, refreshRevision, adapter); }
