import { runtimeError, restoreUsageSnapshot } from '../shared/runtime';
import type { Coverage, PricingSummary, SnapshotMeta, TokenTotals } from '../shared/generated/contracts';
import { promoteUsageQueries, scheduleUsageQuery } from './usage-query-scheduler';
export type SnapshotPage = { meta: SnapshotMeta; summary: TokenTotals; pricing: PricingSummary; coverage: Coverage; next_cursor: string | null };
export type PageAdapter<Query, Page> = { label: string; read: (request: { query: Query; cursor: string | null }) => Promise<Page>; close: (request: { query: Query; cursor: string | null }) => Promise<void>; keys: (page: Page) => string[] };
export type PagedView<Page> = Readonly<{ pages: Page[]; index: number; firstNumber: number; loading: boolean; error: string | null; renewal: boolean; updateAvailable: boolean; hasMore: boolean; restored: boolean }>;

/** Display DTOs never contain a resumable cursor. The controller alone owns authorization. */
export class PagedUsage<Query extends { page_size: number }, Page extends SnapshotPage> {
  private value: PagedView<Page> = { pages: [], index: 0, firstNumber: 1, loading: false, error: null, renewal: false, updateAvailable: false, hasMore: false, restored: false };
  private listeners = new Set<() => void>();
  private owners = new Map<object, boolean>();
  private cursor: string | null = null;
  private tail = Promise.resolve();
  private disposed = false;
  private readAt = 0;
  private openedAt = 0;
  private invalidation = 0;
  private pendingReplacement: 'first_page' | 'all_pages' | null = null;
  private restorationTried = false;
  refreshRevision: number | null = null;
  used = Date.now();
  constructor(private query: Query, private adapter: PageAdapter<Query, Page>) {}
  get = () => this.value;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  get protected() { return this.owners.size > 0 || this.value.loading; }
  get bytes() { return new TextEncoder().encode(JSON.stringify(this.value.pages)).length; }
  private get foreground() { return [...this.owners.values()].some(value => value); }
  private publish(update: Partial<PagedView<Page>>) {
    this.value = { ...this.value, ...update }; this.used = Date.now();
    for (const listener of this.listeners) listener();
  }
  attach(owner: object, foreground: boolean) {
    this.owners.set(owner, foreground); this.used = Date.now(); promoteUsageQueries();
    if (!this.value.pages.length && !this.value.loading) this.reload();
    else if ((this.value.updateAvailable || (foreground && this.value.renewal)) && this.value.pages.length === 1 && this.value.firstNumber === 1 && !this.value.loading && !document.hidden) this.reload();
    if (!this.foreground) this.enqueue(async () => { if (!this.foreground) await this.release(); });
    return () => { this.owners.delete(owner); if (!this.foreground) this.enqueue(async () => { if (!this.foreground) await this.release(); }); };
  }
  private enqueue(work: () => Promise<void>) { this.tail = this.tail.catch(() => {}).then(work); }
  private async release(renewal = true) {
    const cursor = this.cursor; this.cursor = null;
    if (cursor !== null) {
      if (!this.disposed && renewal) this.publish({ renewal: this.value.hasMore });
      await this.adapter.close({ query: this.query, cursor }).catch(() => {});
    }
  }
  dispose() { this.disposed = true; this.publish({ pages: [], index: 0, error: null }); this.enqueue(() => this.release()); }
  invalidate(replacePages = false) {
    ++this.invalidation;
    if (!replacePages && (this.value.pages.length > 1 || this.value.firstNumber > 1)) this.publish({ updateAvailable: true });
    else if (this.value.loading) {
      if (replacePages || this.pendingReplacement !== 'all_pages') this.pendingReplacement = replacePages ? 'all_pages' : 'first_page';
    }
    else if (this.owners.size && !document.hidden) this.reload();
    else this.publish({ updateAvailable: true });
  }
  reload = () => {
    if (this.disposed || this.value.loading) return;
    this.publish({ loading: true, error: null, renewal: false });
    this.enqueue(async () => {
      await this.release(false);
      if (!this.restorationTried && !this.value.pages.length) {
        this.restorationTried = true;
        const restored = await restoreUsageSnapshot(this.adapter.read, { query: this.query, cursor: null }).catch(() => null);
        if (restored && !this.disposed) {
          try {
            const pages = [{ ...restored.value, next_cursor: null }]; admitPages(this, pages);
            this.publish({ pages, index: 0, firstNumber: 1, hasMore: restored.hasMore, renewal: restored.hasMore, restored: true });
          } catch (error) { this.publish({ error: runtimeError(error) }); }
        }
      }
      if (!this.disposed) await this.read(true);
    });
  };
  private async read(replacement: boolean) {
    const version = this.invalidation;
    await scheduleUsageQuery(async () => {
      if (this.disposed) return;
      try {
        const page = await this.adapter.read({ query: this.query, cursor: replacement ? null : this.cursor });
        this.cursor = page.next_cursor;
        if (this.disposed) { await this.release(); return; }
        const first = replacement ? null : this.value.pages[0];
        if (first && JSON.stringify([page.meta, page.summary, page.pricing, page.coverage]) !== JSON.stringify([first.meta, first.summary, first.pricing, first.coverage])) throw new Error(`${this.adapter.label}列表已变化，请重新查询。`);
        const keys = this.adapter.keys(page), seen = new Set(replacement ? [] : this.value.pages.flatMap(this.adapter.keys));
        if (new Set(keys).size !== keys.length || keys.some(key => seen.has(key))) throw new Error(`${this.adapter.label}分页包含重复位置，请重新查询。`);
        if (keys.length > this.query.page_size || (page.next_cursor !== null && !keys.length)) throw new Error(`${this.adapter.label}分页大小无效，请重新查询。`);
        const pages = [...(replacement ? [] : this.value.pages), { ...page, next_cursor: null }];
        let firstNumber = replacement ? 1 : this.value.firstNumber;
        if (pages.length > 10) { pages.shift(); ++firstNumber; }
        const trimmed = admitPages(this, pages); firstNumber += trimmed;
        this.readAt = Date.now(); if (replacement) this.openedAt = this.readAt;
        this.publish({ restored: false, pages, index: pages.length - 1, firstNumber, error: null, renewal: false, hasMore: page.next_cursor !== null, updateAvailable: version !== this.invalidation });
        if (!this.foreground) await this.release();
      } catch (error) { await this.release(); if (!this.disposed) this.publish({ error: runtimeError(error), renewal: this.value.hasMore }); }
      finally {
        if (!this.disposed) {
          this.publish({ loading: false });
          if (this.pendingReplacement === 'all_pages' || (this.pendingReplacement === 'first_page' && this.value.pages.length <= 1)) { this.pendingReplacement = null; this.reload(); }
        }
      }
    }, () => this.foreground, () => !this.disposed, 'lease');
  }
  previous = () => { if (!this.value.loading && this.value.index > 0) this.publish({ index: this.value.index - 1 }); };
  next = () => {
    if (this.disposed || this.value.loading) return;
    if (this.value.index < this.value.pages.length - 1) { this.publish({ index: this.value.index + 1 }); return; }
    if (!this.value.hasMore) return;
    if (this.cursor === null || Date.now() - this.readAt >= 8_000 || Date.now() - this.openedAt >= 28_000) {
      this.publish({ renewal: true }); this.enqueue(() => this.release()); return;
    }
    this.publish({ loading: true }); this.enqueue(() => this.read(false));
  };
}
const views = new Map<string, PagedUsage<{ page_size: number }, SnapshotPage>>();
function admitPages<Query extends { page_size: number }, Page extends SnapshotPage>(current: PagedUsage<Query, Page>, pages: Page[]) {
  const budget = 16 * 1024 * 1024;
  const others = () => [...views.values()].filter(view => view !== (current as unknown));
  const residentOthers = () => others().filter(view => view.get().pages.length > 0).length;
  const size = () => new TextEncoder().encode(JSON.stringify(pages)).length;
  for (const [key, view] of [...views].sort((a, b) => a[1].used - b[1].used)) {
    if (residentOthers() < 10 && others().reduce((sum, item) => sum + item.bytes, 0) + size() <= budget) break;
    if (view !== (current as unknown) && !view.protected) { view.dispose(); views.delete(key); }
  }
  let trimmed = 0;
  while (pages.length > 1 && others().reduce((sum, item) => sum + item.bytes, 0) + size() > budget) { pages.shift(); ++trimmed; }
  if (residentOthers() >= 10 || others().reduce((sum, item) => sum + item.bytes, 0) + size() > budget) throw new Error('查询结果较多，请缩小范围或减少每页数量。');
  return trimmed;
}
export function pagedUsage<Query extends { page_size: number }, Page extends SnapshotPage>(key: string, query: Query, adapter: PageAdapter<Query, Page>): PagedUsage<Query, Page> {
  let view = views.get(key);
  if (!view) {
    for (const [id, old] of [...views].sort((a, b) => a[1].used - b[1].used)) {
      if (views.size < 10 && [...views.values()].reduce((sum, item) => sum + item.bytes, 0) < 16 * 1024 * 1024) break;
      if (!old.protected) { old.dispose(); views.delete(id); }
    }
    view = new PagedUsage(query, adapter) as unknown as PagedUsage<{ page_size: number }, SnapshotPage>; views.set(key, view);
  }
  return view as unknown as PagedUsage<Query, Page>;
}
export function clearPagedUsage() { for (const view of views.values()) view.dispose(); views.clear(); }
export function invalidatePagedUsage() { for (const view of views.values()) view.invalidate(); }
