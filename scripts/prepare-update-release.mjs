// Local assets only. Signing keys, publishing and installed user data are outside this tool.
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const utf8 = bytes => new TextDecoder('utf-8', { fatal: true }).decode(bytes);
const repository = 'https://github.com/LiuYongXinn/token-pulse/releases/download';
const targets = { 'x86_64-pc-windows-msvc': ['windows-x86_64', 'x64'], 'aarch64-pc-windows-msvc': ['windows-aarch64', 'arm64'] };
function fail(message) { throw new Error(message); }
function boundedFile(path, limit) {
  if (!lstatSync(path).isFile() || lstatSync(path).size > limit) fail('Release input is not a bounded regular file');
  const bytes = readFileSync(path);
  if (!bytes.length || bytes.length > limit) fail('Release input is empty or too large');
  return bytes;
}
export function releaseVersion(repositoryRoot = root) {
  const npm = JSON.parse(readFileSync(join(repositoryRoot, 'package.json'), 'utf8')).version;
  const tauri = JSON.parse(readFileSync(join(repositoryRoot, 'src-tauri/tauri.conf.json'), 'utf8'));
  const cargo = readFileSync(join(repositoryRoot, 'Cargo.toml'), 'utf8').match(/\[workspace\.package\]([^\[]*)/)?.[1].match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (typeof npm !== 'string' || npm.length > 128 || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.test(npm) || npm.split('-').slice(1).join('-').split('.').some(part => /^0\d+$/.test(part)) || cargo !== npm || tauri.version !== npm || tauri.identifier !== 'com.tokenpulse.desktop') fail('Release versions or application identifier disagree');
  return npm;
}
export function manifestFromReport(report, { version, installer, signature, notes, publishedAt }) {
  const target = targets[report.target];
  if (report.schema !== 1 || report.version !== version || !target || report.installer_bytes !== installer.length.toString() || report.installer_sha256 !== sha(installer) || report.signature_sha256 !== sha(signature) || !/^[a-f0-9]{64}$/.test(report.public_key_sha256 ?? '')) fail('Native release verification report does not match these assets');
  if (typeof notes !== 'string' || Buffer.byteLength(notes, 'utf8') > 32768 || /\p{Cc}/u.test(notes.replace(/[\r\n\t]/g, ''))) fail('Release notes exceed the application contract');
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(publishedAt) || new Date(publishedAt).toISOString() !== publishedAt) fail('Publication date must be an explicit canonical UTC timestamp');
  const name = `TokenPulse_${version}_${target[1]}-setup.exe`;
  const encodedSignature = utf8(signature).trim();
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(encodedSignature) || encodedSignature.length > 16384) fail('Signature is not a bounded Tauri signature');
  return { name, manifest: { version, notes, pub_date: publishedAt, platforms: { [target[0]]: { signature: encodedSignature, url: `${repository}/v${version}/${name}` } } } };
}
export function prepare(options, runVerifier = (desktop, args) => spawnSync(desktop, args, { windowsHide: true, timeout: 30000, stdio: 'ignore' })) {
  const version = releaseVersion(options.repositoryRoot);
  const output = resolve(options.output);
  if (existsSync(output)) fail('Release destination already exists; refusing to replace it');
  const installer = boundedFile(options.installer, 512 * 1024 * 1024);
  const signature = boundedFile(options.signature, 16384);
  const desktopHash = sha(boundedFile(options.desktop, 512 * 1024 * 1024));
  const notes = options.notes ? utf8(boundedFile(options.notes, 131072)) : '';
  // All inputs are read before creating output. A sibling directory contains only this run's files.
  mkdirSync(dirname(output), { recursive: true });
  const stage = mkdtempSync(join(dirname(output), '.tokenpulse-release-'));
  const ownedStage = realpathSync(stage);
  try {
    const reportPath = join(stage, 'native-verification.json');
    const result = runVerifier(resolve(options.desktop), ['--verify-update-release', resolve(options.installer), resolve(options.signature), reportPath]);
    if (result.error || result.status !== 0) fail('Native verification failed: use a release build with the correct public key and a version-bound signature');
    const report = JSON.parse(utf8(boundedFile(reportPath, 4096)));
    if (sha(boundedFile(options.desktop, 512 * 1024 * 1024)) !== desktopHash) fail('Desktop verifier changed during preparation');
    const prepared = manifestFromReport(report, { version, installer, signature, notes, publishedAt: options.publishedAt });
    if (basename(options.installer) !== prepared.name) fail('Installer filename must match its version and target');
    writeFileSync(join(stage, prepared.name), installer, { flag: 'wx' });
    writeFileSync(join(stage, `${prepared.name}.sig`), signature, { flag: 'wx' });
    writeFileSync(join(stage, 'latest.json'), `${JSON.stringify(prepared.manifest, null, 2)}\n`, { flag: 'wx' });
    writeFileSync(join(stage, 'release-verification.json'), `${JSON.stringify({ ...report, desktop_sha256: desktopHash }, null, 2)}\n`, { flag: 'wx' });
    // Windows directory rename rejects a concurrently created destination, including an empty one.
    renameSync(stage, output);
    return { version, target: report.target, sha256: report.installer_sha256 };
  } finally {
    if (existsSync(stage)) {
      if (!lstatSync(stage).isDirectory() || lstatSync(stage).isSymbolicLink() || realpathSync(stage) !== ownedStage || dirname(resolve(stage)) !== dirname(output) || !basename(stage).startsWith('.tokenpulse-release-')) fail('Owned staging directory changed; cleanup refused');
      rmSync(stage, { recursive: true });
    }
  }
}
function parse(argv) {
  const names = { '--desktop': 'desktop', '--installer': 'installer', '--signature': 'signature', '--output': 'output', '--notes': 'notes', '--published-at': 'publishedAt' };
  const options = {};
  for (let i = 0; i < argv.length; i += 2) {
    const name = names[argv[i]];
    if (!name || !argv[i + 1] || options[name]) fail('Unknown, duplicate or incomplete release argument');
    options[name] = argv[i + 1];
  }
  if (['desktop', 'installer', 'signature', 'output', 'publishedAt'].some(key => !options[key])) fail('Required: --desktop --installer --signature --output --published-at; optional: --notes');
  return options;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.platform !== 'win32') fail('Release preparation requires Windows');
    console.log(`UPDATE_RELEASE_PREPARED: ${JSON.stringify(prepare(parse(process.argv.slice(2))))}`);
  } catch (error) {
    // Do not print native output, raw filesystem errors, signature text or environment values.
    console.error(error instanceof Error && !('code' in error) ? error.message : 'Release preparation could not read or write its own files');
    process.exitCode = 1;
  }
}
