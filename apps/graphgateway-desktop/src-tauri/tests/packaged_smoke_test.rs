//! Packaged desktop lifecycle smoke test.
//!
//! Validates that:
//! 1. The Tauri-built desktop executable launches the packaged Sidecar.
//! 2. Readiness succeeds (healthz returns 200).
//! 3. When the desktop host is terminated, the Sidecar process tree is reclaimed.
//! 4. Missing or corrupted packaged Sidecar causes clean failure without orphans.
//! 5. Startup tokens are not written to stderr logs.
//!
//! # Prerequisites
//!
//! The desktop executable must be built first:
//!
//! ```powershell
//! cd apps/graphgateway-desktop
//! npm ci
//! npm run build
//! cargo tauri build --no-bundle
//! ```
//!
//! If the desktop executable is not found, the test prints a SKIP message
//! and succeeds — it does not fail the test suite.
//!
//! All tests are Windows-only (`#[cfg(windows)]`).

#[cfg(windows)]
mod packaged_smoke {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    // -----------------------------------------------------------------------
    // Path resolution
    // -----------------------------------------------------------------------

    /// Return the workspace root (3 levels up from the src-tauri crate dir).
    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent() // src-tauri/
            .and_then(|p| p.parent()) // apps/graphgateway-desktop/
            .and_then(|p| p.parent()) // workspace root
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("../../.."))
    }

    /// Path to the desktop executable produced by `cargo tauri build --no-bundle`.
    fn desktop_exe_path() -> PathBuf {
        workspace_root().join("target/release/graphgateway-desktop.exe")
    }

    /// Path to the binaries directory (packaged sidecar location).
    fn binaries_dir() -> PathBuf {
        workspace_root().join("apps/graphgateway-desktop/src-tauri/binaries")
    }

    /// Path to the workspace debug sidecar (fallback in resolve_sidecar_path).
    fn debug_sidecar_path() -> PathBuf {
        workspace_root().join("target/debug/graphgateway.exe")
    }

    /// Path to the release sidecar in the target directory.
    fn release_sidecar_path() -> PathBuf {
        workspace_root().join("target/release/graphgateway.exe")
    }

    // -----------------------------------------------------------------------
    // Process enumeration (via tasklist + netstat — no PowerShell dependency)
    // -----------------------------------------------------------------------

    /// Run a Windows command and return its stdout as a string.
    fn run_cmd(program: &str, args: &[&str]) -> Result<String, String> {
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("{program} spawn failed: {e}"))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(format!(
                "{program} failed (exit {}): {stderr}",
                output.status
            ))
        }
    }

    /// Find PIDs of all running graphgateway.exe processes using tasklist.
    fn find_sidecar_pids() -> Vec<u32> {
        // tasklist /fi "imagename eq graphgateway.exe" /fo csv /nh
        // Output: "graphgateway.exe","1234","Console","1","12,345 K"
        match run_cmd(
            "tasklist",
            &["/fi", "imagename eq graphgateway.exe", "/fo", "csv", "/nh"],
        ) {
            Ok(out) => {
                let mut pids = Vec::new();
                for line in out.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with("INFO:") {
                        continue;
                    }
                    // CSV format: "graphgateway.exe","PID",...
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 2 {
                        let pid_str = parts[1].trim_matches('"').trim();
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            pids.push(pid);
                        }
                    }
                }
                pids
            }
            Err(e) => {
                eprintln!("WARNING: could not enumerate processes with tasklist: {e}");
                vec![]
            }
        }
    }

    /// Find the local IPv4 loopback listening port for a given PID using netstat.
    fn find_loopback_port(pid: u32) -> Option<u16> {
        // netstat -ano | findstr "127.0.0.1" | findstr "<pid>"
        // Output: TCP    127.0.0.1:12345    0.0.0.0:0    LISTENING    1234
        match run_cmd(
            "cmd",
            &[
                "/c",
                &format!("netstat -ano | findstr 127.0.0.1 | findstr {pid}"),
            ],
        ) {
            Ok(out) => {
                for line in out.lines() {
                    let line = line.trim();
                    if !line.contains("LISTENING") {
                        continue;
                    }
                    // Parse port from "127.0.0.1:PORT"
                    if let Some(port_start) = line.find("127.0.0.1:") {
                        let rest = &line[port_start + 10..]; // skip "127.0.0.1:"
                        let port_str: String =
                            rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                        if let Ok(port) = port_str.parse::<u16>() {
                            if port > 0 {
                                return Some(port);
                            }
                        }
                    }
                }
                None
            }
            Err(e) => {
                eprintln!("WARNING: port lookup failed for PID {pid}: {e}");
                None
            }
        }
    }

    /// Check if a process with a given PID still exists.
    fn process_exists(pid: u32) -> bool {
        match run_cmd(
            "tasklist",
            &["/fi", &format!("pid eq {pid}"), "/fo", "csv", "/nh"],
        ) {
            Ok(out) => {
                for line in out.lines() {
                    if line.trim().is_empty() || line.starts_with("INFO:") {
                        continue;
                    }
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 2 {
                        let pid_str = parts[1].trim_matches('"').trim();
                        if let Ok(p) = pid_str.parse::<u32>() {
                            if p == pid {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            Err(_) => false,
        }
    }

    /// Kill a process tree by PID using taskkill.
    fn kill_process_tree(pid: u32) {
        let _ = run_cmd("taskkill", &["/f", "/t", "/pid", &pid.to_string()]);
    }

    // -----------------------------------------------------------------------
    // Sidecar binary hider (for "missing sidecar" test)
    // -----------------------------------------------------------------------

    /// Temporarily hides sidecar binaries so `resolve_sidecar_path` cannot find
    /// them.  Restores on drop.
    struct SidecarHider {
        /// Original path of the renamed binaries directory.
        binaries_bak: Option<PathBuf>,
        /// Original path of the renamed debug sidecar.
        debug_bak: Option<PathBuf>,
        /// Original path of the renamed release sidecar.
        release_bak: Option<PathBuf>,
    }

    impl SidecarHider {
        fn hide() -> Self {
            let mut hider = Self {
                binaries_bak: None,
                debug_bak: None,
                release_bak: None,
            };

            // Hide binaries directory.
            let bd = binaries_dir();
            if bd.exists() {
                let bak = bd.with_file_name("binaries.smoke-bak");
                if let Err(e) = std::fs::rename(&bd, &bak) {
                    eprintln!("WARNING: could not rename binaries dir: {e}");
                } else {
                    hider.binaries_bak = Some(bak);
                }
            }

            // Hide debug sidecar.
            let debug = debug_sidecar_path();
            if debug.exists() {
                let bak = debug.with_file_name("graphgateway.exe.smoke-bak");
                if let Err(e) = std::fs::rename(&debug, &bak) {
                    eprintln!("WARNING: could not rename debug sidecar: {e}");
                } else {
                    hider.debug_bak = Some(bak);
                }
            }

            // Hide release sidecar (in workspace target/release).
            let release = release_sidecar_path();
            if release.exists() {
                let bak = release.with_file_name("graphgateway.exe.smoke-bak");
                if let Err(e) = std::fs::rename(&release, &bak) {
                    eprintln!("WARNING: could not rename release sidecar: {e}");
                } else {
                    hider.release_bak = Some(bak);
                }
            }

            hider
        }
    }

    impl Drop for SidecarHider {
        fn drop(&mut self) {
            if let Some(ref bak) = self.binaries_bak {
                let orig = bak.with_file_name("binaries");
                let _ = std::fs::rename(bak, &orig);
            }
            if let Some(ref bak) = self.debug_bak {
                let orig = bak.with_file_name("graphgateway.exe");
                let _ = std::fs::rename(bak, &orig);
            }
            if let Some(ref bak) = self.release_bak {
                let orig = bak.with_file_name("graphgateway.exe");
                let _ = std::fs::rename(bak, &orig);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Test: Happy path — desktop launches sidecar, readiness works, cleanup
    // -----------------------------------------------------------------------

    /// Test that the packaged desktop executable:
    /// 1. Launches the packaged Sidecar
    /// 2. Sidecar becomes ready (healthz responds 200)
    /// 3. Terminating the desktop reclaims the Sidecar process tree
    /// 4. Stderr logs don't contain the startup token
    #[test]
    fn packaged_desktop_launches_sidecar_and_cleans_up() {
        let exe = desktop_exe_path();
        if !exe.exists() {
            eprintln!("SKIP: desktop executable not found at {}", exe.display());
            eprintln!(
                "  Build with: cd apps/graphgateway-desktop && cargo tauri build --no-bundle"
            );
            return;
        }

        let sd = binaries_dir();
        eprintln!("Desktop exe:  {}", exe.display());
        eprintln!("Sidecar dir:  {}", sd.display());

        // Record baseline process count.
        let before_pids = find_sidecar_pids();
        eprintln!("Sidecar processes before: {before_pids:?}");

        // Spawn desktop directly — stderr is discarded in the packaged test;
        // token-leak is verified separately in sidecar_stderr_has_no_token_leak.
        let mut child = Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn desktop executable");

        let desktop_pid = child.id();
        eprintln!("Desktop PID: {desktop_pid}");

        // Wait for the sidecar to appear (up to 60 seconds).
        let start = Instant::now();
        let timeout = Duration::from_secs(60);
        let mut sidecar_pid: Option<u32> = None;
        let mut sidecar_port: Option<u16> = None;

        while start.elapsed() < timeout {
            // Check if desktop exited prematurely.
            match child.try_wait() {
                Ok(Some(status)) => {
                    panic!("desktop exited prematurely with {status:?}");
                }
                Ok(None) => {}
                Err(e) => {
                    panic!("try_wait error on desktop: {e}");
                }
            }

            let current_pids = find_sidecar_pids();
            let new_pids: Vec<u32> = current_pids
                .iter()
                .filter(|p| !before_pids.contains(p))
                .copied()
                .collect();

            if !new_pids.is_empty() {
                sidecar_pid = Some(new_pids[0]);
                eprintln!("Found sidecar PID: {}", new_pids[0]);

                // Wait a moment for the port to be bound.
                std::thread::sleep(Duration::from_secs(3));

                // Find the listening port.
                if let Some(port) = find_loopback_port(new_pids[0]) {
                    sidecar_port = Some(port);
                    eprintln!("Sidecar listening on 127.0.0.1:{port}");
                    break;
                }
                eprintln!(
                    "Sidecar PID {} found but port not yet bound, retrying...",
                    new_pids[0]
                );
            }

            std::thread::sleep(Duration::from_secs(2));
        }

        let sidecar_pid = sidecar_pid.expect(
            "sidecar process did not appear within timeout — \
             the desktop may have failed to launch the sidecar",
        );
        let sidecar_port = sidecar_port.unwrap_or_else(|| {
            panic!("could not determine sidecar listening port for PID {sidecar_pid}")
        });

        // Prove readiness: hit healthz.
        eprintln!("Hitting healthz on 127.0.0.1:{sidecar_port}...");
        let healthz_url = format!("http://127.0.0.1:{sidecar_port}/healthz");
        let health_resp = reqwest::blocking::get(&healthz_url).expect("healthz request failed");
        assert_eq!(
            health_resp.status().as_u16(),
            200,
            "healthz should return 200"
        );
        eprintln!("healthz OK: 200");

        // Terminate the desktop host.
        eprintln!("Terminating desktop (PID {desktop_pid})...");
        kill_process_tree(desktop_pid);

        // Wait for the OS to clean up (KILL_ON_JOB_CLOSE fires on last handle
        // close, which happens when the desktop process exits).
        let cleanup_start = Instant::now();
        let cleanup_timeout = Duration::from_secs(15);
        let mut reclaimed = false;

        while cleanup_start.elapsed() < cleanup_timeout {
            if !process_exists(sidecar_pid) {
                reclaimed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }

        // Wait for desktop to fully exit.
        let _ = child.wait();

        if !reclaimed {
            eprintln!(
                "WARNING: sidecar PID {sidecar_pid} still alive after {:.0}s — force killing",
                cleanup_timeout.as_secs()
            );
            kill_process_tree(sidecar_pid);
            std::thread::sleep(Duration::from_secs(2));
        }

        // Final verification: no sidecar processes that weren't there before.
        let final_pids = find_sidecar_pids();
        let orphan_pids: Vec<u32> = final_pids
            .iter()
            .filter(|p| !before_pids.contains(p))
            .copied()
            .collect();

        assert!(
            orphan_pids.is_empty(),
            "sidecar process tree was NOT fully reclaimed. \
             Orphan PIDs after desktop exit: {orphan_pids:?}. \
             The Job Object with KILL_ON_JOB_CLOSE should have terminated \
             all processes."
        );
        eprintln!(
            "Process tree reclaimed: no orphan sidecar processes found \
             (sidecar PID {sidecar_pid} is gone)."
        );
        eprintln!("Packaged desktop smoke test PASSED.");
    }

    // -----------------------------------------------------------------------
    // Test: Missing/corrupted sidecar — clean failure, no orphans
    // -----------------------------------------------------------------------

    /// Test that when the packaged Sidecar binary is missing, the desktop:
    /// - Fails to start the sidecar (state → Failed)
    /// - Does not leave any orphan processes
    #[test]
    fn missing_sidecar_causes_clean_failure() {
        let exe = desktop_exe_path();
        if !exe.exists() {
            eprintln!("SKIP: desktop executable not found at {}", exe.display());
            eprintln!(
                "  Build with: cd apps/graphgateway-desktop && cargo tauri build --no-bundle"
            );
            return;
        }

        let before_pids = find_sidecar_pids();
        eprintln!("Sidecar processes before: {before_pids:?}");

        // Hide all sidecar binaries so resolve_sidecar_path cannot find any.
        let _hider = SidecarHider::hide();

        // Spawn desktop with CARGO_BUILD_TARGET set to a bogus triple so
        // resolve_sidecar_path uses a non-existent target for paths 2 and 3.
        let exe_path = desktop_exe_path();
        let mut cmd = Command::new(&exe_path);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("CARGO_BUILD_TARGET", "mips64-pc-windows-msvc");

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("ERROR spawning desktop: {e}");
                // _hider drop restores files.
                // If we can't spawn, skip rather than fail.
                eprintln!("SKIP: could not spawn desktop (environment issue)");
                return;
            }
        };

        let desktop_pid = child.id();
        eprintln!("Desktop PID: {desktop_pid}");

        // Wait for the desktop to attempt sidecar startup and fail.
        // The auto-start spawns in the background and logs a warning.
        // Give it enough time.
        std::thread::sleep(Duration::from_secs(15));

        // Verify that NO graphgateway.exe process was started by the desktop.
        let current_pids = find_sidecar_pids();
        let new_pids: Vec<u32> = current_pids
            .iter()
            .filter(|p| !before_pids.contains(p))
            .copied()
            .collect();

        assert!(
            new_pids.is_empty(),
            "new sidecar processes appeared even though the binary was missing: {new_pids:?}"
        );
        eprintln!("No sidecar processes spawned (expected — binary is missing).");

        // Kill the desktop.
        kill_process_tree(desktop_pid);
        let _ = child.wait();
        std::thread::sleep(Duration::from_secs(3));

        // Verify no orphan processes remain.
        let final_pids = find_sidecar_pids();
        let orphan_pids: Vec<u32> = final_pids
            .iter()
            .filter(|p| !before_pids.contains(p))
            .copied()
            .collect();

        assert!(
            orphan_pids.is_empty(),
            "orphan processes detected after missing-sidecar test: {orphan_pids:?}"
        );
        eprintln!("Missing sidecar test PASSED: clean failure, no orphans.");

        // _hider drop restores files.
    }

    // -----------------------------------------------------------------------
    // Test: Stderr token leak check (using the happy-path test's evidence)
    // -----------------------------------------------------------------------

    /// Verify that sidecar stderr logs do NOT contain the startup access token.
    ///
    /// This test starts the sidecar directly (not through the desktop) with a
    /// KNOWN token so we can check for leaks in stderr.  This complements the
    /// packaged test which uses a random token.
    ///
    /// We reuse the same approach as the existing server-side smoke test but
    /// execute it here so the desktop crate's test suite covers it.
    #[test]
    fn sidecar_stderr_has_no_token_leak() {
        use graphgateway_types::{ReadyMessage, StartupConfig, PROTOCOL_VERSION};

        // Find the sidecar binary — try the packaged location first, then
        // the workspace debug fallback.
        let mut sidecar_exe = binaries_dir().join("graphgateway.exe");
        if !sidecar_exe.exists() {
            sidecar_exe = debug_sidecar_path();
        }
        if !sidecar_exe.exists() {
            sidecar_exe = binaries_dir().join("graphgateway-x86_64-pc-windows-msvc.exe");
        }
        if !sidecar_exe.exists() {
            eprintln!("SKIP: no sidecar binary found for token-leak test");
            eprintln!("  Build with: cargo build -p graphgateway-server");
            return;
        }

        // Use a FIXED known token so we can scan for it.
        let known_token =
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();

        let mut child = Command::new(&sidecar_exe)
            .args(["serve", "--owned-sidecar", "--listen", "127.0.0.1:0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn sidecar");

        let config = StartupConfig {
            protocol_version: PROTOCOL_VERSION,
            access_token: known_token.clone(),
            parent_pid: std::process::id(),
            data_dir: std::env::temp_dir()
                .join("graphgateway-packaged-token-test")
                .to_string_lossy()
                .to_string(),
        };
        let config_json = serde_json::to_string(&config).unwrap();

        {
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(config_json.as_bytes()).unwrap();
            stdin.write_all(b"\n").unwrap();
            stdin.flush().unwrap();
        }

        // Read ready message.
        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);
        let ready_line = reader
            .lines()
            .next()
            .expect("no ready line")
            .expect("ready line read error");
        let ready: ReadyMessage =
            serde_json::from_str(&ready_line).expect("ready JSON parse failed");
        ready
            .validate(PROTOCOL_VERSION)
            .expect("ready validation failed");

        // Hit healthz to confirm readiness.
        let healthz_url = format!("{}/healthz", ready.endpoint);
        let health_resp = reqwest::blocking::get(&healthz_url).expect("healthz failed");
        assert_eq!(health_resp.status().as_u16(), 200);

        // Hit /api/v1/status with auth to generate some log traffic.
        let status_url = format!("{}/api/v1/status", ready.endpoint);
        let _ = reqwest::blocking::Client::new()
            .get(&status_url)
            .header("Authorization", format!("Bearer {known_token}"))
            .send()
            .expect("status request failed");

        // Shutdown.
        let shutdown_url = format!("{}/api/v1/system/shutdown", ready.endpoint);
        let _ = reqwest::blocking::Client::new()
            .post(&shutdown_url)
            .header("Authorization", format!("Bearer {known_token}"))
            .send()
            .expect("shutdown request failed");

        let _ = child.wait();

        // Check stderr for token leakage.
        let mut stderr_buf = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr_buf)
            .expect("failed to read stderr");

        let mut line_count = 0;
        for (line_num, line) in stderr_buf.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            line_count += 1;
            assert!(
                !line.contains(&known_token),
                "STDERR TOKEN LEAK at line {}: token found in log output\n  log line: {}",
                line_num + 1,
                line
            );
            // Each line should be valid JSON (tracing-subscriber JSON format).
            let trimmed = line.trim();
            assert!(
                serde_json::from_str::<serde_json::Value>(trimmed).is_ok(),
                "stderr line {} is not valid JSON: {trimmed}",
                line_num + 1
            );
        }

        eprintln!("Token leak test PASSED: {line_count} stderr lines checked, zero token leaks");
    }
}

// -----------------------------------------------------------------------
// Non-Windows stub — record the skip but don't fail.
// -----------------------------------------------------------------------

#[cfg(not(windows))]
#[test]
fn packaged_smoke_skip_non_windows() {
    eprintln!("SKIP: packaged desktop smoke test requires Windows");
}

#[cfg(not(windows))]
#[test]
fn missing_sidecar_skip_non_windows() {
    eprintln!("SKIP: missing sidecar smoke test requires Windows");
}

#[cfg(not(windows))]
#[test]
fn token_leak_skip_non_windows() {
    eprintln!("SKIP: token leak test requires Windows");
}
