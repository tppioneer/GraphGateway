//! Windows Sidecar smoke test.
//!
//! Validates the full lifecycle:
//! 1. Sidecar startup via stdin/stdout handshake
//! 2. Ready message with correct PID, protocol, endpoint
//! 3. /healthz (no auth) and /api/v1/status (with auth)
//! 4. Graceful shutdown via REST API
//! 5. Process reclamation (child exits, no orphans)
//! 6. Stderr logs do NOT contain the startup access token
//!
//! This test is additive to the existing `sidecar_integration_test.rs` which
//! covers basic lifecycle.  This test adds the token-leak and stderr checks.

use graphgateway_types::{PROTOCOL_VERSION, ReadyMessage, StartupConfig};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};

/// Helper: spawn the sidecar with stderr *captured* (not inherited).
///
/// Returns the child process handle, the endpoint URL, the access token,
/// and the stderr reader.
fn start_sidecar_with_stderr()
-> Result<(Child, String, String, BufReader<std::process::ChildStderr>), Box<dyn std::error::Error>>
{
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let binary_path = workspace_root.join("target/debug/graphgateway.exe");
    assert!(
        binary_path.exists(),
        "graphgateway.exe not found at {} — build it first: cargo build -p graphgateway-server",
        binary_path.display()
    );

    let mut child = Command::new(&binary_path)
        .args(["serve", "--owned-sidecar", "--listen", "127.0.0.1:0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped()) // captured for token-leak check
        .spawn()?;

    let token = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    let config = StartupConfig {
        protocol_version: PROTOCOL_VERSION,
        access_token: token.clone(),
        parent_pid: std::process::id(),
        data_dir: std::env::temp_dir()
            .join("graphgateway-smoke-test-data")
            .to_string_lossy()
            .to_string(),
    };
    let config_json = serde_json::to_string(&config)?;

    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(config_json.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        // Drop stdin to close the pipe.
    }

    // Read ready message from stdout.
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);
    let ready_line = reader.lines().next().ok_or("no ready line on stdout")??;
    let ready: ReadyMessage = serde_json::from_str(&ready_line)?;
    ready.validate(PROTOCOL_VERSION)?;

    let stderr = child.stderr.take().unwrap();
    let stderr_reader = BufReader::new(stderr);

    Ok((child, ready.endpoint.clone(), token, stderr_reader))
}

#[test]
fn smoke_full_lifecycle_no_token_in_logs() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // 1. Start sidecar with stderr captured.
    let (mut child, endpoint, token, mut stderr_reader) =
        start_sidecar_with_stderr().expect("failed to start sidecar");
    eprintln!("Smoke test: sidecar ready at {endpoint}");

    // 2. /healthz (no auth)
    let health_url = format!("{endpoint}/healthz");
    let health_status = rt.block_on(async {
        reqwest::get(&health_url)
            .await
            .expect("healthz request failed")
            .status()
            .as_u16()
    });
    assert_eq!(health_status, 200, "healthz should return 200");

    // 3. /api/v1/status (with auth)
    let status_url = format!("{endpoint}/api/v1/status");
    let status_resp = rt.block_on(async {
        reqwest::Client::new()
            .get(&status_url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("status request failed")
    });
    assert!(status_resp.status().is_success());

    let status_body: graphgateway_types::StatusResponse = rt
        .block_on(async { status_resp.json().await })
        .expect("status JSON parse failed");
    assert_eq!(status_body.mode, "owned-sidecar");
    assert_eq!(status_body.api_version, "v1");
    assert_eq!(status_body.endpoint, endpoint);
    assert!(status_body.pid > 0);
    assert!(status_body.uptime_ms > 0);

    // 4. POST /api/v1/system/shutdown (graceful)
    let shutdown_url = format!("{endpoint}/api/v1/system/shutdown");
    let shutdown_status = rt.block_on(async {
        reqwest::Client::new()
            .post(&shutdown_url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("shutdown request failed")
            .status()
            .as_u16()
    });
    assert_eq!(shutdown_status, 200, "shutdown should return 200");

    // 5. Wait for sidecar to exit (process reclamation).
    let exit_status = child.wait().expect("failed to wait for sidecar");
    assert!(
        exit_status.success(),
        "sidecar should exit cleanly, got: {exit_status:?}"
    );

    // 6. Read and check stderr for token leakage.
    let mut stderr_buf = String::new();
    stderr_reader
        .read_to_string(&mut stderr_buf)
        .expect("failed to read stderr");

    // Each stderr line is a JSON log entry.  Verify none contain the token.
    for (line_num, line) in stderr_buf.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        assert!(
            !line.contains(&token),
            "STDERR TOKEN LEAK at line {}: token found in log output\n  log line: {}",
            line_num + 1,
            line
        );
        // Also verify each line is valid JSON (tracing-subscriber json fmt layer).
        let trimmed = line.trim();
        assert!(
            serde_json::from_str::<serde_json::Value>(trimmed).is_ok(),
            "stderr line {} is not valid JSON: {trimmed}",
            line_num + 1
        );
    }

    eprintln!(
        "Smoke test PASSED: {} stderr lines checked, zero token leaks",
        stderr_buf.lines().filter(|l| !l.trim().is_empty()).count()
    );
}

/// Verify that two sidecars can run concurrently without port conflicts.
/// This uses stderr inherit (not captured) for speed.
#[test]
fn smoke_two_concurrent_sidecars() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // This test reuses the existing sidecar_integration_test helper pattern.
    // We use our own helper here for symmetry with smoke_full_lifecycle_no_token_in_logs.

    let (mut child1, endpoint1, token1, _stderr1) =
        start_sidecar_with_stderr().expect("sidecar 1 failed");
    let (mut child2, endpoint2, token2, _stderr2) =
        start_sidecar_with_stderr().expect("sidecar 2 failed");

    eprintln!("Sidecar 1: {endpoint1}, Sidecar 2: {endpoint2}");

    // Different ports
    assert_ne!(endpoint1, endpoint2);

    // Both are healthy
    for (ep, tok) in [(&endpoint1, &token1), (&endpoint2, &token2)] {
        let status = rt.block_on(async {
            reqwest::Client::new()
                .get(format!("{ep}/api/v1/status"))
                .header("Authorization", format!("Bearer {tok}"))
                .send()
                .await
                .expect("status failed")
                .status()
        });
        assert!(status.is_success());
    }

    // Graceful shutdown for both
    for (ep, tok) in [(endpoint1, token1), (endpoint2, token2)] {
        let _ = rt.block_on(async {
            reqwest::Client::new()
                .post(format!("{ep}/api/v1/system/shutdown"))
                .header("Authorization", format!("Bearer {tok}"))
                .send()
                .await
        });
    }

    // Both exit cleanly
    let s1 = child1.wait().expect("sidecar 1 wait failed");
    let s2 = child2.wait().expect("sidecar 2 wait failed");
    assert!(s1.success());
    assert!(s2.success());
}

/// Verify that the sidecar correctly binds to a specific port when
/// port 0 (auto-assign) is NOT used.
#[test]
fn smoke_fixed_port_assignment() {
    // Use a non-privileged high port unlikely to be in use.
    // We try a few ports in case one is occupied.
    let ports_to_try = [45987u16, 45988, 45989];

    for &port in &ports_to_try {
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap());
        let binary_path = workspace_root.join("target/debug/graphgateway.exe");

        let mut child = match Command::new(&binary_path)
            .args([
                "serve",
                "--owned-sidecar",
                "--listen",
                &format!("127.0.0.1:{port}"),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => continue,
        };

        let token = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        let config = StartupConfig {
            protocol_version: PROTOCOL_VERSION,
            access_token: token.clone(),
            parent_pid: std::process::id(),
            data_dir: std::env::temp_dir()
                .join("graphgateway-fixed-port-test")
                .to_string_lossy()
                .to_string(),
        };

        {
            let mut stdin = child.stdin.take().unwrap();
            let _ = stdin.write_all(serde_json::to_string(&config).unwrap().as_bytes());
            let _ = stdin.write_all(b"\n");
            let _ = stdin.flush();
        }

        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);
        let ready_line = match reader.lines().next() {
            Some(Ok(l)) => l,
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                continue;
            }
        };

        let ready: ReadyMessage = match serde_json::from_str(&ready_line) {
            Ok(r) => r,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                continue;
            }
        };

        assert!(
            ready.endpoint.ends_with(&format!(":{port}")),
            "endpoint should use the specified port {port}, got {}",
            ready.endpoint
        );

        // Shutdown
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _ = rt.block_on(async {
            reqwest::Client::new()
                .post(format!("{}/api/v1/system/shutdown", ready.endpoint))
                .header("Authorization", format!("Bearer {token}"))
                .send()
                .await
        });

        let _ = child.wait();
        eprintln!("Fixed port {port} test passed");
        return;
    }

    panic!("could not bind to any of the test ports: {ports_to_try:?}");
}
