import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { releaseLocal } from './release-local.mjs';

function fixture(t) {
  // Real Git and archive extraction establish that the build uses the tag,
  // including paths with spaces, without modifying the development checkout.
  const root = mkdtempSync(join(tmpdir(), 'markdown-note local release test '));
  t.after(() => {
    assert.equal(dirname(root), resolve(tmpdir()));
    assert.ok(basename(root).startsWith('markdown-note local release test '));
    rmSync(root, { recursive: true, force: true });
  });
  const git = args => execFileSync('git', args, { cwd: root, encoding: 'utf8', windowsHide: true }).trim();
  git(['init', '-q']);
  git(['config', 'core.autocrlf', 'false']);
  writeFileSync(join(root, 'Cargo.toml'), '[package]\nname = "markdown-note"\nversion = "1.2.3"\n');
  writeFileSync(join(root, 'committed.txt'), 'tagged content');
  git(['add', '.']);
  git(['-c', 'user.name=Release Test', '-c', 'user.email=release@example.invalid', 'commit', '-qm', 'fixture']);
  git(['tag', 'v1.2.3']);
  const sha = git(['rev-parse', 'HEAD']);
  writeFileSync(join(root, 'committed.txt'), 'uncommitted edit');
  writeFileSync(join(root, 'untracked.txt'), 'not part of the release');
  const compiler = join(root, 'ISCC.exe');
  writeFileSync(compiler, 'compiler fixture');
  const state = { calls: [], fail: null, release: null, uploaded: [] };
  const run = (name, args, options) => {
    state.calls.push({ name, args, ...options });
    if (name !== 'gh') {
      for (const key of ['GH_TOKEN', 'GITHUB_TOKEN', 'MARKDOWN_NOTE_RELEASES_TOKEN']) assert.equal(options.env[key], undefined);
    }
    if (state.fail === name || state.fail === `${name} ${args[0]}`) throw new Error('Injected command failure');
    if (name === 'git' || name === 'tar.exe') {
      return execFileSync(name, args, { cwd: options.cwd, env: options.env, encoding: 'utf8', windowsHide: true }).trim();
    }
    if (name === 'rustup') return '';
    if (name === 'cargo') {
      assert.equal(readFileSync(join(options.cwd, 'committed.txt'), 'utf8'), 'tagged content');
      assert.equal(existsSync(join(options.cwd, 'untracked.txt')), false);
      if (args[0] === 'metadata') {
        const version = readFileSync(join(options.cwd, 'Cargo.toml'), 'utf8').match(/version = "([^"]+)"/)[1];
        return JSON.stringify({ packages: [{ name: 'markdown-note', version }] });
      }
      if (args[0] === 'build') {
        assert.deepEqual(args, ['build', '--release', '--locked', '--target', 'x86_64-pc-windows-msvc', '--bin', 'markdown-note']);
        const output = join(options.env.CARGO_TARGET_DIR, 'x86_64-pc-windows-msvc', 'release');
        mkdirSync(output, { recursive: true });
        writeFileSync(join(output, 'markdown-note.exe'), 'compiled fixture');
      }
      return '';
    }
    if (name === compiler) {
      const dist = args.find(arg => arg.startsWith('/DOutputDir=')).slice('/DOutputDir='.length);
      writeFileSync(join(dist, 'markdown-note-1.2.3-windows-x86_64-setup.exe'), 'installer fixture');
      return '';
    }
    if (name === 'powershell.exe') {
      assert.ok(args.includes('-NonInteractive'));
      assert.equal(readFileSync(options.env.MARKDOWN_NOTE_PACKAGE_EXE, 'utf8'), 'compiled fixture');
      writeFileSync(options.env.MARKDOWN_NOTE_PACKAGE_ZIP, 'ZIP fixture');
      return '';
    }
    if (name === 'gh') {
      assert.equal(options.env.GH_TOKEN, 'fixture-publish-token');
      if (args[1] === 'repos/travisluong/markdown-note') return JSON.stringify({ private: false, permissions: { push: true } });
      if (args.includes('--slurp')) return JSON.stringify([state.release ? [state.release] : []]);
      if (args.includes('POST')) {
        state.release = { id: 42, draft: true, tag_name: 'v1.2.3', body: args.find(arg => arg.startsWith('body=')).slice(5) };
        return JSON.stringify(state.release);
      }
      if (args[1] === 'upload') {
        state.uploaded = args.slice(3, args.indexOf('--repo'));
        return '';
      }
      if (args[1] === 'download') {
        const download = args[args.indexOf('--dir') + 1];
        for (const path of state.uploaded) copyFileSync(path, join(download, basename(path)));
        return '';
      }
      if (args.includes('PATCH')) {
        state.release.draft = false;
        return '{}';
      }
      return JSON.stringify({ ...state.release, assets: state.uploaded.map(path => ({ name: basename(path) })) });
    }
    assert.fail(`Unexpected command ${name}`);
  };
  return { root, sha, state, git, options: {
    root, tag: 'v1.2.3', innoSetup: compiler, platform: 'win32', run, log: () => {},
    env: { ...process.env, MARKDOWN_NOTE_RELEASES_TOKEN: 'fixture-publish-token' },
  } };
}

test('tag snapshot builds, tests, packages, verifies and publishes; dirty checkout is untouched', t => {
  const { root, sha, state, git, options } = fixture(t);
  const before = git(['status', '--porcelain']);
  const result = releaseLocal(options);
  assert.equal(result.sha, sha);
  assert.equal(readFileSync(join(root, 'committed.txt'), 'utf8'), 'uncommitted edit');
  assert.equal(git(['status', '--porcelain']).replace(/^\?\? target\/\r?\n?/m, ''), before);
  assert.equal(state.release.draft, false);
  assert.ok(state.release.body.includes(sha));
  assert.equal(state.uploaded.length, 3);
  assert.ok(state.uploaded.every(path => dirname(path) === result.dist));
  const index = (name, first) => state.calls.findIndex(call => call.name === name && call.args[0] === first);
  assert.ok(index('cargo', 'test') < index('cargo', 'build'));
  assert.ok(index('cargo', 'build') < index('gh', 'release'));
  assert.equal(existsSync(join(root, 'target', 'local-releases', 'release.lock')), false);
  assert.deepEqual(JSON.parse(readFileSync(join(result.directory, 'release.json'), 'utf8')), {
    tag: 'v1.2.3', sha, version: '1.2.3', target: 'x86_64-pc-windows-msvc',
    installer: 'markdown-note-1.2.3-windows-x86_64-setup.exe', zip: 'markdown-note-1.2.3-windows-x86_64.zip',
  });
});

test('build-only packages without invoking GitHub', t => {
  const { state, options } = fixture(t);
  const result = releaseLocal({ ...options, buildOnly: true });
  assert.equal(state.calls.some(call => call.name === 'gh'), false);
  assert.ok(existsSync(join(result.dist, 'SHA256SUMS.txt')));
});

test('malformed tags fail before running tools', t => {
  const { state, options } = fixture(t);
  assert.throws(() => releaseLocal({ ...options, tag: 'v1.2.3-beta.1' }), /stable/);
  assert.equal(state.calls.length, 0);
});

test('tag/package mismatch fails before authentication, tests, or upload', t => {
  const { state, options, git } = fixture(t);
  git(['tag', 'v1.2.4']);
  assert.throws(() => releaseLocal({ ...options, tag: 'v1.2.4' }), /does not match/);
  assert.equal(state.calls.some(call => call.name === 'gh' || call.args[0] === 'test'), false);
});

for (const failed of ['git', 'tar.exe', 'gh', 'cargo test', 'cargo build', 'compiler', 'powershell.exe']) {
  test(`${failed} failure stops publication and releases the local lock`, t => {
    const { root, state, options } = fixture(t);
    state.fail = failed === 'compiler' ? options.innoSetup : failed;
    assert.throws(() => releaseLocal(options), /Injected command failure/);
    assert.equal(state.release, null);
    assert.equal(state.calls.some(call => call.name === 'gh' && call.args[1] === 'upload'), false);
    assert.equal(existsSync(join(root, 'target', 'local-releases', 'release.lock')), false);
  });
}

test('concurrent release is rejected without removing its lock', t => {
  const { root, state, options } = fixture(t);
  const directory = join(root, 'target', 'local-releases');
  mkdirSync(directory, { recursive: true });
  const lock = join(directory, 'release.lock');
  writeFileSync(lock, 'other process');
  assert.throws(() => releaseLocal(options), /Another release/);
  assert.equal(readFileSync(lock, 'utf8'), 'other process');
  assert.equal(state.calls.length, 0);
});

test('real Inno Setup and PowerShell package the snapshot without launching it', {
  skip: !process.env.MARKDOWN_NOTE_TEST_ISCC,
}, t => {
  const { root, git, options } = fixture(t);
  const repository = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  for (const path of ['packaging/windows/markdown-note.iss', 'assets/brand/markdown-note.ico']) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    copyFileSync(join(repository, path), join(root, path));
  }
  git(['add', 'packaging', 'assets']);
  git(['-c', 'user.name=Release Test', '-c', 'user.email=release@example.invalid', 'commit', '-qm', 'packaging fixture']);
  git(['tag', '-f', 'v1.2.3']);
  const compiler = resolve(process.env.MARKDOWN_NOTE_TEST_ISCC);
  const result = releaseLocal({ ...options, innoSetup: compiler, buildOnly: true,
    run: (name, args, context) => {
      if (name === compiler || name === 'powershell.exe') {
        return execFileSync(name, args, { cwd: context.cwd, env: context.env, encoding: 'utf8', windowsHide: true }).trim();
      }
      return options.run(name, args, context);
    },
  });
  const installer = readFileSync(join(result.dist, 'markdown-note-1.2.3-windows-x86_64-setup.exe'));
  assert.equal(installer.subarray(0, 2).toString(), 'MZ');
  const zipContents = execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
    "$ErrorActionPreference = 'Stop'; Add-Type -AssemblyName System.IO.Compression.FileSystem; $zip = [IO.Compression.ZipFile]::OpenRead($env:MARKDOWN_NOTE_TEST_ZIP); try { if ($zip.Entries.Count -ne 1 -or $zip.Entries[0].FullName -ne 'markdown-note.exe') { throw 'Unexpected ZIP entries' }; $reader = [IO.StreamReader]::new($zip.Entries[0].Open()); try { $reader.ReadToEnd() } finally { $reader.Dispose() } } finally { $zip.Dispose() }"],
  { encoding: 'utf8', windowsHide: true, env: { ...process.env, MARKDOWN_NOTE_TEST_ZIP: join(result.dist, 'markdown-note-1.2.3-windows-x86_64.zip') } });
  assert.equal(zipContents.trim(), 'compiled fixture');
});
