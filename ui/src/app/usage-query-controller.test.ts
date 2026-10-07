import { expect, test, vi } from 'vitest';
import { UsageUpdateController } from './usage-query-controller';
const revision = (value: number) => ({ database_id: 'a'.repeat(32), data_revision: '1', price_revision: '1', usage_view_revision: String(value) });
test('coalesces view-only changes, ignores old notifications and unchanged polling, resumes hidden state', async () => {
  vi.useFakeTimers();
  let current = 1, hidden = false, updates = 0;
  const controller = new UsageUpdateController(async () => revision(current), () => ++updates, () => {}, () => hidden);
  await controller.check(); await controller.check(); await vi.advanceTimersByTimeAsync(300); expect(updates).toBe(0);
  controller.accept(revision(2)); controller.accept(revision(3)); controller.accept(revision(2));
  await vi.advanceTimersByTimeAsync(200); expect(updates).toBe(1);
  hidden = true; controller.accept(revision(4)); await vi.advanceTimersByTimeAsync(2000); expect(updates).toBe(1);
  current = 4; hidden = false; await controller.resume(); await vi.advanceTimersByTimeAsync(200); expect(updates).toBe(2);
  controller.dispose(); controller.changed(); await vi.advanceTimersByTimeAsync(500); expect(updates).toBe(2); vi.useRealTimers();
});
