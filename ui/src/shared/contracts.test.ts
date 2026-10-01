import Ajv from 'ajv/dist/2020.js';
import { expect, test } from 'vitest';
import protocol from '../../../schemas/protocol-v1.json';
import type { TokenMeasure, MiniScope, DecimalInt } from './generated/contracts';

const ajv = new Ajv({ strict: false });
test('Rust schema and TS preserve nullable fields and exact decimal strings', () => {
  const validate = ajv.compile(protocol.schemas.TokenMeasure);
  const fixture: TokenMeasure = { value: null, covered_total_tokens: '9007199254740993', complete: false };
  expect(validate(fixture)).toBe(true);
  expect(JSON.parse(JSON.stringify(fixture))).toEqual(fixture);
  expect(validate({ value: 9007199254740992, covered_total_tokens: '0', complete: true })).toBe(false);
  expect(validate({ covered_total_tokens: '0', complete: false })).toBe(false);
  expect(validate({ ...fixture, messages: ['chat'] })).toBe(false);
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
