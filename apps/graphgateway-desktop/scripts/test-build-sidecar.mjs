#!/usr/bin/env node
import assert from "node:assert/strict";
import { buildPlan, resolveTarget } from "./build-sidecar.mjs";

const nonHost = "aarch64-pc-windows-msvc";
const plan = buildPlan(nonHost, "node-test");
assert.deepEqual(plan[0].args, [
  "build", "-p", "graphgateway-server", "--release", "--target", nonHost,
]);
assert.deepEqual(plan[1].args.slice(-2), ["--target", nonHost]);
assert.equal(resolveTarget(null, {
  TAURI_ENV_TARGET_TRIPLE: nonHost,
  CARGO_BUILD_TARGET: "i686-pc-windows-msvc",
}), nonHost);
assert.equal(resolveTarget("i686-pc-windows-msvc", {
  TAURI_ENV_TARGET_TRIPLE: nonHost,
}), "i686-pc-windows-msvc");

console.log(`PASS: Cargo and copy receive identical non-host target ${nonHost}`);
console.log("PASS: CLI > TAURI_ENV_TARGET_TRIPLE > CARGO_BUILD_TARGET precedence");
