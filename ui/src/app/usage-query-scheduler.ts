import { recordQueryTiming } from '../shared/query-timing';
type Resource = 'snapshot' | 'lease';
type Task = { run: () => Promise<void>; foreground: () => boolean; current: () => boolean; resource: Resource };
const pending: Task[] = [];
const running = new Set<Task>();

/** Normal reads and lease actors are independent. Background uses one per pool. */
export function scheduleUsageQuery(work: () => Promise<void>, foreground: () => boolean, current: () => boolean = () => true, resource: Resource = 'snapshot'): Promise<void> {
  const queued = performance.now();
  return new Promise((resolve, reject) => {
    pending.push({ foreground, current, resource, run: async () => {
      try { if (current()) { recordQueryTiming(resource, 'queue', queued); await work(); } resolve(); } catch (error) { reject(error); }
    } });
    promoteUsageQueries();
  });
}
export function promoteUsageQueries() {
  // Skipped work settles without occupying a slot. Running IPC remains counted.
  for (let index = pending.length - 1; index >= 0; --index) if (!pending[index].current()) {
    const [task] = pending.splice(index, 1); void task.run();
  }
  while (running.size < 3) {
    const eligible = (task: Task) => {
      const pool = [...running].filter(active => active.resource === task.resource);
      // One lease is reserved for facets/details/other windows. Foreground normal
      // reads can use the second reader even if a background computation blocks.
      return pool.length < (task.resource === 'lease' ? 1 : task.foreground() ? 2 : 1);
    };
    let index = pending.findIndex(task => task.foreground() && eligible(task));
    if (index < 0) index = pending.findIndex(task => eligible(task));
    if (index < 0) break;
    const [task] = pending.splice(index, 1); running.add(task);
    void task.run().finally(() => { running.delete(task); promoteUsageQueries(); });
  }
}
