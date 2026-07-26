# ADR-001: Sidecar Startup Handshake, Token Passing, and Job Object Ownership

- **Status**: Accepted
- **Date**: 2026-07-26
- **Decision-makers**: bathfire

## Context

The GraphGateway Tauri Product runs `graphgateway.exe` as an Owned Sidecar — a
child process that the Tauri backend starts, monitors, and stops.  The sidecar
exposes a REST management API on `127.0.0.1` protected by a Bearer token.

Three interdependent concerns must be resolved:

1. **Startup handshake** — how the Tauri backend passes configuration (including
   the access token) to the sidecar, and how the sidecar reports its listening
   endpoint back.
2. **Token passing** — the token must never appear in command-line arguments,
   environment variables, URLs, stdout-ready messages, or the frontend.
3. **Process-tree cleanup** — when the Tauri process exits (gracefully or
   crashes), the sidecar and any descendant processes must be terminated.

## Decision

### 1. Versioned stdin/stdout Handshake Protocol

The Tauri backend writes a single JSON line to the sidecar's **stdin** containing
the startup configuration (protocol version, access token, parent PID, data
directory).  The sidecar binds its HTTP listener, then writes a single JSON
"ready" line to **stdout** containing its protocol version, PID, endpoint URL,
API version, and server version.

- **Protocol version**: currently `1`; both sides validate.
- **Stdin size limit**: 8 KiB.
- **Ready message size limit**: 4 KiB.
- **Timeout**: 30 seconds for the ready message; process is killed on timeout.
- **stdout is reserved** for the sidecar control protocol.  All application logs
  go to **stderr** via `tracing` (JSON format).

Rejected alternatives:

- **Command-line arguments**: would expose the token in process lists and
  `GetCommandLineW`.
- **Environment variables**: would expose the token in process environment
  blocks and child process inheritance.
- **Shared file**: adds filesystem I/O, race conditions, and cleanup concerns.
- **Unversioned protocol**: would block future changes to the handshake format.

### 2. Token Generation and Handling

- The Tauri backend generates a **256-bit random hex token** (64 hex chars)
  using `graphgateway_core::generate_access_token()`.
- The token is written to the sidecar's stdin as part of the startup config.
- The token is **never**:
  - Logged (stdin content is not traced).
  - Exposed in `Debug` or `Display` (the `GraphGatewayClient` redacts it).
  - Returned to the frontend (Tauri commands strip it).
  - Placed in ready messages, URLs, or file paths.
- The sidecar's REST API uses **constant-time comparison** (`subtle` crate) for
  Bearer token validation, mitigating timing side-channel leakage.
- GET `/healthz` requires **no authentication** and returns only `{"status":"ok"}`,
  deliberately leaking no version, path, or token information.

### 3. Windows Job Object for Process-Tree Cleanup

The Tauri backend creates a **Windows Job Object** with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.  The sidecar process is assigned to this
job immediately after spawning.

- When the Tauri process exits (even abnormally), the OS closes the last handle
  to the job object, which **terminates all processes in the job** — the sidecar
  and any future descendant processes (mcp-proxy, GitNexus).
- The job object is **unnamed**, so only the Tauri process holds a handle.
- The Tauri backend also implements a **graceful shutdown path**: it calls
  `POST /api/v1/system/shutdown` first, then waits up to 10 seconds for the
  sidecar to exit, then `kill()`s the process.  The job object is the safety
  net for crash scenarios.

### 4. stdout/stderr Separation

| Stream | Purpose |
|--------|---------|
| **stdout** | Sidecar control protocol (ready message).  No application logs. |
| **stderr** | Structured JSON tracing logs via `tracing-subscriber`. |

The Tauri backend reads the **first line** of stdout as the ready message and
discards subsequent stdout.  All stderr output is inherited by the Tauri
process for diagnostics.

This separation prevents log output from being parsed as protocol messages and
vice versa.

## Consequences

- The handshake protocol must be versioned and validated on both sides.
- Adding new startup parameters requires a protocol version bump or a
  backward-compatible extension.
- The Tauri backend must build the sidecar path resolution logic itself (no
  universal Tauri sidecar API used).
- Job Object cleanup depends on Windows-specific APIs; Linux/macOS support
  will require an alternative mechanism.
- The sidecar cannot run without a valid startup config on stdin; standalone
  execution requires a tool or script that provides one.

## References

- [GraphGateway Windows Tauri Product Design](../graphgateway-windows-tauri-product-design.zh-CN.md)
- [Graph Workspace MCP Router Design](../graph-workspace-mcp-router-design.zh-CN.md)
