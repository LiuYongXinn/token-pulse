import Ajv from 'ajv/dist/2020.js';
import { expect, test } from 'vitest';
import protocol from '../../../schemas/protocol-v1.json';
import type { TokenMeasure, MiniScope, DecimalInt, DashboardBundle } from './generated/contracts';

const ajv = new Ajv({ strict: false });

test('calendar selection contracts bound dates and require independent UTC ranges', () => {
  const validate = ajv.compile(protocol.schemas.CalendarSelectionRequest);
  const selection = { timezone: 'America/New_York', selection: { kind: 'custom', start_date: '2026-11-01', end_date_inclusive: '2026-11-01' } };
  expect(validate(selection)).toBe(true);
  expect(validate({ timezone: 'UTC', selection: { kind: 'today' } })).toBe(true);
  expect(validate({ ...selection, selection: { ...selection.selection, start_date: '2026-1-01' } })).toBe(false);
  expect(validate({ ...selection, selection: { kind: 'today', days: 1 } })).toBe(false);
  expect(validate({ ...selection, timezone: 'x'.repeat(129) })).toBe(false);
  const range = { start_ms: Date.parse('2026-11-01T04:00:00Z'), end_ms: Date.parse('2026-11-02T05:00:00Z'), timezone: 'America/New_York' };
  const result = { range, heatmap_range: { ...range, start_ms: 0 }, local_today: '2026-10-02' };
  const validateResult = ajv.compile(protocol.schemas.CalendarSelectionResult);
  expect(validateResult(result)).toBe(true);
  expect(validateResult({ ...result, heatmap_range: undefined })).toBe(false);
});

test('price invalidation carries exact revision and a whole-model marker without private data', () => {
  const validate = ajv.compile(protocol.schemas.PriceChanged);
  expect(validate({ price_revision: '9007199254740993', all_models: true })).toBe(true);
  expect(validate({ price_revision: '3' })).toBe(false);
  expect(validate({ price_revision: 3, all_models: true })).toBe(false);
  expect(validate({ price_revision: '3', all_models: true, root_path: 'private' })).toBe(false);
});

test('raw vectors retain signed diagnostic counters and null while rejecting unsafe numeric transport', () => {
  const validate = ajv.compile(protocol.schemas.RawUsageVector);
  const vector = { input_total: '-1', cached_input: null, output_total: '9007199254740993', reasoning_output: null, reported_total: null };
  expect(validate(vector)).toBe(true);
  expect(validate({ ...vector, output_total: 9007199254740992 })).toBe(false);
  expect(validate({ ...vector, input_total: '-0' })).toBe(false);
  expect(validate({ ...vector, input_total: '01' })).toBe(false);
  expect(validate({ ...vector, messages: ['synthetic private content'] })).toBe(false);
});

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
  const session = { session_key: 'synthetic', display_name: 'Synthetic', latest_at_ms: 1000, latest_model: null, latest_project_id: null, latest_project_name: null, parent_key: null, parent_display_name: null, parent_provider_id: null, child_count: '0', summary: totals, pricing: fixture.pricing, coverage, latest_context: { context_tokens: '9007199254740993', model_context_window: null, percentage: null, observed_at_ms: 12000, quality: 'confirmed' } };
  const sessionPage = { meta: fixture.meta, summary: totals, pricing: fixture.pricing, coverage, sessions: [session], next_cursor: null };
  const validateSessions = ajv.compile(protocol.schemas.SessionsPage);
  expect(validateSessions(sessionPage)).toBe(true);
  expect(validateSessions({ ...sessionPage, sessions: [{ ...session, latest_context: undefined }] })).toBe(false);
  expect(validateSessions({ ...sessionPage, sessions: [{ ...session, summary: { ...totals, reliable_turn_count: 1 } }] })).toBe(false);
  expect(validateSessions({ ...sessionPage, sessions: [{ ...session, messages: ['private chat'] }] })).toBe(false);
  expect(validateSessions({ ...sessionPage, sessions: Array(201).fill(session) })).toBe(false);
  const identity = { session_key: session.session_key, display_name: session.display_name, parent_key: null, parent_display_name: null, parent_provider_id: null };
  const detail = { meta: fixture.meta, identity, summary: totals, pricing: fixture.pricing, coverage, latest_selected_activity: null, latest_context: session.latest_context, child_count: '1', children: [identity], children_truncated: false, classifications: [{ kind: 'inherited', reason_code: 'inherited_prefix', observation_count: '9007199254740993' }] };
  const validateDetail = ajv.compile(protocol.schemas.SessionBundle);
  expect(validateDetail(detail)).toBe(true);
  expect(validateDetail({ ...detail, latest_selected_activity: undefined })).toBe(false);
  expect(validateDetail({ ...detail, children: Array(101).fill(identity) })).toBe(false);
  expect(validateDetail({ ...detail, classifications: Array(65).fill(detail.classifications[0]) })).toBe(false);
  expect(validateDetail({ ...detail, classifications: [{ ...detail.classifications[0], evidence_json: '{}' }] })).toBe(false);
  const turn = { turn_id: 'synthetic-turn', first_at_ms: 0, last_at_ms: 1000, summary: totals, pricing: fixture.pricing };
  const turnPage = { meta: fixture.meta, session_key: 'synthetic', summary: totals, pricing: fixture.pricing, coverage, unidentified_usage_event_count: '9007199254740993', turns: [turn], next_cursor: null };
  const validateTurns = ajv.compile(protocol.schemas.TurnsPage);
  expect(validateTurns(turnPage)).toBe(true);
  expect(validateTurns({ ...turnPage, unidentified_usage_event_count: undefined })).toBe(false);
  expect(validateTurns({ ...turnPage, turns: Array(201).fill(turn) })).toBe(false);
  expect(validateTurns({ ...turnPage, turns: [{ ...turn, messages: ['private'] }] })).toBe(false);
  const raw = { input_total: '100', cached_input: '60', output_total: '10', reasoning_output: '2', reported_total: '110' };
  const event = { event_id: 'synthetic-event', session_key: 'synthetic', session_display_name: 'Synthetic', occurred_at_ms: 1000, model: null, provider: null, project_id: null, project_display_name: null, source_ids: ['synthetic-source'], turn_id: null, total_tokens: '110', usage: raw, raw_last: { ...raw, input_total: '-1' }, raw_cumulative: null, calculation_method: 'synthetic', quality_flags: ['confirmed'], price: { status: 'unpriced', reason: 'unknown_model' }, parser_version: 'synthetic', accounting_version: 'synthetic' };
  const eventPage = { meta: fixture.meta, summary: totals, pricing: fixture.pricing, coverage, events: [event], next_cursor: null };
  const validateEvents = ajv.compile(protocol.schemas.UsageEventsPage);
  expect(validateEvents(eventPage)).toBe(true);
  expect(validateEvents({ ...eventPage, events: [{ ...event, raw_last: undefined }] })).toBe(false);
  expect(validateEvents({ ...eventPage, events: [{ ...event, source_ids: Array(33).fill('synthetic') }] })).toBe(false);
  expect(validateEvents({ ...eventPage, events: [{ ...event, quality_flags: Array(17).fill('synthetic') }] })).toBe(false);
  expect(validateEvents({ ...eventPage, events: [{ ...event, normalized_json: '{}' }] })).toBe(false);
  expect(validateEvents({ ...eventPage, events: Array(201).fill(event) })).toBe(false);
  const validateGroupRequest = ajv.compile(protocol.schemas.GroupedUsageRequest);
  const all = { kind: 'all' };
  const groupRequest = { filter: { range: { start_ms: 0, end_ms: 1000, timezone: 'UTC' }, sources: all, models: all, projects: all, sessions: all }, price_basis: { mode: 'event_time' }, dimension: 'models', sort: 'total_desc', limit: 200 };
  expect(validateGroupRequest(groupRequest)).toBe(true);
  expect(validateGroupRequest({ ...groupRequest, limit: 201 })).toBe(false);
  expect(validateGroupRequest({ ...groupRequest, limit: 0 })).toBe(false);
  const validateSessionRequest = ajv.compile(protocol.schemas.SessionsRequest);
  const sessionRequest = { query: { filter: groupRequest.filter, price_basis: groupRequest.price_basis, sort: 'latest_desc', page_size: 200 }, cursor: null };
  expect(validateSessionRequest(sessionRequest)).toBe(true);
  expect(validateSessionRequest({ ...sessionRequest, query: { ...sessionRequest.query, page_size: 201 } })).toBe(false);
  expect(validateSessionRequest({ ...sessionRequest, query: { ...sessionRequest.query, sort: 'unsupported' } })).toBe(false);
  expect(validateSessionRequest({ ...sessionRequest, cursor: 'a'.repeat(150) })).toBe(false);
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
  const validateClose = ajv.compile(protocol.schemas.CloseQuerySnapshotRequest);
  expect(validateClose({ kind: 'filter_options', request: { query, cursor: 'a'.repeat(151) } })).toBe(true);
  expect(validateClose({ kind: 'filter_options', request: { query, cursor: 'a'.repeat(151) }, snapshot_id: 'unauthorized' })).toBe(false);
  expect(validateClose({ kind: 'arbitrary', request: { query, cursor: 'a'.repeat(151) } })).toBe(false);
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
