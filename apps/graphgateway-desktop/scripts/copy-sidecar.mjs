#!/usr/bin/env node
/**
 * Copies the compiled graphgateway-server binary from the workspace target
 * directory into src-tauri/binaries/ with the correct target-triple name.
 *
 * Usage: node scripts/copy-sidecar.mjs [--target <triple>] [--dry-run]
 *
 * The effective target triple is resolved in order of priority:
 *   1. --target CLI argument (highest priority, explicit manual override)
 *   2. TAURI_ENV_TARGET_TRIPLE environment variable (set by Tauri CLI during
 *      `beforeBuildCommand` — reflects the actual build target)
 *   3. CARGO_BUILD_TARGET environment variable
 *   4. `rustc -vV` host target
 *   5. Node.js host arch fallback (lowest priority)
 *
 * When the effective target differs from the host triple, the script reads
 * ONLY from `target/<triple>/release/graphgateway.exe` and NEVER falls back
 * to `target/release/graphgateway.exe`.  This prevents mislabeling a
 * host-architecture binary as a cross-compiled target.
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
 *   2. TAURI_ENV_TARGET_TRIPLE environment variable (set by Tauri CLI)
 *   3. CARGO_BUILD_TARGET environment variable
 *   4. `rustc -vV` host field
 *   5. Node.js process architecture (lowest priority)
 *
 * @param {string|null} cliTarget
 * @returns {{ targetTriple: string, hostTriple: string }}
 */
function resolveTargetTriple(cliTarget) {
  const hostTriple = rustcHostTriple() || nodeHostTriple();

  let targetTriple;

  // 1. CLI --target argument (highest priority)
  if (cliTarget) {
    targetTriple = cliTarget;
  }
  // 2. TAURI_ENV_TARGET_TRIPLE (set by Tauri CLI during beforeBuildCommand)
  else if (process.env.TAURI_ENV_TARGET_TRIPLE) {
    targetTriple = process.env.TAURI_ENV_TARGET_TRIPLE;
  }
  // 3. CARGO_BUILD_TARGET environment variable
  else if (process.env.CARGO_BUILD_TARGET) {
    targetTriple = process.env.CARGO_BUILD_TARGET;
  }
  // 4. rustc -vV host
  else if (rustcHostTriple()) {
    targetTriple = rustcHostTriple();
  }
  // 5. Node.js host arch fallback
  else {
    targetTriple = nodeHostTriple();
  }

  return { targetTriple, hostTriple };
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
function main() {
  const cliArgs = parseArgs(process.argv);
  const { targetTriple, hostTriple } = resolveTargetTriple(cliArgs.target);

  if (cliArgs.dryRun) {
    console.log(targetTriple);
    return;
  }

  const sidecarName = `graphgateway-${targetTriple}.exe`;
  const isCrossTarget = targetTriple !== hostTriple;

  // Source: Cargo places artifacts under target/<target-triple>/release/
  // when --target is used.  For a cross-compiled target we MUST NOT fall
  // back to the host default directory — that would relabel a host binary.
  const crossSrc = join(
    workspaceRoot,
    "target",
    targetTriple,
    "release",
    "graphgateway.exe"
  );
  const hostSrc = join(workspaceRoot, "target", "release", "graphgateway.exe");

  let src = crossSrc;
  if (!existsSync(crossSrc)) {
    if (!isCrossTarget && existsSync(hostSrc)) {
      // Host build without explicit --target may place artifact in the
      // default directory.  This is safe because the host binary matches
      // the host target.
      src = hostSrc;
    }
  }

  if (!existsSync(src)) {
    console.error(`ERROR: Sidecar binary not found.`);
    console.error(`  Expected at: ${crossSrc}`);
    if (isCrossTarget) {
      console.error(
        `  Cross-compilation target (${targetTriple}) != host (${hostTriple}).`
      );
      console.error(
        `  Refusing to fall back to host binary at ${hostSrc} — that would mislabel a ${hostTriple} binary as ${targetTriple}.`
      );
      console.error(
        `  Build the cross-compiled sidecar first:`
      );
      console.error(
        `    cargo build -p graphgateway-server --release --target ${targetTriple}`
      );
    } else {
      console.error(`  Also tried: ${hostSrc}`);
      console.error(
        "Build it first: cargo build -p graphgateway-server --release"
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
