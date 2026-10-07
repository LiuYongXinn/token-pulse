import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const runner = join(root, 'scripts/project-run.mjs');
test('children started from another directory receive only project-local storage paths', () => {
  const result = spawnSync(process.execPath, [runner, 'node', '-e', `console.log(JSON.stringify({cwd:process.cwd(),temp:process.env.TEMP,cargo:process.env.CARGO_HOME,rustup:process.env.RUSTUP_HOME,browsers:process.env.PLAYWRIGHT_BROWSERS_PATH,npm:process.env.npm_config_cache}))`], { cwd: dirname(root), encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  const paths = JSON.parse(result.stdout);
  assert.equal(paths.cwd, root);
  for (const [key, path] of Object.entries(paths)) {
    if (key !== 'cwd') assert.ok(path.startsWith(join(root, '.local')), `${key} escaped project: ${path}`);
  }
});
test('child arguments stay literal and child failures propagate', () => {
  const literal = 'a & echo ignored; $(not-a-command)';
  const result = spawnSync(process.execPath, [runner, 'node', '-e', 'console.log(process.argv[1]);process.exitCode=23', literal], { encoding: 'utf8' });
  assert.equal(result.stdout.trim(), literal);
  assert.equal(result.status, 23);
});
