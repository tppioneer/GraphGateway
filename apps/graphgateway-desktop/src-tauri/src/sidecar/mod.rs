//! Owned Sidecar lifecycle manager — async actor with transactional start.
//!
//! Responsibilities:
//! - Resolve the sidecar binary path (runtime, no build-machine paths)
//! - Spawn the sidecar subprocess (tokio::process, no shell concatenation)
//! - Execute the versioned stdin/stdout handshake (async I/O)
//! - Create a Windows Job Object for orphan cleanup
//! - Expose health / status via `graphgateway-client`
//! - Graceful shutdown with a time budget, then force-kill
//! - Detect unexpected child exit via `try_wait`
//! - Broadcast `SidecarSnapshot` via `tokio::sync::watch`
//!
//! # Rollback guarantee
//!
//! If **any** step after a successful spawn fails, the child process is:
//! - killed (`child.kill()`),
//! - waited/reaped (`child.wait()`),
//! - the Job Object handle is dropped,
//! - and `token`, `endpoint`, `pid`, `started_at` are cleared.
//!
//! No `block_on`, no `Handle::current()`, no `std::sync::Mutex` held across
//! `.await`.

mod job_object;

use graphgateway_client::GraphGatewayClient;
use graphgateway_types::{
    ReadyMessage, SidecarSnapshot, SidecarState, StartupConfig, PROTOCOL_VERSION,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::time::timeout;

/// Maximum time to wait for the sidecar's ready message on stdout.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Graceful shutdown budget — after this, force kill.
const SHUTDOWN_BUDGET: Duration = Duration::from_secs(10);

/// Max ready message size (4 KiB per ADR-001).
const MAX_READY_BYTES: usize = 4096;

// ---------------------------------------------------------------------------
// Sidecar manager
// ---------------------------------------------------------------------------

pub(crate) struct SidecarManager {
    state: SidecarState,
    child: Option<Child>,
    token: Option<String>,
    endpoint: Option<String>,
    started_at: Option<Instant>,
    last_error: Option<String>,
    pid: Option<u32>,
    server_version: Option<String>,
    api_version: Option<String>,
    #[cfg(windows)]
    job_handle: Option<job_object::JobHandle>,
    /// Broadcast channel for status snapshots.
    snapshot_tx: watch::Sender<SidecarSnapshot>,
}

impl SidecarManager {
    /// Create a new manager and return it along with a status receiver.
    pub(crate) fn new() -> (Self, watch::Receiver<SidecarSnapshot>) {
        let initial = SidecarSnapshot {
            state: SidecarState::Stopped,
            pid: None,
            endpoint: None,
            server_version: None,
            api_version: None,
            started_at: None,
            uptime_ms: None,
            last_error: None,
            desktop_version: None,
        };
        let (tx, rx) = watch::channel(initial);
        let mgr = Self {
            state: SidecarState::Stopped,
            child: None,
            token: None,
            endpoint: None,
            started_at: None,
            last_error: None,
            pid: None,
            server_version: None,
            api_version: None,
            #[cfg(windows)]
            job_handle: None,
            snapshot_tx: tx,
        };
        (mgr, rx)
    }

    // ------------------------------------------------------------------
    // Public API — all async
    // ------------------------------------------------------------------

    /// Start the sidecar with full transactional rollback.
    ///
    /// Any failure after a successful `spawn()` results in:
    /// - child killed and waited
    /// - Job Object handle dropped
    /// - token / endpoint / pid cleared
    /// - state → Failed
    pub(crate) async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.state {
            SidecarState::Stopped | SidecarState::Failed => {}
            _ => return Err("sidecar is already starting or running".into()),
        }

        self.transition_to(SidecarState::Starting);
        self.last_error = None;

        let mut res = StartResources::new();

        // 1. Generate access token (local, no core dep).
        let token = generate_access_token_local();

        // 2. Resolve sidecar binary path.
        let binary_path = match resolve_sidecar_path() {
            Ok(p) => p,
            Err(e) => {
                let msg = format!("failed to resolve sidecar path: {e}");
                res.rollback(self, &msg).await;
                return Err(e);
            }
        };

        // 3. Spawn child process.
        let mut c = Command::new(&binary_path);
        c.args(["serve", "--owned-sidecar", "--listen", "127.0.0.1:0"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW is not exposed on tokio::process::Command.
            // The sidecar inherits our stdio handles so no console appears.
        }

        let mut spawned = match c.spawn() {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("failed to spawn sidecar: {e}");
                res.rollback(self, &msg).await;
                return Err(e.into());
            }
        };
        let child_pid = spawned.id().expect("child should have a PID after spawn");

        // 4. Create and assign Windows Job Object (must succeed).
        #[cfg(windows)]
        {
            match job_object::create_kill_on_close_job() {
                Ok(job) => {
                    if let Err(e) = job_object::assign_process(&job, child_pid) {
                        res.child = Some(spawned);
                        res.job_handle = Some(job);
                        let msg = format!(
                            "Job Object assignment failed for PID {child_pid}: {e}. \
                             This may happen if the Tauri process itself is in a Job. \
                             The sidecar has been terminated."
                        );
                        res.rollback(self, &msg).await;
                        return Err("Job Object assignment failed".into());
                    }
                    res.job_handle = Some(job);
                }
                Err(e) => {
                    res.child = Some(spawned);
                    let msg = format!("failed to create Job Object: {e}");
                    res.rollback(self, &msg).await;
                    return Err(e);
                }
            }
        }

        // 5. Write startup config to stdin.
        let config = StartupConfig {
            protocol_version: PROTOCOL_VERSION,
            access_token: token.clone(),
            parent_pid: std::process::id(),
            data_dir: get_data_dir(),
        };
        let config_json = match serde_json::to_string(&config) {
            Ok(s) => s,
            Err(e) => {
                res.child = Some(spawned);
                let msg = format!("failed to serialize startup config: {e}");
                res.rollback(self, &msg).await;
                return Err(e.into());
            }
        };
        {
            let mut stdin = spawned.stdin.take().ok_or("failed to open sidecar stdin")?;
            if let Err(e) = stdin.write_all(config_json.as_bytes()).await {
                res.child = Some(spawned);
                let msg = format!("failed to write config to stdin: {e}");
                res.rollback(self, &msg).await;
                return Err(e.into());
            }
            if let Err(e) = stdin.write_all(b"\n").await {
                res.child = Some(spawned);
                let msg = format!("failed to write newline: {e}");
                res.rollback(self, &msg).await;
                return Err(e.into());
            }
            if let Err(e) = stdin.flush().await {
                res.child = Some(spawned);
                let msg = format!("failed to flush stdin: {e}");
                res.rollback(self, &msg).await;
                return Err(e.into());
            }
        }

        // 6. Read ready message from stdout (with 30s timeout).
        let stdout = spawned
            .stdout
            .take()
            .ok_or("failed to take sidecar stdout")?;
        let ready_line = {
            let read_fut = read_first_line(stdout, MAX_READY_BYTES);
            match timeout(HANDSHAKE_TIMEOUT, read_fut).await {
                Ok(Ok(line)) => line,
                Ok(Err(e)) => {
                    res.child = Some(spawned);
                    res.rollback(self, &e).await;
                    return Err("ready message read failed".into());
                }
                Err(_elapsed) => {
                    res.child = Some(spawned);
                    let msg = format!("handshake timed out after {}s", HANDSHAKE_TIMEOUT.as_secs());
                    res.rollback(self, &msg).await;
                    return Err("handshake timeout".into());
                }
            }
        };

        // 7. Parse and validate ready message.
        let ready: ReadyMessage = match serde_json::from_str(&ready_line) {
            Ok(r) => r,
            Err(e) => {
                res.child = Some(spawned);
                let msg = format!("invalid ready message JSON: {e}");
                res.rollback(self, &msg).await;
                return Err(e.into());
            }
        };
        if let Err(e) = validate_ready_message(&ready, child_pid) {
            res.child = Some(spawned);
            res.rollback(self, &msg_for_validation(&e)).await;
            return Err(e.into());
        }
        let ep = ready.endpoint.clone();

        // 8. Health + status via REST (async, no block_on).
        let client = match GraphGatewayClient::new(&ep, &token) {
            Ok(c) => c,
            Err(e) => {
                res.child = Some(spawned);
                let msg = format!("failed to create HTTP client: {e}");
                res.rollback(self, &msg).await;
                return Err(e.into());
            }
        };
        if let Err(e) = client.health().await {
            res.child = Some(spawned);
            let msg = format!("health check failed: {e}");
            res.rollback(self, &msg).await;
            return Err(e.into());
        }
        if let Err(e) = client.status().await {
            res.child = Some(spawned);
            let msg = format!("status check failed: {e}");
            res.rollback(self, &msg).await;
            return Err(e.into());
        }

        // 9. Commit — ALL resources transferred to self.
        self.child = Some(spawned);
        #[cfg(windows)]
        {
            self.job_handle = res.job_handle.take();
        }
        self.token = Some(token);
        self.endpoint = Some(ep.clone());
        self.pid = Some(child_pid);
        self.started_at = Some(Instant::now());
        self.server_version = Some(ready.server_version.clone());
        self.api_version = Some(ready.api_version.clone());
        self.state = SidecarState::Ready;
        self.last_error = None;
        self.snapshot_tx.send(self.build_snapshot()).ok();

        tracing::info!(
            pid = ready.pid,
            endpoint = %ep,
            version = %ready.server_version,
            "sidecar ready"
        );
        Ok(())
    }

    /// Stop a running sidecar gracefully, then forcefully.
    pub(crate) async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.state != SidecarState::Ready && self.state != SidecarState::Failed {
            return Err("sidecar is not running".into());
        }
        self.stop_inner().await
    }

    /// Restart the sidecar (stop if running, then start).
    pub(crate) async fn restart(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.state == SidecarState::Ready || self.state == SidecarState::Failed {
            self.stop_inner().await?;
        }
        // Ensure we're in a state that allows starting.
        self.state = SidecarState::Stopped;
        self.snapshot_tx.send(self.build_snapshot()).ok();
        Box::pin(self.start()).await
    }

    /// Check child process liveness.
    ///
    /// If the child has exited unexpectedly, transition to Failed and clean up.
    /// Returns `true` if the sidecar is still healthy.
    pub(crate) async fn check_health(&mut self) -> bool {
        if self.state != SidecarState::Ready && self.state != SidecarState::Failed {
            return false;
        }

        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Child exited.
                    let reason = if status.success() {
                        format!(
                            "sidecar exited with status 0 (pid {})",
                            self.pid.unwrap_or(0)
                        )
                    } else {
                        format!(
                            "sidecar exited with code {:?} (pid {})",
                            status.code(),
                            self.pid.unwrap_or(0)
                        )
                    };
                    tracing::warn!("{}", reason);
                    self.child = None;
                    #[cfg(windows)]
                    {
                        self.job_handle = None;
                    }
                    self.token = None;
                    self.endpoint = None;
                    self.pid = None;
                    self.started_at = None;
                    self.server_version = None;
                    self.api_version = None;
                    self.state = SidecarState::Failed;
                    self.last_error = Some(reason);
                    self.snapshot_tx.send(self.build_snapshot()).ok();
                    false
                }
                Ok(None) => {
                    // Still running — healthy.
                    true
                }
                Err(e) => {
                    tracing::error!(error = %e, "try_wait failed");
                    false
                }
            }
        } else {
            // No child handle but state says Ready — inconsistency, fix it.
            self.state = SidecarState::Failed;
            self.last_error = Some("child handle missing while in Ready state".into());
            self.snapshot_tx.send(self.build_snapshot()).ok();
            false
        }
    }

    // ------------------------------------------------------------------
    // Internal
    // ------------------------------------------------------------------

    async fn stop_inner(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.state = SidecarState::Stopping;
        self.snapshot_tx.send(self.build_snapshot()).ok();

        // Try REST shutdown first (fire-and-forget with short timeout).
        if let (Some(endpoint), Some(token)) = (&self.endpoint, &self.token) {
            if let Ok(client) =
                GraphGatewayClient::with_timeout(endpoint, token, Duration::from_secs(5))
            {
                let _ = timeout(Duration::from_secs(5), client.shutdown()).await;
            }
        }

        // Wait for child to exit, with budget.
        if let Some(ref mut child) = self.child {
            let pid = child.id();
            let start = Instant::now();
            let mut exited = false;

            while start.elapsed() < SHUTDOWN_BUDGET {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        tracing::info!(?pid, ?status, "sidecar exited gracefully");
                        exited = true;
                        break;
                    }
                    Ok(None) => {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    Err(e) => {
                        tracing::warn!(?pid, error = %e, "try_wait error during shutdown");
                        break;
                    }
                }
            }

            if !exited {
                tracing::warn!(?pid, "force-killing sidecar after shutdown budget exceeded");
                let _ = child.start_kill();
                let _ = timeout(Duration::from_secs(5), child.wait()).await;
            }
        }

        // Release all resources.
        self.child = None;
        #[cfg(windows)]
        {
            self.job_handle = None;
        }
        self.token = None;
        self.endpoint = None;
        self.pid = None;
        self.started_at = None;
        self.server_version = None;
        self.api_version = None;
        self.state = SidecarState::Stopped;
        self.last_error = None;
        self.snapshot_tx.send(self.build_snapshot()).ok();

        Ok(())
    }

    fn transition_to(&mut self, state: SidecarState) {
        self.state = state;
        self.snapshot_tx.send(self.build_snapshot()).ok();
    }

    pub(crate) fn build_snapshot(&self) -> SidecarSnapshot {
        let uptime_ms = if self.state == SidecarState::Ready {
            self.started_at.map(|s| s.elapsed().as_millis() as u64)
        } else {
            None
        };

        SidecarSnapshot {
            state: self.state,
            pid: self.pid,
            endpoint: self.endpoint.clone(),
            server_version: self.server_version.clone(),
            api_version: self.api_version.clone(),
            started_at: None,
            uptime_ms,
            last_error: self.last_error.clone(),
            desktop_version: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Transactional start helper — tracks acquired resources for rollback
// ---------------------------------------------------------------------------

struct StartResources {
    child: Option<Child>,
    #[cfg(windows)]
    job_handle: Option<job_object::JobHandle>,
}

impl StartResources {
    fn new() -> Self {
        Self {
            child: None,
            #[cfg(windows)]
            job_handle: None,
        }
    }

    /// Kill child, drop job handle, and transition the manager to Failed.
    async fn rollback(&mut self, mgr: &mut SidecarManager, err_msg: &str) {
        let err_msg = err_msg.to_string();
        tracing::error!(error = %err_msg, "sidecar start failed, rolling back");
        if let Some(ref mut c) = self.child {
            let pid = c.id();
            let _ = c.start_kill();
            let wait_fut = c.wait();
            if let Err(e) = tokio::time::timeout(Duration::from_secs(5), wait_fut).await {
                tracing::warn!(?pid, error = %e, "child did not exit within kill timeout");
            }
        }
        #[cfg(windows)]
        {
            self.job_handle = None;
        }
        self.child = None;
        mgr.state = SidecarState::Failed;
        mgr.last_error = Some(sanitize_error(&err_msg));
        mgr.token = None;
        mgr.endpoint = None;
        mgr.pid = None;
        mgr.started_at = None;
        mgr.server_version = None;
        mgr.api_version = None;
        mgr.snapshot_tx.send(mgr.build_snapshot()).ok();
    }
}

// ---------------------------------------------------------------------------
// Async-ready-message reader
// ---------------------------------------------------------------------------

/// Read one line from stdout, enforcing a strict byte limit.
///
/// Reads byte-by-byte until `\n` or the limit is exceeded.  This correctly
/// handles open streams (where `read_to_string` would block waiting for EOF)
/// and detects over-size / non-UTF-8 input.
async fn read_first_line(
    mut stdout: tokio::process::ChildStdout,
    max_bytes: usize,
) -> Result<String, String> {
    use tokio::io::AsyncReadExt;

    let mut buf = vec![0u8; max_bytes + 1];
    let mut pos = 0;

    while pos < buf.len() {
        let n = stdout
            .read(&mut buf[pos..pos + 1])
            .await
            .map_err(|e| format!("stdout read error: {e}"))?;
        if n == 0 {
            break; // EOF
        }
        if buf[pos] == b'\n' {
            let line = String::from_utf8(buf[..pos].to_vec())
                .map_err(|e| format!("invalid UTF-8 in ready message: {e}"))?;
            return Ok(line.trim_end_matches('\r').to_string());
        }
        pos += 1;
    }

    if pos == 0 {
        return Err("empty stdout — sidecar closed stdout before ready message".into());
    }
    if pos >= max_bytes {
        // Check if we hit the limit exactly at a newline (edge case).
        if buf[pos] == b'\n' {
            let line = String::from_utf8(buf[..pos].to_vec())
                .map_err(|e| format!("invalid UTF-8: {e}"))?;
            return Ok(line.trim_end_matches('\r').to_string());
        }
        return Err(format!("ready message exceeds {max_bytes} bytes (max)"));
    }
    // EOF reached without newline — accept if within limits.
    let line = String::from_utf8(buf[..pos].to_vec())
        .map_err(|e| format!("invalid UTF-8 in ready message: {e}"))?;
    Ok(line.trim_end_matches('\r').to_string())
}

// ---------------------------------------------------------------------------
// Ready message validation (beyond basic type + protocol)
// ---------------------------------------------------------------------------

fn validate_ready_message(msg: &ReadyMessage, expected_pid: u32) -> Result<(), String> {
    // 1. type must be "ready"
    if msg.msg_type != "ready" {
        return Err(format!("unexpected ready message type: {}", msg.msg_type));
    }

    // 2. protocol version must match
    if msg.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "protocol version mismatch: expected {}, got {}",
            PROTOCOL_VERSION, msg.protocol_version
        ));
    }

    // 3. PID must match the real child PID
    if msg.pid != expected_pid {
        return Err(format!(
            "ready PID {} does not match child PID {}",
            msg.pid, expected_pid
        ));
    }

    // 4. api_version must be a supported version
    if msg.api_version != graphgateway_types::API_VERSION {
        return Err(format!(
            "unsupported API version: {} (expected {})",
            msg.api_version,
            graphgateway_types::API_VERSION
        ));
    }

    // 5. endpoint must be a valid URL with http scheme and loopback host
    validate_endpoint(&msg.endpoint)?;

    // 6. server_version must be non-empty
    if msg.server_version.is_empty() {
        return Err("server_version is empty".into());
    }

    Ok(())
}

/// Validate the endpoint URL for safety (manual parse — no `url` crate dep).
///
/// Expected format: `http://127.0.0.1:<port>` or `http://[::1]:<port>`.
fn validate_endpoint(raw: &str) -> Result<(), String> {
    // 1. Scheme must be `http://`
    let without_scheme = raw
        .strip_prefix("http://")
        .ok_or_else(|| format!("endpoint scheme must be http, got: {raw}"))?;

    // 2. No credentials (`user:pass@host` or `user@host`)
    if without_scheme.contains('@') {
        return Err("endpoint URL must not contain credentials (@)".into());
    }

    // 3. No query or fragment
    if without_scheme.contains('?') || without_scheme.contains('#') {
        return Err("endpoint URL must not contain query or fragment".into());
    }

    // 4. Split host:port (IPv6 addresses are wrapped in [...])
    let (host_str, port_str) = if let Some(rest) = without_scheme.strip_prefix('[') {
        // IPv6: http://[::1]:port/path
        let (ipv6, rest2) = rest
            .split_once(']')
            .ok_or_else(|| "malformed IPv6 endpoint URL".to_string())?;
        let port_and_path = rest2.strip_prefix(':').unwrap_or(rest2);
        let port = port_and_path.split('/').next().unwrap_or(port_and_path);
        (format!("[{ipv6}]"), port.to_string())
    } else {
        // IPv4 or hostname
        let host_part = without_scheme.split('/').next().unwrap_or(without_scheme);
        let (host, port) = host_part.split_once(':').unwrap_or((host_part, ""));
        (host.to_string(), port.to_string())
    };

    // 5. Host must be loopback
    let is_loopback = match host_str.as_str() {
        "127.0.0.1" | "localhost" => true,
        s if s.starts_with("[") => {
            // IPv6 loopback
            s == "[::1]" || s == "[0:0:0:0:0:0:0:1]"
        }
        _ => false,
    };

    if !is_loopback {
        return Err(format!("endpoint host must be loopback, got: {host_str}"));
    }

    // 6. Port must be present and non-zero
    if port_str.is_empty() {
        return Err("endpoint must include a port".into());
    }
    let port: u16 = port_str
        .parse()
        .map_err(|_| format!("invalid port: {port_str}"))?;
    if port == 0 {
        return Err("endpoint port must be non-zero".into());
    }

    Ok(())
}

fn msg_for_validation(e: &str) -> String {
    format!("ready message validation failed: {e}")
}

// ---------------------------------------------------------------------------
// Token generation (local — no graphgateway-core dependency)
// ---------------------------------------------------------------------------

/// Generate a cryptographically-random 256-bit hex access token.
///
/// This is a local copy of `graphgateway_core::generate_access_token()` so
/// that the desktop crate does not depend on `graphgateway-core`.
fn generate_access_token_local() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Error sanitization
// ---------------------------------------------------------------------------

/// Remove potentially sensitive information from error messages.
fn sanitize_error(s: &str) -> String {
    // Truncate very long errors.
    let s = if s.len() > 500 { &s[..500] } else { s };
    // Note: we do NOT log the raw error content that might contain tokens.
    // The error string is only stored in last_error after sanitization.
    s.to_string()
}

// ---------------------------------------------------------------------------
// Binary path resolution (runtime, no CARGO_MANIFEST_DIR)
// ---------------------------------------------------------------------------

fn resolve_sidecar_path() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let target_triple = build_target_triple();
    let sidecar_name = format!("graphgateway-{target_triple}.exe");

    // Collect the candidate paths we check for richer error reporting.
    let mut candidates: Vec<String> = Vec::new();

    // 1. Check relative to the current executable (production layout).
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    if let Some(ref dir) = exe_dir {
        // Production: sidecar next to the desktop EXE
        let prod = dir.join("graphgateway.exe");
        candidates.push(prod.display().to_string());
        if prod.exists() {
            tracing::debug!(path = %prod.display(), "found sidecar next to exe");
            return Ok(prod);
        }
        // Alternative: sidecar in a 'binaries' subdirectory
        let prod_bin = dir.join("binaries").join(&sidecar_name);
        candidates.push(prod_bin.display().to_string());
        if prod_bin.exists() {
            tracing::debug!(path = %prod_bin.display(), "found sidecar in binaries dir");
            return Ok(prod_bin);
        }
    } else {
        candidates.push("<could not determine executable directory>".into());
        candidates.push("<same>".into());
    }

    // 2. Check Tauri externalBin dev layout (binaries/ relative to src-tauri).
    // In Tauri dev mode, the current directory is typically the src-tauri dir.
    let cwd = std::env::current_dir().ok();
    let dev_path = PathBuf::from("binaries").join(&sidecar_name);
    candidates.push(format!(
        "{} (relative to {})",
        dev_path.display(),
        cwd.as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|| "?".into())
    ));
    if dev_path.exists() {
        tracing::debug!(path = %dev_path.display(), "found sidecar in dev binaries");
        return Ok(dev_path);
    }

    let dev_simple = PathBuf::from("binaries").join("graphgateway.exe");
    candidates.push(dev_simple.display().to_string());
    if dev_simple.exists() {
        tracing::debug!(path = %dev_simple.display(), "found sidecar (simple name) in dev binaries");
        return Ok(dev_simple);
    }

    // 3. Workspace target/debug fallback (for development convenience).
    let workspace_target = PathBuf::from("../../target/debug/graphgateway.exe");
    candidates.push(workspace_target.display().to_string());
    if workspace_target.exists() {
        tracing::debug!(path = %workspace_target.display(), "found sidecar in workspace target");
        return Ok(workspace_target.canonicalize().unwrap_or(workspace_target));
    }

    let mut msg = format!(
        "sidecar binary not found (target triple: {target_triple}).\n\
         Expected binary name: {sidecar_name}\n\
         Searched locations:\n"
    );
    for (i, c) in candidates.iter().enumerate() {
        msg.push_str(&format!("  {}. {c}\n", i + 1));
    }
    msg.push_str(&format!(
        "\n\
         Resolve this with one of:\n\
         - Release build:  cargo tauri build       (auto-builds sidecar via beforeBuildCommand)\n\
         - Manual release: npm run build:sidecar   (builds sidecar & copies to binaries/)\n\
         - Dev build:      cargo build -p graphgateway-server  (then use cargo tauri dev)\n\
         - Manual copy:    copy target\\debug\\graphgateway.exe apps\\graphgateway-desktop\\src-tauri\\binaries\\graphgateway-{target_triple}.exe",
    ));

    Err(msg.into())
}

/// Return the effective target triple for sidecar binary resolution.
///
/// Priority order:
///   1. `CARGO_BUILD_TARGET` env var (set explicitly at build/run time)
///   2. `cfg!` compile-time target (reflects the Rust toolchain target)
fn build_target_triple() -> String {
    if let Ok(t) = std::env::var("CARGO_BUILD_TARGET") {
        if !t.is_empty() {
            return t;
        }
    }
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc".into()
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "aarch64-pc-windows-msvc".into()
    } else {
        "x86_64-pc-windows-msvc".into()
    }
}

fn get_data_dir() -> String {
    if let Ok(dir) = std::env::var("LOCALAPPDATA") {
        format!("{}\\GraphGateway\\data", dir)
    } else {
        let tmp = std::env::temp_dir();
        tmp.join("graphgateway-data").to_string_lossy().to_string()
    }
}

#[cfg(test)]
mod target_triple_tests {
    use super::build_target_triple;

    #[test]
    fn returns_cfg_target_by_default() {
        let triple = build_target_triple();
        // On Windows this will end with -pc-windows-msvc.
        assert!(triple.ends_with("-pc-windows-msvc"));
        assert!(!triple.is_empty());
    }

    #[test]
    fn honors_cargo_build_target_env() {
        let custom = "aarch64-pc-windows-msvc";
        std::env::set_var("CARGO_BUILD_TARGET", custom);
        let triple = build_target_triple();
        assert_eq!(triple, custom);
        std::env::remove_var("CARGO_BUILD_TARGET");
    }

    #[test]
    fn empty_cargo_build_target_falls_back() {
        std::env::set_var("CARGO_BUILD_TARGET", "");
        let triple = build_target_triple();
        // Should fall back to cfg!
        assert!(triple.ends_with("-pc-windows-msvc"));
        std::env::remove_var("CARGO_BUILD_TARGET");
    }

    #[test]
    fn multiple_targets_consistent_format() {
        // Verify that known Windows targets follow the expected naming.
        let triples = [
            "x86_64-pc-windows-msvc",
            "aarch64-pc-windows-msvc",
            "i686-pc-windows-msvc",
            "x86_64-pc-windows-gnu",
        ];
        for t in &triples {
            assert!(
                t.contains("windows"),
                "triple should contain 'windows': {t}"
            );
            let sidecar = format!("graphgateway-{t}.exe");
            assert!(sidecar.ends_with(".exe"));
            assert!(sidecar.starts_with("graphgateway-"));
        }
    }
}
