#!/usr/bin/env node
// Build, sign (Developer ID), notarize, staple, and publish the OpenWhisper
// macOS desktop app to a GitHub release.
//
// Usage:
//   npm run release
//   npm run release -- --no-upload
//   npm run release -- --version 0.2.0 --force-tag
//
// Credentials it expects on the machine (already configured here):
//   * A "Developer ID Application" identity in the login Keychain
//     (auto-detected; override with --signing-identity).
//   * A notarytool keychain profile (default: "beside"; override with
//     --notary-profile or APPLE_KEYCHAIN_PROFILE). Alternatively set Apple API
//     key env (APPLE_API_KEY/APPLE_API_KEY_ID/APPLE_API_ISSUER) or Apple ID env
//     (APPLE_ID/APPLE_APP_SPECIFIC_PASSWORD/APPLE_TEAM_ID).
//   * `gh` authenticated with repo scope for the target GitHub repo.

import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, mkdtemp, readFile, rm, stat, symlink } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const tauriDir = path.join(root, 'src-tauri');
const bundleDir = path.join(tauriDir, 'target', 'release', 'bundle');
const macosBundleDir = path.join(bundleDir, 'macos');
const dmgBundleDir = path.join(bundleDir, 'dmg');

const opts = parseArgs(process.argv.slice(2));
if (opts.help) {
  printHelp();
  process.exit(0);
}

if (process.platform !== 'darwin') {
  fail('Signed macOS releases must be built on macOS.');
}

const pkg = await readJson(path.join(root, 'package.json'));
const tauriConf = await readJson(path.join(tauriDir, 'tauri.conf.json'));
const productName = tauriConf.productName ?? 'OpenWhisper';
const version = opts.version ?? tauriConf.version ?? pkg.version;
const tag = opts.tag ?? `v${version}`;
const arch = opts.arch;
const tauriArch = arch === 'x64' ? 'x64' : 'aarch64';

assertVersionInputs({ pkg, tauriConf, version, tag, explicitVersion: opts.version });

const repo = opts.repo ?? (await inferRepo());
if (!repo) {
  fail('Could not infer GitHub repo. Pass --repo owner/name or add a git remote.');
}

await assertToolchain();
const signingIdentity = opts.signingIdentity ?? (await resolveDeveloperIdIdentity());
const notaryArgs = makeNotaryArgs(opts.notaryProfile);

if (!opts.skipGitCheck) {
  await assertGitReady(opts.allowNonMain);
}

const appPath = path.join(macosBundleDir, `${productName}.app`);
const dmgName = `${productName}_${version}_${tauriArch}.dmg`;
const dmgPath = path.join(dmgBundleDir, dmgName);
const zipName = `${productName}_${version}_${tauriArch}.app.zip`;
const zipPath = path.join(dmgBundleDir, zipName);

step(`Building ${productName} ${version} (${tauriArch}${opts.features ? `, features: ${opts.features}` : ''})`);
await rm(macosBundleDir, { recursive: true, force: true });
await rm(dmgBundleDir, { recursive: true, force: true });

const buildArgs = ['tauri', 'build'];
if (opts.features) buildArgs.push('--features', opts.features);
await run('npx', buildArgs, {
  cwd: root,
  env: { ...process.env, APPLE_SIGNING_IDENTITY: signingIdentity },
});

await assertArtifactsExist([appPath]);

step('Verifying Tauri-signed .app');
await run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', appPath]);

step('Notarizing and stapling the .app');
await notarizeAndStaple(appPath, notaryArgs);

step('Rebuilding DMG from the stapled .app');
await rebuildDmg({ appPath, dmgPath, productName, version, tauriArch, signingIdentity });

step('Notarizing and stapling the DMG');
await notarizeAndStaple(dmgPath, notaryArgs);

step('Zipping the stapled .app');
await rm(zipPath, { force: true });
await run('ditto', ['-c', '-k', '--keepParent', appPath, zipPath]);

step('Verifying signed + notarized artifacts');
await verifyReleaseArtifacts({ appPath, dmgPath });

const uploadFiles = [dmgPath, zipPath];
await assertArtifactsExist(uploadFiles);

if (opts.noUpload) {
  step('Skipping GitHub upload (--no-upload)');
  printLocalSummary(uploadFiles);
  process.exit(0);
}

step(`Publishing ${tag} to ${repo}`);
await prepareTag(tag, opts.forceTag);
await ensureGitHubRelease({ repo, tag, version, productName });
await run('gh', ['release', 'upload', tag, ...uploadFiles, '--repo', repo, '--clobber'], { cwd: root });

step('Verifying GitHub release assets');
const releaseUrl = await verifyRemoteAssets({ repo, tag, files: uploadFiles });
printLocalSummary(uploadFiles);
console.log(`\n[release] Done: ${releaseUrl}`);

// ---------------------------------------------------------------------------

function parseArgs(argv) {
  const out = {
    arch: 'arm64',
    features: 'metal',
    allowNonMain: false,
    forceTag: false,
    help: false,
    noUpload: false,
    repo: null,
    skipGitCheck: false,
    signingIdentity: null,
    tag: null,
    version: null,
    notaryProfile: process.env.APPLE_KEYCHAIN_PROFILE || 'beside',
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    switch (arg) {
      case '--':
        break;
      case '-h':
      case '--help':
        out.help = true;
        break;
      case '--allow-non-main':
        out.allowNonMain = true;
        break;
      case '--arch':
        out.arch = readValue(argv, ++i, arg);
        break;
      case '--features':
        out.features = readValue(argv, ++i, arg);
        break;
      case '--no-features':
        out.features = '';
        break;
      case '--force-tag':
        out.forceTag = true;
        break;
      case '--no-upload':
        out.noUpload = true;
        break;
      case '--notary-profile':
        out.notaryProfile = readValue(argv, ++i, arg);
        break;
      case '--repo':
        out.repo = readValue(argv, ++i, arg);
        break;
      case '--signing-identity':
        out.signingIdentity = readValue(argv, ++i, arg);
        break;
      case '--skip-git-check':
        out.skipGitCheck = true;
        break;
      case '--tag':
        out.tag = readValue(argv, ++i, arg);
        break;
      case '--version':
        out.version = readValue(argv, ++i, arg);
        break;
      default:
        fail(`Unknown option: ${arg}`);
    }
  }

  if (!['arm64', 'x64'].includes(out.arch)) {
    fail(`Unsupported --arch ${out.arch}. Use arm64 or x64.`);
  }
  return out;
}

function readValue(argv, index, flag) {
  const value = argv[index];
  if (value === undefined || value.startsWith('--')) {
    fail(`${flag} requires a value.`);
  }
  return value;
}

function printHelp() {
  console.log(`Build, sign, notarize, and publish the OpenWhisper macOS app.

Usage:
  npm run release
  npm run release -- --no-upload
  npm run release -- --version 0.2.0 --force-tag

Options:
  --arch arm64|x64          Build architecture. Default: arm64
  --features <list>         Cargo features for the build. Default: metal
  --no-features             Build with no extra Cargo features (plain CPU).
  --version 0.0.1           Require a specific package version.
  --tag v0.0.1              Release tag. Default: v<tauri.conf.json version>
  --repo owner/name         GitHub repo. Default: inferred from git remote.
  --signing-identity <id>   Developer ID identity. Default: auto-detected.
  --notary-profile <name>   notarytool keychain profile. Default: beside
  --force-tag               Move an existing tag to HEAD and force-push it.
  --no-upload               Build + verify locally, but skip tag/release upload.
  --allow-non-main          Permit releasing from a branch other than main.
  --skip-git-check          Skip clean-worktree and branch checks.
`);
}

async function readJson(file) {
  return JSON.parse(await readFile(file, 'utf8'));
}

function assertVersionInputs(input) {
  if (input.pkg.version !== input.tauriConf.version) {
    fail(`package.json version (${input.pkg.version}) must match tauri.conf.json version (${input.tauriConf.version}).`);
  }
  if (input.explicitVersion && input.explicitVersion !== input.tauriConf.version) {
    fail(`--version ${input.explicitVersion} does not match tauri.conf.json (${input.tauriConf.version}).`);
  }
  if (input.tag !== `v${input.version}`) {
    fail(`Release tag (${input.tag}) must match version ${input.version} (expected v${input.version}).`);
  }
}

async function inferRepo() {
  const result = await run('git', ['remote', 'get-url', 'origin'], {
    cwd: root,
    capture: true,
    quiet: true,
    check: false,
  });
  if (result.code !== 0) return null;
  const match = result.stdout.match(/github\.com[:/](.+?\/.+?)(?:\.git)?\s*$/);
  return match?.[1] ?? null;
}

function makeNotaryArgs(notaryProfile) {
  const env = process.env;
  const apiVars = ['APPLE_API_KEY', 'APPLE_API_KEY_ID', 'APPLE_API_ISSUER'];
  const idVars = ['APPLE_ID', 'APPLE_APP_SPECIFIC_PASSWORD', 'APPLE_TEAM_ID'];

  if (apiVars.some((n) => env[n])) {
    requireAll(env, apiVars);
    return ['--key', env.APPLE_API_KEY, '--key-id', env.APPLE_API_KEY_ID, '--issuer', env.APPLE_API_ISSUER];
  }
  if (idVars.some((n) => env[n])) {
    requireAll(env, idVars);
    return ['--apple-id', env.APPLE_ID, '--password', env.APPLE_APP_SPECIFIC_PASSWORD, '--team-id', env.APPLE_TEAM_ID];
  }
  const profile = env.APPLE_KEYCHAIN_PROFILE || notaryProfile;
  if (!profile) {
    fail('No notarization credentials found. Set --notary-profile, APPLE_KEYCHAIN_PROFILE, or Apple API/ID env vars.');
  }
  return ['--keychain-profile', profile];
}

function requireAll(env, names) {
  const missing = names.filter((n) => !env[n]);
  if (missing.length > 0) {
    fail(`Missing required notarization env var(s): ${missing.join(', ')}`);
  }
}

async function assertToolchain() {
  const tools = ['git', 'gh', 'npx', 'cargo', 'security', 'xcrun', 'hdiutil', 'codesign', 'spctl', 'ditto', 'du'];
  for (const tool of tools) {
    await run('/usr/bin/which', [tool], { capture: true, quiet: true });
  }
}

async function assertGitReady(allowNonMain) {
  const branch = (await run('git', ['branch', '--show-current'], { cwd: root, capture: true, quiet: true })).stdout.trim();
  if (branch !== 'main' && !allowNonMain) {
    fail(`Refusing to release from ${branch || 'detached HEAD'}. Use --allow-non-main to override.`);
  }
  const status = (await run('git', ['status', '--porcelain'], { cwd: root, capture: true, quiet: true })).stdout.trim();
  if (status) {
    fail(`Refusing to release with a dirty worktree:\n${status}\nCommit or stash changes, or use --skip-git-check.`);
  }
}

async function resolveDeveloperIdIdentity() {
  const result = await run('security', ['find-identity', '-v', '-p', 'codesigning'], { capture: true, quiet: true });
  const match = result.stdout.match(/"([^"]*Developer ID Application[^"]*)"/);
  if (!match) {
    fail('No "Developer ID Application" signing identity found in Keychain.');
  }
  return match[1];
}

async function assertArtifactsExist(files) {
  for (const file of files) {
    if (!existsSync(file)) {
      fail(`Expected artifact is missing: ${rel(file)}`);
    }
  }
}

async function notarizeAndStaple(target, notaryArgs) {
  const isApp = target.endsWith('.app');
  let submitTarget = target;
  let tmpDir = null;

  if (isApp) {
    tmpDir = await mkdtemp(path.join(os.tmpdir(), 'openwhisper-notarize.'));
    submitTarget = path.join(tmpDir, `${path.basename(target, '.app')}.zip`);
    await run('ditto', ['-c', '-k', '--keepParent', target, submitTarget]);
  }

  try {
    await run('xcrun', ['notarytool', 'submit', submitTarget, ...notaryArgs, '--wait', '--output-format', 'json']);
    await run('xcrun', ['stapler', 'staple', target]);
    await run('xcrun', ['stapler', 'validate', target]);
  } finally {
    if (tmpDir) await rm(tmpDir, { recursive: true, force: true });
  }
}

async function rebuildDmg(input) {
  const workDir = await mkdtemp(path.join(os.tmpdir(), 'openwhisper-dmg.'));
  const mount = path.join(workDir, 'mnt');
  const rwDmg = path.join(workDir, 'rw.dmg');
  let attached = false;

  try {
    await mkdir(mount, { recursive: true });
    await mkdir(path.dirname(input.dmgPath), { recursive: true });
    await rm(input.dmgPath, { force: true });

    const appSizeMb = await duMb(input.appPath);
    const imageSizeMb = Math.ceil(appSizeMb * 1.35 + 128);

    await run('hdiutil', [
      'create', '-size', `${imageSizeMb}m`, '-fs', 'HFS+',
      '-volname', `${input.productName} ${input.version}`, '-ov', rwDmg,
    ]);
    await run('hdiutil', ['attach', '-nobrowse', '-mountpoint', mount, rwDmg]);
    attached = true;
    await run('ditto', ['--rsrc', '--extattr', '--acl', input.appPath, path.join(mount, `${input.productName}.app`)]);
    await symlink('/Applications', path.join(mount, 'Applications'));
    await run('sync', []);
    await run('hdiutil', ['detach', mount]);
    attached = false;

    await run('hdiutil', ['convert', rwDmg, '-format', 'UDZO', '-imagekey', 'zlib-level=9', '-o', input.dmgPath]);
    await run('codesign', ['--force', '--sign', input.signingIdentity, '--timestamp', input.dmgPath]);
  } finally {
    if (attached) {
      await run('hdiutil', ['detach', mount, '-force'], { check: false, capture: true, quiet: true });
    }
    await rm(workDir, { recursive: true, force: true });
  }
}

async function duMb(file) {
  const result = await run('du', ['-sk', file], { capture: true, quiet: true });
  const kb = Number(result.stdout.trim().split(/\s+/)[0]);
  if (!Number.isFinite(kb) || kb <= 0) {
    fail(`Could not determine size for ${rel(file)}.`);
  }
  return kb / 1024;
}

async function verifyReleaseArtifacts({ appPath, dmgPath }) {
  await run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', appPath]);
  await run('xcrun', ['stapler', 'validate', appPath]);
  await run('xcrun', ['stapler', 'validate', dmgPath]);
  await run('spctl', ['-a', '-vvv', '-t', 'exec', appPath]);
  await run('spctl', ['-a', '-vvv', '-t', 'open', '--context', 'context:primary-signature', dmgPath]);
}

async function prepareTag(tagName, forceTag) {
  const head = (await run('git', ['rev-parse', 'HEAD'], { cwd: root, capture: true, quiet: true })).stdout.trim();
  const local = await gitCommitForRef(tagName);
  if (local && local !== head && !forceTag) {
    fail(`Local tag ${tagName} points to ${local}, not HEAD ${head}. Use --force-tag to move it.`);
  }
  if (!local) {
    await run('git', ['tag', tagName, head], { cwd: root });
  } else if (local !== head) {
    await run('git', ['tag', '-f', tagName, head], { cwd: root });
  }

  const remote = await gitRemoteTagCommit(tagName);
  if (remote && remote !== head && !forceTag) {
    fail(`Remote tag ${tagName} points to ${remote}, not HEAD ${head}. Use --force-tag to move it.`);
  }
  const pushArgs = remote && remote !== head
    ? ['push', '--force', 'origin', `refs/tags/${tagName}`]
    : ['push', 'origin', `refs/tags/${tagName}`];
  await run('git', pushArgs, { cwd: root });
}

async function gitCommitForRef(ref) {
  const result = await run('git', ['rev-parse', `${ref}^{commit}`], { cwd: root, capture: true, quiet: true, check: false });
  return result.code === 0 ? result.stdout.trim() : null;
}

async function gitRemoteTagCommit(tagName) {
  const result = await run('git', ['ls-remote', '--tags', 'origin', tagName], { cwd: root, capture: true, quiet: true });
  const lines = result.stdout.trim().split('\n').filter(Boolean);
  const peeled = lines.find((line) => line.endsWith(`refs/tags/${tagName}^{}`));
  const exact = lines.find((line) => line.endsWith(`refs/tags/${tagName}`));
  const selected = peeled ?? exact;
  return selected ? selected.split(/\s+/)[0] : null;
}

async function ensureGitHubRelease(input) {
  const view = await run('gh', ['release', 'view', input.tag, '--repo', input.repo, '--json', 'url'], {
    cwd: root, capture: true, quiet: true, check: false,
  });
  if (view.code === 0) return;

  await run('gh', [
    'release', 'create', input.tag,
    '--repo', input.repo,
    '--title', `${input.productName} ${input.version}`,
    '--notes', `${input.productName} ${input.version}\n\nSigned and notarized macOS build. Open the DMG and drag ${input.productName} to Applications.`,
    '--verify-tag',
  ], { cwd: root });
}

async function verifyRemoteAssets(input) {
  const result = await run('gh', ['release', 'view', input.tag, '--repo', input.repo, '--json', 'assets,url'], {
    cwd: root, capture: true, quiet: true,
  });
  const release = JSON.parse(result.stdout);
  const assetsByName = new Map(release.assets.map((a) => [a.name, a]));

  for (const file of input.files) {
    const asset = assetsByName.get(path.basename(file));
    if (!asset) {
      fail(`Uploaded asset missing from GitHub release: ${path.basename(file)}`);
    }
    const local = await fileInfo(file);
    if (asset.size !== local.size) {
      fail(`Remote size mismatch for ${asset.name}: ${asset.size} != ${local.size}`);
    }
    if (asset.digest && asset.digest !== `sha256:${local.digest}`) {
      fail(`Remote digest mismatch for ${asset.name}: ${asset.digest} != sha256:${local.digest}`);
    }
  }
  return release.url;
}

async function fileInfo(file) {
  const buffer = await readFile(file);
  return { size: buffer.length, digest: createHash('sha256').update(buffer).digest('hex') };
}

function printLocalSummary(files) {
  console.log('\n[release] Local artifacts:');
  for (const file of files) console.log(`  ${rel(file)}`);
}

async function run(command, args, options = {}) {
  const { capture = false, check = true, cwd = root, env = process.env, quiet = false } = options;
  if (!quiet) console.log(`[run] ${command} ${args.map(shellish).join(' ')}`);

  return await new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, env, stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit' });
    let stdout = '';
    let stderr = '';
    if (capture) {
      child.stdout.on('data', (chunk) => { stdout += chunk; if (!quiet) process.stdout.write(chunk); });
      child.stderr.on('data', (chunk) => { stderr += chunk; if (!quiet) process.stderr.write(chunk); });
    }
    child.on('error', reject);
    child.on('exit', (code) => {
      const result = { code, stdout, stderr };
      if (check && code !== 0) {
        const err = new Error(`${command} exited with code ${code}`);
        err.result = result;
        reject(err);
      } else {
        resolve(result);
      }
    });
  });
}

function shellish(value) {
  return /^[A-Za-z0-9_./:=@+-]+$/.test(value) ? value : JSON.stringify(value);
}

function step(message) {
  console.log(`\n==> ${message}`);
}

function rel(file) {
  return path.relative(root, file);
}

function fail(message) {
  console.error(`[release] ${message}`);
  process.exit(1);
}
