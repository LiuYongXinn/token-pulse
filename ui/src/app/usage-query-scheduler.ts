import { recordQueryTiming } from '../shared/query-timing';
type Task = { run: () => Promise<void>; foreground: () => boolean; current: () => boolean };
const pending: Task[] = [];
const running = new Set<Task>();

/** Leave a database reader free for mini/status/detail queries while warming pages. */
export function scheduleUsageQuery(work: () => Promise<void>, foreground: () => boolean, current: () => boolean = () => true): Promise<void> {
  const queued = performance.now();
  return new Promise((resolve, reject) => {
    pending.push({ foreground, current, run: async () => {
      try { if (current()) { recordQueryTiming('snapshot', 'queue', queued); await work(); } resolve(); } catch (error) { reject(error); }
    } });
    drain();
  });
}

function drain() {
  // IPC reads cannot be cancelled. If a filter/privacy change invalidates the
  // in-flight read, allow its replacement on the second reader rather than
  // blocking the new scope behind the obsolete response. Never exceed two.
  while (pending.length && running.size < 2 && ![...running].some(task => task.current())) {
    // Evaluate at dispatch so navigation can promote an already queued page.
    const foreground = pending.findIndex(task => task.foreground());
    const [task] = pending.splice(foreground < 0 ? 0 : foreground, 1);
    running.add(task);
    void task.run().finally(() => { running.delete(task); drain(); });
  }
}
