import assert from 'node:assert/strict';
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';
import { assetNames, checksums, eligible, publish, validateVersion } from './release.mjs';

test('only stable v-prefixed SemVer tags publish; manual builds remain eligible', () => {
  for (const tag of ['v0.0.0', 'v0.1.0', 'v1.2.3', 'v12.345.678']) {
    assert.equal(eligible('push', tag), true, tag);
  }
  for (const tag of ['1.2.3', 'v1.2.3-beta.1', 'v1.2.3+build', 'v01.2.3', 'v1.02.3', 'v1.2.03', 'v1.2', 'v1.2.3.4', 'vnext', 'main']) {
    assert.equal(eligible('push', tag), false, tag);
  }
  assert.equal(eligible('workflow_dispatch', 'main'), true);
  assert.equal(eligible('pull_request', 'v1.2.3'), false);
  assert.equal(eligible('release', 'v1.2.3'), false);
});

test('tag and Cargo version must match; manual builds use Cargo metadata', () => {
  assert.equal(validateVersion('push', 'v1.2.3', '1.2.3'), '1.2.3');
  assert.throws(() => validateVersion('push', 'v1.2.3', '1.2.4'), /does not match/);
  assert.throws(() => validateVersion('push', 'v1.2.3', undefined), /Missing/);
  assert.throws(() => validateVersion('push', 'v1.2.3-beta.1', '1.2.3-beta.1'), /stable/);
  assert.equal(validateVersion('workflow_dispatch', 'main', '1.2.3'), '1.2.3');
});

function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), 'mithril-release-test-'));
  t.after(() => {
    assert.equal(dirname(directory), resolve(tmpdir()));
    rmSync(directory, { recursive: true, force: true });
  });
  const version = '1.2.3';
  const names = [...assetNames(version), 'SHA256SUMS.txt'];
  writeFileSync(join(directory, names[0]), 'installer fixture');
  writeFileSync(join(directory, names[1]), 'ZIP fixture');
  writeFileSync(join(directory, names[2]), checksums(directory, version));
  const state = { release: undefined, calls: [], failUpload: false, corruptDownload: false, extraAsset: false };
  const run = args => {
    state.calls.push(args);
    if (args.includes('--slurp')) return JSON.stringify([[], state.release ? [state.release] : []]);
    if (args.includes('POST')) {
      assert.ok(args.includes('target_commitish=main'));
      assert.ok(args.includes('draft=true'));
      state.release = { id: 42, tag_name: 'v1.2.3', draft: true, body: args.find(arg => arg.startsWith('body=')).slice(5) };
      return JSON.stringify(state.release);
    }
    if (args[1] === 'upload') {
      if (state.failUpload) throw new Error('Upload interrupted');
      return '';
    }
    if (args[1] === 'download') {
      const downloaded = args[args.indexOf('--dir') + 1];
      for (const name of names) copyFileSync(join(directory, name), join(downloaded, name));
      if (state.corruptDownload) {
        writeFileSync(join(downloaded, names[0]), 'wrong bytes');
        // Even a self-consistent manifest must match the original build.
        writeFileSync(join(downloaded, names[2]), checksums(downloaded, version));
      }
      return '';
    }
    if (args.includes('PATCH')) {
      assert.ok(args.includes('draft=false'));
      state.release.draft = false;
      return JSON.stringify(state.release);
    }
    if (args[0] === 'api' && args[1].endsWith('/releases/42')) {
      return JSON.stringify({ ...state.release, assets: [...names, ...(state.extraAsset ? ['unexpected.txt'] : [])].map(name => ({ name })) });
    }
    assert.fail(`Unexpected gh call: ${JSON.stringify(args)}`);
  };
  return { directory, names, state, options: { directory, version, tag: 'v1.2.3', sha: 'a'.repeat(40), run } };
}

test('new release stays draft through upload and download verification', t => {
  const { state, options } = fixture(t);
  publish(options);
  assert.equal(state.release.draft, false);
  assert.ok(state.release.body.includes(options.sha));
  assert.ok(state.calls.findIndex(args => args[1] === 'download') < state.calls.findIndex(args => args.includes('PATCH')));
});

test('interrupted upload leaves a draft and a rerun resumes it', t => {
  const { state, options } = fixture(t);
  state.failUpload = true;
  assert.throws(() => publish(options), /Upload interrupted/);
  assert.equal(state.release.draft, true);
  state.failUpload = false;
  publish(options);
  assert.equal(state.release.draft, false);
  assert.equal(state.calls.filter(args => args.includes('POST')).length, 1);
});

test('published release rerun performs no mutations', t => {
  const { state, options } = fixture(t);
  state.release = { tag_name: options.tag, draft: false };
  publish(options);
  assert.equal(state.calls.length, 1);
});

test('download corruption prevents publication, even with recomputed checksums', t => {
  const { state, options } = fixture(t);
  state.corruptDownload = true;
  assert.throws(() => publish(options), /differs from the build/);
  assert.equal(state.release.draft, true);
  assert.equal(state.calls.some(args => args.includes('PATCH')), false);
});

test('unexpected remote files prevent publication', t => {
  const { state, options } = fixture(t);
  state.extraAsset = true;
  assert.throws(() => publish(options), /unexpected release assets/);
  assert.equal(state.release.draft, true);
});

test('invalid local assets fail before creating a release', t => {
  const { directory, names, state, options } = fixture(t);
  writeFileSync(join(directory, names[0]), 'changed bytes');
  assert.throws(() => publish(options), /checksums do not match/);
  assert.equal(state.release, undefined);
  assert.equal(readFileSync(join(directory, names[2]), 'utf8').split('\n').length, 3);
});

test('auth or network failures do not create releases', t => {
  const { options } = fixture(t);
  let calls = 0;
  assert.throws(() => publish({ ...options, run: () => { calls++; throw new Error('Forbidden'); } }), /Forbidden/);
  assert.equal(calls, 1);
});

test('a draft from another source commit is never overwritten', t => {
  const { state, options } = fixture(t);
  state.release = { tag_name: options.tag, draft: true, body: 'different source' };
  assert.throws(() => publish(options), /different source metadata/);
  assert.equal(state.calls.length, 1);
});
