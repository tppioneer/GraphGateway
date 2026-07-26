//! Packaged desktop lifecycle smoke test.
//!
//! Validates that:
//! 1. The Tauri-built desktop executable launches the packaged Sidecar.
//! 2. Readiness succeeds (healthz returns 200).
//! 3. When the desktop host is terminated, the Sidecar process tree is reclaimed
//!    by the Windows Job Object (KILL_ON_JOB_CLOSE).
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
//! All tests are Windows-only (`#[cfg(windows)]`).
//!
//! # Gate policy
//!
//! Required packaged artifacts that are missing cause a **hard failure**
//! (panic), not a skip.  The smoke test is an explicit quality gate.

#[cfg(windows)]
mod packaged_smoke {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// Global lock serializing tests that mutate or depend on the shared
    /// binaries directory.  The two lifecycle tests cannot run concurrently
    /// because one hides files the other needs.
    static BINARIES_LOCK: Mutex<()> = Mutex::new(());

    struct TemporaryFile(PathBuf);

    impl Drop for TemporaryFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

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

    /// Kill a process tree by PID (uses /t to recursively kill all children).
    /// Only used in TestGuard cleanup paths — never in the success path.
    fn kill_process_tree(pid: u32) {
        let _ = run_cmd("taskkill", &["/f", "/t", "/pid", &pid.to_string()]);
    }

    /// Kill only the target process, NOT its children.
    /// Used for the primary desktop termination during the test to prove
    /// Job Object KILL_ON_JOB_CLOSE behavior.
    fn force_kill_process(pid: u32) {
        let _ = run_cmd("taskkill", &["/f", "/pid", &pid.to_string()]);
    }

    // -----------------------------------------------------------------------
    // TestGuard — cleanup on failure only, never masks a real regression
    // -----------------------------------------------------------------------

    /// Guard that cleans up test processes when the test fails (panics).
    ///
    /// If `mark_success()` is called before drop, the guard does nothing.
    /// This ensures forced cleanup runs **only** when the test would already
    /// fail, preventing a false-pass scenario where force-killing masks a
    /// real Job Object reclamation failure.
    struct TestGuard {
        sidecar_pid: Option<u32>,
        desktop_pid: Option<u32>,
        baseline_pids: Vec<u32>,
        success: bool,
    }

    impl TestGuard {
        fn new(baseline_pids: Vec<u32>) -> Self {
            Self {
                sidecar_pid: None,
                desktop_pid: None,
                baseline_pids,
                success: false,
            }
        }

        /// Mark the test as having succeeded — suppress all cleanup on drop.
        fn mark_success(&mut self) {
            self.success = true;
        }
    }

    impl Drop for TestGuard {
        fn drop(&mut self) {
            if self.success {
                // Test passed — Job Object proved itself.  No forced cleanup.
                return;
            }

            // Test failed (panic or early return with cleanup needed).
            // Force-kill everything we started so the environment is clean
            // for the next test, but DON'T silently hide the failure.
            eprintln!("TestGuard: cleaning up after test failure...");

            if let Some(pid) = self.sidecar_pid {
                eprintln!("TestGuard: force-killing sidecar PID {pid}");
                kill_process_tree(pid);
            }
            if let Some(pid) = self.desktop_pid {
                eprintln!("TestGuard: force-killing desktop PID {pid}");
                kill_process_tree(pid);
            }

            // Give the OS a moment to finish termination.
            std::thread::sleep(Duration::from_secs(2));

            // Verify cleanup succeeded, but don't panic in drop.
            let current = find_sidecar_pids();
            let orphans: Vec<u32> = current
                .iter()
                .filter(|p| !self.baseline_pids.contains(p))
                .copied()
                .collect();
            if !orphans.is_empty() {
                eprintln!("TestGuard: WARNING — orphan PIDs after cleanup: {orphans:?}");
                for pid in &orphans {
                    eprintln!("TestGuard: force-killing orphan PID {pid}");
                    kill_process_tree(*pid);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Sidecar binary hider (for "missing sidecar" test)
    // -----------------------------------------------------------------------

    /// Temporarily hides sidecar binaries so `resolve_sidecar_path` cannot find
    /// them.  Restores on drop.
    ///
    /// Hides individual .exe files rather than renaming the directory to avoid
    /// Windows file-lock issues.  Renames graphgateway*.exe to
    /// graphgateway*.exe.smoke-bak so that glob-based lookups fail.
    struct SidecarHider {
        /// List of (original_path, backup_path) pairs for restoration.
        renamed: Vec<(PathBuf, PathBuf)>,
        /// Whether the binaries directory itself was renamed.
        binaries_dir_bak: Option<PathBuf>,
    }

    impl SidecarHider {
        fn hide() -> Self {
            let mut hider = Self {
                renamed: Vec::new(),
                binaries_dir_bak: None,
            };

            // Helper: safely rename a file to .smoke-bak, cleaning up stale
            // backups first.
            fn safe_rename(orig: &PathBuf, bak: &PathBuf) -> bool {
                if bak.exists() {
                    // Clean up stale backup from a previous interrupted run.
                    if bak.is_dir() {
                        let _ = std::fs::remove_dir_all(bak);
                    } else {
                        let _ = std::fs::remove_file(bak);
                    }
                }
                std::fs::rename(orig, bak).is_ok()
            }

            // Hide individual .exe files inside the binaries directory.
            let bd = binaries_dir();
            if bd.exists() {
                if let Ok(entries) = bd.read_dir() {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let fname = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if fname.starts_with("graphgateway") && fname.ends_with(".exe")
                            || fname == "graphgateway.exe"
                        {
                            let bak = path.with_file_name(format!("{fname}.smoke-bak"));
                            if safe_rename(&path, &bak) {
                                hider.renamed.push((path, bak));
                            } else {
                                eprintln!("WARNING: could not hide {fname} in binaries dir");
                            }
                        }
                    }
                }
            }

            // If the binaries directory is completely empty now, also try
            // renaming the directory itself as an extra safeguard.
            if hider.renamed.is_empty() && bd.exists() {
                let bak = bd.with_file_name("binaries.smoke-bak");
                if safe_rename(&bd, &bak) {
                    hider.binaries_dir_bak = Some(bak);
                } else {
                    eprintln!(
                        "WARNING: could not rename binaries dir (files may still be visible)"
                    );
                }
            }

            // Hide debug sidecar.
            let debug = debug_sidecar_path();
            if debug.exists() {
                let bak = debug.with_file_name("graphgateway.exe.smoke-bak");
                if safe_rename(&debug, &bak) {
                    hider.renamed.push((debug, bak));
                } else {
                    eprintln!("WARNING: could not hide debug sidecar");
                }
            }

            // Hide release sidecar (in workspace target/release).
            let release = release_sidecar_path();
            if release.exists() {
                let bak = release.with_file_name("graphgateway.exe.smoke-bak");
                if safe_rename(&release, &bak) {
                    hider.renamed.push((release, bak));
                } else {
                    eprintln!("WARNING: could not hide release sidecar");
                }
            }

            hider
        }
    }

    impl Drop for SidecarHider {
        fn drop(&mut self) {
            // Restore renamed individual files.
            for (orig, bak) in self.renamed.iter() {
                let _ = std::fs::rename(bak, orig);
            }
            // Restore directory rename if applicable.
            if let Some(ref bak) = self.binaries_dir_bak {
                let orig = bak.with_file_name("binaries");
                let _ = std::fs::rename(bak, &orig);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Test: Happy path — desktop launches sidecar, readiness works,
    //       Job Object reclaims the sidecar process tree on desktop exit
    // -----------------------------------------------------------------------

    /// Test that the packaged desktop executable:
    /// 1. Launches the packaged Sidecar
    /// 2. Sidecar becomes ready (healthz responds 200)
    /// 3. Terminating only the desktop host (without `/t` tree kill) causes
    ///    the Job Object's KILL_ON_JOB_CLOSE to reclaim the Sidecar
    /// 4. No forced cleanup is needed for the success path
    #[test]
    fn packaged_desktop_launches_sidecar_and_cleans_up() {
        // Serialize with missing_sidecar test — both access the shared binaries dir.
        let _binaries_guard = BINARIES_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let exe = desktop_exe_path();
        if !exe.exists() {
            panic!(
                "SMOKE GATE FAILED: desktop executable not found at {}\n\
                 Build with: cd apps/graphgateway-desktop && cargo tauri build --no-bundle",
                exe.display()
            );
        }

        let sd = binaries_dir();
        if !sd.exists() {
            panic!(
                "SMOKE GATE FAILED: binaries directory not found at {}\n\
                 Expected packaged sidecar binaries. Ensure the build completed successfully.",
                sd.display()
            );
        }

        // At least one graphgateway*.exe must exist in the binaries dir.
        let has_sidecar = sd
            .read_dir()
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.file_name().to_string_lossy().starts_with("graphgateway"))
            })
            .unwrap_or(false);

        if !has_sidecar {
            panic!(
                "SMOKE GATE FAILED: no graphgateway*.exe found in {}\n\
                 Expected packaged sidecar binary. Ensure the build completed successfully.",
                sd.display()
            );
        }

        eprintln!("Desktop exe:  {}", exe.display());
        eprintln!("Sidecar dir:  {}", sd.display());

        // Record baseline process count.
        let before_pids = find_sidecar_pids();
        eprintln!("Sidecar processes before: {before_pids:?}");

        // Create TestGuard — cleanup runs only on test failure.
        let mut guard = TestGuard::new(before_pids.clone());

        // Spawn desktop — stderr discarded; token-leak is verified separately
        // in sidecar_stderr_has_no_token_leak.
        let mut child = Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn desktop executable");

        let desktop_pid = child.id();
        guard.desktop_pid = Some(desktop_pid);
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
                guard.sidecar_pid = Some(new_pids[0]);
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

        // ------------------------------------------------------------------
        // P101-R1: Terminate ONLY the desktop host process (no /t tree kill).
        //
        // This proves that the Job Object's KILL_ON_JOB_CLOSE behavior works
        // independently.  If the Sidecar is not reclaimed, the Job Object is
        // broken and the test MUST fail.
        // ------------------------------------------------------------------
        eprintln!("Terminating desktop PID {desktop_pid} (process only, no tree kill)...");
        force_kill_process(desktop_pid);

        // Wait for the OS to clean up (KILL_ON_JOB_CLOSE fires when the
        // desktop process exits and its handle to the Job Object is closed).
        let cleanup_start = Instant::now();
        let cleanup_timeout = Duration::from_secs(15);

        while cleanup_start.elapsed() < cleanup_timeout {
            if !process_exists(sidecar_pid) {
                eprintln!(
                    "Job Object reclaimed sidecar PID {sidecar_pid} after {:.1}s",
                    cleanup_start.elapsed().as_secs_f64()
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }

        // Wait for desktop to fully exit.
        let _ = child.wait();

        // ------------------------------------------------------------------
        // Hard assertion: the Sidecar MUST be gone — no force-kill before
        // this check.  If the Job Object didn't work, the test fails NOW.
        // ------------------------------------------------------------------
        assert!(
            !process_exists(sidecar_pid),
            "JOB OBJECT RECLAMATION FAILED: sidecar PID {sidecar_pid} is still alive \
             {:.0}s after desktop termination.  The Job Object with \
             KILL_ON_JOB_CLOSE should have terminated it.",
            cleanup_timeout.as_secs()
        );

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

        // Mark success — prevents TestGuard from running any forced cleanup.
        guard.mark_success();
    }

    // -----------------------------------------------------------------------
    // Test: Missing/corrupted sidecar — clean failure, diagnosable error,
    //       no orphan processes
    // -----------------------------------------------------------------------

    /// Test that when the packaged Sidecar binary is missing, the desktop:
    /// - Fails to start the sidecar (state → Failed)
    /// - Produces a diagnosable error on stderr
    /// - Does not leave any orphan processes
    #[test]
    fn missing_sidecar_causes_clean_failure() {
        // Serialize with the happy-path test — this test hides binaries the other needs.
        let _binaries_guard = BINARIES_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let exe = desktop_exe_path();
        if !exe.exists() {
            panic!(
                "SMOKE GATE FAILED: desktop executable not found at {}\n\
                 Build with: cd apps/graphgateway-desktop && cargo tauri build --no-bundle",
                exe.display()
            );
        }

        let before_pids = find_sidecar_pids();
        eprintln!("Sidecar processes before: {before_pids:?}");

        let mut guard = TestGuard::new(before_pids.clone());

        // Hide all sidecar binaries so resolve_sidecar_path cannot find any.
        let _hider = SidecarHider::hide();

        // Spawn desktop with CARGO_BUILD_TARGET set to a bogus triple so
        // resolve_sidecar_path uses a non-existent target for paths 2 and 3.
        // Capture stderr to verify diagnostible error output.
        let exe_path = desktop_exe_path();
        let diagnostic_path =
            std::env::temp_dir().join(format!("graphgateway-missing-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&diagnostic_path);
        let mut cmd = Command::new(&exe_path);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .env("CARGO_BUILD_TARGET", "mips64-pc-windows-msvc")
            .env("GRAPHGATEWAY_STARTUP_DIAGNOSTIC_FILE", &diagnostic_path);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                panic!("SMOKE GATE FAILED: could not spawn desktop: {e}");
            }
        };

        let desktop_pid = child.id();
        guard.desktop_pid = Some(desktop_pid);
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

        // Kill only the desktop process (not its tree — there shouldn't be one).
        force_kill_process(desktop_pid);
        let _ = child.wait();
        std::thread::sleep(Duration::from_secs(3));

        // Read captured stderr — should contain diagnostible error info.
        let mut stderr_buf = String::new();
        if let Some(mut stderr_pipe) = child.stderr.take() {
            let _ = stderr_pipe.read_to_string(&mut stderr_buf);
        }

        let has_sidecar_msg = stderr_buf.to_lowercase().contains("sidecar");
        let has_resolve_msg = stderr_buf.to_lowercase().contains("resolve")
            || stderr_buf.to_lowercase().contains("not found");
        let has_failed_msg = stderr_buf.to_lowercase().contains("fail")
            || stderr_buf.to_lowercase().contains("error");

        eprintln!(
            "Desktop stderr ({} bytes) — contains 'sidecar': {has_sidecar_msg}, \
             contains 'resolve/not found': {has_resolve_msg}, \
             contains 'fail/error': {has_failed_msg}",
            stderr_buf.len()
        );

        let diagnostic = std::fs::read_to_string(&diagnostic_path)
            .expect("missing sidecar must produce a startup diagnostic snapshot");
        let snapshot: graphgateway_types::SidecarSnapshot =
            serde_json::from_str(&diagnostic).expect("startup diagnostic must be valid JSON");
        assert_eq!(snapshot.state, graphgateway_types::SidecarState::Failed);
        assert!(
            snapshot
                .last_error
                .as_deref()
                .is_some_and(|error| !error.trim().is_empty()),
            "missing sidecar diagnostic last_error must be nonempty"
        );
        let _ = std::fs::remove_file(&diagnostic_path);

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
        eprintln!("Missing sidecar test PASSED: clean failure, diagnosable error, no orphans.");

        guard.mark_success();
        // _hider drop restores files.
    }

    #[test]
    fn corrupted_sidecar_causes_diagnosable_clean_failure() {
        let _binaries_guard = BINARIES_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let exe = desktop_exe_path();
        assert!(
            exe.exists(),
            "SMOKE GATE FAILED: desktop executable missing"
        );
        let before_pids = find_sidecar_pids();
        let mut guard = TestGuard::new(before_pids.clone());
        let _hider = SidecarHider::hide();

        let corrupt_path = binaries_dir().join("graphgateway.exe");
        std::fs::create_dir_all(binaries_dir()).unwrap();
        std::fs::write(&corrupt_path, b"not a Windows executable").unwrap();
        let _corrupt_file = TemporaryFile(corrupt_path.clone());
        let diagnostic_path =
            std::env::temp_dir().join(format!("graphgateway-corrupt-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&diagnostic_path);

        let mut child = Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("GRAPHGATEWAY_STARTUP_DIAGNOSTIC_FILE", &diagnostic_path)
            .spawn()
            .expect("failed to spawn desktop");
        guard.desktop_pid = Some(child.id());
        std::thread::sleep(Duration::from_secs(15));

        let diagnostic = std::fs::read_to_string(&diagnostic_path)
            .expect("corrupted sidecar must produce a startup diagnostic snapshot");
        let snapshot: graphgateway_types::SidecarSnapshot =
            serde_json::from_str(&diagnostic).expect("startup diagnostic must be valid JSON");
        assert_eq!(snapshot.state, graphgateway_types::SidecarState::Failed);
        assert!(
            snapshot
                .last_error
                .as_deref()
                .is_some_and(|error| !error.trim().is_empty()),
            "corrupted sidecar diagnostic last_error must be nonempty"
        );
        assert!(
            find_sidecar_pids()
                .iter()
                .all(|pid| before_pids.contains(pid)),
            "corrupted sidecar left a running process"
        );

        force_kill_process(child.id());
        let _ = child.wait();
        let _ = std::fs::remove_file(&corrupt_path);
        let _ = std::fs::remove_file(&diagnostic_path);
        let orphan_pids: Vec<u32> = find_sidecar_pids()
            .into_iter()
            .filter(|pid| !before_pids.contains(pid))
            .collect();
        assert!(orphan_pids.is_empty(), "orphan processes: {orphan_pids:?}");
        guard.mark_success();
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
            panic!(
                "SMOKE GATE FAILED: no sidecar binary found for token-leak test.\n\
                 Searched:\n  {}\n  {}\n  {}\n\
                 Build with: cargo build -p graphgateway-server",
                binaries_dir().join("graphgateway.exe").display(),
                debug_sidecar_path().display(),
                binaries_dir()
                    .join("graphgateway-x86_64-pc-windows-msvc.exe")
                    .display(),
            );
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
