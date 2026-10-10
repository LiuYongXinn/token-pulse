import { mkdirSync, realpathSync } from 'node:fs';
import { dirname, join, resolve, delimiter } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

export const root = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), '..'));
const local = join(root, '.local');
export const environment = {
  ...process.env,
  TOKENPULSE_PROJECT_ROOT: root,
  CARGO_HOME: join(local, 'tools/cargo'),
  RUSTUP_HOME: join(local, 'tools/rustup'),
  npm_config_cache: join(local, 'cache/npm'),
  PLAYWRIGHT_BROWSERS_PATH: join(local, 'cache/playwright'),
  TEMP: join(local, 'tmp'),
  TMP: join(local, 'tmp'),
  TMPDIR: join(local, 'tmp'),
  PYTHONPYCACHEPREFIX: join(local, 'cache/python'),
};
for (const key of ['CARGO_HOME', 'RUSTUP_HOME', 'npm_config_cache', 'PLAYWRIGHT_BROWSERS_PATH', 'TEMP', 'PYTHONPYCACHEPREFIX']) {
  mkdirSync(environment[key], { recursive: true });
}
environment.PATH = `${dirname(process.execPath)}${delimiter}${join(environment.CARGO_HOME, 'bin')}${delimiter}${process.env.PATH ?? ''}`;

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [program, ...args] = process.argv.slice(2);
  if (!program) throw new Error('Usage: node scripts/project-run.mjs <program> [arguments]');
  const packageEntrypoints = {
    vite: 'vite/bin/vite.js',
    tsc: 'typescript/bin/tsc',
    vitest: 'vitest/vitest.mjs',
    playwright: '@playwright/test/cli.js',
    tauri: '@tauri-apps/cli/tauri.js',
  };
  const entrypoint = packageEntrypoints[program];
  const child = spawn(entrypoint || program === 'node' ? process.execPath : program,
    entrypoint ? [join(root, 'node_modules', entrypoint), ...args] : args,
    { cwd: root, env: environment, stdio: 'inherit', windowsHide: true });
  child.on('error', error => { console.error(error.message); process.exitCode = 1; });
  child.on('exit', (code, signal) => { process.exitCode = code ?? (signal ? 1 : 0); });
}
