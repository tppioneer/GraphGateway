//! Integration test: real subprocess start → handshake → REST API → shutdown.
//!
//! This test spawns `graphgateway.exe serve`, completes the stdin/stdout
//! handshake, verifies /healthz, /api/v1/status (auth), and triggers a
//! graceful shutdown.

use graphgateway_types::{PROTOCOL_VERSION, ReadyMessage, StartupConfig, StatusResponse};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

/// Helper: spawn the sidecar, write config to stdin, read ready from stdout.
fn start_sidecar() -> Result<(Child, String, String), Box<dyn std::error::Error>> {
    // Resolve the binary — workspace root is 2 levels up from the crate dir.
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
        .stderr(Stdio::inherit())
        .spawn()?;

    // Write startup config
    let token = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    let config = StartupConfig {
        protocol_version: PROTOCOL_VERSION,
        access_token: token.clone(),
        parent_pid: std::process::id(),
        data_dir: std::env::temp_dir()
            .join("graphgateway-test-data")
            .to_string_lossy()
            .to_string(),
    };
    let config_json = serde_json::to_string(&config)?;

    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(config_json.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        // Drop to close the pipe — the server reads until EOF.
    }

    // Read ready message from stdout (with timeout)
    let mut stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(&mut stdout);
    let ready_line = reader.lines().next().ok_or("no ready line on stdout")??;

    let ready: ReadyMessage = serde_json::from_str(&ready_line)?;
    ready.validate(PROTOCOL_VERSION)?;

    Ok((child, ready.endpoint.clone(), token))
}

#[test]
fn integration_full_lifecycle() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // 1. Start sidecar
    let (mut child, endpoint, token) = start_sidecar().expect("failed to start sidecar");

    eprintln!("Sidecar ready at {endpoint}");

    // 2. Test /healthz (no auth)
    let health_url = format!("{endpoint}/healthz");
    let health_resp = rt.block_on(async {
        reqwest::get(&health_url)
            .await
            .expect("healthz request failed")
    });
    assert!(
        health_resp.status().is_success(),
        "healthz should return 2xx"
    );
    let health_body: serde_json::Value = rt
        .block_on(async { health_resp.json().await })
        .expect("healthz JSON parse failed");
    assert_eq!(health_body["status"], "ok");

    // 3. Test /api/v1/status without auth → 401
    let status_url = format!("{endpoint}/api/v1/status");
    let no_auth_resp = rt.block_on(async {
        reqwest::Client::new()
            .get(&status_url)
            .send()
            .await
            .expect("status no-auth request failed")
    });
    assert_eq!(
        no_auth_resp.status().as_u16(),
        401,
        "status without auth should return 401"
    );

    // 4. Test /api/v1/status with wrong token → 401
    let wrong_token_resp = rt.block_on(async {
        reqwest::Client::new()
            .get(&status_url)
            .header("Authorization", "Bearer wrong-token")
            .send()
            .await
            .expect("status wrong-token request failed")
    });
    assert_eq!(
        wrong_token_resp.status().as_u16(),
        401,
        "status with wrong token should return 401"
    );

    // 5. Test /api/v1/status with correct token → 200
    let status_resp = rt.block_on(async {
        reqwest::Client::new()
            .get(&status_url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("status request failed")
    });
    assert!(
        status_resp.status().is_success(),
        "status should return 2xx"
    );
    let status_body: StatusResponse = rt
        .block_on(async { status_resp.json().await })
        .expect("status JSON parse failed");

    // Validate status fields
    assert_eq!(status_body.mode, "owned-sidecar");
    assert_eq!(status_body.api_version, "v1");
    assert_eq!(status_body.endpoint, endpoint);
    assert!(status_body.pid > 0);
    assert!(!status_body.server_version.is_empty());
    assert!(status_body.uptime_ms > 0);

    eprintln!(
        "Status OK: pid={}, version={}, uptime={}ms",
        status_body.pid, status_body.server_version, status_body.uptime_ms
    );

    // 6. Test /api/v1/system/shutdown with correct token
    let shutdown_url = format!("{endpoint}/api/v1/system/shutdown");
    let shutdown_resp = rt.block_on(async {
        reqwest::Client::new()
            .post(&shutdown_url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("shutdown request failed")
    });
    assert!(
        shutdown_resp.status().is_success(),
        "shutdown should return 2xx"
    );

    // 7. Wait for the sidecar to exit
    let exit_status = child.wait().expect("failed to wait for sidecar");
    eprintln!("Sidecar exited with {exit_status:?}");
    assert!(exit_status.success(), "sidecar should exit cleanly");
}

#[test]
fn integration_two_independent_sidecars() {
    // Start two independent sidecar processes — they should each get
    // different ports and run concurrently without conflict.
    // This tests the port=0 dynamic assignment, not SidecarManager
    // duplicate prevention (which is tested at the unit level).
    let (mut child1, endpoint1, _token1) = start_sidecar().expect("failed to start first sidecar");
    let (mut child2, endpoint2, _token2) = start_sidecar().expect("failed to start second sidecar");

    eprintln!("Sidecar 1 at {endpoint1}, Sidecar 2 at {endpoint2}");

    // They should have different ports
    assert_ne!(endpoint1, endpoint2);

    // Cleanup — shutdown both
    let rt = tokio::runtime::Runtime::new().unwrap();
    for (endpoint, token) in [(endpoint1, _token1), (endpoint2, _token2)] {
        let shutdown_url = format!("{endpoint}/api/v1/system/shutdown");
        let _ = rt.block_on(async {
            reqwest::Client::new()
                .post(&shutdown_url)
                .header("Authorization", format!("Bearer {token}"))
                .send()
                .await
        });
    }

    child1.wait().ok();
    child2.wait().ok();
}
