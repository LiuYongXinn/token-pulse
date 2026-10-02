import type { QuotaSnapshot } from '../../ui/src/shared/generated/contracts';
/** Explicit synthetic DTO for independent presentation expectations, never production data. */
export function syntheticQuota(now = Date.now()): QuotaSnapshot {
  return { connection_epoch: 'synthetic-display-account', quota_revision: '9007199254740993', state: 'ready', selected_limit_id: 'codex', available_limits: [{ limit_id: 'codex', display_name: 'SYNTHETIC ACCOUNT BUCKET' }], fetched_at_ms: now, last_attempt_at_ms: now, error_code: null, windows: [
    { window_id: 'primary', duration_mins: 10080, used_percent: 86, remaining_percent: 14, resets_at_ms: now + 60_000 },
    { window_id: 'secondary', duration_mins: 120, used_percent: 100, remaining_percent: 0, resets_at_ms: now - 1 },
    { window_id: 'unknown', duration_mins: null, used_percent: null, remaining_percent: null, resets_at_ms: null },
  ] };
}
