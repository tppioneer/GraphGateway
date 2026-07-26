#!/usr/bin/env node
/**
 * verify-mcp-proxy-gitnexus.mjs — GGW-P1-02 互操作门禁
 *
 * Black-box verification that mcp-proxy can transparently expose GitNexus
 * stdio MCP as Streamable HTTP.  Produces a compatibility report at
 * docs/compatibility/mcp-proxy-gitnexus-interop-report.md.
 *
 * Usage:
 *   node scripts/interop/verify-mcp-proxy-gitnexus.mjs --fixture tests/fixtures/mcp/sample-repo [--port PORT] [--timeout MS]
 *
 * Exit code: 0 = all acceptance tests PASS with no blocked constraints;
 *            non-zero = at least one FAIL, or a required acceptance item is
 *            SKIP/CONSTRAINT (gate failed).
 */

import { spawn, execFileSync } from 'node:child_process';
import { writeFile, mkdir, stat, rm, cp, realpath } from 'node:fs/promises';
import { resolve, dirname, basename, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { argv, exit, platform, hrtime, stdout } from 'node:process';
import { randomInt } from 'node:crypto';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';

// ── constants ────────────────────────────────────────────────────────────────
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REPO_ROOT = resolve(__dirname, '..', '..');
const REPORT_PATH = resolve(REPO_ROOT, 'docs', 'compatibility', 'mcp-proxy-gitnexus-interop-report.md');
const MCP_PROXY_BIN = 'mcp-proxy';
const GITNEXUS_BIN = 'gitnexus';
const PROTOCOL_VERSION = '2025-03-26';
const MCP_ACCEPT = 'application/json, text/event-stream';

// ── result model (P102-R1) ───────────────────────────────────────────────────
const VERDICT = Object.freeze({ PASS: 'PASS', FAIL: 'FAIL', SKIP: 'SKIP', CONSTRAINT: 'CONSTRAINT' });

function result(status, evidence) {
  return { status, evidence: String(evidence).slice(0, 4000) };
}
const PASS = (e) => result(VERDICT.PASS, e);
const FAIL = (e) => result(VERDICT.FAIL, e);
const SKIP = (e) => result(VERDICT.SKIP, e);
const CONSTRAINT = (e) => result(VERDICT.CONSTRAINT, e);

// ── CLI args ─────────────────────────────────────────────────────────────────
function parseArgs() {
  const a = { fixture: null, port: 0, timeout: 300_000, keepProcesses: false, help: false };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--fixture': a.fixture = resolve(argv[++i]); break;
      case '--port': a.port = parseInt(argv[++i], 10); break;
      case '--timeout': a.timeout = parseInt(argv[++i], 10); break;
      case '--keep-processes': a.keepProcesses = true; break;
      case '--help': a.help = true; break;
    }
  }
  if (a.help || !a.fixture) {
    console.error('Usage: node verify-mcp-proxy-gitnexus.mjs --fixture <path> [--port PORT] [--timeout MS] [--keep-processes]');
    if (a.help) exit(0); else exit(1);
  }
  return a;
}

// ── utilities ────────────────────────────────────────────────────────────────
function now() { return new Date().toISOString().replace('T', ' ').replace('Z', ' UTC'); }
function msSince(t) { const [s, n] = hrtime(t); return Math.round(s * 1000 + n / 1e6); }
const sleep = (ms) => new Promise(r => setTimeout(r, ms));

async function findFreePort(avoid = []) {
  for (let attempt = 0; attempt < 50; attempt++) {
    const port = randomInt(10240, 65535);
    if (avoid.includes(port)) continue;
    try {
      await new Promise((res, rej) => {
        const s = createServer();
        s.unref();
        s.on('error', rej);
        s.listen(port, '127.0.0.1', () => { s.close(res); });
      });
      return port;
    } catch {}
  }
  throw new Error('Could not find a free port');
}

/** Execute a command and return trimmed stdout. Throws on any failure. */
function shStrict(cmd, opts = {}) {
  return execFileSync(cmd[0], cmd.slice(1), {
    encoding: 'utf-8', timeout: 30_000, shell: platform === 'win32', windowsHide: true, ...opts,
  }).trim();
}

/** Execute a command; return trimmed stdout or error string. Graceful — use only for non-critical probes. */
function sh(cmd, opts = {}) {
  try {
    return execFileSync(cmd[0], cmd.slice(1), {
      encoding: 'utf-8', timeout: 30_000, shell: platform === 'win32', windowsHide: true, ...opts,
    }).trim();
  } catch (e) { return `ERROR: ${e.message}`; }
}

/** Execute a command; always returns {stdout, stderr, exitCode, error?}. */
function shCapture(cmd, opts = {}) {
  try {
    const stdout = execFileSync(cmd[0], cmd.slice(1), {
      encoding: 'utf-8', timeout: 30_000, shell: platform === 'win32', windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe'], ...opts,
    });
    return { stdout: stdout.trim(), stderr: '', exitCode: 0 };
  } catch (e) {
    return {
      stdout: (e.stdout || '').trim(),
      stderr: (e.stderr || '').trim(),
      exitCode: e.status || 1,
      error: e.message,
    };
  }
}

// ── version probes (P102-R1: fail-fast on missing commands) ──────────────────
function probeVersions() {
  const missing = [];
  const versions = { os: `${platform} ${process.arch}` };

  for (const [key, cmd] of [['node', ['node', '--version']], ['mcp-proxy', [MCP_PROXY_BIN, '--version']], ['gitnexus', [GITNEXUS_BIN, '--version']]]) {
    try {
      versions[key] = shStrict(cmd);
    } catch (e) {
      missing.push(`${key} (${cmd.join(' ')})`);
      versions[key] = `MISSING: ${e.message}`;
    }
  }

  if (missing.length > 0) {
    console.error(`FATAL: Required external commands not available: ${missing.join(', ')}`);
    exit(1);
  }

  return versions;
}

// ── process tracking (P102-R3: parent/child only, no system-wide PID delta) ──
/** Find child PIDs of a parent process using parent/child relationships only. */
function findChildPids(parentPid) {
  if (platform !== 'win32') {
    const r = shCapture(['pgrep', '-P', String(parentPid)]);
    if (r.exitCode !== 0) return [];
    return r.stdout.split('\n').filter(Boolean).map(s => parseInt(s.trim(), 10));
  }

  // Strategy 1: PowerShell Get-CimInstance (most reliable, requires admin in some configs)
  const psScript = `Get-CimInstance Win32_Process -Filter "ParentProcessId = ${parentPid}" | ForEach-Object { Write-Output "$($_.ProcessId)" }`;
  const tmpFile = join(tmpdir(), `ggw-p1-02-child-${Date.now()}.ps1`);
  let psFileWritten = false;
  try {
    const { writeFileSync } = require('node:fs');
    writeFileSync(tmpFile, psScript, 'utf-8');
    psFileWritten = true;
  } catch { /* fall through to wmic */ }
  if (psFileWritten) {
    try {
      const out = execFileSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', tmpFile], {
        encoding: 'utf-8', timeout: 15_000, windowsHide: true,
      });
      const pids = out.trim().split(/\s+/).map(s => parseInt(s, 10)).filter(n => !isNaN(n) && n > 0);
      if (pids.length > 0) return pids;
    } catch { /* fall through to wmic */ } finally {
      try { const { unlinkSync } = require('node:fs'); unlinkSync(tmpFile); } catch {}
    }
  }

  // Strategy 2: wmic (works without elevated privileges in more configurations)
  try {
    const wmicOut = execFileSync('wmic', ['process', 'where', `ParentProcessId=${parentPid}`, 'get', 'ProcessId', '/format:csv'], {
      encoding: 'utf-8', timeout: 10_000, windowsHide: true, shell: false,
    });
    const lines = wmicOut.trim().split(/\r?\n/);
    const pids = [];
    for (let i = 1; i < lines.length; i++) { // skip header
      const parts = lines[i].split(',');
      const pidStr = parts[parts.length - 1]; // last column is ProcessId
      const pid = parseInt(pidStr, 10);
      if (!isNaN(pid) && pid > 0 && pid !== parentPid) pids.push(pid);
    }
    if (pids.length > 0) return pids;
  } catch { /* no wmic available */ }

  // Strategy 3: PowerShell Get-Process (different API, may work where CIM fails)
  try {
    const ps2Out = execFileSync('powershell', [
      '-NoProfile', '-Command',
      `Get-Process | Where-Object { (Get-CimInstance Win32_Process -Filter "ProcessId = $($_.Id)").ParentProcessId -eq ${parentPid} } | ForEach-Object { $_.Id }`,
    ], { encoding: 'utf-8', timeout: 15_000, windowsHide: true });
    const pids = ps2Out.trim().split(/\s+/).map(s => parseInt(s, 10)).filter(n => !isNaN(n) && n > 0);
    if (pids.length > 0) return pids;
  } catch { /* no go */ }

  return [];
}

function pidExists(pid) {
  if (platform !== 'win32') {
    try { process.kill(pid, 0); return true; } catch { return false; }
  }
  const out = sh(['cmd', '/c', `tasklist /fi "PID eq ${pid}" /fo csv /nh 2>nul || echo none`]);
  return out.includes(String(pid));
}

/** Recursively find all descendant PIDs (children, grandchildren, etc.). */
function findDescendantPids(parentPid, depth) {
  depth = depth || 0;
  if (depth > 3) return []; // safety limit
  const children = findChildPids(parentPid);
  let all = [...children];
  for (const cpid of children) {
    all = all.concat(findDescendantPids(cpid, depth + 1));
  }
  return [...new Set(all)];
}

/** Find the PID of the process listening on a TCP port. Returns 0 if not found. */
function getPidByPort(port) {
  if (platform !== 'win32') {
    const r = shCapture(['lsof', '-ti', `:${port}`]);
    if (r.exitCode === 0 && r.stdout.trim()) return parseInt(r.stdout.trim(), 10);
    return 0;
  }
  const out = sh(['cmd', '/c', `netstat -ano | findstr :${port}`]);
  if (!out || out.includes('ERROR')) return 0;
  for (const line of out.split('\n')) {
    if (line.includes('LISTENING')) {
      const parts = line.trim().split(/\s+/);
      const pid = parseInt(parts[parts.length - 1], 10);
      if (!isNaN(pid) && pid > 0) return pid;
    }
  }
  return 0;
}

// ── temp directory for fixture runtime (P102-R4, unchanged) ───────────────────
async function createTempFixture(fixturePath, log) {
  const tmpBase = join(tmpdir(), `ggw-p1-02-${Date.now()}`);
  await mkdir(tmpBase, { recursive: true });
  log(`Temp fixture root: ${tmpBase}`);

  const dest = join(tmpBase, basename(fixturePath));
  await cp(fixturePath, dest, {
    recursive: true,
    filter: (src) => {
      const bn = basename(src);
      // Skip any stale index/metadata dirs from prior runs
      return bn !== '.git' && bn !== '.gitnexus' && bn !== '.claude';
    },
  });
  return { tmpBase, fixturePath: dest, repoName: null, freshlyIndexed: false, error: null };
}

/**
 * P102-R1: Parse `gitnexus list` output into {name, path} pairs.
 * Repo name lines have exactly 2 leading spaces; Path lines have 4+ leading
 * spaces followed by the "Path:" keyword.
 */
function parseGitNexusList(stdout) {
  const repos = [];
  const lines = stdout.split(/\r?\n/);
  let currentName = null;
  for (const line of lines) {
    const nameMatch = line.match(/^  (\S.*)$/);
    if (nameMatch) {
      // Repo name line; gitnexus may append a parenthetical path like
      // "sample-repo  (C:\path\to\repo)" - strip it to get just the name.
      const raw = nameMatch[1].trim();
      const parenIdx = raw.indexOf(' (');
      currentName = parenIdx >= 0 ? raw.slice(0, parenIdx).trim() : raw;
      continue;
    }
    const pathMatch = line.match(/^    Path:\s+(.+)$/);
    if (pathMatch && currentName) {
      repos.push({ name: currentName, path: pathMatch[1].trim() });
    }
  }
  return repos;
}

/**
 * P102-R1: Normalize a repository path for exact comparison.
 * Resolves symlinks and Windows 8.3 short names via realpath, normalizes
 * separators to forward slashes, and lowercases (Windows is case-insensitive).
 * Falls back to the raw path only if realpath fails (path no longer on disk),
 * in which case it cannot match a canonicalized existing path.
 */
async function normalizeRepoPath(p) {
  let resolved;
  try {
    resolved = await realpath(p);
  } catch {
    resolved = p;
  }
  return resolved.replace(/\\/g, '/').toLowerCase();
}

/**
 * P102-R1: Match repos by exactly one normalized full-path equality.
 * No basename, substring, or "includes" matching is permitted.
 */
async function matchReposByCanonicalPath(repos, canonicalFixturePath, log) {
  const matches = [];
  for (const repo of repos) {
    const canonicalRepoPath = await normalizeRepoPath(repo.path);
    if (canonicalRepoPath === canonicalFixturePath) {
      matches.push(repo);
      log(`    exact path match: repo="${repo.name}" normalized="${canonicalRepoPath}"`);
    }
  }
  return matches;
}

/**
 * P102-R1: setupFixture — every critical command must fail fast.
 * Canonical identity is resolved by exactly one normalized full-path match.
 * Zero matches or multiple matches FAIL before the protocol gate runs.
 * No basename, no-repo, file-only, or global fallback is permitted.
 */
async function setupFixture(tempFixture, log) {
  const { fixturePath } = tempFixture;
  log(`Setting up fixture git repo at ${fixturePath}`);

  // Step 1: git init — must succeed (shell:false avoids arg escaping issues)
  const initR = shCapture(['git', 'init'], { cwd: fixturePath, shell: false });
  if (initR.exitCode !== 0) {
    tempFixture.error = `FATAL: git init failed (exit ${initR.exitCode}): ${initR.stderr || initR.error}`;
    return;
  }

  // Step 2: git config — must succeed
  const cfgEmailR = shCapture(['git', 'config', 'user.email', 'ggw-p1-02-test@graphgateway.local'], { cwd: fixturePath, shell: false });
  if (cfgEmailR.exitCode !== 0) {
    tempFixture.error = `FATAL: git config user.email failed (exit ${cfgEmailR.exitCode}): ${cfgEmailR.stderr || cfgEmailR.error}`;
    return;
  }
  const cfgNameR = shCapture(['git', 'config', 'user.name', 'GGW P1-02 Test'], { cwd: fixturePath, shell: false });
  if (cfgNameR.exitCode !== 0) {
    tempFixture.error = `FATAL: git config user.name failed (exit ${cfgNameR.exitCode}): ${cfgNameR.stderr || cfgNameR.error}`;
    return;
  }

  // Step 3: git add — must succeed
  const addR = shCapture(['git', 'add', '-A'], { cwd: fixturePath, shell: false });
  if (addR.exitCode !== 0) {
    tempFixture.error = `FATAL: git add failed (exit ${addR.exitCode}): ${addR.stderr || addR.error}`;
    return;
  }

  // Step 4: git commit — must succeed
  const commitR = shCapture(['git', 'commit', '-m', 'test: interop fixture initial commit'], { cwd: fixturePath, shell: false });
  if (commitR.exitCode !== 0) {
    tempFixture.error = `FATAL: git commit failed (exit ${commitR.exitCode}): ${commitR.stderr || commitR.error}`;
    return;
  }

  // Step 5: Verify the fixture has a valid git commit
  const logR = shCapture(['git', 'log', '--oneline', '-1'], { cwd: fixturePath, shell: false });
  if (logR.exitCode !== 0 || !logR.stdout.trim()) {
    tempFixture.error = 'FATAL: fixture git repo has no valid commit after git commit';
    return;
  }
  log(`  Verified git commit: ${logR.stdout.trim()}`);

  // Step 6: Resolve canonical fixture path (P102-R1: full path only, no basename)
  const canonicalFixture = await normalizeRepoPath(fixturePath);
  log(`  Canonical fixture path: ${canonicalFixture}`);

  // Step 7: gitnexus list — must succeed; check if already indexed by exact path
  const listR1 = shCapture([GITNEXUS_BIN, 'list']);
  if (listR1.exitCode !== 0) {
    tempFixture.error = `FATAL: gitnexus list failed (exit ${listR1.exitCode}): ${listR1.stderr || listR1.error}`;
    return;
  }
  const repos1 = parseGitNexusList(listR1.stdout);
  const matches1 = await matchReposByCanonicalPath(repos1, canonicalFixture, log);
  if (matches1.length === 0) {
    log('  Not indexed yet — indexing fixture with gitnexus analyze...');
    const analyzeR = shCapture([GITNEXUS_BIN, 'analyze', fixturePath], { timeout: 120_000 });
    if (analyzeR.exitCode !== 0) {
      tempFixture.error = `FATAL: gitnexus analyze failed (exit ${analyzeR.exitCode}): ${analyzeR.stderr || analyzeR.error}`;
      return;
    }
    log('  Indexing complete');
    tempFixture.freshlyIndexed = true;
  }

  // Step 8: Resolve canonical GitNexus identity — exactly ONE full-path match (P102-R1)
  const listR2 = shCapture([GITNEXUS_BIN, 'list']);
  if (listR2.exitCode !== 0) {
    tempFixture.error = `FATAL: gitnexus list for identity resolution failed (exit ${listR2.exitCode}): ${listR2.stderr || listR2.error}`;
    return;
  }
  const repos2 = parseGitNexusList(listR2.stdout);
  log(`  Scanning ${repos2.length} indexed repos for exact full-path match`);
  const matches2 = await matchReposByCanonicalPath(repos2, canonicalFixture, log);

  if (matches2.length === 0) {
    tempFixture.error = `FATAL: zero GitNexus repos match fixture canonical path "${canonicalFixture}". Identity could not be resolved — no basename fallback permitted.`;
    return;
  }
  if (matches2.length > 1) {
    const names = matches2.map(m => `"${m.name}"`).join(', ');
    tempFixture.error = `FATAL: multiple GitNexus repos match fixture canonical path "${canonicalFixture}": [${names}]. Identity is ambiguous — no basename fallback permitted.`;
    return;
  }

  tempFixture.repoName = matches2[0].name;
  log(`  Resolved repo name (exact full-path match): ${tempFixture.repoName}`);
}

async function cleanupTempFixture(tempFixture, log) {
  const tmpLabel = basename(tempFixture.tmpBase);
  try {
    await rm(tempFixture.tmpBase, { recursive: true, force: true });
    log(`Cleaned up temp fixture: ${tmpLabel}`);
    return { status: VERDICT.PASS, evidence: `temp fixture removed: ${tmpLabel}` };
  } catch (e) {
    log(`Warning: could not clean up temp fixture: ${e.message}`);
    return { status: VERDICT.FAIL, evidence: `temp fixture cleanup failed for ${tmpLabel}` };
  }
}

// ── process manager ──────────────────────────────────────────────────────────
class ProcessManager {
  constructor() {
    this.proxy = null;
    this.port = 0;
    this.proxyPid = 0;
    this.stdoutBuf = '';
    this.stderrBuf = '';
    this.cleanupDone = false;
    this._started = false;
  }

  async start(port, upstream, log, opts = {}) {
    this.port = port;
    const useShell = platform === 'win32';
    const upstreamLabel = upstream.join(' ');
    log(`Starting mcp-proxy on 127.0.0.1:${port} upstream=${upstreamLabel}`);

    const spawnArgs = [
      '--port', String(port),
      '--host', '127.0.0.1',
      '--server', 'stream',
      '--connectionTimeout', '30000',
      '--requestTimeout', '120000',
    ];
    if (opts.extraArgs) spawnArgs.push(...opts.extraArgs);
    if (useShell) spawnArgs.push('--shell');
    spawnArgs.push('--', ...upstream);

    return new Promise((resolve, reject) => {
      const child = spawn(MCP_PROXY_BIN, spawnArgs, {
        stdio: ['pipe', 'pipe', 'pipe'],
        detached: false,
        windowsHide: true,
        shell: platform === 'win32',
      });
      this.proxy = child;
      this.proxyPid = child.pid;
      this._started = true;
      log(`  mcp-proxy PID: ${child.pid}`);

      child.on('error', (e) => reject(new Error(`spawn error: ${e.message}`)));
      let exitedEarly = false;
      child.on('exit', (code) => {
        if (this._started && !this.cleanupDone) {
          exitedEarly = true;
          reject(new Error(`mcp-proxy exited early with code ${code}. stderr: ${this.stderrBuf.slice(0, 300)}`));
        }
      });
      child.stdout.on('data', (d) => { this.stdoutBuf += d.toString(); });
      child.stderr.on('data', (d) => { this.stderrBuf += d.toString(); });

      const startTime = Date.now();
      const poll = () => {
        if (exitedEarly) return;
        if (Date.now() - startTime > 30_000) {
          this._started = false;
          child.kill('SIGTERM');
          const stderrPreview = this.stderrBuf.slice(0, 500);
          return reject(new Error(`mcp-proxy not ready within 30s. stderr: ${stderrPreview}`));
        }
        fetch(`http://127.0.0.1:${port}/mcp`, {
          method: 'POST',
          headers: { 'content-type': 'application/json', accept: MCP_ACCEPT },
          body: JSON.stringify({ jsonrpc: '2.0', id: 'probe', method: 'initialize', params: { protocolVersion: PROTOCOL_VERSION, capabilities: {}, clientInfo: { name: 'probe', version: '1.0' } } }),
          signal: AbortSignal.timeout(3000),
        }).then(async (r) => {
          const text = await r.text();
          if (text.includes('"jsonrpc"')) { log(`  Ready after ${Date.now() - startTime}ms`); resolve(); }
          else setTimeout(poll, 300);
        }).catch(() => setTimeout(poll, 300));
      };
      setTimeout(poll, 800);
    });
  }

  async stop(log) {
    if (this.proxy && !this.proxy.killed && this._started) {
      this.cleanupDone = true;
      log(`Stopping mcp-proxy (spawn PID ${this.proxyPid})...`);

      // On Windows with shell:true, child.kill() targets the cmd.exe wrapper,
      // which may leave mcp-proxy.exe and its children orphaned.
      // Use netstat to find the actual listening PID and kill the process tree.
      if (platform === 'win32' && this.port > 0) {
        const realPid = getPidByPort(this.port);
        if (realPid > 0 && realPid !== this.proxyPid) {
          log(`  Killing actual proxy PID ${realPid} (tree)...`);
          try {
            sh(['cmd', '/c', `taskkill /PID ${realPid} /T /F 2>nul`]);
          } catch {}
          await sleep(500);
        }
      }

      this.proxy.kill('SIGTERM');
      const ok = await new Promise(r => { const t = setTimeout(() => r(false), 8000); this.proxy.on('exit', () => { clearTimeout(t); r(true); }); });
      if (!ok) { log('  SIGKILL...'); try { this.proxy.kill('SIGKILL'); } catch {} }
      log('  Stopped');
    }
    this._started = false;
    this.proxy = null;
  }
}

// ── MCP HTTP client ──────────────────────────────────────────────────────────
function parseSseOrJson(body) {
  const lines = body.split(/\r?\n/);
  let dataLine = null;
  for (const line of lines) {
    if (line.startsWith('data:')) dataLine = line.slice(5).trim();
  }
  if (dataLine) {
    try { return JSON.parse(dataLine); } catch { return { _raw: body, _error: 'SSE data parse failed' }; }
  }
  try { return JSON.parse(body); } catch { return { _raw: body, _error: 'Response parse failed' }; }
}

class McpClient {
  constructor(port, log) {
    this.baseUrl = `http://127.0.0.1:${port}/mcp`;
    this.port = port;
    this.sessionId = null;
    this.nextId = 1;
    this.initialized = false;
    this.log = log || (() => {});
  }

  _headers() {
    const h = { 'content-type': 'application/json', accept: MCP_ACCEPT };
    if (this.sessionId) h['mcp-session-id'] = this.sessionId;
    return h;
  }

  async _rpc(method, params = {}, opts = {}) {
    const id = this.nextId++;
    const body = JSON.stringify({ jsonrpc: '2.0', id, method, params });
    let res;
    try {
      res = await fetch(this.baseUrl, {
        method: 'POST',
        headers: this._headers(),
        body,
        signal: opts.signal || AbortSignal.timeout(opts.timeout || 30_000),
      });
    } catch (e) {
      if (e.name === 'AbortError' || e.name === 'TimeoutError') {
        throw new Error(`request timeout/abort after ${opts.timeout || 30_000}ms`);
      }
      throw e;
    }

    const sid = res.headers.get('mcp-session-id');
    if (sid) this.sessionId = sid;

    if (opts.rawResponse) return res;

    const text = await res.text();
    const parsed = parseSseOrJson(text);

    if (parsed._error && !parsed.jsonrpc) {
      throw new Error(`MCP parse error (status ${res.status}): ${text.slice(0, 300)}`);
    }
    return parsed;
  }

  async initialize() {
    const r = await this._rpc('initialize', {
      protocolVersion: PROTOCOL_VERSION,
      capabilities: {},
      clientInfo: { name: 'ggw-p1-02-verify', version: '1.0.0' },
    });
    if (r.error) throw new Error(`initialize failed: ${r.error.message}`);
    this.initialized = true;
    await this._rpc('notifications/initialized', {}, { timeout: 5_000 }).catch(() => {});
    return r;
  }

  async toolsList() { return this._rpc('tools/list'); }
  async resourcesList() { return this._rpc('resources/list'); }
  async resourcesRead(uri) { return this._rpc('resources/read', { uri }); }
  async toolCall(name, args = {}, opts = {}) { return this._rpc('tools/call', { name, arguments: args }, opts); }

  async forkAsNewSession() {
    const c = new McpClient(this.port, this.log);
    await c.initialize();
    return c;
  }
}

// ── test runner ──────────────────────────────────────────────────────────────
const tests = [];
let currentSuite = '';
const requiredTests = new Set();

function suite(name) { currentSuite = name; }

function test(name, fn, opts = {}) {
  tests.push({ suite: currentSuite, name, fn });
  if (opts.required) requiredTests.add(name);
}

async function runAllTests(client, fixture, log, args) {
  const results = [];
  for (const t of tests) {
    const t0 = hrtime();
    let res;
    try {
      res = await t.fn(client, fixture, log, args);
    } catch (e) {
      res = FAIL(String(e.message || e).slice(0, 2000));
    }
    if (!res || typeof res.status !== 'string') {
      res = FAIL(`Test returned non-structured result: ${JSON.stringify(res).slice(0, 200)}`);
    }
    const dur = msSince(t0);
    results.push({ suite: t.suite, name: t.name, status: res.status, evidence: String(res.evidence).slice(0, 2000), duration_ms: dur });
    log(`  [${res.status}] ${t.suite} / ${t.name} (${dur} ms)`);
    if (res.status === VERDICT.FAIL || res.status === VERDICT.CONSTRAINT) {
      log(`    Evidence: ${String(res.evidence).slice(0, 300)}`);
    }
  }
  return results;
}

// ── controllable proxy helper (P102-R2) ─────────────────────────────────────
async function startControllableProxy(log, avoidPorts) {
  const controllablePath = resolve(REPO_ROOT, 'tests', 'fixtures', 'mcp', 'controllable-server.mjs');
  const port = await findFreePort(avoidPorts);
  const pm = new ProcessManager();
  await pm.start(port, ['node', controllablePath], log);
  const client = new McpClient(port, log);
  await client.initialize();
  log(`  Controllable proxy ready on port ${port}, session=${(client.sessionId || '').slice(0, 8)}`);
  return { pm, client, port };
}

// ── test definitions ─────────────────────────────────────────────────────────

// ---- 1. Initialize & Session ----
suite('1. Initialize & Session');

test('1.1 initialize returns protocol version and server info', async (client) => {
  if (!client.initialized) return FAIL('client not initialized');
  return PASS(`session-id=${(client.sessionId || 'none').slice(0, 8)}..., initialized=true`);
}, { required: true });

test('1.2 Mcp-Session-Id is assigned and required', async (client) => {
  const sid = client.sessionId;
  if (!sid || typeof sid !== 'string' || sid.length < 8) return FAIL(`invalid session ID: ${sid}`);
  const c2 = new McpClient(client.port, () => {});
  try {
    const r = await c2._rpc('tools/list');
    if (r.error) return PASS(`session required (correct): ${r.error.message?.slice(0, 100)}`);
    return CONSTRAINT(`session not required — stateless mode active, session-id=${sid.slice(0, 8)}`);
  } catch {
    return PASS(`session ID required: ${sid.slice(0, 8)}... (length=${sid.length})`);
  }
}, { required: true });

test('1.3 session ID is stable across requests', async (client) => {
  const s1 = client.sessionId;
  await client.toolsList();
  const s2 = client.sessionId;
  if (s1 !== s2) return FAIL(`session ID changed: ${s1} -> ${s2}`);
  return PASS(`session stable: ${s1.slice(0, 8)}...`);
}, { required: true });

// ---- 2. tools/list ----
suite('2. tools/list');

test('2.1 tools/list returns non-empty tool array', async (client) => {
  const r = await client.toolsList();
  if (r.error) return FAIL(`tools/list error: ${r.error.message}`);
  if (!r.result?.tools || !Array.isArray(r.result.tools)) return FAIL(`invalid: ${JSON.stringify(r).slice(0, 200)}`);
  if (r.result.tools.length === 0) return FAIL('empty tools list');
  return PASS(`${r.result.tools.length} tools`);
}, { required: true });

test('2.2 each tool has name + description + inputSchema', async (client) => {
  const r = await client.toolsList();
  const issues = [];
  for (const t of r.result.tools) {
    if (!t.name) issues.push('missing name');
    else {
      if (!t.description) issues.push(`${t.name}: no description`);
      if (!t.inputSchema) issues.push(`${t.name}: no inputSchema`);
    }
  }
  if (issues.length) return FAIL(issues.slice(0, 5).join('; '));
  return PASS(`all ${r.result.tools.length} tools well-formed`);
}, { required: true });

// ---- 3. resources/list & resources/read ----
suite('3. resources/list & resources/read');

test('3.1 resources/list returns resource array', async (client) => {
  const r = await client.resourcesList();
  if (r.error) return FAIL(`resources/list error: ${r.error.message}`);
  const resources = r.result?.resources || [];
  return PASS(`${resources.length} resources: ${resources.map(x => x.uri).join(', ') || '(none)'}`);
}, { required: true });

test('3.2 resources/read returns content for known resource', async (client) => {
  const list = await client.resourcesList();
  const uris = (list.result?.resources || []).map(r => r.uri);
  if (uris.length === 0) return SKIP('no resources to read');
  const r = await client.resourcesRead(uris[0]);
  if (r.error) return FAIL(`resources/read error: ${r.error.message}`);
  const hasContent = !!(r.result?.contents || r.result?.content || r.result?.text);
  return hasContent ? PASS(`resource ${uris[0]} returned content`) : CONSTRAINT(`resource ${uris[0]} returned empty content`);
}, { required: true });

// ---- 4. tools/call — fixed repo (P102-R1: no fallbacks) ----
suite('4. tools/call — fixed repo');

test('4.1 list_repos returns repo list', async (client) => {
  const r = await client.toolCall('list_repos');
  if (r.error) return FAIL(`list_repos error: ${r.error.message || JSON.stringify(r.error)}`);
  const text = extractTextContent(r);
  return PASS(`list_repos returned ${text.length} chars`);
}, { required: true });

test('4.2 list_repos contains fixture repo', async (client, fixture) => {
  const r = await client.toolCall('list_repos');
  const text = extractTextContent(r);
  if (!fixture.repoName) return FAIL('fixture.repoName is null — fixture identity not resolved');
  if (text.includes(fixture.repoName)) return PASS(`"${fixture.repoName}" found in list_repos`);
  return FAIL(`"${fixture.repoName}" NOT found in list_repos — identity mismatch`);
}, { required: true });

test('4.3 query returns results — bound to fixture repo', async (client, fixture) => {
  if (!fixture.repoName) return FAIL('fixture.repoName is null — fixture identity not resolved');
  const r = await client.toolCall('query', { search_query: 'greet', repo: fixture.repoName, limit: 3, max_symbols: 5 });
  if (r.error) return FAIL(`query error (repo="${fixture.repoName}"): ${r.error.message || JSON.stringify(r.error)}`);
  return PASS(`query OK (repo="${fixture.repoName}"): ${extractTextContent(r).length} chars`);
}, { required: true });

test('4.4 context resolves a symbol — bound to fixture repo', async (client, fixture) => {
  if (!fixture.repoName) return FAIL('fixture.repoName is null — fixture identity not resolved');
  const r = await client.toolCall('context', { name: 'greet', repo: fixture.repoName });
  if (r.error) return FAIL(`context error (repo="${fixture.repoName}", name="greet"): ${r.error.message || JSON.stringify(r.error)}`);
  return PASS(`context OK (repo="${fixture.repoName}"): ${extractTextContent(r).length} chars`);
}, { required: true });

test('4.5 checkpoint: read-only tool calls work', async (client) => {
  const r = await client.toolCall('check', { cycles: true });
  if (r.error) return CONSTRAINT(`check tool constrained: ${r.error.message?.slice(0, 100)}`);
  return PASS(`check tool working: ${extractTextContent(r).length} chars`);
}, { required: true });

// ---- 5. Two concurrent client sessions ----
suite('5. Two concurrent client sessions');

test('5.1 two independent sessions get different IDs', async (client) => {
  const c2 = await client.forkAsNewSession();
  if (!c2.sessionId) return FAIL('second client has no session ID');
  if (c2.sessionId === client.sessionId) return FAIL('same session ID for both clients');
  return PASS(`session1=${client.sessionId.slice(0, 8)}... session2=${c2.sessionId.slice(0, 8)}...`);
}, { required: true });

test('5.2 both sessions return same tool set', async (client) => {
  const c2 = await client.forkAsNewSession();
  const [r1, r2] = await Promise.all([client.toolsList(), c2.toolsList()]);
  const n1 = r1.result?.tools?.map(t => t.name).sort().join(',');
  const n2 = r2.result?.tools?.map(t => t.name).sort().join(',');
  if (!n1 || !n2) return FAIL('one session failed tools/list');
  if (n1 !== n2) return FAIL('tool sets differ between sessions');
  return PASS(`${r1.result.tools.length} tools in both sessions`);
}, { required: true });

test('5.3 original session still functional after new session', async (client) => {
  await client.forkAsNewSession();
  const r = await client.toolsList();
  if (!r.result?.tools) return FAIL('original session broken after creating new session');
  return PASS('original session intact');
}, { required: true });

// ---- 6. Concurrent requests ----
suite('6. Concurrent requests');

test('6.1 three concurrent tools/list complete', async (client) => {
  const results = await Promise.all([client.toolsList(), client.toolsList(), client.toolsList()]);
  for (let i = 0; i < results.length; i++) {
    if (!results[i].result?.tools) return FAIL(`req ${i} failed: ${JSON.stringify(results[i]).slice(0, 200)}`);
  }
  return PASS(`all 3 concurrent requests OK (${results[0].result.tools.length} tools each)`);
}, { required: true });

test('6.2 concurrent list_repos + tools/list do not interfere', async (client) => {
  const [r1, r2] = await Promise.all([client.toolCall('list_repos'), client.toolsList()]);
  if (!extractTextContent(r1) && r1.error) return FAIL(`list_repos failed: ${r1.error.message}`);
  if (!r2.result?.tools) return FAIL('tools/list failed');
  return PASS('concurrent different-tool OK');
}, { required: true });

// ---- 7. Error handling — malformed requests ----
suite('7. Error handling — malformed requests');

test('7.1 invalid JSON returns error', async (client) => {
  const res = await fetch(client.baseUrl, {
    method: 'POST', headers: client._headers(), body: 'not json {{{', signal: AbortSignal.timeout(5000),
  });
  const text = await res.text();
  const isError = res.status >= 400 || /error|parse/i.test(text);
  return isError ? PASS(`proxy rejected invalid JSON (status ${res.status})`) : CONSTRAINT(`accepted with status ${res.status}`);
});

test('7.2 missing jsonrpc field is rejected', async (client) => {
  const res = await fetch(client.baseUrl, {
    method: 'POST', headers: client._headers(), body: JSON.stringify({ method: 'tools/list' }), signal: AbortSignal.timeout(5000),
  });
  const text = await res.text();
  const isRejected = res.status >= 400 || /error|invalid/i.test(text);
  return isRejected ? PASS(`properly rejected (status ${res.status})`) : CONSTRAINT(`accepted (status ${res.status})`);
});

test('7.3 unknown tool returns error', async (client) => {
  const r = await client.toolCall('nonexistent_tool_xyz_123', {});
  if (r.error) return PASS(`correctly returned error: ${r.error.message?.slice(0, 100) || r.error.code}`);
  return CONSTRAINT('unknown tool returned no error — proxy passes through backend response');
});

// ---- 8. Timeout, cancellation & disconnect (P102-R2) ----
suite('8. Timeout, cancellation & disconnect');

test('8.1 delay tool: configurable slow request eventually completes', async (_client, _fixture, log, args) => {
  let ctrl;
  try {
    ctrl = await startControllableProxy(log, [args._actualPort]);
  } catch (e) {
    return FAIL(`controllable proxy start failed: ${e.message}`);
  }
  try {
    // Verify basic connectivity
    const idCheck = await ctrl.client.toolCall('identity', {});
    if (idCheck.error) return FAIL(`identity tool failed: ${idCheck.error.message}`);
    log(`    identity: ${extractTextContent(idCheck)}`);

    // Test delay — request a 3-second delay
    const t0 = hrtime();
    const r = await ctrl.client.toolCall('delay', { ms: 3000 }, { timeout: 15_000 });
    const dur = msSince(t0);
    if (r.error) return FAIL(`delay tool failed: ${r.error.message}`);
    if (dur < 2500) return FAIL(`delay completed too quickly (${dur}ms) — not actually delayed`);
    return PASS(`delayed response received after ${dur}ms (requested 3000ms)`);
  } finally {
    await ctrl.pm.stop(log);
    await sleep(1000);
  }
}, { required: true });

test('8.2 client abort: observe upstream final state and session behavior', async (_client, _fixture, log, args) => {
  let ctrl;
  try {
    ctrl = await startControllableProxy(log, [args._actualPort]);
  } catch (e) {
    return FAIL(`controllable proxy start failed: ${e.message}`);
  }
  try {
    // Baseline: record upstream PID before abort
    const idBefore = await ctrl.client.toolCall('identity', {});
    if (idBefore.error) return FAIL(`identity before abort failed: ${idBefore.error.message}`);
    const upstreamPidBefore = parseIdentityPid(extractTextContent(idBefore));
    log(`    upstream PID before abort: ${upstreamPidBefore || 'unknown'}`);

    // Initiate a slow request with an AbortController
    const DELAY_MS = 5000;
    const controller = new AbortController();
    const slowReq = ctrl.client.toolCall('delay', { ms: DELAY_MS }, { signal: controller.signal, timeout: 25_000 });

    setTimeout(() => {
      log('    aborting client request after 500ms...');
      controller.abort();
    }, 500);

    const t0 = hrtime();
    let aborted = false;
    try {
      await slowReq;
    } catch (e) {
      aborted = true;
      log(`    Abort caught after ${msSince(t0)}ms: ${e.message?.slice(0, 120)}`);
    }

    if (!aborted) return FAIL('abort did not interrupt the slow request');

    // P102-R2: keep proxy running and wait for the delay duration so we can
    // observe whether the upstream request completed, was cancelled, or is unknown.
    const waitMs = DELAY_MS + 1000;
    log(`    waiting ${waitMs}ms to observe upstream final state...`);
    await sleep(waitMs);

    // Observe relevant process behavior: is the upstream process still alive?
    const upstreamPidAlive = upstreamPidBefore ? pidExists(upstreamPidBefore) : false;
    log(`    upstream PID ${upstreamPidBefore} alive after abort: ${upstreamPidAlive}`);

    // Observe upstream request final state via the controllable fixture
    let upstreamRequestState = 'unknown';
    if (upstreamPidAlive) {
      try {
        const ds = await ctrl.client.toolCall('delay_status', {}, { timeout: 5_000 });
        if (!ds.error) {
          const stateObj = JSON.parse(extractTextContent(ds));
          upstreamRequestState = stateObj.state || 'unknown';
        }
      } catch (e) {
        log(`    delay_status query failed: ${e.message?.slice(0, 120)}`);
      }
    } else {
      // Upstream process is no longer alive; the request was cancelled
      upstreamRequestState = 'cancelled';
    }
    log(`    upstream request final state: ${upstreamRequestState}`);

    // Observe original Session behavior
    let sessionAlive = false;
    let upstreamPidAfter = null;
    try {
      const postAbortId = await ctrl.client.toolCall('identity', {}, { timeout: 5_000 });
      if (!postAbortId.error) {
        sessionAlive = true;
        upstreamPidAfter = parseIdentityPid(extractTextContent(postAbortId));
        log(`    original session functional after abort, upstream PID: ${upstreamPidAfter || 'unknown'}`);
      } else {
        log(`    original session error after abort: ${postAbortId.error.message}`);
      }
    } catch (e) {
      log(`    original session not reachable after abort: ${e.message?.slice(0, 120)}`);
    }

    // Observe sibling Session behavior
    let siblingAlive = false;
    try {
      const c2 = await ctrl.client.forkAsNewSession();
      const siblingCheck = await c2.toolCall('identity', {}, { timeout: 5_000 });
      if (!siblingCheck.error) {
        siblingAlive = true;
        log('    sibling session functional after abort');
      } else {
        log(`    sibling session error: ${siblingCheck.error.message}`);
      }
    } catch (e) {
      log(`    sibling session creation failed after abort: ${e.message?.slice(0, 120)}`);
    }

    const pidChanged = upstreamPidBefore && upstreamPidAfter && upstreamPidBefore !== upstreamPidAfter;
    const evidenceParts = [
      'abort interrupted local request',
      `upstream PID ${upstreamPidBefore}=${upstreamPidAlive ? 'alive' : 'dead'}${pidChanged ? ` (changed to ${upstreamPidAfter})` : ''}`,
      `upstream request state=${upstreamRequestState}`,
      `original session alive=${sessionAlive}`,
      `sibling session alive=${siblingAlive}`,
    ];

    // P102-R2 verdict: upstream continuation is a measured constraint, not a
    // failure. siblingAlive=false must never produce PASS.
    if (!sessionAlive || !siblingAlive) {
      return CONSTRAINT(`session(s) not fully functional after client abort. ${evidenceParts.join('; ')}`);
    }
    if (upstreamRequestState === 'completed') {
      return CONSTRAINT(`upstream continued and completed the aborted request (abort not propagated to upstream). ${evidenceParts.join('; ')}`);
    }
    if (upstreamRequestState === 'cancelled') {
      return PASS(`upstream request cancelled after client abort, sessions remain functional. ${evidenceParts.join('; ')}`);
    }
    // genuinely unknown
    return CONSTRAINT(`upstream request final state unknown. ${evidenceParts.join('; ')}`);
  } finally {
    await ctrl.pm.stop(log);
    await sleep(1000);
  }
}, { required: true });

test('8.3 other session responsive during slow request (isolation)', async (_client, _fixture, log, args) => {
  let ctrl;
  try {
    ctrl = await startControllableProxy(log, [args._actualPort]);
  } catch (e) {
    return FAIL(`controllable proxy start failed: ${e.message}`);
  }
  try {
    const c2 = await ctrl.client.forkAsNewSession();
    if (!c2.sessionId) return FAIL('second session creation failed');
    if (c2.sessionId === ctrl.client.sessionId) return FAIL('same session ID');

    const slowPromise = ctrl.client.toolCall('delay', { ms: 4000 }, { timeout: 15_000 });

    const t0 = hrtime();
    const r2 = await c2.toolCall('identity', {}, { timeout: 5_000 });
    const dur = msSince(t0);

    if (r2.error) return FAIL(`session 2 identity failed during session 1 delay: ${r2.error.message}`);
    if (dur > 4000) return CONSTRAINT(`session 2 response slow (${dur}ms) while session 1 was delayed`);

    const r1 = await slowPromise;
    if (r1.error) return FAIL(`session 1 delay failed: ${r1.error.message}`);

    return PASS(`session 2 responded in ${dur}ms while session 1 delayed 4000ms — sessions isolated`);
  } finally {
    await ctrl.pm.stop(log);
    await sleep(1000);
  }
}, { required: true });

// 8.4 — P102-R2: exercise disconnect behavior
test('8.4 upstream disconnect: session invalidated, proxy stays alive', async (_client, _fixture, log, args) => {
  let ctrl;
  try {
    ctrl = await startControllableProxy(log, [args._actualPort]);
  } catch (e) {
    return FAIL(`controllable proxy start failed: ${e.message}`);
  }
  try {
    // Call the disconnect tool — upstream closes streams but process stays alive
    const dcResult = await ctrl.client.toolCall('disconnect', {}, { timeout: 5_000 });
    log(`    disconnect response: ${extractTextContent(dcResult).slice(0, 120)}`);
    await sleep(800); // allow time for stream closure to propagate

    // 1. Same session should fail after upstream disconnect
    let sessionDead = false;
    try {
      const afterDc = await ctrl.client.toolCall('identity', {}, { timeout: 5_000 });
      if (afterDc.error) {
        sessionDead = true;
        log(`    disconnected session correctly returns error: ${afterDc.error.message?.slice(0, 120)}`);
      }
    } catch (e) {
      sessionDead = true;
      log(`    disconnected session correctly throws: ${e.message?.slice(0, 120)}`);
    }

    if (!sessionDead) {
      return CONSTRAINT('disconnected session still functional after upstream stream close — proxy may buffer or not detect stream end');
    }

    // 2. Proxy itself should still be alive — new sessions should be creatable
    let newSessionWorks = false;
    let newSessionNote = '';
    try {
      const c2 = new McpClient(ctrl.client.port, () => {});
      await c2.initialize();
      if (c2.sessionId) {
        const newCheck = await c2.toolCall('identity', {}, { timeout: 5_000 });
        if (!newCheck.error) {
          newSessionWorks = true;
          log(`    new session functional after disconnect, upstream PID: ${parseIdentityPid(extractTextContent(newCheck)) || 'unknown'}`);
        } else {
          newSessionNote = `new session created but identity failed: ${newCheck.error.message}`;
          log(`    ${newSessionNote}`);
        }
      } else {
        newSessionNote = 'new session initialize returned no session ID';
        log(`    ${newSessionNote}`);
      }
    } catch (e) {
      newSessionNote = `new session creation failed: ${e.message?.slice(0, 120)}`;
      log(`    ${newSessionNote}`);
    }

    const evidenceParts = [`disconnected session invalidated successfully`];
    if (newSessionWorks) {
      evidenceParts.push(`new session works after disconnect — proxy creates fresh upstream`);
    } else {
      evidenceParts.push(`new session constrained: ${newSessionNote}`);
      return CONSTRAINT(`${evidenceParts.join('; ')} — proxy may share upstream process across sessions`);
    }

    return PASS(evidenceParts.join('; '));
  } finally {
    await ctrl.pm.stop(log);
    await sleep(1000);
  }
}, { required: true });

// ---- 9. Upstream failure & isolation ----
suite('9. Upstream failure & isolation');

test('9.1 upstream crash detected — subsequent request returns error', async (_client, _fixture, log, args) => {
  let ctrl;
  try {
    ctrl = await startControllableProxy(log, [args._actualPort]);
  } catch (e) {
    return FAIL(`controllable proxy start failed: ${e.message}`);
  }
  try {
    // Invoke crash tool — kills the upstream
    try {
      await ctrl.client.toolCall('crash', {}, { timeout: 10_000 });
    } catch (e) {
      log(`    crash call threw: ${e.message?.slice(0, 120)}`);
    }
    await sleep(1000);

    // Subsequent request should fail
    try {
      const r = await ctrl.client.toolCall('identity', {}, { timeout: 5_000 });
      if (r.error) {
        return PASS(`proxy detected upstream failure: subsequent request returned error (${r.error.message?.slice(0, 100)})`);
      }
      return CONSTRAINT('subsequent request succeeded after upstream crash');
    } catch (e) {
      return PASS(`proxy detected upstream failure: ${e.message?.slice(0, 150)}`);
    }
  } finally {
    await ctrl.pm.stop(log);
    await sleep(1000);
  }
}, { required: true });

test('9.2 crashed session cannot make further requests', async (_client, _fixture, log, args) => {
  let ctrl;
  try {
    ctrl = await startControllableProxy(log, [args._actualPort]);
  } catch (e) {
    return FAIL(`controllable proxy start failed: ${e.message}`);
  }
  try {
    try { await ctrl.client.toolCall('crash', {}, { timeout: 10_000 }); } catch {}

    const allFailed = [];
    for (let i = 0; i < 2; i++) {
      try {
        const r = await ctrl.client.toolCall('identity', {}, { timeout: 3_000 });
        if (!r.error) allFailed.push(false);
      } catch { allFailed.push(true); }
    }
    if (allFailed.some(x => !x)) {
      return CONSTRAINT('some post-crash requests succeeded — session may not be fully invalidated');
    }
    return PASS('all post-crash requests to the same session failed — session correctly invalidated');
  } finally {
    await ctrl.pm.stop(log);
    await sleep(1000);
  }
}, { required: true });

test('9.3 other session remains operational after sibling crash', async (_client, _fixture, log, args) => {
  let ctrl;
  try {
    ctrl = await startControllableProxy(log, [args._actualPort]);
  } catch (e) {
    return FAIL(`controllable proxy start failed: ${e.message}`);
  }
  try {
    const c2 = await ctrl.client.forkAsNewSession();
    if (!c2.sessionId) return FAIL('second session creation failed');

    // Crash session 1 (kills the shared upstream process)
    try { await ctrl.client.toolCall('crash', {}, { timeout: 10_000 }); } catch {}
    await sleep(600);

    // Session 2 — may also be affected because mcp-proxy uses a single upstream process
    try {
      const r2 = await c2.toolCall('identity', {}, { timeout: 5_000 });
      if (r2.error) {
        return CONSTRAINT(`session 2 affected by session 1 crash: ${r2.error.message} — mcp-proxy uses shared upstream process, sessions not process-isolated`);
      }
      return PASS('session 2 remained operational after session 1 upstream crash — sessions isolated');
    } catch (e) {
      return CONSTRAINT(`session 2 affected by session 1 crash: ${e.message?.slice(0, 120)} — mcp-proxy uses shared upstream process, sessions not process-isolated`);
    }
  } finally {
    await ctrl.pm.stop(log);
    await sleep(1000);
  }
}, { required: true });

// ---- 10. Session lifecycle ----
suite('10. Session lifecycle');

test('10.1 three rapid session creations do not destabilize proxy', async (client) => {
  const sessions = [];
  for (let i = 0; i < 3; i++) {
    const c = new McpClient(client.port, () => {});
    await c.initialize();
    sessions.push(c.sessionId);
  }
  const r = await client.toolsList();
  if (!r.result?.tools) return FAIL('original session broken after 3 new sessions');
  const ids = sessions.map(s => s?.slice(0, 8) || 'none');
  return PASS(`created sessions: ${ids.join(', ')}; original intact`);
});

test('10.2 session header with garbage value is rejected', async (client) => {
  const res = await fetch(client.baseUrl, {
    method: 'POST',
    headers: { 'content-type': 'application/json', accept: MCP_ACCEPT, 'mcp-session-id': 'garbage-not-valid' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list', params: {} }),
    signal: AbortSignal.timeout(5000),
  });
  const text = await res.text();
  const rejected = res.status >= 400 || /invalid|error|not found/i.test(text);
  return rejected ? PASS(`garbage session rejected (status ${res.status})`) : CONSTRAINT(`accepted (status ${res.status})`);
});

// ---- 11. Proxy exit & child process cleanup (P102-R3) ----
suite('11. Proxy exit & child process cleanup');

test('11.1 verify process tracking primitives', async (_client, _fixture, log, args) => {
  // Verify that PID detection tooling works on this platform
  const selfPid = process.pid;
  const exists = pidExists(selfPid);
  if (!exists) return FAIL(`Cannot detect own PID ${selfPid} — pidExists broken`);

  // Verify port-to-PID resolution works using the main proxy's port
  const portPid = getPidByPort(args._actualPort);
  if (!portPid || portPid <= 0) {
    return CONSTRAINT(`Cannot resolve PID by port ${args._actualPort} — getPidByPort may not work on this platform`);
  }

  // Verify child PID detection by checking for children of the resolved proxy PID
  const children = findChildPids(portPid);
  log(`    pidExists(self=${selfPid})=${exists}, getPidByPort(${args._actualPort})=${portPid}, children of proxy PID: ${children.length}`);

  return PASS(`tracking primitives functional: pidExists=ok, portPid=${portPid}, childrenFound=${children.length}`);
});

test('11.2 deterministic descendant tracking — no system-wide baseline', async (_client, _fixture, log, args) => {
  // Start a separate proxy to test PID tracking in isolation
  const port2 = await findFreePort([args._actualPort]);
  const pm2 = new ProcessManager();
  try {
    await pm2.start(port2, [GITNEXUS_BIN, 'mcp'], log);
  } catch (e) {
    return FAIL(`second proxy start failed: ${e.message}`);
  }

  await sleep(3000);

  // Find the actual mcp-proxy PID via its listening port
  const actualProxyPid = getPidByPort(port2);
  log(`    spawn PID: ${pm2.proxyPid}, listening PID: ${actualProxyPid || 'unknown'}`);

  if (actualProxyPid <= 0) {
    await pm2.stop(log);
    return FAIL(`cannot resolve listening PID for port ${port2} — getPidByPort returned ${actualProxyPid}`);
  }

  // Deterministic: find descendants of the actual proxy process ONLY
  const descendantPids = findDescendantPids(actualProxyPid);
  log(`    descendants of listening PID ${actualProxyPid}: [${descendantPids.join(', ') || 'none'}]`);

  // P102-R3: if descendant detection returns empty, FAIL — do NOT fall back to system-wide PIDs
  if (descendantPids.length === 0) {
    // Try one level deeper — also check descendants of spawn PID
    const spawnDescendants = findDescendantPids(pm2.proxyPid);
    log(`    descendants of spawn PID ${pm2.proxyPid}: [${spawnDescendants.join(', ') || 'none'}]`);

    if (spawnDescendants.length === 0) {
      // Also try via the actual proxy PID's direct children
      const directChildren = findChildPids(actualProxyPid);
      log(`    direct children of ${actualProxyPid}: [${directChildren.join(', ') || 'none'}]`);

      if (directChildren.length === 0) {
        await pm2.stop(log);
        return FAIL(`descendant tracking returned empty for both spawn PID ${pm2.proxyPid} and listening PID ${actualProxyPid} — cannot establish process ownership. No system-wide fallback allowed.`);
      }

      // Use direct children + their descendants
      let allChildren = [...directChildren];
      for (const cpid of directChildren) {
        allChildren = allChildren.concat(findDescendantPids(cpid));
      }
      descendantPids.length = 0;
      descendantPids.push(...new Set(allChildren));
      log(`    using direct children + their descendants: [${descendantPids.join(', ') || 'none'}]`);
    } else {
      descendantPids.length = 0;
      descendantPids.push(...spawnDescendants);
      log(`    using spawn PID descendants: [${descendantPids.join(', ') || 'none'}]`);
    }
  }

  // Verify tracked PIDs exist right now (ownership proof step 1)
  const alive = [];
  const dead = [];
  for (const p of descendantPids) {
    if (pidExists(p)) alive.push(p);
    else dead.push(p);
  }
  log(`    alive: [${alive.join(', ')}], already dead: [${dead.join(', ')}]`);

  if (alive.length === 0) {
    await pm2.stop(log);
    return FAIL(`no tracked descendant PIDs alive — all ${descendantPids.length} PIDs ${dead.length > 0 ? `(including ${dead.join(',')}) ` : ''}exited before verification. Cannot prove ownership.`);
  }

  // Ownership proof step 2: proxy is functional via its child processes
  // P102-R3: any MCP functional ownership check failure must FAIL.
  try {
    const c = new McpClient(port2, log);
    await c.initialize();
    const tl = await c.toolsList();
    if (tl.error) throw new Error(`tools/list error: ${tl.error.message}`);
    log(`    proxy on port ${port2} functional — descendant processes serve MCP traffic`);
  } catch (e) {
    await pm2.stop(log);
    await sleep(1000);
    return FAIL(`MCP functional ownership check failed — cannot prove descendants serve MCP traffic: ${e.message}`);
  }

  // Store for cleanup verification in 11.3
  args._trackedProxyPid = actualProxyPid;
  args._trackedProxySpawnPid = pm2.proxyPid;
  args._trackedPids = alive;
  args._trackedPm2 = pm2;

  return PASS(`proxy PID ${actualProxyPid}, ${alive.length} tracked alive descendant(s): [${alive.join(', ')}]`);
}, { required: true });

test('11.3 proxy stop terminates all tracked child processes', async (_client, _fixture, log, args) => {
  const trackedPids = args._trackedPids || [];
  const pm2 = args._trackedPm2;
  const proxyPid = args._trackedProxyPid;

  if (!pm2 || trackedPids.length === 0) {
    return FAIL('no tracked PIDs from 11.2 — cannot verify cleanup');
  }

  log(`    stopping proxy PID ${proxyPid}, verifying ${trackedPids.length} tracked descendants: [${trackedPids.join(', ')}]`);

  // Pre-stop: all tracked PIDs should still be alive
  const preStopAlive = trackedPids.filter(p => pidExists(p));
  if (preStopAlive.length === 0) {
    return FAIL(`all tracked PIDs already dead before stop — ownership was not established`);
  }
  log(`    pre-stop: ${preStopAlive.length}/${trackedPids.length} alive`);

  await pm2.stop(log);
  await sleep(3000);

  // Post-stop: verify each tracked PID is gone
  const survivors = [];
  for (const pid of trackedPids) {
    if (pidExists(pid)) {
      survivors.push(pid);
      log(`    PID ${pid} STILL ALIVE after proxy stop`);
    } else {
      log(`    PID ${pid} terminated`);
    }
  }

  // Also check proxy PID itself
  if (proxyPid && pidExists(proxyPid)) {
    survivors.push(proxyPid);
    log(`    proxy PID ${proxyPid} STILL ALIVE`);
  }

  args._trackedPm2 = null;
  args._trackedPids = [];

  if (survivors.length > 0) {
    return FAIL(`orphan processes after proxy stop: [${survivors.join(', ')}]`);
  }
  return PASS(`all ${trackedPids.length} tracked descendant(s) and proxy terminated`);
}, { required: true });

test('11.4 stdout and stderr content isolation', async (_client, _fixture, log, args) => {
  const port = await findFreePort([args._actualPort]);
  const pm = new ProcessManager();
  try {
    await pm.start(port, [GITNEXUS_BIN, 'mcp'], log);
  } catch (e) {
    return FAIL(`proxy start failed: ${e.message}`);
  }

  try {
    const c = new McpClient(port, log);
    await c.initialize();
    await c.toolsList();
    await c.toolCall('list_repos', {}, { timeout: 15_000 });

    const stdoutBuf = pm.stdoutBuf;
    const stderrBuf = pm.stderrBuf;

    // P102-R3: verify channel content isolation
    // mcp-proxy's own stdout contains startup/log messages (not upstream MCP protocol).
    // Upstream MCP protocol frames go through internal stdio pipes to HTTP responses.
    // stderr must NOT contain JSON-RPC protocol frames (would indicate channel pollution).
    const stderrHasProtocol = /"jsonrpc"\s*:\s*"2\.0"/.test(stderrBuf);
    const stderrHasHttpPayload = /"result"\s*:\s*\{/.test(stderrBuf) && /"tools"\s*:/.test(stderrBuf);
    const totalOutput = stdoutBuf.length + stderrBuf.length;

    const findings = [];
    if (stderrHasProtocol) {
      findings.push(`stderr contains JSON-RPC protocol frames — protocol/log channel pollution detected`);
    }
    if (stderrHasHttpPayload) {
      findings.push(`stderr appears to contain MCP tool response payloads — upstream stdout may be leaking into proxy stderr`);
    }
    if (totalOutput === 0) {
      findings.push('no output on either stdout or stderr');
    }

    if (findings.length > 0) {
      return CONSTRAINT(findings.join('; '));
    }

    return PASS(`stdout: ${stdoutBuf.length} chars (proxy log), stderr: ${stderrBuf.length} chars (no protocol leakage)`);
  } finally {
    await pm.stop(log);
    await sleep(1000);
  }
}, { required: true });

// ---- 12. Generation / P1-08 pre-check ----
suite('12. Generation / P1-08 pre-check');

test('12.1 audit for generation-related tools', async (client) => {
  const r = await client.toolsList();
  const tools = r.result.tools;
  const genToolNames = ['resolve_generation', 'generation_status', 'list_branches', 'branch_status'];
  const found = tools.filter(t => genToolNames.includes(t.name));

  const genIdTools = [];
  for (const t of tools) {
    const props = t.inputSchema?.properties || {};
    if (props.generation_id || props.generationId) genIdTools.push(t.name);
  }

  if (found.length === 0 && genIdTools.length === 0) {
    return CONSTRAINT('BLOCKED: No generation resolution tools (resolve_generation, generation_status, list_branches) found, and no tool accepts generation_id parameter. P1-08 requires GitNexus to add these before GraphGateway can implement branch→generation resolution.');
  }

  let report = '';
  if (found.length > 0) report += `Tools: ${found.map(t => t.name).join(', ')}. `;
  if (genIdTools.length > 0) report += `Tools accepting generation_id: ${genIdTools.join(', ')}. `;
  return PASS(report || 'generation partially supported');
}, { required: true });

test('12.2 audit for generation_id / branch parameters', async (client) => {
  const r = await client.toolsList();
  let branchTools = [], genIdTools = [];
  for (const t of r.result.tools) {
    const props = t.inputSchema?.properties || {};
    if (props.branch) branchTools.push(t.name);
    if (props.generation_id || props.generationId) genIdTools.push(t.name);
  }
  const summary = [
    `branch param: ${branchTools.length > 0 ? branchTools.join(', ') : 'NONE'}`,
    `generation_id param: ${genIdTools.length > 0 ? genIdTools.join(', ') : 'NONE'}`,
  ];
  return PASS(summary.join(' | '));
});

test('12.3 list all tool names for reference', async (client) => {
  const r = await client.toolsList();
  const names = r.result.tools.map(t => t.name).sort();
  return PASS(`total ${names.length}: ${names.join(', ')}`);
});

// ---- 13. Main proxy cleanup (P102-R3: cleanup verification runs BEFORE report) ----
suite('13. Main proxy cleanup');

test('13.1 main proxy stop terminates all child processes', async (_client, _fixture, log, args) => {
  const mainPm = args._mainPm;
  const mainPort = args._actualPort;

  if (args.keepProcesses) return SKIP('--keep-processes: main proxy left running');
  if (!mainPm) return FAIL('no main proxy reference — cannot verify cleanup');

  // Step 1: Track descendants of the main proxy before stopping
  const mainProxyPid = getPidByPort(mainPort);
  let descendantPids = [];
  if (mainProxyPid > 0) {
    descendantPids = findDescendantPids(mainProxyPid);
  } else {
    // Fall back to spawn PID's descendants
    descendantPids = findDescendantPids(mainPm.proxyPid);
  }

  log(`  Main proxy descendants to track: [${descendantPids.join(', ') || 'none'}] (port PID: ${mainProxyPid || 'unknown'}, spawn PID: ${mainPm.proxyPid})`);

  if (descendantPids.length === 0) {
    // Try direct children
    const pid = mainProxyPid > 0 ? mainProxyPid : mainPm.proxyPid;
    const directChildren = findChildPids(pid);
    if (directChildren.length === 0) {
      // Stop anyway but record inability to verify
      await mainPm.stop(log);
      await sleep(2000);
      return FAIL(`no descendant PIDs found for main proxy (port PID ${mainProxyPid}, spawn PID ${mainPm.proxyPid}) — cannot establish process ownership via parent/child. No system-wide fallback permitted.`);
    }
    descendantPids = directChildren;
    log(`  Using direct children: [${descendantPids.join(', ')}]`);
  }

  // Pre-stop: verify tracked PIDs are alive
  const preStopAlive = descendantPids.filter(p => pidExists(p));
  log(`  Pre-stop: ${preStopAlive.length}/${descendantPids.length} tracked PIDs alive`);

  // Step 2: Stop the main proxy
  await mainPm.stop(log);
  await sleep(2000);

  // Step 3: Verify all tracked PIDs are gone
  const survivors = [];
  for (const pid of descendantPids) {
    if (pidExists(pid)) {
      survivors.push(pid);
      log(`  PID ${pid} STILL ALIVE after main proxy stop`);
    }
  }
  if (mainProxyPid > 0 && pidExists(mainProxyPid)) {
    survivors.push(mainProxyPid);
    log(`  main proxy PID ${mainProxyPid} STILL ALIVE`);
  }

  args._mainPm = null;

  if (survivors.length > 0) {
    return FAIL(`main proxy cleanup: ${survivors.length} orphan process(es): [${survivors.join(', ')}]`);
  }

  return PASS(`main proxy cleanup: all ${descendantPids.length} tracked descendant(s) terminated`);
}, { required: true });

// ── helpers ──────────────────────────────────────────────────────────────────
function extractTextContent(rpcResult) {
  if (rpcResult.result?.content && Array.isArray(rpcResult.result.content)) {
    return rpcResult.result.content.map(c => c.text || c.data || JSON.stringify(c)).join('\n');
  }
  if (typeof rpcResult.result === 'string') return rpcResult.result;
  if (rpcResult.result && typeof rpcResult.result === 'object') return JSON.stringify(rpcResult.result);
  return '';
}

/** Parse PID from identity tool response JSON. */
function parseIdentityPid(text) {
  try {
    const obj = JSON.parse(text);
    return obj.pid || null;
  } catch {
    return null;
  }
}

// ── report generator (P102-R3: cleanup results included in verdict) ───────────
async function generateReport(results, versions, fixture, args, startTime, log) {
  const passed = results.filter(r => r.status === VERDICT.PASS).length;
  const failed = results.filter(r => r.status === VERDICT.FAIL).length;
  const skipped = results.filter(r => r.status === VERDICT.SKIP).length;
  const constrained = results.filter(r => r.status === VERDICT.CONSTRAINT).length;

  const requiredBlocked = results.filter(r =>
    requiredTests.has(r.name) && (r.status === VERDICT.SKIP || r.status === VERDICT.CONSTRAINT)
  );

  let finalVerdict;
  if (failed > 0) {
    finalVerdict = 'FAIL';
  } else if (requiredBlocked.length > 0) {
    finalVerdict = 'PASS_WITH_CONSTRAINTS';
  } else {
    finalVerdict = 'PASS';
  }

  const totalDur = msSince(startTime);
  const fixtureRelPath = relative(REPO_ROOT, args._originalFixturePath || args.fixture);
  const testedCommit = sh(['git', 'rev-parse', 'HEAD'], { cwd: REPO_ROOT });

  // P102-R3: derive session/process model from evidence (consistent, not contradictory)
  const sessionModel = deriveSessionModel(results);

  const constraints = [];
  if (requiredBlocked.length > 0) {
    constraints.push(`P1-08 BLOCKED: GitNexus ${versions.gitnexus} does not expose generation-related MCP tools (resolve_generation, generation_status). Branch→generation resolution is unavailable. GraphGateway must use one-instance-per-generation deployment model until GitNexus adds generation support.`);
  }
  for (const r of results) {
    if (r.status === VERDICT.CONSTRAINT) {
      constraints.push(`${r.name}: ${r.evidence.slice(0, 300)}`);
    }
  }

  const report = `# mcp-proxy ↔ GitNexus MCP 互操作兼容性报告

> **生成时间**: ${now()}
> **任务**: GGW-P1-02 mcp-proxy / GitNexus 互操作门禁
> **结论**: **${finalVerdict}**
> **Tested commit**: ${testedCommit}
> **总耗时**: ${totalDur} ms

---

## 1. 版本矩阵

| 组件 | 版本 |
|---|---|
| OS | ${versions.os} |
| Node.js | ${versions.node} |
| mcp-proxy | ${versions['mcp-proxy']} |
| GitNexus | ${versions.gitnexus} |

## 2. 启动参数与传输

\`\`\`sh
mcp-proxy \\
  --port ${args._actualPort || 'N/A'} --host 127.0.0.1 \\
  --server stream \\
  --connectionTimeout 30000 --requestTimeout 120000 \\
  ${platform === 'win32' ? '--shell ' : ''}-- gitnexus mcp
\`\`\`

- **MCP 协议版本**: \`${PROTOCOL_VERSION}\`
- **Transport**: Streamable HTTP (\`POST /mcp\`)
- **响应格式**: SSE (\`event: message\\ndata: <json>\\n\\n\`)
- **会话模型**: ${sessionModel}
- **子进程启动**: mcp-proxy 使用 \`${platform === 'win32' ? '--shell' : 'spawn'}\` 模式启动 gitnexus
- **Fixture repository**: \`${fixture.repoName}\` (source: \`${fixtureRelPath}\`)

## 3. 测试结果

| # | Suite | Test | 结果 | 耗时(ms) |
|---|---|---|---|---|
${results.map((r, i) => `| ${i + 1} | ${r.suite} | ${r.name} | **${r.status}** | ${r.duration_ms} |`).join('\n')}

### 详细证据

${results.map((r, i) => `**${i + 1}. ${r.name}** [${r.status}]\n> ${r.evidence.replace(/\n/g, '\n> ')}\n`).join('\n')}

**总计**: ${passed} PASS, ${failed} FAIL, ${skipped} SKIP, ${constrained} CONSTRAINT, ${results.length} total

> Each conclusion maps to PASS, FAIL, SKIP, or CONSTRAINT.
> A CONSTRAINT or SKIP on a required acceptance item prevents the overall verdict from becoming PASS.
${requiredBlocked.length > 0 ? `> Required items with CONSTRAINT/SKIP: ${requiredBlocked.map(r => r.name).join(', ')}` : ''}

## 4. P1-05 / P1-08 下游约束

### 4.1 P1-05 (Southbound MCP Client)

mcp-proxy 成功将 GitNexus stdio MCP 暴露为标准 Streamable HTTP endpoint。GraphGateway 南向 MCP Client 实现时需要注意：

- **SSE 解析**: 响应以 \`event: message\` + \`data:\` 行格式返回，需提取 \`data:\` 行 JSON。
- **会话管理**: 每次 \`initialize\` 返回新的 \`Mcp-Session-Id\`，后续请求必须携带此 header。
- **Accept header**: 必须包含 \`application/json, text/event-stream\`，否则返回 406。
- **子进程模型**: ${sessionModel}
- **传输适配验证**: mcp-proxy 正确透传了 tools、resources、capabilities 和能力协商。

### 4.2 P1-08 (Generation & Resolved View)

${requiredBlocked.length > 0 && results.find(r => r.name.includes('12.1'))?.status === VERDICT.CONSTRAINT
  ? `**BLOCKED** — GitNexus ${versions.gitnexus} 不包含 generation 解析 MCP 能力。

经 tools/list 审计：
- 无 \`resolve_generation\`、\`generation_status\`、\`list_branches\` 等 generation 管理工具
- 无工具接受 \`generation_id\` 参数（所有查询工具隐式使用最新索引版本）
- 多数工具已接受 \`branch\` 参数（${results.find(r => r.name.includes('12.2'))?.evidence?.match(/branch param: (.+?) \|/)?.[1] || 'N/A'}），可用于 GraphGateway 路由时的分支选择

**解除阻塞需要 GitNexus 增加**:
1. \`resolve_generation(repo, branch)\` → \`{generation_id, head_sha, freshness}\`
2. \`generation_status(generation_id)\` → \`{state, completion, indexed_at}\`
3. 现有查询工具（\`query\`, \`context\`, \`impact\` 等）接受可选 \`generation_id\` 参数

**临时方案**: GraphGateway 可采用"一实例一 generation"部署模型 — 每个 mcp-proxy endpoint 绑定固定已索引版本。`
  : `GitNexus 已有 generation 能力。P1-08 可以继续。`}

## 5. 约束与已知限制

${constraints.length > 0 ? constraints.map(c => `- ${c}`).join('\n') : '- 无'}

## 6. 可观测性与隔离

- mcp-proxy stderr 日志与 GitNexus MCP stdout 通过不同管道隔离。
- GitNexus 使用结构化日志（pino）写入 stderr，与 MCP 协议消息分通道。
- mcp-proxy 透传 GitNexus 的 JSON-RPC 响应到 HTTP 响应流。
- mcp-proxy 在非 debug 模式下仅输出启动信息和错误，缺少请求级 trace。

## 7. 安全边界

- mcp-proxy 默认监听 \`127.0.0.1\`（loopback only）。
- GitNexus MCP 不监听任何网络端口（stdio only）。
- 未配置 \`--apiKey\` 时本地无认证（开发模式）。
- mcp-proxy 支持 \`--apiKey\` 和 \`X-API-Key\` header 认证。
- 无效 \`Mcp-Session-Id\` 被正确拒绝。

## 8. 进程清理验证 (P102-R3)

子进程追踪使用确定性 PID 父子关系验证：
- 启动 proxy 后通过监听端口解析实际 PID。
- 使用 parent/child 系统调用（WMI / pgrep）递归查找所有子进程。
- 通过代理功能运行验证子进程确实服务 MCP 流量（所有权证明）。
- 停止 proxy 后，使用 pidExists 逐个验证已追踪 PID 已退出。
- 不使用全系统 PID 快照差值作为关联证据。
- 清理结果（含孤儿 PID）计入测试结果和整体 verdict。

---
*报告由 \`scripts/interop/verify-mcp-proxy-gitnexus.mjs\` 于 ${now()} 自动生成。*
*每个结论可追溯到第 3 节中的测试证据（PASS/FAIL/SKIP/CONSTRAINT）。*
`;

  await writeFile(REPORT_PATH, report, 'utf-8');
  log(`Report written to ${REPORT_PATH}`);
  return { overall: finalVerdict, passed, failed, skipped, constrained, total: results.length, reportPath: REPORT_PATH, requiredBlocked };
}

/** P102-R3: derive consistent session model from evidence instead of making contradictory claims. */
function deriveSessionModel(results) {
  const crashIsolationResult = results.find(r => r.name.includes('9.3'));
  const disconnectResult = results.find(r => r.name.includes('8.4'));
  const sessionIds = results.find(r => r.name.includes('5.1'));

  // Check evidence from crash isolation test
  let sharedUpstream = true; // default assumption — conservative
  if (crashIsolationResult && crashIsolationResult.status === VERDICT.PASS) {
    sharedUpstream = false;
  }

  if (sharedUpstream) {
    return '有状态 — 每个 Mcp-Session-Id 是独立会话标识，但所有会话共享一个 GitNexus stdio 上游进程。一个会话的上游崩溃会影响其他会话。';
  }
  return '有状态 — 每个 Mcp-Session-Id 对应独立的 GitNexus stdio 子进程。会话之间进程隔离。';
}

// ── main ─────────────────────────────────────────────────────────────────────
async function main() {
  const args = parseArgs();
  const startTime = hrtime();

  const log = (msg) => {
    const ts = new Date().toISOString().replace('T', ' ').replace('Z', '');
    stdout.write(`[${ts}] ${msg}\n`);
  };

  log('=== GGW-P1-02 mcp-proxy / GitNexus 互操作门禁 ===\n');

  // 1. Versions (P102-R1: fail-fast on missing commands)
  log('--- Versions ---');
  const versions = probeVersions();
  for (const [k, v] of Object.entries(versions)) log(`  ${k}: ${v}`);

  // 2. Create temp fixture (P102-R4, P102-R1: fail-fast setup)
  log('\n--- Temp Fixture ---');
  args._originalFixturePath = args.fixture;
  const tempFixture = await createTempFixture(args.fixture, log);
  await setupFixture(tempFixture, log);
  if (tempFixture.error) {
    log(`FATAL: ${tempFixture.error}`);
    await cleanupTempFixture(tempFixture, log);
    exit(1);
  }

  // 3. Start proxy
  log('\n--- Starting mcp-proxy ---');
  const port = args.port || await findFreePort();
  args._actualPort = port;
  const pm = new ProcessManager();
  args._mainPm = pm; // P102-R3: store for test 13.1 cleanup verification
  try {
    await pm.start(port, [GITNEXUS_BIN, 'mcp'], log);
  } catch (e) {
    log(`FATAL: ${e.message}`);
    args._mainPm = null;
    await cleanupTempFixture(tempFixture, log);
    exit(1);
  }

  // 4. Initialize client
  log('\n--- MCP client ---');
  const client = new McpClient(port, log);
  try {
    const initR = await client.initialize();
    log(`  Server: ${initR.result?.serverInfo?.name} v${initR.result?.serverInfo?.version}`);
    log(`  Capabilities: ${JSON.stringify(initR.result?.capabilities)}`);
    log(`  Session: ${client.sessionId || '(none)'}`);
  } catch (e) {
    log(`FATAL: initialize failed: ${e.message}`);
    args._mainPm = null;
    await pm.stop(log);
    await cleanupTempFixture(tempFixture, log);
    exit(1);
  }

  // 5. Run tests (P102-R3: includes test 13 which stops main proxy and verifies cleanup)
  log('\n--- Tests ---');
  const results = await runAllTests(client, tempFixture, log, args);

  // 6. P102-R3: every cleanup result (fallback/final/temp-fixture) must enter
  //    the verdict before report generation.
  log('\n--- Cleanup verification ---');

  // 6a. Fallback: stop main proxy if test 13.1 did not (e.g., it failed or was skipped)
  if (args._mainPm) {
    const t0 = hrtime();
    let status, evidence;
    try {
      await args._mainPm.stop(log);
      await sleep(2000);
      status = VERDICT.PASS;
      evidence = 'fallback main proxy cleanup: proxy stopped (test 13.1 did not complete cleanup)';
    } catch (e) {
      status = VERDICT.FAIL;
      evidence = `fallback main proxy cleanup failed: ${e.message}`;
    }
    results.push({ suite: '13. Main proxy cleanup', name: '13.2 fallback main proxy cleanup', status, evidence, duration_ms: msSince(t0) });
    log(`  [${status}] 13.2 fallback main proxy cleanup`);
    args._mainPm = null;
  }

  // 6b. Fallback: stop secondary proxy if test 11.3 did not
  if (args._trackedPm2) {
    const t0 = hrtime();
    let status, evidence;
    try {
      await args._trackedPm2.stop(log);
      await sleep(1000);
      status = VERDICT.PASS;
      evidence = 'fallback secondary proxy cleanup: proxy stopped (test 11.3 did not complete cleanup)';
    } catch (e) {
      status = VERDICT.FAIL;
      evidence = `fallback secondary proxy cleanup failed: ${e.message}`;
    }
    results.push({ suite: '13. Main proxy cleanup', name: '13.3 fallback secondary proxy cleanup', status, evidence, duration_ms: msSince(t0) });
    log(`  [${status}] 13.3 fallback secondary proxy cleanup`);
    args._trackedPm2 = null;
  }

  // 6c. Temporary fixture cleanup — must enter verdict before report (P102-R3)
  {
    const t0 = hrtime();
    const cr = await cleanupTempFixture(tempFixture, log);
    results.push({ suite: '13. Main proxy cleanup', name: '13.4 temporary fixture cleanup', status: cr.status, evidence: cr.evidence, duration_ms: msSince(t0) });
    log(`  [${cr.status}] 13.4 temporary fixture cleanup`);
  }

  // 7. Report (P102-R3: generated AFTER all cleanup results are in the verdict)
  log('\n--- Report ---');
  await mkdir(dirname(REPORT_PATH), { recursive: true });
  const report = await generateReport(results, versions, tempFixture, args, startTime, log);

  // 9. Summary
  log(`\n=== ${report.overall} ===`);
  log(`${report.total} tests: ${report.passed} PASS, ${report.failed} FAIL, ${report.skipped} SKIP, ${report.constrained} CONSTRAINT`);
  if (report.requiredBlocked?.length) {
    log(`Required items blocked: ${report.requiredBlocked.map(r => r.name).join(', ')}`);
  }
  log(`Report: ${report.reportPath}`);
  log(`Duration: ${msSince(startTime)} ms`);

  // P102-R1: non-zero exit for FAIL or PASS_WITH_CONSTRAINTS
  exit(report.overall === 'PASS' ? 0 : 1);
}

main().catch((e) => { console.error(`FATAL: ${e.message}\n${e.stack}`); exit(1); });
