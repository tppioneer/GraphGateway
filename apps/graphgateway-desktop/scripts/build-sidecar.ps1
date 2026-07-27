<#
.SYNOPSIS
    Builds the graphgateway-server sidecar binary and copies it into
    src-tauri/binaries/ with the correct target-triple name so that
    Tauri's externalBin resolution finds it.
.DESCRIPTION
    Detects the effective target triple, builds `graphgateway-server` in
    release mode, and copies the resulting .exe into the Tauri binaries
    directory.  Must be run from the workspace root.

    The effective target triple is resolved in order of priority:
      1. -Target CLI parameter
      2. $env:TAURI_ENV_TARGET_TRIPLE (set by Tauri CLI during beforeBuildCommand)
      3. $env:CARGO_BUILD_TARGET
      4. rustc -vV host field
      5. $env:PROCESSOR_ARCHITECTURE fallback (lowest priority)
.PARAMETER Target
    Explicit Rust target triple (e.g. "aarch64-pc-windows-msvc").
#>

param(
    [string]$Target
)

$ErrorActionPreference = "Stop"

# Resolve workspace root — walk up from this script's directory until we
# find a Cargo.toml that defines a [workspace].
$workspaceRoot = $PSScriptRoot
while (-not (Test-Path "$workspaceRoot\Cargo.toml")) {
    $workspaceRoot = Split-Path $workspaceRoot -Parent
    if (-not $workspaceRoot) {
        throw "Could not locate workspace root (no Cargo.toml found)"
    }
}

Write-Host "Workspace root: $workspaceRoot"

# ---------------------------------------------------------------------------
# Resolve the effective target triple
# ---------------------------------------------------------------------------

function Get-RustcHostTriple {
    try {
        $rustcOut = & rustc -vV 2>$null | Out-String
        foreach ($line in ($rustcOut -split "`n")) {
            if ($line -match "^host:\s*(.+)$") {
                return $matches[1].Trim()
            }
        }
    } catch {
        # rustc not available
    }
    return $null
}

function Get-NodeHostTriple {
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
        return "aarch64-pc-windows-msvc"
    }
    return "x86_64-pc-windows-msvc"
}

# Determine the host triple (used for fallback gating).
$hostTriple = $rustcTriple = Get-RustcHostTriple
if (-not $hostTriple) {
    $hostTriple = Get-NodeHostTriple
}

# Priority order: CLI param > TAURI_ENV_TARGET_TRIPLE > CARGO_BUILD_TARGET > rustc host > node arch
$targetTriple = if ($Target) {
    $Target
} elseif ($env:TAURI_ENV_TARGET_TRIPLE) {
    $env:TAURI_ENV_TARGET_TRIPLE
} elseif ($env:CARGO_BUILD_TARGET) {
    $env:CARGO_BUILD_TARGET
} else {
    $triple = Get-RustcHostTriple
    if ($triple) {
        $triple
    } else {
        Get-NodeHostTriple
    }
}

$sidecarName = "graphgateway-${targetTriple}.exe"
$isCrossTarget = $targetTriple -ne $hostTriple

Write-Host "Target triple: $targetTriple"
Write-Host "Host triple:   $hostTriple"
if ($isCrossTarget) {
    Write-Host "Cross-compilation: YES (target != host)"
}
Write-Host "Sidecar binary name: $sidecarName"

# ---------------------------------------------------------------------------
# Build the sidecar in release mode
#
# Always pass --target to Cargo so that the artifact lands at the
# target/<triple>/release/ path consistently.  This is safe for host builds
# too — Cargo handles it correctly.
# ---------------------------------------------------------------------------

Write-Host "Building graphgateway-server (release) for target $targetTriple..."
Push-Location $workspaceRoot
try {
    cargo build -p graphgateway-server --release --target $targetTriple
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build -p graphgateway-server --release --target $targetTriple failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}

# ---------------------------------------------------------------------------
# Copy the binary
#
# The artifact is always at target/<targetTriple>/release/graphgateway.exe
# because we always pass --target to Cargo.
#
# P101-R3: For a cross-compiled target, we NEVER fall back to the host
# default directory (target/release/).  That would mislabel a host-arch
# binary as a different target.
# ---------------------------------------------------------------------------

$crossSrc = "$workspaceRoot\target\$targetTriple\release\graphgateway.exe"
$hostSrc = "$workspaceRoot\target\release\graphgateway.exe"

$src = $crossSrc
if (-not (Test-Path $crossSrc)) {
    if (-not $isCrossTarget) {
        # Host build: it's safe to check the default target directory because
        # the host binary matches the host target.
        if (Test-Path $hostSrc) {
            Write-Host "Using host default target directory (host == target)."
            $src = $hostSrc
        }
    }
}

if (-not (Test-Path $src)) {
    Write-Host "ERROR: Sidecar binary not found."
    Write-Host "  Expected at: $crossSrc"
    if ($isCrossTarget) {
        Write-Host "  Cross-compilation target ($targetTriple) != host ($hostTriple)."
        Write-Host "  Refusing to fall back to host binary at $hostSrc —"
        Write-Host "  that would mislabel a $hostTriple binary as $targetTriple."
        Write-Host "  Build the cross-compiled sidecar first:"
        Write-Host "    cargo build -p graphgateway-server --release --target $targetTriple"
    } else {
        Write-Host "  Also tried: $hostSrc"
        Write-Host "  Build first: cargo build -p graphgateway-server --release"
    }
    throw "Sidecar binary not found"
}

$destDir = "$workspaceRoot\apps\graphgateway-desktop\src-tauri\binaries"
$dest = "$destDir\$sidecarName"

Write-Host "Copying sidecar:"
Write-Host "  from: $src"
Write-Host "  to:   $dest"

New-Item -ItemType Directory -Force -Path $destDir | Out-Null
Copy-Item -Force $src $dest

# Also produce a plain graphgateway.exe copy for the resolve_sidecar_path
# fallback (path 1 in that function).
$plainDest = "$destDir\graphgateway.exe"
Copy-Item -Force $src $plainDest

Write-Host "Sidecar ready: $sidecarName"
