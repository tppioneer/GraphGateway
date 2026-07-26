# GraphGateway Desktop

Tauri v2 desktop product that manages a local GraphGateway sidecar.

## Prerequisites

- **Windows** (x86_64 or ARM64)
- **Rust** stable (≥ 1.97), via [rustup](https://rustup.rs)
- **Node.js** ≥ 18 and npm ≥ 9

## Quick start (development)

```powershell
# From the workspace root:
cargo build -p graphgateway-server        # build the sidecar once
cd apps\graphgateway-desktop
npm ci
npm run tauri dev                         # starts Vite + Tauri dev window
```

The sidecar binary is resolved at runtime from `../../target/debug/graphgateway.exe`
(workspace target directory).  No manual copy is needed for dev mode.

Use `cargo build -p graphgateway-server` again after changing the sidecar.

## Release build

```powershell
# From apps/graphgateway-desktop:
npm ci
cargo tauri build --no-bundle             # build frontend + sidecar + Tauri app
```

The `beforeBuildCommand` in `tauri.conf.json` automatically:

1. Builds the frontend (`npm run build`)
2. Builds the sidecar (`cargo build -p graphgateway-server --release`)
3. Copies `graphgateway.exe` into `src-tauri/binaries/` with the correct
   `graphgateway-{target-triple}.exe` name

Tauri then bundles the sidecar alongside the desktop executable.

To produce a full Windows installer, omit `--no-bundle`:

```powershell
cargo tauri build
```

## Manual sidecar build

If you only want to build and place the sidecar (without the full Tauri build):

```powershell
npm run build:sidecar
```

This runs `cargo build -p graphgateway-server --release` and then uses
`scripts/copy-sidecar.mjs` (a Node.js script) to:

- Detect the target triple (`x86_64-pc-windows-msvc` or `aarch64-pc-windows-msvc`)
- Copy the release binary into `src-tauri/binaries/`

A PowerShell alternative (`scripts/build-sidecar.ps1`) is also available.

## Directory layout

```
apps/graphgateway-desktop/
├── src/                    # React + TypeScript frontend
│   ├── api/                # Tauri invoke wrappers
│   ├── pages/              # Page components
│   └── state/              # TanStack Query hooks
├── src-tauri/              # Tauri Rust backend
│   ├── binaries/           # Sidecar binary (populated at build time)
│   ├── capabilities/       # Tauri capability permissions
│   ├── src/
│   │   ├── commands.rs     # Tauri IPC command handlers
│   │   ├── lib.rs          # App setup and lifecycle
│   │   ├── main.rs         # Windows entry point
│   │   └── sidecar/        # Sidecar manager + Job Object
│   ├── Cargo.toml
│   └── tauri.conf.json
├── scripts/
│   ├── copy-sidecar.mjs     # Sidecar binary copy (Node.js, used by Tauri)
│   └── build-sidecar.ps1    # Sidecar build + copy (PowerShell alternative)
├── package.json
└── vite.config.ts
```

## Verification

```powershell
# From the workspace root:
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-features -- -D warnings

# From apps/graphgateway-desktop:
npm ci
npm run build
cargo tauri build --no-bundle
```

## Troubleshooting

### "sidecar binary not found"

The Tauri runtime looks for the sidecar in several locations (see
`resolve_sidecar_path` in `sidecar/mod.rs`):

1. Next to the desktop executable (`graphgateway.exe`)
2. `binaries/graphgateway-{triple}.exe` relative to the executable
3. Developer layout: `binaries/` relative to `src-tauri/`
4. Workspace `target/debug/` (dev fallback)

If none are found, the error message lists every searched path together with
the expected target triple.

### "Job Object assignment failed"

The Tauri backend requires a Windows Job Object with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` for sidecar process-tree cleanup.
Job Object creation and assignment are **hard startup requirements**:

- If `CreateJobObjectW` or `AssignProcessToJobObject` fails, the sidecar
  child process is immediately killed and reaped.
- All acquired resources (child handle, Job Object handle, token, endpoint)
  are rolled back.
- The desktop startup or `sidecar_start` command returns an error.
- The sidecar state transitions to `Failed` with a diagnostic message in
  `last_error`.

This failure typically occurs when the Tauri process itself is already
assigned to a Windows Job Object (e.g., some CI runners, process managers,
or debuggers).  The parent Job Object prevents assignment to a second one.

**Diagnostic procedure:**

1. Check whether the Tauri process is already in a Job Object:
   ```powershell
   Get-Process -Id $pid | Select-Object -Property Name, Id
   # In Process Explorer: double-click the process → Job tab
   ```
2. If running under a CI runner or process manager that creates Job Objects,
   configure the runner to not assign the Tauri process to a Job, or use a
   dedicated test machine without Job Object nesting.
3. Verify the Windows version supports `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
   (Windows 8 / Server 2012 or later).
4. Run the Tauri app outside the job-nesting environment (e.g., directly from
   Explorer or an unmanaged command prompt) to confirm the Job Object is
   created successfully in a normal desktop session.

### Sidecar fails to start after upgrade

Delete `%LOCALAPPDATA%\GraphGateway\run\` to clear any stale runtime state.
