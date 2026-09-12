# Windows installer

`mithril.iss` packages the Windows x86-64 executable with Inno Setup 6.3 or
newer. GitHub's `windows-2022` runner includes Inno Setup 6. The release workflow
compiles this script and publishes the installer, a versioned portable ZIP and
`SHA256SUMS.txt` to `travisluong/mithril-releases`. Only pushed stable tags such
as `v0.1.0` publish. A manual workflow run only uploads Actions artifacts.

## One-time GitHub setup

1. Create the public repository `travisluong/mithril-releases`, initialized with
   a README on `main`. This repository holds public Release assets; no application
   source or binaries are committed to its Git history. It needs no workflow.
2. In GitHub account Settings > Developer settings > Personal access tokens >
   Fine-grained tokens, create a token with resource owner `travisluong`, access
   only to `mithril-releases`, and repository **Contents: Read and write**.
   Choose an expiration and renew the secret before the token expires.
3. In the source `mithril` repository, open Settings > Secrets and variables >
   Actions and create the repository secret `MITHRIL_RELEASES_TOKEN` with that
   token. Never put its value in the repository, logs or release notes.

The built-in `GITHUB_TOKEN` stays read-only and is scoped to the source
repository. The separate credential is available only in the publishing step.
See GitHub's [token scope documentation](https://docs.github.com/en/actions/concepts/security/github_token)
and [release API permissions](https://docs.github.com/en/rest/releases/releases#create-a-release).

## Publish a version

1. Bump the package version in `Cargo.toml` and update its entry in `Cargo.lock`.
   Commit the version changes and release workflow, then push the source commit.
2. Tag that commit with its matching version and push that one tag:

   ```powershell
   git tag -a v0.1.0 -m "Mithril 0.1.0"
   git push origin v0.1.0
   ```

The workflow accepts exactly `vMAJOR.MINOR.PATCH` with no leading zeroes (except
zero itself). Prereleases, build metadata, unprefixed and malformed tags do not
build or publish automatically. A qualifying tag must match Cargo metadata or
the build fails. Pushing an existing tag again does not trigger a new release;
use Actions' rerun controls for a failed run.

The pinned Rust toolchain runs Windows headless tests, builds the x86-64 MSVC
release executable, then packages these assets:

- `mithril-<version>-windows-x86_64-setup.exe`
- `mithril-<version>-windows-x86_64.zip` containing `mithril.exe`
- `SHA256SUMS.txt` containing the SHA-256 hashes of both downloads

Publication creates a draft in the public repository with the source tag and
commit SHA recorded in its notes. Its tag targets the public repository's own
`main`, not the source commit. GitHub's automatic source archives therefore
contain the public repository's README, not Mithril source code.

The publisher uploads all three assets, downloads them again, and verifies the
exact filenames and bytes before publishing. An upload/verification failure
leaves a draft. Reruns replace assets only within a matching unpublished draft;
a draft with different source metadata fails for inspection. A published
release is left unchanged and the rerun succeeds. Runs for the same source ref
are serialized without cancelling an active run. Do not manually publish or
edit a draft while its workflow is running.

Use Actions > Build Windows release > Run workflow for a build without
publication, including when selecting a tag. Download its Actions artifact to
check the installer and ZIP. Release downloads are at
[mithril-releases](https://github.com/travisluong/mithril-releases/releases).
Other platforms retain their separate validation workflow but have no release
builds. Signing and automatic application updates remain separate work.

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
CI uses the explicit MSVC target's release directory instead, and writes to
`dist/`. All jobs keep the built-in token at `contents: read`. The publish step
uses `MITHRIL_RELEASES_TOKEN` to write to the destination repository. Release
metadata enters the script through environment variables and is passed to
GitHub CLI as argument arrays, without shell interpolation.

## Verification

Run `node --test .github/scripts/release.test.mjs` for tag/version validation,
draft creation/recovery, published-release preservation, and failed or corrupt
uploads. The tests mock GitHub CLI and never access GitHub. They also run in the
workflow's prepare job. Compile the actual installer script with ISCC and check
its exit status. Check workflow syntax with `actionlint .github/workflows/build.yml`.
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
