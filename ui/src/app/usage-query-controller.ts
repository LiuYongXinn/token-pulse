import { useEffect } from 'react';
import type { UsageRevision } from '../shared/generated/contracts';
import { getUsageRevision, onUsageChanged, onPriceRulesChanged, onSettingsChanged } from '../shared/runtime';
import { displayPolicy } from '../shared/display-policy';
import { usageQueryCache } from './usage-query-cache';
import { clearPagedUsage, invalidatePagedUsage } from './paged-usage-cache';
import { promoteUsageQueries } from './usage-query-scheduler';

export class UsageUpdateController {
  private latest: UsageRevision | null = null;
  private dirty = false;
  private startedAt = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private reading = false;
  private disposed = false;
  constructor(private read: () => Promise<UsageRevision>, private update: () => void, private reset: () => void, private hidden: () => boolean) {}
  accept = (revision: UsageRevision) => {
    if (this.disposed) return;
    if (![revision.data_revision, revision.price_revision, revision.usage_view_revision].every(value => /^(0|[1-9][0-9]*)$/.test(value) && value.length <= 39) || !/^[a-f0-9]{32}$/.test(revision.database_id)) return;
    const old = this.latest;
    if (old?.database_id === revision.database_id && BigInt(revision.usage_view_revision) < BigInt(old.usage_view_revision)) return;
    this.latest = revision;
    if (old && old.database_id !== revision.database_id) this.reset();
    if (old && JSON.stringify(old) !== JSON.stringify(revision)) this.changed();
  };
  changed = () => {
    if (this.disposed) return;
    this.dirty = true;
    if (this.hidden()) return;
    if (!this.startedAt) this.startedAt = performance.now();
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => this.flush(), Math.min(200, Math.max(0, 1000 - (performance.now() - this.startedAt))));
  };
  private flush() {
    this.timer = null; this.startedAt = 0;
    if (!this.disposed && !this.hidden() && this.dirty) { this.dirty = false; this.update(); }
  }
  check = async () => {
    if (this.disposed || this.hidden() || this.reading) return;
    this.reading = true;
    try { this.accept(await this.read()); } catch { /* Connection failure never revokes saved DTOs. */ }
    finally { this.reading = false; if (this.dirty) this.changed(); }
  };
  resume = async () => { await this.check(); if (this.dirty) this.changed(); };
  dispose() { this.disposed = true; if (this.timer) clearTimeout(this.timer); }
}

/** One main-window subscription set and one lightweight revision poll. */
export function useUsageQueryController() {
  useEffect(() => {
    let active = true;
    const stops: (() => void)[] = [];
    const controller = new UsageUpdateController(getUsageRevision, () => { usageQueryCache.invalidate(); invalidatePagedUsage(); }, () => { usageQueryCache.clear(); clearPagedUsage(); }, () => document.hidden || displayPolicy.get().pending);
    void Promise.all([onUsageChanged(controller.accept), onPriceRulesChanged(controller.changed), onSettingsChanged(controller.changed)].map(promise => promise.then(stop => { if (active) stops.push(stop); else stop(); }).catch(() => {}))).then(() => { if (active) void controller.check(); });
    const timer = setInterval(() => void controller.check(), 30_000);
    const visible = () => { if (!document.hidden) { promoteUsageQueries(); void controller.resume(); } };
    document.addEventListener('visibilitychange', visible);
    return () => { active = false; controller.dispose(); stops.forEach(stop => stop()); clearInterval(timer); document.removeEventListener('visibilitychange', visible); };
  }, []);
}
