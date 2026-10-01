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
