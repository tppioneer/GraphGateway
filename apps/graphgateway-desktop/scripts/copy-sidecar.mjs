#!/usr/bin/env node
/**
 * Copies the compiled graphgateway-server binary from the workspace target
 * directory into src-tauri/binaries/ with the correct target-triple name.
 *
 * Usage: node scripts/copy-sidecar.mjs [--target <triple>] [--dry-run]
 *
 * The effective target triple is resolved in order of priority:
 *   1. --target CLI argument
 *   2. CARGO_BUILD_TARGET environment variable
 *   3. `rustc -vV` host target
 *   4. Node.js host arch fallback (lowest priority)
 *
 * Must be run from apps/graphgateway-desktop/.
 */

import { copyFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync } from "node:child_process";
import { arch } from "node:os";

const __dirname = dirname(fileURLToPath(import.meta.url));

// Workspace root: 3 levels up from scripts/
const workspaceRoot = join(__dirname, "..", "..", "..");

// ---------------------------------------------------------------------------
// Parse CLI arguments
// ---------------------------------------------------------------------------
function parseArgs(argv) {
  const args = { target: null, dryRun: false };
  let i = 2; // skip node and script path
  while (i < argv.length) {
    switch (argv[i]) {
      case "--target":
        args.target = argv[i + 1];
        i += 2;
        break;
      case "--dry-run":
        args.dryRun = true;
        i += 1;
        break;
      default:
        console.error(`Unknown argument: ${argv[i]}`);
        process.exit(2);
    }
  }
  return args;
}

// ---------------------------------------------------------------------------
// Resolve the effective target triple
// ---------------------------------------------------------------------------

/**
 * @returns {string|null} host target triple from `rustc -vV` or null on failure.
 */
function rustcHostTriple() {
  try {
    const out = execSync("rustc -vV", { encoding: "utf-8", timeout: 10_000 });
    for (const line of out.split("\n")) {
      const [key, ...rest] = line.split(":");
      if (key.trim() === "host") {
        return rest.join(":").trim();
      }
    }
  } catch {
    // rustc not available — fall through
  }
  return null;
}

/**
 * @returns {string} target triple from the Node.js host architecture (last resort).
 */
function nodeHostTriple() {
  return arch() === "arm64" ? "aarch64-pc-windows-msvc" : "x86_64-pc-windows-msvc";
}

/**
 * Resolve one effective target triple from available build inputs.
 *
 * Priority order:
 *   1. Explicit --target CLI argument
 *   2. CARGO_BUILD_TARGET environment variable
 *   3. `rustc -vV` host field
 *   4. Node.js process architecture (lowest priority)
 *
 * @param {string|null} cliTarget
 * @returns {string}
 */
function resolveTargetTriple(cliTarget) {
  // 1. CLI --target argument (highest priority)
  if (cliTarget) {
    return cliTarget;
  }

  // 2. CARGO_BUILD_TARGET environment variable
  if (process.env.CARGO_BUILD_TARGET) {
    return process.env.CARGO_BUILD_TARGET;
  }

  // 3. rustc -vV host
  const rustcTarget = rustcHostTriple();
  if (rustcTarget) {
    return rustcTarget;
  }

  // 4. Node.js host arch fallback
  return nodeHostTriple();
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
function main() {
  const cliArgs = parseArgs(process.argv);
  const targetTriple = resolveTargetTriple(cliArgs.target);

  if (cliArgs.dryRun) {
    console.log(targetTriple);
    return;
  }

  const sidecarName = `graphgateway-${targetTriple}.exe`;

  // Source: when a non-default target is used, Cargo places artifacts under
  // target/<target-triple>/ rather than target/.  Detect which layout exists.
  let src = join(workspaceRoot, "target", targetTriple, "release", "graphgateway.exe");
  if (!existsSync(src)) {
    // Fall back to host-default target directory.
    src = join(workspaceRoot, "target", "release", "graphgateway.exe");
  }

  if (!existsSync(src)) {
    console.error(`ERROR: Sidecar binary not found.`);
    console.error(`  Tried: ${join(workspaceRoot, "target", targetTriple, "release", "graphgateway.exe")}`);
    console.error(`  Tried: ${join(workspaceRoot, "target", "release", "graphgateway.exe")}`);
    console.error(
      "Build it first: cargo build -p graphgateway-server --release"
    );
    if (targetTriple !== "x86_64-pc-windows-msvc") {
      console.error(
        `  For target ${targetTriple}, use: cargo build -p graphgateway-server --release --target ${targetTriple}`
      );
    }
    process.exit(1);
  }

  // Destination: src-tauri/binaries/graphgateway-{triple}.exe
  const destDir = join(__dirname, "..", "src-tauri", "binaries");
  const dest = join(destDir, sidecarName);
  const plainDest = join(destDir, "graphgateway.exe");

  mkdirSync(destDir, { recursive: true });

  console.log(`Copying sidecar (target triple: ${targetTriple}):`);
  console.log(`  from: ${src}`);
  console.log(`  to:   ${dest}`);
  copyFileSync(src, dest);

  console.log(`  also: ${plainDest}`);
  copyFileSync(src, plainDest);

  console.log(`Sidecar ready: ${sidecarName}`);
}

main();
