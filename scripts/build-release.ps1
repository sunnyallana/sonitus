#Requires -Version 5.1
<#
.SYNOPSIS
    Build a Sonitus release bundle for Windows desktop.

.DESCRIPTION
    Runs `dx bundle --platform desktop --release` and reports where the
    .msi / .exe ended up. Use run-dev.ps1 first to verify the env.
#>

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $RepoRoot

Write-Host "==> Building Sonitus release for Windows..." -ForegroundColor Cyan
& dx bundle --package sonitus-ui --platform desktop --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$dist = Join-Path $RepoRoot 'dist'
if (Test-Path $dist) {
    Write-Host ""
    Write-Host "==> Output:" -ForegroundColor Cyan
    Get-ChildItem -Path $dist -Recurse -Include *.msi, *.exe | ForEach-Object {
        Write-Host "    $($_.FullName)"
    }
} else {
    Write-Host "    No dist/ directory found; check dx output above."
}
