import { expect, test } from 'vitest';
import { UsageQueryCache } from './usage-query-cache';
import { usageQueryKey } from './usage-query-key';
const message = (error: unknown) => String(error);
test('scope keys normalize sets but preserve dates, timezone, unknown and pricing', () => {
  const request = { range: { start_ms: 1, end_ms: 2, timezone: 'UTC' }, models: { kind: 'ids', ids: ['b', 'a', 'a'], include_unknown: false }, price_basis: { mode: 'event_time' } };
  const key = usageQueryKey('models', request, 1);
  expect(usageQueryKey('models', { ...request, models: { ...request.models, ids: ['a', 'b'] } }, 1)).toBe(key);
  for (const changed of [{ ...request, range: { ...request.range, end_ms: 3 } }, { ...request, range: { ...request.range, timezone: 'Asia/Shanghai' } }, { ...request, models: { ...request.models, include_unknown: true } }, { ...request, price_basis: { mode: 'specified_time', at_ms: 1 } }]) expect(usageQueryKey('models', changed, 1)).not.toBe(key);
  expect(usageQueryKey('models', request, 2)).not.toBe(key);
});
test('shared flight, stable snapshots, retained failure and invalidation follow-up', async () => {
  const cache = new UsageQueryCache();
  const stop = cache.subscribe('a', () => {});
  expect(cache.get('a')).toBe(cache.get('a'));
  let calls = 0, release!: (value: number) => void;
  const work = () => { ++calls; return calls === 1 ? new Promise<number>(resolve => { release = resolve; }) : Promise.resolve(2); };
  const first = cache.read('a', work, message), second = cache.read('a', work, message);
  expect(calls).toBe(1);
  cache.invalidate('a'); cache.invalidate('a'); release(1);
  await Promise.all([first, second]);
  expect(calls).toBe(2); expect(cache.get('a').value).toBe(2);
  cache.invalidate('a'); await cache.read('a', async () => { throw new Error('failed'); }, message);
  expect(cache.get('a')).toMatchObject({ value: 2, loading: false, stale: true, error: 'Error: failed' });
  stop();
});
test('privacy clear blocks late responses and capacity removes unobserved ranges', async () => {
  const cache = new UsageQueryCache(2);
  let release!: (value: string) => void;
  const old = cache.read('a', () => new Promise<string>(resolve => { release = resolve; }), message);
  cache.clear(); release('sensitive'); await old;
  expect(cache.get('a').value).toBeNull();
  await cache.read('b', async () => 'b', message);
  await cache.read('c', async () => 'c', message);
  expect(cache.stats().keys).toBeLessThanOrEqual(2);
  expect(cache.get('c').value).toBe('c');
});
