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
      2. $env:CARGO_BUILD_TARGET
      3. rustc -vV host field
      4. $env:PROCESSOR_ARCHITECTURE fallback (lowest priority)
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

# Priority order: CLI param > env var > rustc host > node arch
$targetTriple = if ($Target) {
    $Target
} elseif ($env:CARGO_BUILD_TARGET) {
    $env:CARGO_BUILD_TARGET
} else {
    $rustcTriple = Get-RustcHostTriple
    if ($rustcTriple) {
        $rustcTriple
    } else {
        Get-NodeHostTriple
    }
}

$sidecarName = "graphgateway-${targetTriple}.exe"

Write-Host "Target triple: $targetTriple"
Write-Host "Sidecar binary name: $sidecarName"

# ---------------------------------------------------------------------------
# Build the sidecar in release mode
# ---------------------------------------------------------------------------

Write-Host "Building graphgateway-server (release)..."
Push-Location $workspaceRoot
try {
    if ($Target) {
        cargo build -p graphgateway-server --release --target $targetTriple
    } else {
        cargo build -p graphgateway-server --release
    }
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build -p graphgateway-server --release failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}

# ---------------------------------------------------------------------------
# Copy the binary
# ---------------------------------------------------------------------------

# When a non-default target is used, Cargo places artifacts under
# target/<target-triple>/ rather than target/.
$src = "$workspaceRoot\target\$targetTriple\release\graphgateway.exe"
if (-not (Test-Path $src)) {
    # Fall back to host-default target directory.
    $src = "$workspaceRoot\target\release\graphgateway.exe"
}

if (-not (Test-Path $src)) {
    throw "Sidecar binary not found.  Tried:`n  $workspaceRoot\target\$targetTriple\release\graphgateway.exe`n  $workspaceRoot\target\release\graphgateway.exe"
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
