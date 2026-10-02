import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { cargoEntries, npmEntries, pinnedLicense, render } from './generate-third-party-notices.mjs';

function fixture(run) {
  const directory = mkdtempSync(join(tmpdir(), 'tokenpulse-notice-test-'));
  try { run(directory); } finally { assert.equal(resolve(directory).startsWith(resolve(tmpdir())), true); rmSync(directory, { recursive: true, force: true }); }
}
test('filtered Rust graph includes exact licenses and nested vendor notices, omits other platforms', () => fixture(directory => {
  mkdirSync(join(directory, 'vendor')); writeFileSync(join(directory, 'LICENSE'), 'Synthetic license and copyright.'); writeFileSync(join(directory, 'vendor/NOTICE.txt'), 'Synthetic vendored attribution.');
  const packageInfo = { id: 'resolved', name: 'synthetic', version: '1.2.3', source: 'registry+https://example.invalid', license: 'MIT', manifest_path: join(directory, 'Cargo.toml') };
  const entries = cargoEntries({ packages: [packageInfo, { ...packageInfo, id: 'unresolved', name: 'other-platform' }], resolve: { nodes: [{ id: 'resolved' }] } }, directory);
  assert.equal(entries.length, 1); assert.equal(entries[0].version, '1.2.3'); assert.equal(entries[0].licenses.length, 2); assert.equal(entries[0].source, 'https://crates.io/api/v1/crates/synthetic/1.2.3/download');
  const text = render(entries, 'synthetic-test-target'); assert.match(text, /Synthetic vendored attribution/); assert.doesNotMatch(text, new RegExp(directory.replaceAll('\\', '\\\\')));
}));
test('new dependency without full license text stops preparation instead of using metadata alone', () => fixture(directory => {
  assert.throws(() => cargoEntries({ packages: [{ id: 'one', name: 'new-dependency', version: '2.0.0', license: 'MIT', source: 'registry+https://example.invalid', manifest_path: join(directory, 'Cargo.toml') }], resolve: { nodes: [{ id: 'one' }] } }, directory), /Full license text missing/);
}));
test('pinned license must match its digest and remain inside cache', () => fixture(directory => {
  const text = 'Synthetic pinned copyright and license'; writeFileSync(join(directory, 'license.txt'), text);
  const entry = { id: 'test', file: 'license.txt', source: 'https://example.invalid/exact-commit/LICENSE', sha256: createHash('sha256').update(text).digest('hex') };
  writeFileSync(join(directory, 'sources.json'), JSON.stringify([entry])); assert.equal(pinnedLicense('test', directory).text, text);
  writeFileSync(join(directory, 'license.txt'), 'Changed text'); assert.throws(() => pinnedLicense('test', directory), /changed/);
  writeFileSync(join(directory, 'sources.json'), JSON.stringify([{ ...entry, file: '../outside.txt' }])); assert.throws(() => pinnedLicense('test', directory), /Invalid pinned/);
}));
test('frontend inventory follows runtime lock and rejects installed version drift', () => fixture(directory => {
  const packagePath = 'node_modules/synthetic'; mkdirSync(join(directory, packagePath), { recursive: true }); writeFileSync(join(directory, packagePath, 'LICENSE'), 'Synthetic frontend copyright.'); writeFileSync(join(directory, packagePath, 'package.json'), JSON.stringify({ name: 'synthetic', version: '1.0.0', license: 'MIT' }));
  const lock = { packages: { '': {}, [packagePath]: { version: '1.0.0' }, 'node_modules/dev-tool': { version: '1.0.0', dev: true } } };
  assert.equal(npmEntries(lock, directory).length, 1); lock.packages[packagePath].version = '2.0.0'; assert.throws(() => npmEntries(lock, directory), /does not match lock/);
}));
test('all checked-in upstream snapshots are intact', () => {
  const cache = resolve('scripts/third-party'); const entries = JSON.parse(readFileSync(join(cache, 'sources.json'), 'utf8'));
  assert.equal(entries.length, 10); for (const entry of entries) { const value = pinnedLicense(entry.id, cache); assert.ok(value.text.length > 1000); assert.match(value.source, /^https:\/\//); }
});
