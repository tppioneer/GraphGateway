#!/usr/bin/env node
/**
 * Copies the compiled graphgateway-server binary from the workspace target
 * directory into src-tauri/binaries/ with the correct target-triple name.
 *
 * Usage: node scripts/copy-sidecar.mjs
 *
 * Must be run from apps/graphgateway-desktop/.
 */

import { copyFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { arch } from "node:os";

const __dirname = dirname(fileURLToPath(import.meta.url));

// Workspace root: 3 levels up from scripts/
const workspaceRoot = join(__dirname, "..", "..", "..");

// Detect target triple
const targetTriple =
  arch() === "arm64"
    ? "aarch64-pc-windows-msvc"
    : "x86_64-pc-windows-msvc";

const sidecarName = `graphgateway-${targetTriple}.exe`;

// Source: workspace target/release/graphgateway.exe
const src = join(workspaceRoot, "target", "release", "graphgateway.exe");

if (!existsSync(src)) {
  console.error(`ERROR: Sidecar binary not found at: ${src}`);
  console.error(
    "Build it first: cargo build -p graphgateway-server --release"
  );
  process.exit(1);
}

// Destination: src-tauri/binaries/graphgateway-{triple}.exe
const destDir = join(__dirname, "..", "src-tauri", "binaries");
const dest = join(destDir, sidecarName);
const plainDest = join(destDir, "graphgateway.exe");

mkdirSync(destDir, { recursive: true });

console.log(`Copying sidecar:`);
console.log(`  from: ${src}`);
console.log(`  to:   ${dest}`);
copyFileSync(src, dest);

console.log(`  also: ${plainDest}`);
copyFileSync(src, plainDest);

console.log(`Sidecar ready: ${sidecarName}`);
