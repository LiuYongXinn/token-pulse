/** Only ID sets are unordered. Every other field retains its meaning. */
export function canonicalRequest(value: unknown): string {
  function normalize(input: unknown): unknown {
    if (Array.isArray(input)) return input.map(normalize);
    if (input && typeof input === 'object') {
      const object = input as Record<string, unknown>;
      return Object.fromEntries(Object.keys(object).sort().map(key => [key,
        key === 'ids' && Array.isArray(object[key]) ? [...new Set(object[key])].sort() : normalize(object[key])]));
    }
    return input;
  }
  return JSON.stringify(normalize(value));
}
export function usageQueryKey(kind: string, request: unknown, epoch: number): string {
  return JSON.stringify([kind, canonicalRequest(request), epoch]);
}
