import { readFileSync, readdirSync, mkdirSync, writeFileSync, renameSync, existsSync } from 'node:fs';
import { resolve, dirname, join, relative, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const sha = data => createHash('sha256').update(data).digest('hex');
export function readLicense(file, label) {
  const bytes = readFileSync(file);
  if (!bytes.length || bytes.length > 1024 * 1024) throw new Error(`Invalid license size: ${label}`);
  const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  return { name: label, text, sha256: sha(bytes) };
}
export function licenseFiles(directory) {
  const found = [];
  function visit(path, depth) {
    if (depth > 12) throw new Error('License tree exceeds supported depth');
    for (const entry of readdirSync(path, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name, 'en'))) {
      if (['.git', 'target', 'node_modules'].includes(entry.name)) continue;
      const file = join(path, entry.name);
      if (entry.isSymbolicLink()) { if (/^(license|licence|copying|notice|copyright|authors)/i.test(entry.name)) throw new Error('License symlink is unsupported'); continue; }
      if (entry.isDirectory()) visit(file, depth + 1);
      else if (/^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT|AUTHORS)([._-]|$)/i.test(entry.name)) found.push(readLicense(file, relative(directory, file).replaceAll('\\', '/')));
    }
  }
  visit(directory, 0);
  return found;
}
const fallbackIds = {
  'alloc-stdlib@0.3.0': ['alloc-stdlib-0.3.0'],
  'defmt-parser@1.0.0': ['defmt-parser-1.0.0-mit', 'defmt-parser-1.0.0-apache'],
  'selectors@0.38.0': ['selectors-0.38.0-mpl'],
  'tauri-plugin@2.7.1': ['tauri-plugin-2.7.1-mit'],
  'ts-rs@12.0.1': ['ts-rs-12.0.1-mit'],
  'ts-rs-macros@12.0.1': ['ts-rs-12.0.1-mit'],
  'webview2-com@0.39.1': ['webview2-com-0.39.1-mit'],
  'webview2-com-macros@0.8.1': ['webview2-com-macros-0.8.1-mit'],
  'webview2-com-sys@0.39.1': ['webview2-com-0.39.1-mit'],
};
export function pinnedLicense(id, cache) {
  const entry = JSON.parse(readFileSync(join(cache, 'sources.json'), 'utf8')).find(item => item.id === id);
  if (!entry || isAbsolute(entry.file) || entry.file.includes('..') || /[\\/]/.test(entry.file)) throw new Error(`Invalid pinned license: ${id}`);
  const value = readLicense(join(cache, entry.file), entry.file);
  if (value.sha256 !== entry.sha256) throw new Error(`Pinned license changed: ${id}`);
  return { ...value, source: entry.source };
}
export function cargoEntries(metadata, cache) {
  const ids = new Set(metadata.resolve.nodes.map(node => node.id));
  return metadata.packages.filter(pkg => pkg.source && ids.has(pkg.id)).map(pkg => {
    if (!pkg.source.startsWith('registry+') || !pkg.license || !/^[\w.+-]+$/.test(pkg.name) || !/^[\w.+-]+$/.test(pkg.version)) throw new Error('Unreviewed dependency source or license');
    let licenses = licenseFiles(dirname(pkg.manifest_path));
    if (!licenses.length) licenses = (fallbackIds[`${pkg.name}@${pkg.version}`] ?? []).map(id => pinnedLicense(id, cache));
    if (!licenses.length) throw new Error(`Full license text missing: ${pkg.name}@${pkg.version}`);
    return { kind: 'Rust Windows dependency (includes build/test)', name: pkg.name, version: pkg.version, license: pkg.license, source: `https://crates.io/api/v1/crates/${pkg.name}/${pkg.version}/download`, licenses };
  });
}
export function npmEntries(lock, repository) {
  return Object.entries(lock.packages).filter(([path, pkg]) => path && !pkg.dev && !pkg.devOptional && path.startsWith('node_modules/')).map(([path, pkg]) => {
    const directory = join(repository, path);
    const installed = JSON.parse(readFileSync(join(directory, 'package.json'), 'utf8'));
    if (installed.version !== pkg.version || !installed.license || !/^[\w.+-]+$/.test(pkg.version)) throw new Error('Installed frontend dependency does not match lock');
    const licenses = licenseFiles(directory);
    if (!licenses.length) throw new Error(`Full license text missing: ${installed.name}@${pkg.version}`);
    return { kind: 'Frontend runtime dependency', name: installed.name, version: pkg.version, license: installed.license, source: `https://www.npmjs.com/package/${installed.name}/v/${pkg.version}`, licenses };
  });
}
export function render(entries, target) {
  const blocks = [`TokenPulse third-party notices\nTarget: ${target}\n\nThis inventory includes the resolved Windows Rust dependency graph (including build/test dependencies), frontend runtime packages, and installer components. Listing does not imply every package is linked into the binary. Full upstream license/notice texts are reproduced below; metadata license expressions retain the upstream alternatives. Sources for unchanged Rust packages, including MPL-covered files, are available at the exact-version source URLs. No dependency sources are modified by this notice generator. Microsoft WebView2 is an external runtime obtained from Microsoft and governed by its own terms: https://developer.microsoft.com/microsoft-edge/webview2/\n`];
  for (const entry of entries) {
    blocks.push(`\n${'='.repeat(78)}\n${entry.kind}: ${entry.name} ${entry.version}\nLicense metadata: ${entry.license}\nSource: ${entry.source}\n`);
    for (const license of entry.licenses) blocks.push(`\n--- ${license.name} ---\nSHA-256: ${license.sha256}\n${license.source ? `Text source: ${license.source}\n` : ''}\n${license.text}\n`);
  }
  return blocks.join('');
}
function command(program, args) {
  const result = spawnSync(program, args, { cwd: root, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  if (result.status !== 0) throw new Error(`Dependency inventory command failed: ${program}`);
  return result.stdout;
}
export function generate(target) {
  if (!['x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc'].includes(target)) throw new Error('Unsupported notice target');
  const cache = join(root, 'scripts/third-party');
  const metadata = JSON.parse(command('cargo', ['metadata', '--locked', '--offline', '--format-version', '1', '--filter-platform', target]));
  const entries = [...cargoEntries(metadata, cache), ...npmEntries(JSON.parse(readFileSync(join(root, 'package-lock.json'), 'utf8')), root)];
  const nsis = join(root, 'target/.tauri/NSIS/makensis.exe');
  // The pinned CLI bootstraps these tools after beforeBuildCommand on first build.
  // Verify existing caches, but do not require a prior installer build.
  const cli = JSON.parse(readFileSync(join(root, 'node_modules/@tauri-apps/cli/package.json'), 'utf8'));
  if (cli.version !== '2.12.1') throw new Error('Bundler version requires notice review');
  if (existsSync(nsis) && command(nsis, ['/VERSION']).trim() !== 'v3.11') throw new Error('NSIS version requires notice review');
  const helper = join(dirname(nsis), 'Plugins/x86-unicode/additional/nsis_tauri_utils.dll');
  if (existsSync(helper) && sha(readFileSync(helper)) !== '5ba143b5db4a87d32d6e7802e033330aae56cbceabe0d1e3ba41948385ad4709') throw new Error('Installer helper requires notice review');
  entries.push({ kind: 'Installer runtime', name: 'NSIS', version: '3.11', license: 'Zlib; compression exceptions in COPYING', source: 'https://github.com/kichik/nsis/tree/v311', licenses: [pinnedLicense('nsis-3.11-copying', cache)] });
  // libsqlite3-sys bundles the unmodified amalgamation. Include the actual source notice.
  const sqlite = metadata.packages.find(pkg => pkg.name === 'libsqlite3-sys');
  const source = readFileSync(join(dirname(sqlite.manifest_path), 'sqlite3/sqlite3.c'), 'utf8');
  const beginning = source.indexOf('** The author disclaims copyright');
  const end = source.indexOf('*************************************************************************', beginning);
  if (beginning < 0 || end < beginning) throw new Error('Bundled SQLite source notice changed');
  const text = source.slice(beginning, end);
  entries.push({ kind: 'Bundled native library', name: 'SQLite', version: source.match(/version (\d+\.\d+\.\d+)\./)?.[1] ?? 'unknown', license: 'Public domain source dedication', source: `https://crates.io/api/v1/crates/libsqlite3-sys/${sqlite.version}/download`, licenses: [{ name: 'sqlite3.c source notice', text, sha256: sha(Buffer.from(text)) }] });
  entries.push({ kind: 'Installer helper', name: 'nsis-tauri-utils', version: '0.5.3', license: 'MIT branch of Apache-2.0 OR MIT', source: 'https://github.com/tauri-apps/nsis-tauri-utils/tree/13d9edd27b69310e108d6fbd49f90992f8a05390', licenses: [pinnedLicense('nsis-tauri-utils-0.5.3-mit', cache)] });
  entries.sort((a, b) => `${a.kind}/${a.name}/${a.version}`.localeCompare(`${b.kind}/${b.name}/${b.version}`, 'en'));
  return { text: render(entries, target), entries };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const target = process.argv[process.argv.indexOf('--target') + 1];
  if (!process.argv.includes('--target')) throw new Error('--target is required');
  const { text, entries } = generate(target);
  const output = join(root, 'src-tauri/resources/third-party-notices.txt');
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(`${output}.tmp`, text, 'utf8'); renameSync(`${output}.tmp`, output);
  console.log(`THIRD_PARTY_NOTICES_READY: ${entries.length} components; ${Buffer.byteLength(text)} bytes; ${sha(Buffer.from(text))}`);
}
