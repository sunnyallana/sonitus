# scripts/

Helper scripts for local development on Windows.

## `run-dev.ps1` / `run-dev.cmd`

Starts Sonitus in dev mode with hot-reload.

```powershell
# From PowerShell:
.\scripts\run-dev.ps1

# From cmd.exe:
scripts\run-dev.cmd

# Release build (faster runtime, slower compile):
.\scripts\run-dev.ps1 -Release

# Run the web target instead of desktop:
.\scripts\run-dev.ps1 -Platform web

# Skip the environment checks (you've already verified):
.\scripts\run-dev.ps1 -SkipChecks
```

The script verifies:

1. `rustup` is on PATH.
2. The toolchain pinned in `rust-toolchain.toml` is installed.
3. `x86_64-pc-windows-msvc` target is added.
4. MSVC build tools (link.exe) are findable.
5. `dx` (Dioxus CLI) is installed — installs it if not.
6. WebView2 runtime is present (Windows desktop only).

## `build-release.ps1`

Bundles a release `.msi` / `.exe` via `dx bundle --release`.
