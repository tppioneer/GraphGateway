<#
.SYNOPSIS
    Builds the graphgateway-server sidecar binary and copies it into
    src-tauri/binaries/ with the correct target-triple name so that
    Tauri's externalBin resolution finds it.
.DESCRIPTION
    Detects the host target triple, builds `graphgateway-server` in
    release mode, and copies the resulting .exe into the Tauri binaries
    directory.  Must be run from the workspace root.
#>

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

# Detect host target triple.
$targetTriple = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
    "aarch64-pc-windows-msvc"
} else {
    "x86_64-pc-windows-msvc"
}
$sidecarName = "graphgateway-${targetTriple}.exe"

Write-Host "Target triple: $targetTriple"
Write-Host "Sidecar binary name: $sidecarName"

# Build the sidecar in release mode.
Write-Host "Building graphgateway-server (release)..."
Push-Location $workspaceRoot
try {
    cargo build -p graphgateway-server --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build -p graphgateway-server --release failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}

# Copy the binary.
$src = "$workspaceRoot\target\release\graphgateway.exe"
if (-not (Test-Path $src)) {
    throw "Sidecar binary not found at: $src"
}

$destDir = "$workspaceRoot\apps\graphgateway-desktop\src-tauri\binaries"
$dest = "$destDir\$sidecarName"

Write-Host "Copying sidecar:"
Write-Host "  from: $src"
Write-Host "  to:   $dest"

New-Item -ItemType Directory -Force -Path $destDir | Out-Null
Copy-Item -Force $src $dest

# Also produce a plain graphgateway.exe copy for the resolve_sidecar_path
# fallback (path 4 in that function).
$plainDest = "$destDir\graphgateway.exe"
Copy-Item -Force $src $plainDest

Write-Host "Sidecar ready: $sidecarName"
