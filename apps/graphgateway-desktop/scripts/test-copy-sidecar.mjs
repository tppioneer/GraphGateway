#!/usr/bin/env node
/**
 * Tests for copy-sidecar.mjs target triple resolution and fail-closed
 * artifact checking.
 *
 * This test exercises:
 * - --dry-run mode with different inputs to prove the target triple
 *   derivation logic works correctly for non-default targets.
 * - TAURI_ENV_TARGET_TRIPLE precedence (P101-R3).
 * - Fail-closed behavior: a missing target-specific artifact is rejected
 *   even when a host artifact exists (P101-R3).
 *
 * Usage: node scripts/test-copy-sidecar.mjs
 */

import { execSync } from "node:child_process";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { existsSync } from "node:fs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = join(__dirname, "..", "..", "..");
const copyScript = join(__dirname, "copy-sidecar.mjs");

let passed = 0;
let failed = 0;

function runDryRun(envOverrides, cliTarget = null) {
  const env = { ...process.env };
  if (envOverrides) {
    for (const [key, value] of Object.entries(envOverrides)) {
      if (value === null || value === undefined) {
        delete env[key];
      } else {
        env[key] = value;
      }
    }
  }
  const args = [copyScript, "--dry-run"];
  if (cliTarget) {
    args.push("--target", cliTarget);
  }
  try {
    const out = execSync(`node ${args.join(" ")}`, {
      encoding: "utf-8",
      env,
      timeout: 10_000,
    });
    return out.trim();
  } catch (e) {
    console.error(`SCRIPT ERROR: ${e.message}`);
    if (e.stdout) console.error(`stdout: ${e.stdout.toString()}`);
    if (e.stderr) console.error(`stderr: ${e.stderr.toString()}`);
    return null;
  }
}

/**
 * Run copy-sidecar.mjs WITHOUT --dry-run and return { code, stdout, stderr }.
 * Used for fail-closed artifact tests.
 */
function runReal(envOverrides, cliTarget = null) {
  const env = { ...process.env };
  if (envOverrides) {
    for (const [key, value] of Object.entries(envOverrides)) {
      if (value === null || value === undefined) {
        delete env[key];
      } else {
        env[key] = value;
      }
    }
  }
  const args = [copyScript];
  if (cliTarget) {
    args.push("--target", cliTarget);
  }
  try {
    const out = execSync(`node ${args.join(" ")}`, {
      encoding: "utf-8",
      env,
      timeout: 10_000,
    });
    return { code: 0, stdout: out, stderr: "" };
  } catch (e) {
    return {
      code: e.status || 1,
      stdout: e.stdout ? e.stdout.toString() : "",
      stderr: e.stderr ? e.stderr.toString() : "",
    };
  }
}

function check(name, actual, expected) {
  if (actual === expected) {
    console.log(`  PASS: ${name} => "${actual}"`);
    passed++;
  } else {
    console.log(`  FAIL: ${name}`);
    console.log(`    expected: "${expected}"`);
    console.log(`    actual:   "${actual}"`);
    failed++;
  }
}

function check_contains(name, actual, substr) {
  if (actual && actual.includes(substr)) {
    console.log(`  PASS: ${name} => contains "${substr}"`);
    passed++;
  } else {
    console.log(`  FAIL: ${name}`);
    console.log(`    expected to contain: "${substr}"`);
    console.log(`    actual: "${actual}"`);
    failed++;
  }
}

function check_exit(name, actualCode, expectedCode, stderr) {
  if (actualCode === expectedCode) {
    console.log(`  PASS: ${name} => exit code ${actualCode}`);
    passed++;
  } else {
    console.log(`  FAIL: ${name}`);
    console.log(`    expected exit code: ${expectedCode}`);
    console.log(`    actual exit code:   ${actualCode}`);
    if (stderr) console.log(`    stderr: ${stderr}`);
    failed++;
  }
}

// ---------------------------------------------------------------------------
// Test 1: Default detection (should produce a valid Windows triple)
// ---------------------------------------------------------------------------
console.log("\nTest 1: Default detection");
const defaultTriple = runDryRun({ CARGO_BUILD_TARGET: null, TAURI_ENV_TARGET_TRIPLE: null });
check_contains("default triple contains 'windows'", defaultTriple, "windows");
check_contains("default triple contains 'pc'", defaultTriple, "pc");

// ---------------------------------------------------------------------------
// Test 2: CLI --target flag (highest priority)
// ---------------------------------------------------------------------------
console.log("\nTest 2: CLI --target flag (highest priority)");
// Even with both TAURI_ENV_TARGET_TRIPLE and CARGO_BUILD_TARGET set,
// CLI flag wins.
const cliOverride = runDryRun(
  {
    TAURI_ENV_TARGET_TRIPLE: "aarch64-pc-windows-msvc",
    CARGO_BUILD_TARGET: "i686-pc-windows-msvc",
  },
  "x86_64-pc-windows-msvc"
);
check("CLI --target overrides TAURI_ENV_TARGET_TRIPLE", cliOverride, "x86_64-pc-windows-msvc");

// ---------------------------------------------------------------------------
// Test 3: TAURI_ENV_TARGET_TRIPLE (P101-R3 — Tauri build pipeline)
// ---------------------------------------------------------------------------
console.log("\nTest 3: TAURI_ENV_TARGET_TRIPLE (Tauri build pipeline)");
// TAURI_ENV_TARGET_TRIPLE takes priority over CARGO_BUILD_TARGET.
const tauriTriple = runDryRun({
  TAURI_ENV_TARGET_TRIPLE: "aarch64-pc-windows-msvc",
  CARGO_BUILD_TARGET: "i686-pc-windows-msvc",
});
check(
  "TAURI_ENV_TARGET_TRIPLE overrides CARGO_BUILD_TARGET",
  tauriTriple,
  "aarch64-pc-windows-msvc"
);

// TAURI_ENV_TARGET_TRIPLE alone (no CARGO_BUILD_TARGET).
const tauriTripleOnly = runDryRun({
  TAURI_ENV_TARGET_TRIPLE: "i686-pc-windows-msvc",
  CARGO_BUILD_TARGET: null,
});
check("TAURI_ENV_TARGET_TRIPLE honored without CARGO_BUILD_TARGET", tauriTripleOnly, "i686-pc-windows-msvc");

// ---------------------------------------------------------------------------
// Test 4: CARGO_BUILD_TARGET env var (falls back when no TAURI_ENV_TARGET_TRIPLE)
// ---------------------------------------------------------------------------
console.log("\nTest 4: CARGO_BUILD_TARGET env var");
const envTriple = runDryRun({
  TAURI_ENV_TARGET_TRIPLE: null,
  CARGO_BUILD_TARGET: "aarch64-pc-windows-msvc",
});
check("CARGO_BUILD_TARGET is honored", envTriple, "aarch64-pc-windows-msvc");

// Custom non-host target
const customTriple = runDryRun({
  TAURI_ENV_TARGET_TRIPLE: null,
  CARGO_BUILD_TARGET: "i686-pc-windows-msvc",
});
check("custom CARGO_BUILD_TARGET honored", customTriple, "i686-pc-windows-msvc");

// ---------------------------------------------------------------------------
// Test 5: Empty env vars fall back to host detection
// ---------------------------------------------------------------------------
console.log("\nTest 5: Empty env vars fall back to host detection");
const emptyTauri = runDryRun({
  TAURI_ENV_TARGET_TRIPLE: "",
  CARGO_BUILD_TARGET: null,
});
check_contains("empty TAURI_ENV_TARGET_TRIPLE falls back", emptyTauri, "windows");

const emptyCargo = runDryRun({
  TAURI_ENV_TARGET_TRIPLE: null,
  CARGO_BUILD_TARGET: "",
});
check_contains("empty CARGO_BUILD_TARGET falls back", emptyCargo, "windows");

// ---------------------------------------------------------------------------
// Test 6: TAURI_ENV_TARGET_TRIPLE empty but CARGO_BUILD_TARGET set
// ---------------------------------------------------------------------------
console.log("\nTest 6: TAURI_ENV_TARGET_TRIPLE empty, CARGO_BUILD_TARGET set");
const tauriEmptyCargoSet = runDryRun({
  TAURI_ENV_TARGET_TRIPLE: "",
  CARGO_BUILD_TARGET: "aarch64-pc-windows-msvc",
});
check(
  "CARGO_BUILD_TARGET used when TAURI_ENV_TARGET_TRIPLE is empty",
  tauriEmptyCargoSet,
  "aarch64-pc-windows-msvc"
);

// ---------------------------------------------------------------------------
// Test 7: Fail-closed — missing cross-compiled artifact is rejected even
//          when host artifact exists (P101-R3)
// ---------------------------------------------------------------------------
console.log("\nTest 7: Fail-closed — missing cross-target artifact rejected");
const hostArtifactPath = join(workspaceRoot, "target", "release", "graphgateway.exe");
const hostArtifactExists = existsSync(hostArtifactPath);

if (!hostArtifactExists) {
  console.log(
    "  SKIP: host artifact does not exist at target/release/graphgateway.exe —\n" +
      "  cannot verify that fallback is rejected when host artifact exists.\n" +
      "  Build the sidecar first: cargo build -p graphgateway-server --release"
  );
  // This is a soft skip — the test is inconclusive without a host artifact.
} else {
  // Use a non-host target triple that definitely has no built artifact.
  const crossTarget = "aarch64-pc-windows-msvc";
  const crossArtifactPath = join(
    workspaceRoot,
    "target",
    crossTarget,
    "release",
    "graphgateway.exe"
  );
  const crossArtifactExists = existsSync(crossArtifactPath);

  if (crossArtifactExists) {
    console.log(
      `  SKIP: cross-compiled artifact unexpectedly exists at ${crossArtifactPath}.\n` +
        `  Cannot test fail-closed behavior when the artifact is present.`
    );
  } else {
    // Run the script for real (no --dry-run) with TAURI_ENV_TARGET_TRIPLE
    // set to the cross target.  The script MUST exit non-zero because the
    // target-specific artifact is missing, even though the host artifact
    // exists.
    const result = runReal({
      TAURI_ENV_TARGET_TRIPLE: crossTarget,
      CARGO_BUILD_TARGET: null,
    });

    check_exit(
      `cross-target (${crossTarget}) fails when artifact missing (host artifact exists)`,
      result.code,
      1,
      result.stderr
    );

    // Verify the error output explains WHY it failed (no fallback).
    check_contains(
      "error message mentions refusing fallback",
      result.stderr + result.stdout,
      "Refusing"
    );

    console.log(
      `  Host artifact at: ${hostArtifactPath} (exists: ${hostArtifactExists})`
    );
    console.log(
      `  Cross artifact at: ${crossArtifactPath} (exists: false)`
    );
  }
}

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------
console.log(`\n${"=".repeat(50)}`);
console.log(`Results: ${passed} passed, ${failed} failed`);
if (failed > 0) {
  process.exit(1);
}
console.log("All copy-sidecar target triple tests passed.");
