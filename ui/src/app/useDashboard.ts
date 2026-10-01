import type { DashboardRequest } from '../shared/generated/contracts';
import { getDashboardBundle } from '../shared/runtime';
import { useSnapshotQuery } from './useSnapshotQuery';

export function useDashboard(request: DashboardRequest, refreshRevision: number) {
  return useSnapshotQuery(request, refreshRevision, getDashboardBundle);
}
