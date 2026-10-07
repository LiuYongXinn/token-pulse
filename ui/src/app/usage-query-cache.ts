export type CachedResult<T> = Readonly<{ value: T | null; loading: boolean; stale: boolean; error: string | null; fetchedAt: number | null }>;
type Entry<T> = { snapshot: CachedResult<T>; listeners: Set<() => void>; flight: Promise<void> | null; invalidation: number; bytes: number; used: number };

/** Failed refreshes keep a complete DTO; external-store references stay stable. */
export class UsageQueryCache {
  private entries = new Map<string, Entry<unknown>>();
  constructor(private limit = 20, private budget = 16 * 1024 * 1024) {}
  private entry<T>(key: string): Entry<T> {
    let entry = this.entries.get(key);
    if (!entry) {
      this.evict(1);
      entry = { snapshot: { value: null, loading: false, stale: true, error: null, fetchedAt: null }, listeners: new Set(), flight: null, invalidation: 0, bytes: 0, used: Date.now() };
      this.entries.set(key, entry);
    }
    return entry as Entry<T>;
  }
  get<T>(key: string): CachedResult<T> { return this.entry<T>(key).snapshot; }
  subscribe(key: string, listener: () => void) {
    const entry = this.entry(key); entry.used = Date.now(); entry.listeners.add(listener);
    return () => { entry.listeners.delete(listener); this.evict(); };
  }
  private publish<T>(entry: Entry<T>, update: Partial<CachedResult<T>>) {
    entry.snapshot = { ...entry.snapshot, ...update };
    for (const listener of entry.listeners) listener();
  }
  invalidate(key?: string) {
    for (const [id, entry] of this.entries) if (key === undefined || id === key) {
      ++entry.invalidation; this.publish(entry, { stale: true });
    }
  }
  clear() {
    const entries = [...this.entries.values()]; this.entries.clear();
    for (const entry of entries) { ++entry.invalidation; this.publish(entry, { value: null, stale: true, loading: false, error: null }); }
  }
  async read<T>(key: string, work: () => Promise<T>, errorText: (error: unknown) => string): Promise<void> {
    const entry = this.entry<T>(key);
    if (entry.flight) return entry.flight;
    if (!entry.snapshot.stale && entry.snapshot.value !== null) return;
    const version = entry.invalidation;
    this.publish(entry, { loading: true });
    entry.flight = (async () => {
      try {
        const value = await work();
        if (this.entries.get(key) !== entry) return;
        entry.bytes = new TextEncoder().encode(JSON.stringify(value)).length;
        this.publish(entry, { value, loading: false, stale: version !== entry.invalidation, error: null, fetchedAt: Date.now() });
      } catch (error) {
        if (this.entries.get(key) === entry) this.publish(entry, { loading: false, stale: true, error: errorText(error) });
      } finally { entry.flight = null; this.evict(); }
    })();
    await entry.flight;
    if (this.entries.get(key) === entry && entry.listeners.size && version !== entry.invalidation) await this.read(key, work, errorText);
  }
  private evict(reserve = 0) {
    let bytes = [...this.entries.values()].reduce((sum, entry) => sum + entry.bytes, 0);
    for (const [key, entry] of [...this.entries].sort((a, b) => a[1].used - b[1].used)) {
      if (this.entries.size + reserve <= this.limit && bytes <= this.budget) break;
      if (entry.listeners.size || entry.flight) continue;
      this.entries.delete(key); bytes -= entry.bytes;
    }
  }
  stats() { return { keys: this.entries.size, bytes: [...this.entries.values()].reduce((sum, entry) => sum + entry.bytes, 0) }; }
}
export const usageQueryCache = new UsageQueryCache();
