import Ajv from 'ajv/dist/2020.js';
import { expect, test } from 'vitest';
import protocol from '../../../schemas/protocol-v1.json';
import type { TokenMeasure, MiniScope, DecimalInt, DashboardBundle } from './generated/contracts';

const ajv = new Ajv({ strict: false });

test('dashboard contract requires one complete bundle with null metrics and real metadata', () => {
  const measure = { value: null, covered_total_tokens: '0', complete: false };
  const totals = { total_tokens: '0', input_total: measure, cached_input: measure, noncached_input: measure, output_total: measure, reasoning_output: measure, session_count: '0', usage_event_count: '0', reliable_turn_count: null, reliable_turns_complete: false };
  const coverage = { state: 'unknown' as const, pending_observation_count: '0', unattributed_observation_count: '0', unattributed_total_tokens: null, pending_file_count: '0', source_issues: [], format_issues: [], breakdown_complete: false };
  const fixture: DashboardBundle = { meta: { snapshot_id: 'synthetic-bundle', data_revision: '0', price_revision: '0', generated_at_ms: 0, parser_versions: [], accounting_versions: [], display_timezone: 'UTC' }, summary: totals, pricing: { redacted: false, basis: { mode: 'event_time' }, currencies: [], priced_total_tokens: '0', unpriced_total_tokens: '0', reasons: [], calculating: false }, coverage, series: [{ start_ms: 0, end_ms: 1000, display_label: 'synthetic', utc_offset: '+00:00', totals, coverage }], heatmap: [], recent_sessions: [] };
  const validate = ajv.compile(protocol.schemas.DashboardBundle);
  expect(validate(fixture)).toBe(true);
  expect(validate({ ...fixture, pricing: undefined })).toBe(false);
  expect(validate({ ...fixture, meta: { ...fixture.meta, price_revision: undefined } })).toBe(false);
  expect(validate({ ...fixture, series: [{ ...fixture.series[0], coverage: undefined }] })).toBe(false);
  expect(validate({ ...fixture, series: Array(2001).fill(fixture.series[0]) })).toBe(false);
  const group = { key: null, display_name: '未知模型', totals, pricing: fixture.pricing, coverage };
  const grouped = { meta: fixture.meta, summary: totals, pricing: fixture.pricing, coverage, total_group_count: '1', truncated: false, groups: [group] };
  const validateGroups = ajv.compile(protocol.schemas.GroupedUsageBundle);
  expect(validateGroups(grouped)).toBe(true);
  expect(validateGroups({ ...grouped, total_group_count: undefined })).toBe(false);
  expect(validateGroups({ ...grouped, groups: [{ ...group, pricing: undefined }] })).toBe(false);
  expect(validateGroups({ ...grouped, groups: Array(201).fill(group) })).toBe(false);
  const validateGroupRequest = ajv.compile(protocol.schemas.GroupedUsageRequest);
  const all = { kind: 'all' };
  const groupRequest = { filter: { range: { start_ms: 0, end_ms: 1000, timezone: 'UTC' }, sources: all, models: all, projects: all, sessions: all }, price_basis: { mode: 'event_time' }, dimension: 'models', sort: 'total_desc', limit: 200 };
  expect(validateGroupRequest(groupRequest)).toBe(true);
  expect(validateGroupRequest({ ...groupRequest, limit: 201 })).toBe(false);
  expect(validateGroupRequest({ ...groupRequest, limit: 0 })).toBe(false);
});
test('Rust schema and TS preserve nullable fields and exact decimal strings', () => {
  const validate = ajv.compile(protocol.schemas.TokenMeasure);
  const fixture: TokenMeasure = { value: null, covered_total_tokens: '9007199254740993', complete: false };
  expect(validate(fixture)).toBe(true);
  expect(JSON.parse(JSON.stringify(fixture))).toEqual(fixture);
  expect(validate({ value: 9007199254740992, covered_total_tokens: '0', complete: true })).toBe(false);
  expect(validate({ covered_total_tokens: '0', complete: false })).toBe(false);
  expect(validate({ ...fixture, messages: ['chat'] })).toBe(false);
});

test('facet pages preserve null categories, exact counts, bounded queries and signed cursor shape', () => {
  const all = { kind: 'all' };
  const query = { filter: { range: { start_ms: 0, end_ms: 1000, timezone: 'UTC' }, sources: all, models: all, projects: all, sessions: all }, dimension: 'models', search: '中'.repeat(256), page_size: 200 };
  const validateRequest = ajv.compile(protocol.schemas.FilterOptionsRequest);
  expect(validateRequest({ query, cursor: null })).toBe(true);
  expect(validateRequest({ query: { ...query, page_size: 201 }, cursor: null })).toBe(false);
  expect(validateRequest({ query: { ...query, search: '中'.repeat(257) }, cursor: null })).toBe(false);
  expect(validateRequest({ query, cursor: 'a'.repeat(150) })).toBe(false);
  const page = { meta: { snapshot_id: 'synthetic-facet', data_revision: '7', price_revision: '3', generated_at_ms: 1000, parser_versions: [], accounting_versions: [], display_timezone: 'UTC' }, dimension: 'models', options: [{ key: null, display_name: '未知模型', count: '9007199254740993' }], next_cursor: null };
  const validatePage = ajv.compile(protocol.schemas.FilterOptionsPage);
  expect(validatePage(page)).toBe(true);
  expect(validatePage({ ...page, options: [{ ...page.options[0], count: 9007199254740992 }] })).toBe(false);
  expect(validatePage({ ...page, options: Array(201).fill(page.options[0]) })).toBe(false);
  expect(validatePage({ ...page, next_cursor: undefined })).toBe(false);
});
test('tagged DTO variants do not accept fields from other variants', () => {
  const validate = ajv.compile(protocol.schemas.MiniScope);
  const scope: MiniScope = { kind: 'today_all_sources' };
  expect(validate(scope)).toBe(true);
  expect(validate({ ...scope, session_key: 'unexpected' })).toBe(false);
  const integer = ajv.compile(protocol.schemas.DecimalInt);
  const big: DecimalInt = '170141183460469231731687303715884105727';
  expect(integer(big)).toBe(true);
  for (const invalid of ['01', '-1', '1e3', 0]) expect(integer(invalid)).toBe(false);
});

test('price DTOs keep exact atoms and distinguish an unpriced outcome from zero cost', () => {
  const outcome = ajv.compile(protocol.schemas.PriceOutcome);
  expect(outcome({ status: 'unpriced', reason: 'missing_rule' })).toBe(true);
  expect(outcome({ status: 'unpriced', reason: 'missing_rule', estimated_cost: '0' })).toBe(false);
  expect(outcome({ status: 'priced', rule_id: 'fixture', currency: 'USD', cost_atoms: '1', estimated_cost: '0.000000000000001' })).toBe(true);
  expect(outcome({ status: 'priced', rule_id: 'fixture', currency: 'USD', cost_atoms: 1, estimated_cost: 0.000000000000001 })).toBe(false);
  expect(outcome({ status: 'priced', rule_id: 'fixture', currency: 'USD', cost_atoms: '9007199254740993', estimated_cost: '9.007199254740993' })).toBe(true);
});

test('price writes cannot impersonate an offline rule or supply publication revisions', () => {
  const mutation = ajv.compile(protocol.schemas.PriceRuleMutation);
  const draft = { provider: 'fixture', model_exact: 'fixture', source_id: null, currency: 'USD', effective_from_ms: 0, effective_to_ms: null, priority: 0, input_rate_atoms: '1', cached_rate_atoms: null, output_rate_atoms: '2', origin_reference: null };
  expect(mutation({ kind: 'create', draft })).toBe(true);
  expect(mutation({ kind: 'create', draft: { ...draft, origin: 'offline' } })).toBe(false);
  expect(mutation({ kind: 'create', draft: { ...draft, introduced_revision: '12' } })).toBe(false);
  expect(mutation({ kind: 'create', draft: { ...draft, priority: -1 } })).toBe(false);
  expect(mutation({ kind: 'retire', rule_id: 'fixture', draft })).toBe(false);
});
