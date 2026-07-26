#!/usr/bin/env node
import assert from "node:assert/strict";
import { join } from "node:path";
import { resolveTargetTriple, targetArtifactPath } from "./copy-sidecar.mjs";

function withEnv(values, callback) {
  const saved = {};
  for (const [key, value] of Object.entries(values)) {
    saved[key] = process.env[key];
    if (value === null) delete process.env[key];
    else process.env[key] = value;
  }
  try {
    callback();
  } finally {
    for (const [key, value] of Object.entries(saved)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

withEnv({
  TAURI_ENV_TARGET_TRIPLE: "aarch64-pc-windows-msvc",
  CARGO_BUILD_TARGET: "i686-pc-windows-msvc",
}, () => {
  assert.equal(resolveTargetTriple(null).targetTriple, "aarch64-pc-windows-msvc");
  assert.equal(resolveTargetTriple("x86_64-pc-windows-msvc").targetTriple,
    "x86_64-pc-windows-msvc");
});

const fixture = join("C:", "sidecar-test-fixture");
const target = "aarch64-pc-windows-msvc";
assert.equal(
  targetArtifactPath(fixture, target),
  join(fixture, "target", target, "release", "graphgateway.exe")
);
assert.notEqual(
  targetArtifactPath(fixture, target),
  join(fixture, "target", "release", "graphgateway.exe")
);

console.log("PASS: target precedence is CLI > TAURI_ENV_TARGET_TRIPLE > CARGO_BUILD_TARGET");
console.log("PASS: copy source is deterministic exact-target-only; no host artifact fixture is used");
