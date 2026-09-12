# Windows installer

`mithril.iss` packages the Windows x86-64 executable with Inno Setup 6.3 or
newer. GitHub's `windows-2022` runner includes Inno Setup 6. The release workflow
compiles this script, retains the portable ZIP, and attaches the downloads to a
published GitHub Release. A manual workflow run only uploads Actions artifacts.

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
`dist/`. Only the publish job has `contents: write`; compilation jobs stay
read-only. Release tags enter the upload command through an environment variable.
Rerunning a release workflow replaces matching assets with `--clobber`.

## Verification

Compile the actual script with ISCC and check its exit status. Check workflow
syntax with actionlint. Compilation establishes valid packaging, but not native
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
