import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, readdirSync, rmSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { releaseVersion, manifestFromReport, prepare } from './prepare-update-release.mjs';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const fixture = fn => {
  const directory = mkdtempSync(join(tmpdir(), 'tokenpulse-release-test-'));
  const owned = realpathSync(directory);
  try {
    mkdirSync(join(directory, 'src-tauri'));
    writeFileSync(join(directory, 'package.json'), JSON.stringify({ version: '0.1.0' }));
    writeFileSync(join(directory, 'Cargo.toml'), '[workspace.package]\nversion = "0.1.0"\n');
    writeFileSync(join(directory, 'src-tauri/tauri.conf.json'), JSON.stringify({ version: '0.1.0', identifier: 'com.tokenpulse.desktop' }));
    return fn(directory);
  } finally {
    assert.equal(realpathSync(directory), owned); assert.ok(directory.startsWith(join(tmpdir(), 'tokenpulse-release-test-')));
    rmSync(directory, { recursive: true });
  }
};
function assets(directory) {
  const installer = Buffer.from('explicit synthetic signed payload; never executed');
  const signature = Buffer.from('c3ludGhldGlj\n');
  const options = { repositoryRoot: directory, desktop: join(directory, 'desktop.exe'), installer: join(directory, 'TokenPulse_0.1.0_x64-setup.exe'), signature: join(directory, 'installer.sig'), output: join(directory, 'release'), publishedAt: '2026-10-03T00:00:00.000Z' };
  writeFileSync(options.installer, installer); writeFileSync(options.signature, signature); writeFileSync(options.desktop, 'synthetic verifier placeholder');
  const report = { schema: 1, version: '0.1.0', target: 'x86_64-pc-windows-msvc', installer_bytes: installer.length.toString(), installer_sha256: sha(installer), signature_sha256: sha(signature), public_key_sha256: 'a'.repeat(64) };
  const verifier = (_desktop, args) => { assert.equal(args[0], '--verify-update-release'); writeFileSync(args[3], JSON.stringify(report), { flag: 'wx' }); return { status: 0 }; };
  return { installer, signature, report, options, verifier };
}
test('manifest uses a fixed repository, exact target/version, explicit date and signature', () => fixture(directory => {
  const { installer, signature, report } = assets(directory);
  const value = manifestFromReport(report, { version: '0.1.0', installer, signature, notes: '<b>literal release notes</b>', publishedAt: '2026-10-03T00:00:00.000Z' });
  assert.equal(value.name, 'TokenPulse_0.1.0_x64-setup.exe');
  assert.deepEqual(value.manifest, { version: '0.1.0', notes: '<b>literal release notes</b>', pub_date: '2026-10-03T00:00:00.000Z', platforms: { 'windows-x86_64': { signature: 'c3ludGhldGlj', url: 'https://github.com/LiuYongXinn/token-pulse/releases/download/v0.1.0/TokenPulse_0.1.0_x64-setup.exe' } } });
}));
test('workspace/package/config versions and production identifier must agree', () => fixture(directory => {
  assert.equal(releaseVersion(directory), '0.1.0');
  writeFileSync(join(directory, 'package.json'), JSON.stringify({ version: '0.2.0' }));
  assert.throws(() => releaseVersion(directory), /disagree/);
}));
test('altered hashes, version, target, unknown byte count or oversized Unicode notes reject', () => fixture(directory => {
  const { installer, signature, report } = assets(directory);
  const input = { version: '0.1.0', installer, signature, notes: '', publishedAt: '2026-10-03T00:00:00.000Z' };
  for (const change of [{ version: '0.2.0' }, { target: 'linux-x86_64' }, { installer_bytes: null }, { installer_sha256: 'b'.repeat(64) }, { signature_sha256: 'b'.repeat(64) }, { public_key_sha256: null }]) assert.throws(() => manifestFromReport({ ...report, ...change }, input), /does not match/);
  assert.throws(() => manifestFromReport(report, { ...input, notes: '额'.repeat(11000) }), /contract/);
  assert.throws(() => manifestFromReport(report, { ...input, notes: '\u0000' }), /contract/);
  assert.throws(() => manifestFromReport(report, { ...input, publishedAt: '2026-10-03' }), /timestamp/);
}));
test('successful preparation copies verified bytes and refuses to overwrite output', () => fixture(directory => {
  const { installer, signature, options, verifier } = assets(directory);
  assert.deepEqual(prepare(options, verifier), { version: '0.1.0', target: 'x86_64-pc-windows-msvc', sha256: sha(installer) });
  assert.deepEqual(readFileSync(join(options.output, 'TokenPulse_0.1.0_x64-setup.exe')), installer);
  assert.deepEqual(readFileSync(join(options.output, 'TokenPulse_0.1.0_x64-setup.exe.sig')), signature);
  assert.equal(JSON.parse(readFileSync(join(options.output, 'latest.json'))).version, '0.1.0');
  assert.throws(() => prepare(options, verifier), /already exists/);
  assert.equal(readdirSync(directory).filter(name => name.startsWith('.tokenpulse-release-')).length, 0);
}));
test('native failure and changed inputs leave no publication or staging output', () => fixture(directory => {
  const { options, verifier } = assets(directory);
  assert.throws(() => prepare(options, () => ({ status: 14 })), /Native verification failed/);
  assert.ok(!existsSync(options.output));
  assert.throws(() => prepare(options, (...args) => { const result = verifier(...args); writeFileSync(options.desktop, 'changed'); return result; }), /changed/);
  assert.ok(!existsSync(options.output));
  assert.equal(readdirSync(directory).filter(name => name.startsWith('.tokenpulse-release-')).length, 0);
}));
test('concurrently created output remains untouched on Windows', { skip: process.platform !== 'win32' }, () => fixture(directory => {
  const { options, verifier } = assets(directory);
  assert.throws(() => prepare(options, (...args) => { const result = verifier(...args); mkdirSync(options.output); return result; }));
  assert.deepEqual(readdirSync(options.output), []);
  assert.equal(readdirSync(directory).filter(name => name.startsWith('.tokenpulse-release-')).length, 0);
}));
