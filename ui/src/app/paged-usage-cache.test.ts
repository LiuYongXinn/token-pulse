import { expect, test, vi } from 'vitest';
import { PagedUsage, pagedUsage, clearPagedUsage, type SnapshotPage } from './paged-usage-cache';
const base = { meta: { snapshot_id: 'old' }, summary: {}, pricing: {}, coverage: {} } as SnapshotPage;
type Page = SnapshotPage & { rows: string[] };
const flush = async () => { for (let i = 0; i < 20; ++i) await Promise.resolve(); };
test('released leases retain pages and position, new data never appends or jumps', async () => {
  vi.stubGlobal('document', { hidden: false });
  const reads: (string | null)[] = [], closes: string[] = [];
  const controller = new PagedUsage({ page_size: 1 }, { label: 'test', keys: (page: Page) => page.rows,
    read: async ({ cursor }) => { reads.push(cursor); return { ...base, rows: [cursor ? 'b' : 'a'], next_cursor: cursor ? 'next-b' : 'next-a' }; },
    close: async ({ cursor }) => { if (cursor) closes.push(cursor); } });
  const owner = {}; const leave = controller.attach(owner, true); await flush();
  expect(controller.get().pages[0].next_cursor).toBeNull();
  controller.next(); await flush(); expect(controller.get().index).toBe(1);
  controller.invalidate(); expect(controller.get()).toMatchObject({ index: 1, updateAvailable: true });
  leave(); await flush(); expect(closes).toEqual(['next-b']);
  const leaveAgain = controller.attach(owner, true); await flush();
  expect(controller.get().index).toBe(1); expect(reads).toEqual([null, 'next-a']);
  controller.previous(); controller.next(); expect(controller.get().index).toBe(1);
  controller.next(); await flush(); expect(reads).toEqual([null, 'next-a']);
  expect(controller.get().renewal).toBe(true);
  controller.reload(); await flush(); expect(controller.get()).toMatchObject({ index: 0, firstNumber: 1, updateAvailable: false });
  expect(controller.get().pages).toHaveLength(1);
  leaveAgain(); controller.dispose(); await flush(); vi.unstubAllGlobals();
});
test('failed replacement keeps old multi-page display and snapshot mismatch never appends', async () => {
  vi.stubGlobal('document', { hidden: false });
  let fail = false;
  const controller = new PagedUsage({ page_size: 1 }, { label: 'test', keys: (page: Page) => page.rows,
    read: async ({ cursor }) => { if (fail) throw new Error('failed'); return { ...base, meta: { ...base.meta, snapshot_id: cursor ? 'other' : 'old' }, rows: [cursor ? 'b' : 'a'], next_cursor: 'cursor' }; }, close: async () => {} });
  const stop = controller.attach({}, true); await flush(); controller.next(); await flush();
  expect(controller.get().pages).toHaveLength(1); expect(controller.get().error).toContain('不一致');
  fail = true; controller.reload(); await flush();
  expect(controller.get().pages[0].meta.snapshot_id).toBe('old'); expect(controller.get().error).toBe('failed');
  stop(); controller.dispose(); await flush(); vi.unstubAllGlobals();
});

test('ten protected resident ranges retain their pages and reject an eleventh result', async () => {
  vi.stubGlobal('document', { hidden: false });
  clearPagedUsage();
  const stops: (() => void)[] = [], views: PagedUsage<{ page_size: number }, Page>[] = [];
  for (let n = 0; n < 11; ++n) {
    const view = pagedUsage<{ page_size: number }, Page>(`capacity-${n}`, { page_size: 1 }, { label: 'capacity', keys: (page: Page) => page.rows, read: async () => ({ ...base, rows: [`row-${n}`], next_cursor: null }), close: async () => {} });
    views.push(view); stops.push(view.attach({}, true)); await flush();
  }
  for (const view of views.slice(0, 10)) expect(view.get().pages).toHaveLength(1);
  expect(views[10].get().pages).toHaveLength(0);
  expect(views[10].get().error).toContain('缓存容量');
  for (const stop of stops) stop(); clearPagedUsage(); await flush(); vi.unstubAllGlobals();
});
