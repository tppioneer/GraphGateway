#!/usr/bin/env node
/**
 * Tests for copy-sidecar.mjs target triple resolution.
 *
 * This test exercises the --dry-run mode with different inputs to prove
 * the target triple derivation logic works correctly for non-default
 * targets, WITHOUT requiring those target binaries to be built or executed.
 *
 * Usage: node scripts/test-copy-sidecar.mjs
 */

import { execSync } from "node:child_process";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const copyScript = join(__dirname, "copy-sidecar.mjs");

let passed = 0;
let failed = 0;

function runDryRun(envOverride, cliTarget = null) {
  const env = { ...process.env };
  if (envOverride !== undefined) {
    if (envOverride === null) {
      delete env.CARGO_BUILD_TARGET;
    } else {
      env.CARGO_BUILD_TARGET = envOverride;
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

// ---------------------------------------------------------------------------
// Test 1: Default detection (should produce a valid Windows triple)
// ---------------------------------------------------------------------------
console.log("\nTest 1: Default detection");
const defaultTriple = runDryRun(null); // no CARGO_BUILD_TARGET
check_contains("default triple contains 'windows'", defaultTriple, "windows");
check_contains("default triple contains 'pc'", defaultTriple, "pc");

// ---------------------------------------------------------------------------
// Test 2: CLI --target flag (highest priority)
// ---------------------------------------------------------------------------
console.log("\nTest 2: CLI --target flag");
// Even with CARGO_BUILD_TARGET set to something else, CLI flag wins.
const cliOverride = runDryRun("aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc");
check("CLI --target overrides env", cliOverride, "x86_64-pc-windows-msvc");

// ---------------------------------------------------------------------------
// Test 3: CARGO_BUILD_TARGET env var
// ---------------------------------------------------------------------------
console.log("\nTest 3: CARGO_BUILD_TARGET env var");
const envTriple = runDryRun("aarch64-pc-windows-msvc");
check("CARGO_BUILD_TARGET is honored", envTriple, "aarch64-pc-windows-msvc");

// Custom non-host target
const customTriple = runDryRun("i686-pc-windows-msvc");
check("custom CARGO_BUILD_TARGET honored", customTriple, "i686-pc-windows-msvc");

// ---------------------------------------------------------------------------
// Test 4: Empty CARGO_BUILD_TARGET falls back
// ---------------------------------------------------------------------------
console.log("\nTest 4: Empty CARGO_BUILD_TARGET falls back");
const emptyTriple = runDryRun("");
check_contains("empty env var falls back", emptyTriple, "windows");

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------
console.log(`\n${"=".repeat(50)}`);
console.log(`Results: ${passed} passed, ${failed} failed`);
if (failed > 0) {
  process.exit(1);
}
console.log("All copy-sidecar target triple tests passed.");
