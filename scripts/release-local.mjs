import { execFileSync } from 'node:child_process';
import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, unlinkSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { assetNames, checksums, eligible, publish, validateVersion } from '../.github/scripts/release.mjs';

const repository = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const target = 'x86_64-pc-windows-msvc';
const destination = 'travisluong/mithril-releases';

function execute(command, args, { cwd, env, capture = false }) {
  return execFileSync(command, args, {
    cwd, env, encoding: 'utf8', windowsHide: true,
    stdio: capture ? ['ignore', 'pipe', 'inherit'] : ['ignore', 'inherit', 'inherit'],
    maxBuffer: 16 * 1024 * 1024,
  })?.trim();
}

function findCompiler(explicit, env) {
  const candidates = explicit ? [resolve(explicit)] : [
    env['ProgramFiles(x86)'] && join(env['ProgramFiles(x86)'], 'Inno Setup 6', 'ISCC.exe'),
    env.ProgramFiles && join(env.ProgramFiles, 'Inno Setup 6', 'ISCC.exe'),
    env.LOCALAPPDATA && join(env.LOCALAPPDATA, 'Programs', 'Inno Setup 6', 'ISCC.exe'),
  ];
  const compiler = candidates.find(path => path && existsSync(path));
  if (!compiler) throw new Error('Install Inno Setup 6.3+ or pass -InnoSetup <path to ISCC.exe>.');
  return compiler;
}

// This function also supports an injected command runner for headless integration
// tests. The real runner never uses a shell to interpolate paths or credentials.
export function releaseLocal({
  tag, buildOnly = false, innoSetup, root = repository,
  env = process.env, platform = process.platform, run = execute, log = console.log,
}) {
  if (platform !== 'win32') throw new Error('Run the Windows release on Windows.');
  if (!eligible('push', tag)) throw new Error('Expected a stable vMAJOR.MINOR.PATCH tag.');
  const compiler = findCompiler(innoSetup, env);
  const buildEnv = { ...env,
    CARGO_TARGET_DIR: join(root, 'target', 'local-release-cache') };
  // A public-download credential is never inherited by the compiler or tests.
  for (const name of ['GH_TOKEN', 'GITHUB_TOKEN', 'MITHRIL_RELEASES_TOKEN']) delete buildEnv[name];
  const runs = join(root, 'target', 'local-releases');
  mkdirSync(runs, { recursive: true });
  const lockPath = join(runs, 'release.lock');
  let lock;
  try {
    lock = openSync(lockPath, 'wx');
  } catch (error) {
    if (error.code !== 'EEXIST') throw error;
    throw new Error(`Another release may be running. If it has stopped, remove ${lockPath} and retry.`);
  }
  try {
    writeFileSync(lock, `pid=${process.pid}\ntag=${tag}\n`);
    const command = (name, args, cwd = root, capture = false) => run(name, args, { cwd, env: buildEnv, capture });
    const sha = command('git', ['rev-parse', '--verify', `refs/tags/${tag}^{commit}`], root, true);
    if (!/^[0-9a-f]{40}$/.test(sha)) throw new Error('Could not resolve the source tag to a full commit SHA.');
    const directory = mkdtempSync(join(runs, `${tag}-`));
    const source = join(directory, 'source');
    const dist = join(directory, 'dist');
    mkdirSync(source);
    mkdirSync(dist);
    log(`Building ${tag} from ${sha}\nRelease directory: ${directory}`);
    // Archive only committed files. No checkout, reset, clean, tag, or push is
    // performed in the user's development tree. Retain the snapshot for diagnosis.
    const archive = join(directory, 'source.tar');
    command('git', ['archive', '--format=tar', `--output=${archive}`, sha]);
    command('tar.exe', ['-xf', archive, '-C', source]);
    unlinkSync(archive);
    command('rustup', ['show', 'active-toolchain'], source);
    const metadata = JSON.parse(command('cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1'], source, true));
    const version = validateVersion('push', tag, metadata.packages.find(item => item.name === 'mithril')?.version);
    const publishEnv = { ...env };
    if (env.MITHRIL_RELEASES_TOKEN) publishEnv.GH_TOKEN = env.MITHRIL_RELEASES_TOKEN;
    const github = args => run('gh', args, { cwd: root, env: publishEnv, capture: true });
    if (!buildOnly) {
      // Fail before the expensive build if CLI authentication/repo access is missing.
      const repo = JSON.parse(github(['api', `repos/${destination}`]));
      if (repo.private !== false) throw new Error('The downloads repository must be public.');
      if (!repo.permissions?.push) throw new Error(`GitHub credentials need Contents read/write access to ${destination}.`);
    }
    command('rustup', ['target', 'add', target], source);
    command('cargo', ['test', '--locked', '--target', target, '--features', 'editor-diagnostics'], source);
    command('cargo', ['build', '--release', '--locked', '--target', target, '--bin', 'mithril'], source);
    const executable = join(buildEnv.CARGO_TARGET_DIR, target, 'release', 'mithril.exe');
    const [installer, zip] = assetNames(version);
    command(compiler, [`/DAppVersion=${version}`, `/DSourceDir=${dirname(executable)}`, `/DOutputDir=${dist}`,
      join(source, 'packaging', 'windows', 'mithril.iss')], source);
    // Use the built-in .NET ZIP API: Compress-Archive's script module may be
    // unavailable under Windows PowerShell's execution policy/module search path.
    const zipCommand = [
      "$ErrorActionPreference = 'Stop'",
      'Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem',
      '$archive = [IO.Compression.ZipFile]::Open($env:MITHRIL_PACKAGE_ZIP, [IO.Compression.ZipArchiveMode]::Create)',
      "try { [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $env:MITHRIL_PACKAGE_EXE, 'mithril.exe', [IO.Compression.CompressionLevel]::Optimal) | Out-Null } finally { $archive.Dispose() }",
    ].join('; ');
    run('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', zipCommand],
    { cwd: source, env: { ...buildEnv, MITHRIL_PACKAGE_EXE: executable, MITHRIL_PACKAGE_ZIP: join(dist, zip) }, capture: false });
    writeFileSync(join(dist, 'SHA256SUMS.txt'), checksums(dist, version));
    writeFileSync(join(directory, 'release.json'), JSON.stringify({ tag, sha, version, target,
      installer, zip }, null, 2) + '\n');
    if (buildOnly) {
      log(`Build complete; nothing uploaded. Downloads: ${dist}`);
    } else {
      publish({ directory: dist, version, tag, sha, run: github });
      log(`Downloads: ${dist}`);
    }
    return { directory, dist, sha, version };
  } finally {
    closeSync(lock);
    unlinkSync(lockPath);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const { values } = parseArgs({ options: {
      tag: { type: 'string' }, 'build-only': { type: 'boolean' }, 'inno-setup': { type: 'string' },
    } });
    releaseLocal({ tag: values.tag, buildOnly: values['build-only'], innoSetup: values['inno-setup'] });
  } catch (error) {
    console.error(`Release failed: ${error.message}`);
    process.exitCode = 1;
  }
}
