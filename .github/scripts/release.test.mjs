import assert from 'node:assert/strict';
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';
import { assetNames, checksums, eligible, publish, validateVersion, verifySourceTag } from './release.mjs';

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
  const directory = mkdtempSync(join(tmpdir(), 'markdown-note-release-test-'));
  t.after(() => {
    assert.equal(dirname(directory), resolve(tmpdir()));
    rmSync(directory, { recursive: true, force: true });
  });
  const version = '1.2.3';
  const names = [...assetNames(version), 'SHA256SUMS.txt'];
  writeFileSync(join(directory, names[0]), 'installer fixture');
  writeFileSync(join(directory, names[1]), 'ZIP fixture');
  writeFileSync(join(directory, names[2]), checksums(directory, version));
  const state = { release: undefined, calls: [], failUpload: false, corruptDownload: false, extraAsset: false,
    sourceRef: { ref: 'refs/tags/v1.2.3', object: { type: 'commit', sha: 'a'.repeat(40) } },
    tagObjects: new Map(), refError: null, afterCreate: null, afterDownload: null };
  const run = args => {
    state.calls.push(args);
    if (args[1] === 'repos/travisluong/markdown-note/git/ref/tags/v1.2.3') {
      if (state.refError) throw state.refError;
      return typeof state.sourceRef === 'string' ? state.sourceRef : JSON.stringify(state.sourceRef);
    }
    if (args[1]?.startsWith('repos/travisluong/markdown-note/git/tags/')) {
      return JSON.stringify(state.tagObjects.get(args[1].split('/').at(-1)));
    }
    if (args.includes('--slurp')) return JSON.stringify([[], state.release ? [state.release] : []]);
    if (args.includes('POST')) {
      assert.ok(args.includes('target_commitish=' + 'a'.repeat(40)));
      assert.ok(args.includes('draft=true'));
      state.release = { id: 42, tag_name: 'v1.2.3', draft: true, body: args.find(arg => arg.startsWith('body=')).slice(5) };
      state.afterCreate?.();
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
      state.afterDownload?.();
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
  assert.equal(state.calls.length, 2);
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
  assert.equal(state.calls.length, 2);
});


test('lightweight source tags resolve to the built commit', t => {
  const { state, options } = fixture(t);
  assert.equal(verifySourceTag(options), options.sha);
  assert.equal(state.calls.length, 1);
});

test('nested annotated source tags resolve to the built commit', t => {
  const { state, options } = fixture(t);
  const outer = 'b'.repeat(40);
  const inner = 'c'.repeat(40);
  state.sourceRef.object = { type: 'tag', sha: outer };
  state.tagObjects.set(outer, { sha: outer, object: { type: 'tag', sha: inner } });
  state.tagObjects.set(inner, { sha: inner, object: { type: 'commit', sha: options.sha } });
  publish(options);
  assert.equal(state.release.draft, false);
  assert.ok(state.calls.some(args => args[1]?.endsWith('/git/tags/' + inner)));
});

const invalidReferences = [
  ['wrong ref', { ref: 'refs/tags/v9.9.9', object: { type: 'commit', sha: 'a'.repeat(40) } }, /Invalid source tag reference/],
  ['null response', null, /Invalid source tag reference/],
  ['missing object', { ref: 'refs/tags/v1.2.3' }, /Invalid source tag object/],
  ['short SHA', { ref: 'refs/tags/v1.2.3', object: { type: 'commit', sha: 'abc123' } }, /Invalid source tag object/],
  ['non-string SHA', { ref: 'refs/tags/v1.2.3', object: { type: 'commit', sha: ['a'.repeat(40)] } }, /Invalid source tag object/],
  ['tree object', { ref: 'refs/tags/v1.2.3', object: { type: 'tree', sha: 'a'.repeat(40) } }, /must resolve to a commit/],
  ['wrong commit', { ref: 'refs/tags/v1.2.3', object: { type: 'commit', sha: 'd'.repeat(40) } }, /does not match the built commit/],
  ['malformed JSON', 'not-json', /JSON|Unexpected token/],
];
for (const [name, reference, message] of invalidReferences) {
  test(name + ' fails before release creation or upload', t => {
    const { state, options } = fixture(t);
    state.sourceRef = reference;
    assert.throws(() => publish(options), message);
    assert.equal(state.release, undefined);
    assert.equal(state.calls.some(args => args.includes('POST') || args[1] === 'upload'), false);
  });
}

test('missing remote tag is not created by publication', t => {
  const { state, options } = fixture(t);
  state.refError = new Error('Source tag not found (404)');
  assert.throws(() => publish(options), /not found/);
  assert.equal(state.calls.length, 1);
  assert.equal(state.release, undefined);
});

test('annotated tag cycles and invalid objects stop before publication', t => {
  const { state, options } = fixture(t);
  const tagSha = 'b'.repeat(40);
  state.sourceRef.object = { type: 'tag', sha: tagSha };
  state.tagObjects.set(tagSha, { sha: tagSha, object: { type: 'tag', sha: tagSha } });
  assert.throws(() => publish(options), /Cycle/);
  state.tagObjects.set(tagSha, { sha: 'c'.repeat(40), object: { type: 'commit', sha: options.sha } });
  assert.throws(() => publish(options), /Invalid annotated source tag/);
  state.tagObjects.set(tagSha, { sha: tagSha, object: null });
  assert.throws(() => publish(options), /Invalid source tag object/);
  assert.equal(state.release, undefined);
  assert.equal(state.calls.some(args => args.includes('POST') || args[1] === 'upload'), false);
});

test('annotated tag nesting is bounded', t => {
  const { state, options } = fixture(t);
  const tagSha = index => index.toString(16).padStart(40, '0');
  state.sourceRef.object = { type: 'tag', sha: tagSha(1) };
  for (let index = 1; index <= 33; index++) {
    state.tagObjects.set(tagSha(index), { sha: tagSha(index), object: { type: 'tag', sha: tagSha(index + 1) } });
  }
  assert.throws(() => publish(options), /nesting exceeds/);
  assert.equal(state.calls.length, 33);
  assert.equal(state.release, undefined);
});

test('published reruns still reject source tags pointing to another commit', t => {
  const { state, options } = fixture(t);
  state.release = { tag_name: options.tag, draft: false };
  state.sourceRef.object.sha = 'd'.repeat(40);
  assert.throws(() => publish(options), /does not match the built commit/);
  assert.equal(state.calls.length, 1);
  assert.equal(state.release.draft, false);
});

test('a tag moved after draft creation stops asset upload', t => {
  const { state, options } = fixture(t);
  state.afterCreate = () => { state.sourceRef.object.sha = 'd'.repeat(40); };
  assert.throws(() => publish(options), /does not match the built commit/);
  assert.equal(state.release.draft, true);
  assert.equal(state.calls.some(args => args[1] === 'upload' || args.includes('PATCH')), false);
});

test('a tag moved during asset verification leaves the release as a draft', t => {
  const { state, options } = fixture(t);
  state.afterDownload = () => { state.sourceRef.object.sha = 'd'.repeat(40); };
  assert.throws(() => publish(options), /does not match the built commit/);
  assert.equal(state.release.draft, true);
  assert.equal(state.calls.some(args => args.includes('PATCH')), false);
});
