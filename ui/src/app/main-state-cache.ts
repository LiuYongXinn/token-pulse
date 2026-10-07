import { getSources } from '../shared/runtime';
import { useSnapshotQuery } from './useSnapshotQuery';
/** Saved read-only state, shared by filters, sources and diagnostics. */
export function useSourcesSnapshot(refreshRevision = 0) {
  return useSnapshotQuery(undefined, refreshRevision, getSources);
}
