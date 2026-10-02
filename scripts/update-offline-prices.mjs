// Development-only, read-only official documentation retrieval. Never run at app startup.
// Review the resulting factual catalog and its diff before committing a new release.
import { writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const args = process.argv.slice(2);
if (args.length !== 3 || args[0] !== '--write' || !/^\d{4}-\d{2}-\d{2}$/.test(args[1]) || !/^openai-text-[a-z0-9-]+$/.test(args[2])) {
  throw new Error('Usage: node scripts/update-offline-prices.mjs --write YYYY-MM-DD openai-text-VERSION');
}
const verified = Date.parse(`${args[1]}T00:00:00Z`);
if (!Number.isSafeInteger(verified) || new Date(verified).toISOString().slice(0, 10) !== args[1]) throw new Error('Invalid verification date');
const reference = 'https://developers.openai.com/api/docs/pricing';
async function document(url) {
  const response = await fetch(`${url}.md`, { signal: AbortSignal.timeout(30000) });
  if (!response.ok || !response.url.startsWith('https://developers.openai.com/')) throw new Error(`Official document unavailable: ${url}`);
  return response.text();
}
function rate(value) {
  if (value === '-') return null;
  if (!/^\$\d+(?:\.\d{1,9})?$/.test(value)) throw new Error(`Unrecognized rate: ${value}`);
  return value.slice(1);
}
const md = await document(reference);
const tierNames = ['standard', 'batch', 'flex', 'fast', 'ultrafast'];
const tables = tierNames.map(tier => {
  const label = tier[0].toUpperCase() + tier.slice(1);
  const section = md.split(`### ${label} pricing data`);
  if (section.length !== 2) throw new Error(`Missing/ambiguous ${label} table`);
  // Take only this immediate table, never fine-tuning, tools, or multimedia tables.
  const end = section[1].split('\n').findIndex((line, index, lines) => index > 2 && line.trim() === '' && lines[index - 1].startsWith('| '));
  if (end < 0) throw new Error(`Invalid ${label} table boundary`);
  return { tier, rows: section[1].split('\n').slice(0, end).filter(line => line.startsWith('| ')).slice(2) };
});
const parsed = tables.flatMap(({ tier, rows }) => rows.map(line => {
  const cells = line.split('|').slice(1, -1).map(value => value.trim());
  if (cells.length !== 9) throw new Error(`Unexpected columns: ${line}`);
  const model = cells[0].replace(/ \(<272K context length\)$/, '');
  if (!/^[a-z0-9.-]+$/.test(model)) throw new Error(`Unrecognized model: ${model}`);
  return { tier, model, values: cells.slice(1).map(rate) };
}));
const conditional = new Set(parsed.filter(row => row.values[4] !== null || row.values[2] !== null).map(row => row.model));
// A table label explicitly restricts the short band even if a particular tier has no long offer.
for (const { rows } of tables) for (const row of rows) if (row.includes('(<272K context length)')) conditional.add(row.split('|')[1].trim().replace(/ \(<272K context length\)$/, ''));
const entries = [];
function entry(model, tier, context, values, source) {
  if (values[0] === null || values[3] === null) throw new Error('Input and output must be published');
  entries.push({ model_exact: model, tier, context, input_per_million: values[0], cached_per_million: values[1], cache_write_per_million: values[2], output_per_million: values[3], reference: source });
}
for (const row of parsed) {
  entry(row.model, row.tier, conditional.has(row.model) ? 'short' : 'all', row.values.slice(0, 4), reference);
  if (row.values[4] !== null) entry(row.model, row.tier, 'long', row.values.slice(4), reference);
}
// Codex historical exact model IDs have their own official pages and remain useful in local logs.
for (const model of ['gpt-5-codex', 'gpt-5.1-codex', 'gpt-5.1-codex-max', 'gpt-5.1-codex-mini', 'gpt-5.2-codex', 'gpt-5.3-codex', 'codex-mini-latest']) {
  const source = `https://developers.openai.com/api/docs/models/${model}`;
  const modelMd = await document(source);
  const prices = ['Input', 'Cached input', 'Output'].map(label => {
    const matches = [...modelMd.matchAll(new RegExp(`^\\| ${label} \\| (\\$[0-9.]+) \\| 1M tokens \\|$`, 'gm'))];
    if (matches.length !== 1) throw new Error(`Missing/ambiguous ${model} ${label}`);
    return rate(matches[0][1]);
  });
  entry(model, 'standard', 'all', [prices[0], prices[1], null, prices[2]], source);
}
// Specialized text offers; future billing and embedding/moderation are deliberately separate.
for (const [model, tier, context, values] of [
  ['gpt-5.6-cyber', 'standard', 'short', ['$12.50', '$1.25', '$15.625', '$75.00']],
  ['gpt-5.5-cyber', 'standard', 'short', ['$12.50', '$1.25', '-', '$75.00']],
  ['chat-latest', 'standard', 'short', ['$5.00', '$0.50', '-', '$30.00']],
  ['gpt-5-search-api', 'standard', 'all', ['$1.25', '$0.125', '-', '$10.00']],
  ['gpt-5.3-codex', 'fast', 'all', ['$3.50', '$0.35', '-', '$28.00']],
]) {
  const expected = model.includes('cyber') ? `| ${model} | ${values.join(' | ')} |` : `| ${model} | ${values[0]} | ${values[1]} | ${values[3]} |`;
  if (!md.includes(expected)) throw new Error(`Specialized price changed: ${model}/${tier}`);
  entry(model, tier, context, values.map(rate), reference);
}
entries.sort((a, b) => `${a.model_exact}/${a.tier}/${a.context}`.localeCompare(`${b.model_exact}/${b.tier}/${b.context}`, 'en'));
const keys = new Set(entries.map(row => `${row.model_exact}/${row.tier}/${row.context}`));
if (keys.size !== entries.length || entries.length < 150 || entries.length > 500) throw new Error('Incomplete or duplicate catalog');
const catalog = { format_version: 1, catalog_id: args[2], verified_at_ms: verified, provider: 'openai', currency: 'USD', short_context_max_input: 272000, reference_basis: 'global_api_reference', entries };
await writeFile(fileURLToPath(new URL('../crates/token-pulse-core/data/offline-prices.json', import.meta.url)), `${JSON.stringify(catalog, null, 2)}\n`);
console.log(`Wrote ${entries.length} factual prices, ${new Set(entries.map(row => row.model_exact)).size} exact models; review sources and diff.`);
