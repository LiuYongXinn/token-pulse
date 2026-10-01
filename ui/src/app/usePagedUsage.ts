import { useEffect, useRef, useState } from 'react';
import type { Coverage, PricingSummary, SnapshotMeta, TokenTotals } from '../shared/generated/contracts';
import { onPriceRulesChanged, runtimeError } from '../shared/runtime';

type SnapshotPage = { meta: SnapshotMeta; summary: TokenTotals; pricing: PricingSummary; coverage: Coverage; next_cursor: string | null };
export type PageAdapter<Query, Page> = { label: string; read: (request: { query: Query; cursor: string | null }) => Promise<Page>; close: (request: { query: Query; cursor: string | null }) => Promise<void>; keys: (page: Page) => string[] };
type Controller<Query, Page> = { query: Query; disposed: boolean; busy: boolean; cursor: string | null; pages: Page[]; index: number; firstNumber: number; error: string | null };
type Result<Page> = { key: string; page: Page | null; number: number; previous: boolean; next: boolean; loading: boolean; error: string | null; trimmed: boolean };
const CACHE_PAGES = 10;

/** Read and release serially; cache a bounded set of immutable DTO pages. */
export function usePagedUsage<Query extends { page_size: number }, Page extends SnapshotPage>(query: Query, refreshRevision: number, adapter: PageAdapter<Query, Page>) {
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState<Result<Page> | null>(null);
  const tail = useRef<Promise<void>>(Promise.resolve());
  const current = useRef<Controller<Query, Page> | null>(null);
  const key = JSON.stringify([adapter.label, query, refreshRevision, revision]);
  const enqueue = (work: () => Promise<void>) => { tail.current = tail.current.catch(() => {}).then(work); };
  const release = async (controller: Controller<Query, Page>) => {
    const cursor = controller.cursor; controller.cursor = null;
    if (cursor !== null) await adapter.close({ query: controller.query, cursor }).catch(() => {});
  };
  const display = (controller: Controller<Query, Page>, resultKey: string) => {
    if (!controller.disposed) setResult({ key: resultKey, page: controller.pages[controller.index] ?? null, number: controller.firstNumber + controller.index, previous: controller.index > 0, next: controller.index < controller.pages.length - 1 || controller.cursor !== null, loading: controller.busy, error: controller.error, trimmed: controller.firstNumber > 1 });
  };
  const read = async (controller: Controller<Query, Page>, resultKey: string) => {
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
    } catch (error) { await release(controller); controller.error = runtimeError(error); }
    finally { controller.busy = false; display(controller, resultKey); }
  };
  useEffect(() => {
    const controller: Controller<Query, Page> = { query, disposed: false, busy: true, cursor: null, pages: [], index: 0, firstNumber: 1, error: null };
    current.current = controller; display(controller, key); enqueue(() => read(controller, key));
    const invalidate = () => { if (!controller.disposed && !document.hidden) setRevision(value => value + 1); };
    let stop: (() => void) | null = null;
    void onPriceRulesChanged(invalidate).then(unsubscribe => { if (controller.disposed) unsubscribe(); else stop = unsubscribe; }).catch(() => {});
    document.addEventListener('visibilitychange', invalidate);
    return () => { controller.disposed = true; controller.pages = []; stop?.(); document.removeEventListener('visibilitychange', invalidate); enqueue(() => release(controller)); };
  }, [key, adapter]);
  const previous = () => { const controller = current.current; if (controller && !controller.disposed && !controller.busy && controller.index > 0) { --controller.index; display(controller, key); } };
  const next = () => {
    const controller = current.current; if (!controller || controller.disposed || controller.busy) return;
    if (controller.index < controller.pages.length - 1) { ++controller.index; display(controller, key); }
    else if (controller.cursor !== null) { controller.busy = true; display(controller, key); enqueue(() => read(controller, key)); }
  };
  const visible = result?.key === key ? result : null;
  return { page: visible?.page ?? null, pageNumber: visible?.number ?? 1, hasPrevious: visible?.previous ?? false, hasNext: visible?.next ?? false, trimmed: visible?.trimmed ?? false, loading: visible?.loading ?? true, error: visible?.error ?? null, previous, next, reload: () => setRevision(value => value + 1) };
}
