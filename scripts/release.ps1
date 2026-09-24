<#
.SYNOPSIS
Build, verify, and publish a tagged Windows release from this machine.
.EXAMPLE
.\scripts\release.ps1 -Tag v0.1.1
.EXAMPLE
.\scripts\release.ps1 -Tag v0.1.1 -BuildOnly
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Tag,
    [switch]$BuildOnly,
    [string]$InnoSetup
)

$ErrorActionPreference = 'Stop'
$releaseArgs = @((Join-Path $PSScriptRoot 'release-local.mjs'), '--tag', $Tag)
if ($BuildOnly) { $releaseArgs += '--build-only' }
if ($InnoSetup) { $releaseArgs += @('--inno-setup', $InnoSetup) }
& node @releaseArgs
if ($LASTEXITCODE -ne 0) { throw 'Mithril release failed. See the error above; no successful publication is implied.' }
