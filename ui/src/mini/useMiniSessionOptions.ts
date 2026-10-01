import { useEffect, useRef, useState } from 'react';
import type { MiniSessionOption, MiniSessionsQuery, MiniSessionsRequest, SnapshotMeta } from '../shared/generated/contracts';
import { closeQuerySnapshot, queryMiniSessions, runtimeError } from '../shared/runtime';

type Result = { key: string; options: MiniSessionOption[]; meta: SnapshotMeta | null; more: boolean; limited: boolean; loading: boolean; error: string | null };
type Controller = { query: MiniSessionsQuery; disposed: boolean; cursor: string | null; busy: boolean; options: MiniSessionOption[]; meta: SnapshotMeta | null };
const MAX_OPTIONS = 1000;

/** Serializes search/close/page requests and cleans up late responses too. */
export function useMiniSessionOptions(query: MiniSessionsQuery | null) {
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState<Result | null>(null);
  const tail = useRef<Promise<void>>(Promise.resolve());
  const current = useRef<Controller | null>(null);
  const key = JSON.stringify([query, revision]);
  const enqueue = (work: () => Promise<void>) => { tail.current = tail.current.catch(() => {}).then(work); };
  const release = async (controller: Controller) => {
    const cursor = controller.cursor; controller.cursor = null;
    if (cursor !== null) {
      // Failure still has bounded server TTL; never retry using a new query.
      await closeQuerySnapshot({ kind: 'mini_sessions', request: { query: controller.query, cursor } }).catch(() => {});
    }
  };
  const read = async (controller: Controller, resultKey: string) => {
    if (controller.disposed) return;
    try {
      const request: MiniSessionsRequest = { query: controller.query, cursor: controller.cursor };
      const page = await queryMiniSessions(request);
      controller.cursor = page.next_cursor;
      if (controller.disposed) { await release(controller); return; }
      if ((controller.meta && JSON.stringify(page.meta) !== JSON.stringify(controller.meta))) throw new Error('候选分页身份不一致，请重新查询。');
      const known = new Set(controller.options.map(option => option.session_key));
      if (page.options.some(option => known.has(option.session_key))) throw new Error('候选分页包含重复位置，请重新查询。');
      controller.options = [...controller.options, ...page.options]; controller.meta = page.meta;
      const limited = controller.options.length >= MAX_OPTIONS && page.next_cursor !== null;
      if (limited) await release(controller);
      if (!controller.disposed) setResult({ key: resultKey, options: controller.options, meta: page.meta, more: controller.cursor !== null, limited, loading: false, error: null });
    } catch (error) {
      await release(controller);
      if (!controller.disposed) setResult({ key: resultKey, options: controller.options, meta: controller.meta, more: false, limited: false, loading: false, error: runtimeError(error) });
    } finally { controller.busy = false; }
  };
  useEffect(() => {
    if (query === null) { current.current = null; return; }
    const controller: Controller = { query, disposed: false, cursor: null, busy: true, options: [], meta: null };
    current.current = controller;
    setResult({ key, options: [], meta: null, more: false, limited: false, loading: true, error: null });
    enqueue(() => read(controller, key));
    return () => { controller.disposed = true; enqueue(() => release(controller)); };
  }, [key]);
  const loadMore = () => {
    const controller = current.current;
    if (!controller || controller.disposed || controller.busy || controller.cursor === null) return;
    controller.busy = true;
    setResult(value => value?.key === key ? { ...value, loading: true } : value);
    enqueue(() => read(controller, key));
  };
  const visible = result?.key === key ? result : null;
  return { options: visible?.options ?? [], meta: visible?.meta ?? null, more: visible?.more ?? false, limited: visible?.limited ?? false, loading: query !== null && (visible?.loading ?? true), error: visible?.error ?? null, loadMore, reload: () => setRevision(value => value + 1) };
}
