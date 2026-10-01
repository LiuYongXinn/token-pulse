import type { Page } from '@playwright/test';
declare global { interface Window { __syntheticCalendar: (command: string, args: Record<string, unknown>) => unknown } }
/** Explicit synthetic fixed-offset DTO bridge for rendering tests only. Rust tests own DST accuracy. */
export async function installSyntheticCalendar(page: Page) {
  await page.addInitScript(() => {
    window.__syntheticCalendar = (command, args) => {
      if (command === 'get_display_settings') return { settings_version: 1, settings_revision: '1', preferences: { privacy: false, display_timezone: 'Asia/Shanghai' } };
      const request = args.request as { timezone: string; selection: { kind: 'today' | 'last7' | 'last30' | 'custom'; start_date?: string; end_date_inclusive?: string } };
      const offset = 8 * 3_600_000, day = 86_400_000;
      const local = new Date(Date.now() + offset);
      const start = Date.UTC(local.getUTCFullYear(), local.getUTCMonth(), local.getUTCDate()) - offset;
      const rangeStart = request.selection.kind === 'custom' ? Date.parse(request.selection.start_date! + 'T00:00:00Z') - offset : start - ({ today: 0, last7: 6, last30: 29 }[request.selection.kind]) * day;
      const rangeEnd = request.selection.kind === 'custom' ? Date.parse(request.selection.end_date_inclusive! + 'T00:00:00Z') + day - offset : start + day;
      return { range: { start_ms: rangeStart, end_ms: rangeEnd, timezone: request.timezone }, heatmap_range: { start_ms: start - 181 * day, end_ms: start + day, timezone: request.timezone }, local_today: local.toISOString().slice(0, 10) };
    };
  });
}
