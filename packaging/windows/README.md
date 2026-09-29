# Windows packaging and releases

No Markdown Note binary release is available yet. This guide describes preparing
a future Windows x86-64 release after the source passes its required checks.
The public repository `travisluong/markdown-note` holds both application source
and release downloads.

`scripts/release.sh` tests, builds and packages a tagged source snapshot with Inno
Setup 6.3+. By default it also publishes the installer, portable ZIP and checksums.
Use `--build-only` to keep everything local. There are no GitHub Actions workflows,
and neither mode launches the app or installer.

## One-time setup

1. Install Git, Node.js 20+, Rust through rustup, Visual Studio C++ Build Tools
   with a Windows SDK, and Inno Setup 6.3+. Git, Node, rustup and Cargo must be on
   PATH. Windows supplies `tar.exe` and Windows PowerShell. The source snapshot's
   `rust-toolchain.toml` selects the required Rust version.
2. For publication, install GitHub CLI (`gh`) and authenticate with an account
   that can write to `travisluong/markdown-note`. The repository must be public
   and contain the source commit and tag being released.
3. Alternatively, supply a fine-grained token scoped to that repository with
   **Contents: Read and write** through `MARKDOWN_NOTE_RELEASES_TOKEN` or
   `GH_TOKEN`. The former takes precedence. Keep tokens out of source and logs.

The script looks for Inno Setup in standard machine and per-user locations.
For a custom installation, add `--inno-setup 'C:/path/to/ISCC.exe'`.

## Prepare a local build

Run these commands from the repository root in Git Bash. Commit completed source,
update the package version in `Cargo.toml` and its `Cargo.lock` entry, then create
a matching tag. For example, for version 0.1.1:

```bash
git tag -a v0.1.1 -m "Markdown Note 0.1.1"
./scripts/release.sh v0.1.1 --build-only
```

Build-only mode makes no GitHub requests and needs no publishing credentials.
Compilation may download the pinned toolchain or dependencies. Do not publish
until the required Rust checks and build complete successfully.

The tag supplies the application source and installer. The script archives that
commit under ignored `target/local-releases`, excluding uncommitted and untracked
files. It leaves the development checkout in place, runs headless tests, builds
the executable, compiles the installer and creates exactly:

- `markdown-note-<version>-windows-x86_64-setup.exe`
- `markdown-note-<version>-windows-x86_64.zip` containing `markdown-note.exe`
- `SHA256SUMS.txt` with SHA-256 hashes of both downloads

Each invocation prints its output directory. Its `dist` folder contains those
three assets; the adjacent source snapshot and `release.json` record stay local.
Build products are cached in `target/local-release-cache`. A per-repository lock
prevents concurrent releases from sharing that cache. If a process is forcibly
terminated, confirm it has stopped before removing
`target/local-releases/release.lock`. Retained run directories can be removed
when no longer needed.

The Bash wrapper preserves the command's exit status and works from other
working directories. Use `./scripts/release.sh --help` for its options. The
PowerShell entry point is `./scripts/release.ps1 -Tag v0.1.1 -BuildOnly`;
`node scripts/release-local.mjs --tag v0.1.1 --build-only` invokes the same
implementation directly.

## Publish a verified version

After verification, push the source commit and its matching tag to the public
source repository, then run the release command without `--build-only`:

```bash
git push origin main refs/tags/v0.1.1
./scripts/release.sh v0.1.1
```

The command does not create or push Git tags. Only stable `vMAJOR.MINOR.PATCH`
tags without leading zeroes are accepted, and the version must match Cargo
metadata. The remote tag must already exist and resolve to the exact locally
built source commit. Both lightweight and annotated tags are supported; missing,
mismatched, malformed or cyclic tag responses stop publication.

The remote source tag is checked before compilation, again before creating a
release, before uploading assets, and before publishing the verified draft.
Rechecks also apply to already-published releases. New releases target the source
commit SHA, so GitHub's automatic source archives contain the application source
for that version.

The publisher keeps a release as a draft while uploading all three assets,
downloading them again and checking filenames, checksums and bytes. Failures leave
the draft unpublished. A rerun replaces assets only within a matching draft;
different source metadata requires inspection. An already-published release is
left unchanged. Do not publish/edit the draft manually or run another publisher
from a different checkout or machine concurrently.

Future downloads will appear on the
[Markdown Note releases page](https://github.com/travisluong/markdown-note/releases).
Signing, automatic updates and binary releases for other platforms remain
separate work.

## Installation contract

- Per-user installation defaults to `%LOCALAPPDATA%\Programs\Markdown Note`,
  without elevation, file associations, PATH edits or machine-wide changes.
- The installer provides a Start menu shortcut, an optional desktop shortcut,
  and an Installed apps entry with version, publisher and uninstaller.
- Keep Markdown Note's `AppId` stable across future releases. It is independent
  of the previous application's identity, so installing Markdown Note does not
  replace or uninstall that application. Later Markdown Note releases reuse the
  existing install directory and uninstall entry.
- The payload is explicitly limited to `markdown-note.exe`. There are no
  wildcard deletion rules. Notebooks and `%APPDATA%\markdown-note` preferences
  are outside installer ownership and survive uninstall. Keep notes outside the
  installation directory.
- Save and close the app before upgrading or uninstalling. The installer does
  not close or restart the editor through Windows Restart Manager. Locked files
  may require a restart.
- Launch on completion is skipped for silent installs. Updates are installed by
  running a newer installer; older installers are not automatically blocked.
- The app and installer are unsigned. Publisher text is a display label, not a
  verified signing identity. Windows embeds the checked-in icon in the executable;
  the installer uses the same multi-resolution ICO.

## Headless verification

```bash
node --test .github/scripts/release.test.mjs scripts/release-local.test.mjs
```

Tests cover source tag verification, real Git snapshot isolation, credential
scoping, build/package failure handling, locking, draft recovery, published-release
preservation and corrupt uploads. Compilation and GitHub calls are mocked; these
tests do not publish or launch desktop UI.

To also compile the actual installer script and exercise PowerShell ZIP packaging
with a synthetic executable payload, point the optional test at an installed
Inno Setup compiler:

```bash
MARKDOWN_NOTE_TEST_ISCC='C:/Program Files (x86)/Inno Setup 6/ISCC.exe' \
  node --test scripts/release-local.test.mjs
```

Compilation verifies packaging syntax, not native installation behavior. Agents
must not install or launch the application for testing. Before distributing a
release, the maintainer should manually check install, upgrade, uninstall,
file-in-use handling, portable ZIP operation and silent installation in a clean
Windows account or VM. Confirm notebooks and preferences survive removal, future
upgrades reuse one Markdown Note entry, and a previous product installation is
left intact.
