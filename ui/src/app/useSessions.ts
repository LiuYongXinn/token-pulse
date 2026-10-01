import { useEffect, useRef, useState } from 'react';
import type { SessionsPage, SessionsQuery } from '../shared/generated/contracts';
import { closeQuerySnapshot, onPriceRulesChanged, querySessions, runtimeError } from '../shared/runtime';

type Controller = { query: SessionsQuery; disposed: boolean; busy: boolean; cursor: string | null; pages: SessionsPage[]; index: number; firstNumber: number; error: string | null };
type Result = { key: string; page: SessionsPage | null; number: number; previous: boolean; next: boolean; loading: boolean; error: string | null; trimmed: boolean };
const CACHE_PAGES = 10;

/** Navigation retains at most ten immutable pages; requests/cleanup are serial. */
export function useSessions(query: SessionsQuery, refreshRevision: number) {
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState<Result | null>(null);
  const tail = useRef<Promise<void>>(Promise.resolve());
  const current = useRef<Controller | null>(null);
  const key = JSON.stringify([query, refreshRevision, revision]);
  const enqueue = (work: () => Promise<void>) => { tail.current = tail.current.catch(() => {}).then(work); };
  const release = async (controller: Controller) => {
    const cursor = controller.cursor; controller.cursor = null;
    if (cursor !== null) await closeQuerySnapshot({ kind: 'sessions', request: { query: controller.query, cursor } }).catch(() => {});
  };
  const display = (controller: Controller, resultKey: string) => {
    if (!controller.disposed) setResult({ key: resultKey, page: controller.pages[controller.index] ?? null, number: controller.firstNumber + controller.index, previous: controller.index > 0, next: controller.index < controller.pages.length - 1 || controller.cursor !== null, loading: controller.busy, error: controller.error, trimmed: controller.firstNumber > 1 });
  };
  const read = async (controller: Controller, resultKey: string) => {
    if (controller.disposed) return;
    try {
      const page = await querySessions({ query: controller.query, cursor: controller.cursor });
      controller.cursor = page.next_cursor;
      if (controller.disposed) { await release(controller); return; }
      const first = controller.pages[0];
      if (first && JSON.stringify([page.meta, page.summary, page.pricing, page.coverage]) !== JSON.stringify([first.meta, first.summary, first.pricing, first.coverage])) throw new Error('会话分页快照不一致，请重新查询。');
      const seen = new Set(controller.pages.flatMap(cached => cached.sessions.map(session => session.session_key)));
      const keys = page.sessions.map(session => session.session_key);
      if (new Set(keys).size !== keys.length || keys.some(session => seen.has(session))) throw new Error('会话分页包含重复位置，请重新查询。');
      if (page.sessions.length > controller.query.page_size || (page.next_cursor !== null && page.sessions.length === 0)) throw new Error('会话分页大小无效，请重新查询。');
      controller.pages = [...controller.pages, page];
      if (controller.pages.length > CACHE_PAGES) { controller.pages = controller.pages.slice(1); ++controller.firstNumber; }
      controller.index = controller.pages.length - 1; controller.error = null;
    } catch (error) { await release(controller); controller.error = runtimeError(error); }
    finally { controller.busy = false; display(controller, resultKey); }
  };
  useEffect(() => {
    const controller: Controller = { query, disposed: false, busy: true, cursor: null, pages: [], index: 0, firstNumber: 1, error: null };
    current.current = controller; display(controller, key);
    enqueue(() => read(controller, key));
    const invalidate = () => { if (!controller.disposed && !document.hidden) setRevision(value => value + 1); };
    let stop: (() => void) | null = null;
    void onPriceRulesChanged(invalidate).then(unsubscribe => { if (controller.disposed) unsubscribe(); else stop = unsubscribe; }).catch(() => {});
    document.addEventListener('visibilitychange', invalidate);
    return () => { controller.disposed = true; stop?.(); document.removeEventListener('visibilitychange', invalidate); enqueue(() => release(controller)); };
  }, [key]);
  const previous = () => { const controller = current.current; if (controller && !controller.disposed && !controller.busy && controller.index > 0) { --controller.index; display(controller, key); } };
  const next = () => {
    const controller = current.current; if (!controller || controller.disposed || controller.busy) return;
    if (controller.index < controller.pages.length - 1) { ++controller.index; display(controller, key); }
    else if (controller.cursor !== null) { controller.busy = true; display(controller, key); enqueue(() => read(controller, key)); }
  };
  const visible = result?.key === key ? result : null;
  return { page: visible?.page ?? null, pageNumber: visible?.number ?? 1, hasPrevious: visible?.previous ?? false, hasNext: visible?.next ?? false, trimmed: visible?.trimmed ?? false, loading: visible?.loading ?? true, error: visible?.error ?? null, previous, next, reload: () => setRevision(value => value + 1) };
}
