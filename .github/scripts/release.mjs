import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { appendFileSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const destination = 'travisluong/markdown-note';
const stableTag = /^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/;

export function eligible(event, ref) {
  return event === 'workflow_dispatch' || (event === 'push' && stableTag.test(ref));
}

export function validateVersion(event, ref, version) {
  if (!version) throw new Error('Missing markdown-note package version');
  if (event === 'workflow_dispatch') return version;
  if (!eligible(event, ref)) throw new Error('Expected a stable vMAJOR.MINOR.PATCH tag');
  if (ref.slice(1) !== version) {
    throw new Error(`Tag ${ref} does not match Cargo package version ${version}`);
  }
  return version;
}

export function assetNames(version) {
  // Cargo supplies the version; validate again before using it in paths.
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error('Invalid package version');
  }
  return [`markdown-note-${version}-windows-x86_64-setup.exe`, `markdown-note-${version}-windows-x86_64.zip`];
}

function digest(path) {
  const bytes = readFileSync(path);
  if (!bytes.length) throw new Error(`Empty release asset: ${path}`);
  return createHash('sha256').update(bytes).digest('hex');
}

export function checksums(directory, version) {
  return assetNames(version).map(name => `${digest(join(directory, name))}  ${name}\n`).join('');
}

function verifyAssets(directory, version) {
  const expected = [...assetNames(version), 'SHA256SUMS.txt'].sort();
  if (JSON.stringify(readdirSync(directory).sort()) !== JSON.stringify(expected)) {
    throw new Error('Expected exactly the Windows installer, ZIP and SHA256SUMS.txt');
  }
  if (readFileSync(join(directory, 'SHA256SUMS.txt'), 'utf8') !== checksums(directory, version)) {
    throw new Error('Release asset checksums do not match');
  }
}

function gh(args) {
  return execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] });
}

export function publish({ directory, version, tag, sha, run = gh }) {
  validateVersion('push', tag, version);
  if (!/^[0-9a-f]{40}$/.test(sha)) throw new Error('Expected the full source commit SHA');
  const api = `repos/${destination}`;
  // Listing includes drafts for the authenticated owner. Do not mistake an auth
  // or network failure for an absent release and create a duplicate.
  const pages = JSON.parse(run(['api', '--paginate', '--slurp', `${api}/releases?per_page=100`]));
  let release = pages.flat().find(item => item.tag_name === tag);
  if (release && !release.draft) {
    console.log(`${tag} is already published; its assets are unchanged.`);
    return;
  }
  verifyAssets(directory, version);
  const notes = `Windows x86-64 downloads for Markdown Note ${version}.\n\nSource tag: ${tag}\nSource commit: ${sha}\n\nDownload the setup executable to install Markdown Note, or extract the portable ZIP. SHA256SUMS.txt contains download checksums.\n`;
  if (release && release.body !== notes) {
    throw new Error('Existing draft has different source metadata; inspect it before retrying');
  }
  if (!release) {
    release = JSON.parse(run(['api', '--method', 'POST', `${api}/releases`,
      '-f', `tag_name=${tag}`, '-f', 'target_commitish=main',
      '-f', `name=Markdown Note ${version}`, '-f', `body=${notes}`,
      '-F', 'draft=true', '-F', 'prerelease=false']));
  }
  // Clobber is confined to our unpublished draft so interrupted uploads resume.
  // Download and compare every asset before making the release public.
  const names = [...assetNames(version), 'SHA256SUMS.txt'];
  run(['release', 'upload', tag, ...names.map(name => join(directory, name)),
    '--repo', destination, '--clobber']);
  const uploaded = JSON.parse(run(['api', `${api}/releases/${release.id}`]));
  if (!uploaded.draft) throw new Error('Release was published externally during upload');
  if (JSON.stringify(uploaded.assets.map(asset => asset.name).sort()) !== JSON.stringify([...names].sort())) {
    throw new Error('Draft contains missing or unexpected release assets');
  }
  const downloaded = mkdtempSync(join(tmpdir(), 'markdown-note-release-'));
  try {
    run(['release', 'download', tag, '--repo', destination, '--dir', downloaded]);
    verifyAssets(downloaded, version);
    for (const name of names) {
      if (digest(join(downloaded, name)) !== digest(join(directory, name))) {
        throw new Error(`Uploaded asset differs from the build: ${name}`);
      }
    }
    run(['api', '--method', 'PATCH', `${api}/releases/${release.id}`, '-F', 'draft=false']);
  } finally {
    if (dirname(downloaded) !== resolve(tmpdir())) throw new Error('Unexpected temporary download path');
    rmSync(downloaded, { recursive: true, force: true });
  }
  console.log(`Published https://github.com/${destination}/releases/tag/${tag}`);
}

function main(command) {
  const event = process.env.GITHUB_EVENT_NAME;
  const ref = process.env.GITHUB_REF_NAME;
  const output = value => appendFileSync(process.env.GITHUB_OUTPUT, `${value}\n`);
  switch (command) {
    case 'eligibility':
      output(`build=${eligible(event, ref)}`);
      break;
    case 'version': {
      const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1'], { encoding: 'utf8' }));
      const version = validateVersion(event, ref, metadata.packages.find(item => item.name === 'markdown-note')?.version);
      output(`version=${version}`);
      break;
    }
    case 'checksums':
      writeFileSync('dist/SHA256SUMS.txt', checksums('dist', process.env.APP_VERSION));
      verifyAssets('dist', process.env.APP_VERSION);
      break;
    case 'publish':
      if (!process.env.GH_TOKEN) throw new Error('Set the source repository Actions secret MARKDOWN_NOTE_RELEASES_TOKEN');
      publish({ directory: resolve('dist'), version: process.env.APP_VERSION, tag: ref, sha: process.env.GITHUB_SHA });
      break;
    default:
      throw new Error('Expected eligibility, version, checksums or publish');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv[2]);
}
