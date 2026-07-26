#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { arch } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = join(scriptDir, "..", "..", "..");

export function parseTarget(argv) {
  const index = argv.indexOf("--target");
  if (index === -1) return null;
  if (!argv[index + 1]) throw new Error("--target requires a target triple");
  return argv[index + 1];
}

export function rustcHost(run = execFileSync) {
  const output = run("rustc", ["-vV"], { encoding: "utf8" });
  const line = output.split(/\r?\n/).find((value) => value.startsWith("host:"));
  if (!line) throw new Error("rustc -vV did not report a host triple");
  return line.slice(5).trim();
}

export function resolveTarget(cliTarget, env = process.env, hostResolver = rustcHost) {
  return cliTarget || env.TAURI_ENV_TARGET_TRIPLE || env.CARGO_BUILD_TARGET ||
    hostResolver() ||
    (arch() === "arm64" ? "aarch64-pc-windows-msvc" : "x86_64-pc-windows-msvc");
}

export function buildPlan(target, node = process.execPath) {
  return [
    {
      file: "cargo",
      args: ["build", "-p", "graphgateway-server", "--release", "--target", target],
      cwd: workspaceRoot,
    },
    {
      file: node,
      args: [join(scriptDir, "copy-sidecar.mjs"), "--target", target],
      cwd: join(workspaceRoot, "apps", "graphgateway-desktop"),
    },
  ];
}

export function runBuild(target, run = execFileSync) {
  for (const command of buildPlan(target)) {
    console.log(`> ${command.file} ${command.args.join(" ")}`);
    run(command.file, command.args, { cwd: command.cwd, stdio: "inherit" });
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const target = resolveTarget(parseTarget(process.argv.slice(2)));
  console.log(`Building and copying sidecar for ${target}`);
  runBuild(target);
}
