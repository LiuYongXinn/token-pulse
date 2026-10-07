import { useEffect, useRef, useState } from 'react';
import type { Coverage, PricingSummary, SnapshotMeta, TokenTotals } from '../shared/generated/contracts';
import { onPriceRulesChanged, runtimeError } from '../shared/runtime';
import { scheduleUsageQuery } from './usage-query-scheduler';

type SnapshotPage = { meta: SnapshotMeta; summary: TokenTotals; pricing: PricingSummary; coverage: Coverage; next_cursor: string | null };
export type PageAdapter<Query, Page> = { label: string; read: (request: { query: Query; cursor: string | null }) => Promise<Page>; close: (request: { query: Query; cursor: string | null }) => Promise<void>; keys: (page: Page) => string[] };
type Controller<Query, Page> = { query: Query; disposed: boolean; busy: boolean; cursor: string | null; pages: Page[]; index: number; firstNumber: number; error: string | null; readAt: number; needsRenewal: boolean };
type Result<Page> = { key: string; scope: string; page: Page | null; number: number; previous: boolean; next: boolean; loading: boolean; error: string | null; trimmed: boolean };
const CACHE_PAGES = 10;

/** Read and release serially; cache a bounded set of immutable DTO pages. */
export function usePagedUsage<Query extends { page_size: number }, Page extends SnapshotPage>(query: Query, refreshRevision: number, adapter: PageAdapter<Query, Page>, background = false) {
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState<Result<Page> | null>(null);
  const tail = useRef<Promise<void>>(Promise.resolve());
  const current = useRef<Controller<Query, Page> | null>(null);
  const offscreen = useRef(background);
  offscreen.current = background;
  const scope = JSON.stringify([adapter.label, query]);
  const key = JSON.stringify([scope, refreshRevision, revision]);
  const enqueue = (work: () => Promise<void>) => { tail.current = tail.current.catch(() => {}).then(work); };
  const release = async (controller: Controller<Query, Page>) => {
    const cursor = controller.cursor; controller.cursor = null;
    if (cursor !== null) { controller.needsRenewal = true; await adapter.close({ query: controller.query, cursor }).catch(() => {}); }
  };
  const display = (controller: Controller<Query, Page>, resultKey: string) => {
    if (controller.disposed) return;
    setResult(previous => {
      const retained = previous?.scope === scope ? previous : null;
      return {
        key: resultKey, scope,
        page: controller.pages[controller.index] ?? retained?.page ?? null,
        number: controller.pages.length ? controller.firstNumber + controller.index : retained?.number ?? 1,
        previous: controller.index > 0,
        next: controller.index < controller.pages.length - 1 || controller.cursor !== null,
        loading: controller.busy, error: controller.error, trimmed: controller.firstNumber > 1,
      };
    });
  };
  const read = (controller: Controller<Query, Page>, resultKey: string) => scheduleUsageQuery(async () => {
    if (controller.disposed) return;
    try {
      const page = await adapter.read({ query: controller.query, cursor: controller.cursor });
      controller.cursor = page.next_cursor;
      if (controller.disposed) { await release(controller); return; }
      const first = controller.pages[0];
      if (first && JSON.stringify([page.meta, page.summary, page.pricing, page.coverage]) !== JSON.stringify([first.meta, first.summary, first.pricing, first.coverage])) throw new Error(`${adapter.label}分页快照不一致，请重新查询。`);
      const seen = new Set(controller.pages.flatMap(adapter.keys));
      const keys = adapter.keys(page);
      if (new Set(keys).size !== keys.length || keys.some(id => seen.has(id))) throw new Error(`${adapter.label}分页包含重复位置，请重新查询。`);
      if (keys.length > controller.query.page_size || (page.next_cursor !== null && keys.length === 0)) throw new Error(`${adapter.label}分页大小无效，请重新查询。`);
      controller.pages = [...controller.pages, page];
      if (controller.pages.length > CACHE_PAGES) { controller.pages = controller.pages.slice(1); ++controller.firstNumber; }
      controller.index = controller.pages.length - 1; controller.error = null;
      // Only two SQLite lease actors exist. An offscreen DTO must not hold a
      // reader that the active page's facets or turn list may need.
      if (offscreen.current) await release(controller);
    } catch (error) { await release(controller); controller.error = runtimeError(error); }
    finally { controller.readAt = Date.now(); controller.busy = false; display(controller, resultKey); }
  }, () => !controller.disposed && !offscreen.current, () => !controller.disposed);
  useEffect(() => {
    const controller: Controller<Query, Page> = { query, disposed: false, busy: true, cursor: null, pages: [], index: 0, firstNumber: 1, error: null, readAt: 0, needsRenewal: false };
    current.current = controller; display(controller, key); enqueue(() => read(controller, key));
    const invalidate = () => { if (!controller.disposed && !document.hidden) setRevision(value => value + 1); };
    let stop: (() => void) | null = null;
    void onPriceRulesChanged(invalidate).then(unsubscribe => { if (controller.disposed) unsubscribe(); else stop = unsubscribe; }).catch(() => {});
    document.addEventListener('visibilitychange', invalidate);
    return () => { controller.disposed = true; controller.pages = []; stop?.(); document.removeEventListener('visibilitychange', invalidate); enqueue(() => release(controller)); };
  }, [key, adapter]);
  // Warm only the first page offscreen. Browsing a frozen multi-page snapshot
  // must never jump back to page one because a background timer fired.
  useEffect(() => {
    const warm = () => {
      const controller = current.current;
      if (!document.hidden && controller && !controller.disposed && !controller.busy && controller.pages.length <= 1 && controller.firstNumber === 1 && Date.now() - controller.readAt >= 10_000) setRevision(value => value + 1);
    };
    const controller = current.current;
    if (!background) {
      if (controller && !controller.disposed && !controller.busy && controller.needsRenewal) setRevision(value => value + 1);
      else warm();
      return;
    }
    if (controller && !controller.disposed && !controller.busy) enqueue(() => release(controller));
    const timer = setInterval(warm, 10_000);
    return () => clearInterval(timer);
  }, [background]);
  const previous = () => { const controller = current.current; if (controller && !controller.disposed && !controller.busy && controller.index > 0) { --controller.index; display(controller, key); } };
  const next = () => {
    const controller = current.current; if (!controller || controller.disposed || controller.busy) return;
    if (controller.index < controller.pages.length - 1) { ++controller.index; display(controller, key); }
    else if (controller.cursor !== null) { controller.busy = true; display(controller, key); enqueue(() => read(controller, key)); }
  };
  const visible = result?.scope === scope ? result : null;
  const loading = visible?.key !== key || (visible?.loading ?? true);
  return { page: visible?.page ?? null, pageNumber: visible?.number ?? 1, hasPrevious: !loading && (visible?.previous ?? false), hasNext: !loading && (visible?.next ?? false), trimmed: visible?.trimmed ?? false, loading, error: visible?.error ?? null, previous, next, reload: () => setRevision(value => value + 1) };
}
