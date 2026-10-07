import { expect, test } from 'vitest';
import { scheduleUsageQuery } from './usage-query-scheduler';

test('slow page warming uses one reader and promotes a page selected while queued', async () => {
  const order: string[] = [];
  let release!: () => void;
  let selected = 'overview';
  const slow = scheduleUsageQuery(async () => {
    order.push('overview');
    await new Promise<void>(resolve => { release = resolve; });
  }, () => selected === 'overview', () => true, 'lease');
  const model = scheduleUsageQuery(async () => { order.push('models'); }, () => selected === 'models', () => true, 'lease');
  const project = scheduleUsageQuery(async () => { order.push('projects'); }, () => selected === 'projects', () => true, 'lease');
  const session = scheduleUsageQuery(async () => { order.push('sessions'); }, () => selected === 'sessions', () => true, 'lease');
  expect(order).toEqual(['overview']);
  selected = 'sessions';
  release();
  await Promise.all([slow, model, project, session]);
  expect(order).toEqual(['overview', 'sessions', 'models', 'projects']);
});

test('blocked background normal read cannot block foreground or independent lease work', async () => {
  let release!: () => void;
  const order: string[] = [];
  const background = scheduleUsageQuery(() => new Promise<void>(resolve => { release = resolve; }), () => false);
  const queued = scheduleUsageQuery(async () => { order.push('background'); }, () => false);
  const lease = scheduleUsageQuery(async () => { order.push('lease'); }, () => true, () => true, 'lease');
  const foreground = scheduleUsageQuery(async () => { order.push('foreground'); }, () => true);
  await Promise.all([lease, foreground]);
  expect(order).toContain('foreground'); expect(order).toContain('lease');
  release(); await Promise.all([background, queued]);
});

test('a failed query frees the queue for subsequent reads', async () => {
  const failure = scheduleUsageQuery(async () => { throw new Error('read failed'); }, () => true);
  let completed = false;
  const next = scheduleUsageQuery(async () => { completed = true; }, () => true);
  await expect(failure).rejects.toThrow('read failed');
  await next;
  expect(completed).toBe(true);
});

test('a new scope can replace an obsolete in-flight read without dispatching obsolete queued work', async () => {
  let current = true, completed = false, skipped = false;
  let release!: () => void;
  const old = scheduleUsageQuery(() => new Promise<void>(resolve => { release = resolve; }), () => true, () => current);
  const queued = scheduleUsageQuery(async () => { skipped = true; }, () => false, () => current);
  current = false;
  const replacement = scheduleUsageQuery(async () => { completed = true; }, () => true);
  await replacement;
  expect(completed).toBe(true);
  expect(skipped).toBe(false);
  release();
  await Promise.all([old, queued]);
});
