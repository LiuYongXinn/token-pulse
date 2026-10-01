/** UTC editor labels, independent of display timezone. Matching rules stays in Rust. */
export function utcPriceInput(at: number): string { return new Date(at).toISOString().slice(0, -1); }
export function parsePriceInstant(value: string): number | null {
  const match = /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2})(?::(\d{2})(?:\.(\d{1,3}))?)?$/.exec(value);
  if (!match) return null;
  const canonical = `${match[1]}:${match[2] ?? '00'}.${(match[3] ?? '0').padEnd(3, '0')}Z`;
  const at = Date.parse(canonical);
  return Number.isSafeInteger(at) && new Date(at).toISOString() === canonical ? at : null;
}
