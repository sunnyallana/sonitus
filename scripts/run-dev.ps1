#Requires -Version 5.1
<#
.SYNOPSIS
    Start Sonitus in development mode (desktop, hot-reload).

.DESCRIPTION
    Verifies prerequisites (rustup, the pinned toolchain, the dx CLI),
    installs missing pieces, then runs `dx serve --platform desktop`.

.PARAMETER Release
    Build with --release. Slower compile, much faster runtime audio path.

.PARAMETER Platform
    Override the dx target. Default: desktop. Other options: web, ios, android.

.PARAMETER SkipChecks
    Skip the prerequisite checks (use if you've already verified everything).

.EXAMPLE
    .\scripts\run-dev.ps1

.EXAMPLE
    .\scripts\run-dev.ps1 -Release

.EXAMPLE
    .\scripts\run-dev.ps1 -Platform web
#>

[CmdletBinding()]
param(
    [switch]$Release,
    [ValidateSet('desktop', 'web', 'ios', 'android')]
    [string]$Platform = 'desktop',
    [switch]$SkipChecks
)

$ErrorActionPreference = 'Stop'

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Write-Ok {
    param([string]$Message)
    Write-Host "    OK  $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host "    !!  $Message" -ForegroundColor Yellow
}

function Test-Command {
    param([string]$Name)
    $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

# Resolve repo root (parent of this script's directory) so the script
# works regardless of which directory the user invokes it from.
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $RepoRoot
Write-Host "Sonitus dev launcher" -ForegroundColor Magenta
Write-Host "Repo root: $RepoRoot"

if (-not $SkipChecks) {
    # --- 1. rustup -----------------------------------------------------
    Write-Step "Checking rustup"
    if (-not (Test-Command 'rustup')) {
        Write-Warn "rustup not found on PATH."
        Write-Host "    Install it from https://rustup.rs and re-run this script."
        Write-Host "    (winget install Rustlang.Rustup)"
        exit 1
    }
    Write-Ok (rustup --version)

    # --- 2. Pinned toolchain -------------------------------------------
    Write-Step "Checking pinned Rust toolchain"
    $toolchainFile = Join-Path $RepoRoot 'rust-toolchain.toml'
    if (Test-Path $toolchainFile) {
        $channelLine = Select-String -Path $toolchainFile -Pattern '^channel\s*=\s*"([^"]+)"' | Select-Object -First 1
        if ($channelLine) {
            $channel = $channelLine.Matches[0].Groups[1].Value
            Write-Host "    rust-toolchain.toml channel: $channel"
            $installed = & rustup toolchain list 2>$null
            if (-not ($installed -match [Regex]::Escape($channel))) {
                Write-Warn "Toolchain $channel is not installed."
                Write-Host "    Attempting: rustup toolchain install $channel"
                & rustup toolchain install $channel
                if ($LASTEXITCODE -ne 0) {
                    Write-Warn "rustup couldn't install $channel."
                    Write-Host "    Most likely cause: $channel does not exist yet."
                    Write-Host "    Edit rust-toolchain.toml to a real version (e.g. 'stable') and try again."
                    exit 1
                }
            }
            Write-Ok "Toolchain $channel is available."
        }
    } else {
        Write-Warn "No rust-toolchain.toml found; using rustup default."
    }

    # --- 3. Required Windows target for desktop ------------------------
    Write-Step "Checking target: x86_64-pc-windows-msvc"
    $targets = & rustup target list --installed 2>$null
    if (-not ($targets -match 'x86_64-pc-windows-msvc')) {
        Write-Host "    Adding target..."
        & rustup target add x86_64-pc-windows-msvc
    }
    Write-Ok "Target installed."

    # --- 4. MSVC build tools (linker) ----------------------------------
    Write-Step "Checking MSVC build tools"
    if (-not (Test-Command 'link')) {
        Write-Warn "MSVC linker (link.exe) not detected on PATH."
        Write-Host "    Install Visual Studio 2022 Build Tools with the 'Desktop development with C++' workload:"
        Write-Host "    https://visualstudio.microsoft.com/downloads/"
        Write-Host "    (winget install Microsoft.VisualStudio.2022.BuildTools)"
        Write-Host ""
        Write-Host "    Continuing anyway -- cargo may locate it via the registry on its own."
    } else {
        Write-Ok "link.exe found."
    }

    # --- 5. Dioxus CLI -------------------------------------------------
    Write-Step "Checking dioxus-cli (dx)"
    if (-not (Test-Command 'dx')) {
        Write-Host "    Installing dioxus-cli..."
        & cargo install dioxus-cli --locked
        if ($LASTEXITCODE -ne 0) {
            Write-Warn "dx install failed."
            exit 1
        }
    }
    Write-Ok (dx --version 2>&1 | Select-Object -First 1)

    # --- 6. WebView2 runtime (desktop only) ----------------------------
    if ($Platform -eq 'desktop') {
        Write-Step "Checking WebView2 runtime"
        $wvKey  = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
        $altKey = 'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
        $present = (Test-Path $wvKey) -or (Test-Path $altKey)
        if ($present) {
            Write-Ok "WebView2 runtime is installed."
        } else {
            Write-Warn "WebView2 runtime not detected. Windows 11 includes it; Windows 10 may need it:"
            Write-Host "    https://developer.microsoft.com/en-us/microsoft-edge/webview2/"
        }
    }
}

# --- 7. Launch ---------------------------------------------------------
# `--package sonitus-ui` is required because we're a workspace with
# multiple crates; dx can't infer the binary target from the root manifest.
$dxArgs = @('serve', '--package', 'sonitus-ui', '--platform', $Platform)
if ($Release) {
    $dxArgs += '--release'
}

Write-Step "Launching: dx $($dxArgs -join ' ')"
Write-Host "    Press Ctrl+C in this window to stop."
Write-Host ""

& dx @dxArgs
exit $LASTEXITCODE
