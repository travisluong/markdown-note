# Windows installer

`scripts/release.sh` (Git Bash) or `scripts/release.ps1` (PowerShell) tests, builds and publishes a
Windows x86-64 release from your local machine. `mithril.iss` packages the
executable with Inno Setup 6.3 or
newer. GitHub hosts the installer, portable ZIP and checksums; GitHub Actions
is not involved in local publishing.

## One-time setup

1. Install Git, Node.js 20 or newer, Rust through rustup, Visual Studio C++ Build
   Tools with a Windows SDK, Inno Setup 6.3+, and GitHub CLI (`gh`). Git, Node,
   rustup, Cargo and gh must be on PATH. Windows supplies `tar.exe` and Windows
   PowerShell. The tagged `rust-toolchain.toml` selects the Rust version; rustup
   may download that toolchain and its Windows target on the first run.
2. Create the public repository `travisluong/mithril-releases`, initialized with
   a README on `main`. This repository holds public Release assets; no application
   source or binaries are committed to its Git history. It needs no workflow.
3. Authenticate locally with `gh auth login --hostname github.com` using an
   account with write access to the downloads repository. Alternatively, create
   a fine-grained token scoped only to `mithril-releases` with **Contents: Read
   and write**, and supply it through `MITHRIL_RELEASES_TOKEN` or `GH_TOKEN` in
   the release process environment. `MITHRIL_RELEASES_TOKEN` takes precedence.
   Keep tokens out of source files and logs; renew expiring tokens as needed.
The script looks for Inno Setup in the standard machine and per-user install
locations. For a portable/custom installation, append
`-InnoSetup 'C:\path\to\ISCC.exe'` in PowerShell, or
`--inno-setup 'C:/path/to/ISCC.exe'` in Git Bash.

## Publish a version

1. Bump the package version in `Cargo.toml` and update its entry in `Cargo.lock`.
   Commit the completed source and version changes.
2. Create a local tag on that commit, using its matching version, for example:

   ```powershell
   git tag -a v0.1.1 -m "Mithril 0.1.1"
   ```

3. Run the one-command release from the repository root:

   ```powershell
   .\scripts\release.ps1 -Tag v0.1.1
   ```

   Or, in Git Bash:

   ```bash
   ./scripts/release.sh v0.1.1
   ```

The Bash wrapper forwards remaining options to the same Node release command,
preserves its exit status, and finds the tooling relative to the script even
when invoked from another directory. Use `./scripts/release.sh --help` for usage. It
still builds on Windows and uses Windows PowerShell internally for ZIP packaging.

If your PowerShell policy blocks local scripts, the equivalent entry point is
`node scripts/release-local.mjs --tag v0.1.1` (append `--build-only` to avoid
publication). This does not change the machine's execution policy.

This command publishes once every build and verification step succeeds. It
does not create or push source tags/commits. Source pushes are a separate choice.
Only `vMAJOR.MINOR.PATCH` with no leading zeroes (except zero itself) is accepted;
prereleases and build metadata are rejected. The tag must exist locally and
match Cargo metadata. Run the command from the current checkout containing the
release tooling; the selected tag supplies the application source and installer.

The script archives the tagged commit into an isolated directory under ignored
`target/local-releases`, excluding uncommitted and untracked development files.
It never checks out, resets or cleans your working tree. It runs the pinned
toolchain's headless tests, builds the release executable, compiles the installer
and creates:

- `mithril-<version>-windows-x86_64-setup.exe`
- `mithril-<version>-windows-x86_64.zip` containing `mithril.exe`
- `SHA256SUMS.txt` containing the SHA-256 hashes of both downloads

Each invocation prints its unique output directory. Its `dist` folder contains
only those three public assets. The adjacent `source` snapshot and `release.json`
(source identity and public build key) stay local. Build products are cached in
`target/local-release-cache`. A per-repository lock prevents concurrent release
commands from sharing that cache. If the process is forcibly terminated, check
that no release/compiler process remains before removing
`target/local-releases/release.lock` and retrying. Old local run directories can
be removed when no longer needed. No app or installer is launched automatically.

Publication creates a draft in the public repository with the source tag and
commit SHA recorded in its notes. Its tag targets the public repository's own
`main`, not the source commit. GitHub's automatic source archives therefore
contain the public repository's README, not Mithril source code.

The publisher uploads all three assets, downloads them again, and verifies the
exact filenames and bytes before publishing. An upload/verification failure
leaves a draft. Reruns replace assets only within a matching unpublished draft;
a draft with different source metadata fails for inspection. A published
release is left unchanged and the rerun succeeds. Retry by running the same
command with the same tag. Do not manually publish/edit its draft or run another
publisher from a different checkout/machine concurrently.

For a full local build without any GitHub requests or uploads:

```powershell
.\scripts\release.ps1 -Tag v0.1.1 -BuildOnly
```

Or, in Git Bash:

```bash
./scripts/release.sh v0.1.1 --build-only
```

This still tests and packages the tagged source. BuildOnly does not require gh
or publishing credentials, but compilation may download toolchains/dependencies.

Builds and validation run locally; there are no GitHub Actions workflows.
The local release command never pushes tags.

Release downloads are at
[mithril-releases](https://github.com/travisluong/mithril-releases/releases).
Other platforms have no release builds. Signing and automatic application
updates remain separate work.

## Installation contract

- One per-user installation at `%LOCALAPPDATA%\Programs\Mithril` by default;
  no elevation, machine-wide registry changes, file associations or PATH edits.
- A Start menu shortcut, an optional unchecked desktop shortcut, and an Installed
  apps entry with version, publisher, support link and uninstaller.
- Keep `AppId` stable across releases. Inno Setup reuses the previous install
  directory and uninstall entry. `AppVersion` comes from `Cargo.toml` package
  metadata; bump it for a new release. Running an older installer is not blocked.
- The payload is explicitly limited to `mithril.exe`. There are no wildcard
  deletion rules. Notebooks and `%APPDATA%\mithril` preferences remain outside
  installer ownership, including on uninstall. Keep notes outside the install
  directory.
- Save and close Mithril before upgrading or uninstalling. The installer does
  not ask Windows Restart Manager to close or restart the editor. Windows may
  require a restart if files are still locked.
- Launch on completion is skipped during silent installation. Automatic updates
  are not implemented; users run a newer installer to upgrade.
- Both application and installer are unsigned. Publisher metadata is a display
  label, not a verified signing identity. Configure signing separately before
  advertising signed downloads. Shortcuts use the marketing logo embedded in
  the executable; the installer uses the same multi-resolution ICO.

## Build locally

Install [Inno Setup 6](https://jrsoftware.org/isdl.php), then run from the
repository root in PowerShell. No installer or application is launched by these
commands. Output stays under ignored `target/`.

```powershell
$ErrorActionPreference = 'Stop'
cargo build --release --locked --bin mithril
if ($LASTEXITCODE -ne 0) { throw 'Application build failed' }
$metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
$version = ($metadata.packages | Where-Object name -eq 'mithril').version
$compiler = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'
$sourceDir = (Resolve-Path target/release).Path
$outputDir = Join-Path (Get-Location) 'target/installer'
& $compiler "/DAppVersion=$version" "/DSourceDir=$sourceDir" "/DOutputDir=$outputDir" packaging/windows/mithril.iss
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
```

The result is `target/installer/mithril-<version>-windows-x86_64-setup.exe`.
The automated local command uses the explicit MSVC target, an isolated snapshot
and its own cache instead. Paths and release metadata are passed to tools as
argument arrays or environment variables without shell interpolation. Publishing
credentials are removed from the environment inherited by builds and tests.

## Verification

Run `node --test .github/scripts/release.test.mjs scripts/release-local.test.mjs`
for tag/version validation, real Git snapshot isolation, build/package failure
handling, credential scoping, local locking, draft recovery, published-release
preservation, and failed/corrupt uploads. Tests mock compilation and GitHub;
they never publish or launch desktop UI. Set `MITHRIL_TEST_ISCC` to an actual
ISCC.exe path to also exercise the real installer compiler and PowerShell ZIP
packaging with a synthetic executable payload. Compile the actual installer
script with ISCC and check its exit status.
Compilation establishes valid packaging, but not native
Windows behavior. Agents must not install the application or launch desktop UI
for these checks under the repository's testing rules.

Before distributing a release, manually check on a clean Windows account or VM:

1. Install as a standard user. Confirm no UAC request, the default directory,
   Start menu launch, Installed apps metadata and optional desktop shortcut.
2. Create a notebook outside the install directory and change a preference.
   Save and close the app, install a newer version, and confirm one Installed
   apps entry, the same install location and preserved notebook/preferences.
3. Exercise the file-in-use prompt with the app open. Cancel installation and
   confirm the running editor and unsaved note remain intact.
4. Close and uninstall. Confirm the executable, shortcuts and uninstall entry
   are removed, while notebook files and preferences remain.
5. Reinstall and confirm existing notebooks can still be opened. Check the
   portable ZIP separately. Check silent installation in the VM using
   `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART`; it must not launch the app.
