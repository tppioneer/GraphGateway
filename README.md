# GraphGateway

Code graph workspace MCP router — Phase 1 engineering skeleton with Owned Sidecar vertical slice.

## Status

- **Branch**: `mcp`
- **Phase**: 1 — Engineering skeleton + Owned Sidecar
- **Platform**: Windows (MSVC toolchain)
- **Rust**: stable 1.97+ (edition 2024 for lib crates, 2021 for Tauri)
- **Node**: 24+

## Architecture

```
apps/graphgateway-desktop/     ← Tauri v2 Product (React + TypeScript + Vite)
crates/
├─ graphgateway-types/         ← Shared types (no transport/storage deps)
├─ graphgateway-core/          ← Domain logic (token gen, constant-time eq)
├─ graphgateway-rest/          ← Axum routes, auth middleware, app state
├─ graphgateway-server/        ← CLI binary (handshake, serve command)
└─ graphgateway-client/        ← Strongly-typed REST client (reqwest)
```

Dependency direction: `desktop → client → HTTP API`  |  `server → rest → core → types`

Tauri Product does NOT depend on `graphgateway-core`.

## Quick Start

### Prerequisites

- Rust stable with `x86_64-pc-windows-msvc` target (run `rustup default stable-x86_64-pc-windows-msvc`)
- Visual Studio Build Tools (for MSVC linker)
- Node.js 24+ with npm

### Build

```bash
# Build everything
cargo build --workspace

# Build the sidecar binary
cargo build -p graphgateway-server

# Copy sidecar to Tauri binaries directory (required for Tauri build)
copy target\debug\graphgateway.exe apps\graphgateway-desktop\src-tauri\binaries\graphgateway-x86_64-pc-windows-msvc.exe

# Install frontend dependencies & build
cd apps\graphgateway-desktop
npm install
npx tsc --noEmit
npx vite build
```

### Run Tests

```bash
# All tests (unit + integration)
cargo test --workspace --all-features

# Frontend type-check
cd apps\graphgateway-desktop && npx tsc --noEmit

# Lint
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### Run the Sidecar Standalone

```bash
# Start with a test token via stdin
echo '{"protocol_version":1,"access_token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","parent_pid":1234,"data_dir":"C:\\Temp\\gw-data"}' | cargo run -p graphgateway-server -- serve --owned-sidecar --listen 127.0.0.1:0
```

### Run the Tauri Desktop App

```bash
cd apps\graphgateway-desktop
npm run tauri dev
```

## API Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/healthz` | None | Minimal liveness check |
| GET | `/api/v1/status` | Bearer | Server status (version, pid, uptime, mode) |
| POST | `/api/v1/system/shutdown` | Bearer | Graceful shutdown |

### Sidecar Startup Handshake

Tauri Backend → Sidecar stdin (one JSON line):
```json
{"protocol_version":1,"access_token":"<hex>","parent_pid":1234,"data_dir":"<path>"}
```

Sidecar → stdout (one JSON line):
```json
{"type":"ready","protocol_version":1,"pid":1234,"endpoint":"http://127.0.0.1:<port>","api_version":"v1","server_version":"0.1.0"}
```

## Sidecar Target Triple Naming

For Tauri v2 `externalBin`, place the sidecar binary at:
```
apps/graphgateway-desktop/src-tauri/binaries/graphgateway-x86_64-pc-windows-msvc.exe
```

## What's Not Implemented (Phase 2+)

- Shared Service / Remote mode / Windows Service
- MCP Server/Client, mcp-proxy, GitNexus integration
- Workspace/Source CRUD, SQLite, QueryPlan
- Management pages beyond Overview
- Tauri process-in-process plugin / DLL / C ABI
- Linux / macOS support

## ADRs

- [ADR-001: Sidecar Startup Handshake, Token Passing, and Job Object Ownership](docs/adr/001-sidecar-handshake-and-job-object.md)

## License

MIT
