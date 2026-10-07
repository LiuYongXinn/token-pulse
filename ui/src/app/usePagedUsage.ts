import { useEffect, useRef, useSyncExternalStore } from 'react';
import { displayPolicy } from '../shared/display-policy';
import { usageQueryKey } from './usage-query-key';
import { clearPagedUsage, pagedUsage, type PageAdapter, type SnapshotPage } from './paged-usage-cache';
export type { PageAdapter } from './paged-usage-cache';
let epoch = displayPolicy.get().epoch;
displayPolicy.subscribe(() => { if (epoch !== displayPolicy.get().epoch) { epoch = displayPolicy.get().epoch; clearPagedUsage(); } });
export function usePagedUsage<Query extends { page_size: number }, Page extends SnapshotPage>(query: Query, refreshRevision: number, adapter: PageAdapter<Query, Page>, background = false) {
  const policy = useSyncExternalStore(displayPolicy.subscribe, displayPolicy.get);
  const key = usageQueryKey(adapter.label, query, policy.epoch);
  const controller = pagedUsage(key, query, adapter);
  const owner = useRef({});
  const view = useSyncExternalStore(controller.subscribe, controller.get);
  useEffect(() => {
    if (policy.pending) return;
    return controller.attach(owner.current, !background);
  }, [controller, background, policy.pending]);
  useEffect(() => {
    if (controller.refreshRevision !== null && controller.refreshRevision !== refreshRevision) controller.invalidate();
    controller.refreshRevision = refreshRevision;
  }, [controller, refreshRevision]);
  return { page: view.pages[view.index] ?? null, pageNumber: view.firstNumber + view.index, hasPrevious: view.index > 0, hasNext: view.index < view.pages.length - 1 || view.hasMore, trimmed: view.firstNumber > 1, loading: view.loading, error: view.error, renewal: view.renewal, updateAvailable: view.updateAvailable, previous: controller.previous, next: controller.next, reload: controller.reload };
}
