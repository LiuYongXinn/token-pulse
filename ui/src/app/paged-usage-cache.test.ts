import { expect, test, vi } from 'vitest';
import { PagedUsage, pagedUsage, clearPagedUsage, type SnapshotPage } from './paged-usage-cache';
const base = { meta: { snapshot_id: 'old' }, summary: {}, pricing: {}, coverage: {} } as SnapshotPage;
type Page = SnapshotPage & { rows: string[] };
const flush = async () => { for (let i = 0; i < 20; ++i) await Promise.resolve(); };
test('slow replacement does not request renewal while retaining the old page', async () => {
  vi.stubGlobal('document', { hidden: false });
  let finish: ((page: Page) => void) | undefined;
  let hold = false;
  const closes: string[] = [];
  const controller = new PagedUsage({ page_size: 1 }, { label: 'test', keys: (page: Page) => page.rows,
    read: async () => hold ? new Promise<Page>(resolve => { finish = resolve; }) : { ...base, rows: ['a'], next_cursor: 'old-cursor' },
    close: async ({ cursor }) => { if (cursor) closes.push(cursor); } });
  const stop = controller.attach({}, true); await flush();
  hold = true; controller.reload(); await flush();
  expect(closes).toEqual(['old-cursor']);
  expect(controller.get()).toMatchObject({ loading: true, renewal: false });
  expect(controller.get().pages[0].rows).toEqual(['a']);
  finish!({ ...base, rows: ['new'], next_cursor: 'new-cursor' }); await flush();
  expect(controller.get()).toMatchObject({ loading: false, renewal: false, hasMore: true });
  controller.next(); await flush();
  finish!({ ...base, rows: ['next'], next_cursor: null }); await flush();
  expect(controller.get().pages[1].rows).toEqual(['next']);
  stop(); controller.dispose(); await flush(); vi.unstubAllGlobals();
});

test('foreground return automatically renews a released first page', async () => {
  vi.stubGlobal('document', { hidden: false });
  const reads: (string | null)[] = [];
  const controller = new PagedUsage({ page_size: 1 }, { label: 'test', keys: (page: Page) => page.rows,
    read: async ({ cursor }) => { reads.push(cursor); return { ...base, rows: [cursor ? 'b' : 'a'], next_cursor: cursor ? null : 'cursor' }; }, close: async () => {} });
  const owner = {}; const stopBackground = controller.attach(owner, false); await flush();
  expect(controller.get()).toMatchObject({ renewal: true, loading: false });
  stopBackground(); const stop = controller.attach(owner, true); await flush();
  expect(reads).toEqual([null, null]);
  expect(controller.get()).toMatchObject({ renewal: false, loading: false });
  controller.next(); await flush(); expect(reads).toEqual([null, null, 'cursor']);
  stop(); controller.dispose(); await flush(); vi.unstubAllGlobals();
});

test('queued background cleanup cannot release a reattached foreground cursor', async () => {
  vi.stubGlobal('document', { hidden: false });
  const closes: string[] = [];
  const controller = new PagedUsage({ page_size: 1 }, { label: 'test', keys: (page: Page) => page.rows,
    read: async ({ cursor }) => ({ ...base, rows: [cursor ? 'b' : 'a'], next_cursor: cursor ? null : 'cursor' }),
    close: async ({ cursor }) => { if (cursor) closes.push(cursor); } });
  const owner = {}; const leave = controller.attach(owner, true); await flush();
  leave(); const stop = controller.attach(owner, true); await flush();
  expect(closes).toEqual([]); expect(controller.get().renewal).toBe(false);
  controller.next(); await flush(); expect(controller.get().pages[1].rows).toEqual(['b']);
  stop(); controller.dispose(); await flush(); vi.unstubAllGlobals();
});

test('expired continuation still requests renewal and preserves the displayed page', async () => {
  vi.stubGlobal('document', { hidden: false });
  let now = 1_000;
  const time = vi.spyOn(Date, 'now').mockImplementation(() => now);
  const read = vi.fn(async () => ({ ...base, rows: ['a'], next_cursor: 'cursor' }));
  const controller = new PagedUsage({ page_size: 1 }, { label: 'test', keys: (page: Page) => page.rows, read, close: async () => {} });
  const stop = controller.attach({}, true); await flush();
  now += 8_000; controller.next(); await flush();
  expect(read).toHaveBeenCalledTimes(1);
  expect(controller.get()).toMatchObject({ renewal: true, loading: false, index: 0 });
  expect(controller.get().pages[0].rows).toEqual(['a']);
  controller.reload(); await flush();
  expect(read).toHaveBeenCalledTimes(2); expect(controller.get().renewal).toBe(false);
  stop(); controller.dispose(); await flush(); time.mockRestore(); vi.unstubAllGlobals();
});

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
  expect(controller.get().pages).toHaveLength(1); expect(controller.get().error).toContain('列表已变化');
  fail = true; controller.reload(); await flush();
  expect(controller.get().pages[0].meta.snapshot_id).toBe('old'); expect(controller.get().error).toBe('failed');
  stop(); controller.dispose(); await flush(); vi.unstubAllGlobals();
});

test('explicit refresh waits for an in-flight continuation and replaces all pages while background changes preserve position', async () => {
  vi.stubGlobal('document', { hidden: false });
  let finish: ((page: Page) => void) | undefined;
  let hold = false;
  const read = vi.fn(async ({ cursor }: { cursor: string | null }) => hold
    ? new Promise<Page>(resolve => { finish = resolve; })
    : { ...base, rows: [cursor ?? 'a'], next_cursor: 'b' });
  const controller = new PagedUsage({ page_size: 1 }, { label: 'test', keys: (page: Page) => page.rows, read, close: async () => {} });
  const stop = controller.attach({}, true); await flush();
  hold = true; controller.next(); await flush();
  controller.invalidate();
  finish!({ ...base, rows: ['b'], next_cursor: 'c' }); await flush();
  expect(controller.get()).toMatchObject({ index: 1, updateAvailable: true });
  expect(read).toHaveBeenCalledTimes(2);
  controller.next(); await flush();
  controller.invalidate(true);
  hold = false; finish!({ ...base, rows: ['c'], next_cursor: 'd' }); await flush();
  expect(read).toHaveBeenCalledTimes(4);
  expect(read.mock.calls.at(-1)?.[0].cursor).toBeNull();
  expect(controller.get()).toMatchObject({ index: 0, firstNumber: 1, loading: false, updateAvailable: false });
  expect(controller.get().pages).toHaveLength(1);
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
  expect(views[10].get().error).toContain('查询结果较多');
  for (const stop of stops) stop(); clearPagedUsage(); await flush(); vi.unstubAllGlobals();
});
