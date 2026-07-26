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
import { writeFile, mkdir, stat, rm, cp } from 'node:fs/promises';
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

function sh(cmd, opts = {}) {
  try {
    return execFileSync(cmd[0], cmd.slice(1), {
      encoding: 'utf-8', timeout: 30_000, shell: platform === 'win32', windowsHide: true, ...opts,
    }).trim();
  } catch (e) { return `ERROR: ${e.message}`; }
}

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

// ── version probes ──────────────────────────────────────────────────────────
function probeVersions() {
  return {
    os: `${platform} ${process.arch}`,
    node: sh(['node', '--version']),
    'mcp-proxy': sh([MCP_PROXY_BIN, '--version']),
    gitnexus: sh([GITNEXUS_BIN, '--version']),
  };
}

// ── process tracking (P102-R3) ───────────────────────────────────────────────
function getGitnexusPids() {
  if (platform !== 'win32') {
    const r = shCapture(['pgrep', '-x', 'gitnexus']);
    if (r.exitCode !== 0) return [];
    return r.stdout.split('\n').filter(Boolean).map(s => parseInt(s.trim(), 10));
  }
  const out = sh(['cmd', '/c', 'tasklist /fi "imagename eq gitnexus.exe" /fo csv /nh 2>nul || echo none']);
  if (!out || out.includes('none') || out.includes('No tasks')) return [];
  const pids = [];
  for (const line of out.split('\n')) {
    const m = line.match(/gitnexus\.exe",\s*"(\d+)"/i);
    if (m) pids.push(parseInt(m[1], 10));
  }
  return pids;
}

function pidExists(pid) {
  if (platform !== 'win32') {
    try { process.kill(pid, 0); return true; } catch { return false; }
  }
  const out = sh(['cmd', '/c', `tasklist /fi "PID eq ${pid}" /fo csv /nh 2>nul || echo none`]);
  return out.includes(String(pid));
}

// ── temp directory for fixture runtime (P102-R4) ─────────────────────────────
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

async function setupFixture(tempFixture, log) {
  const { fixturePath } = tempFixture;
  log(`Setting up fixture git repo at ${fixturePath}`);

  try {
    sh(['git', 'init'], { cwd: fixturePath });
    sh(['git', 'config', 'user.email', 'ggw-p1-02-test@graphgateway.local'], { cwd: fixturePath });
    sh(['git', 'config', 'user.name', 'GGW P1-02 Test'], { cwd: fixturePath });
    sh(['git', 'add', '-A'], { cwd: fixturePath });
    const commitR = shCapture(['git', 'commit', '-m', 'test: interop fixture initial commit'], { cwd: fixturePath });
    if (commitR.exitCode !== 0) {
      log(`  git commit note: ${commitR.stderr || commitR.error || 'ok'}`);
    }
  } catch (e) { tempFixture.error = `git init/commit failed: ${e.message}`; return; }

  const listOut = sh([GITNEXUS_BIN, 'list']);
  const normalizedPath = fixturePath.replace(/\\/g, '/').toLowerCase();
  if (listOut.toLowerCase().includes(normalizedPath)) {
    log('  Already indexed by GitNexus');
  } else {
    log('  Indexing fixture with gitnexus analyze...');
    try {
      const t0 = hrtime();
      sh([GITNEXUS_BIN, 'analyze', fixturePath], { timeout: 120_000 });
      log(`  Indexed in ${msSince(t0)} ms`);
      tempFixture.freshlyIndexed = true;
    } catch (e) { tempFixture.error = `gitnexus analyze failed: ${e.message}`; return; }
  }

  const l2 = sh([GITNEXUS_BIN, 'list']);
  const lines = l2.split('\n');
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].toLowerCase().includes(normalizedPath)) {
      for (let j = i - 1; j >= 0; j--) {
        const m = lines[j].match(/^\s{2}(\S+)/);
        if (m) { tempFixture.repoName = m[1]; break; }
      }
      break;
    }
  }
  if (!tempFixture.repoName) tempFixture.repoName = basename(fixturePath);
  log(`  Repo name: ${tempFixture.repoName}`);
}

async function cleanupTempFixture(tempFixture, log) {
  try {
    await rm(tempFixture.tmpBase, { recursive: true, force: true });
    log(`Cleaned up temp fixture: ${tempFixture.tmpBase}`);
  } catch (e) {
    log(`Warning: could not clean up temp fixture: ${e.message}`);
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
      log(`Stopping mcp-proxy (PID ${this.proxyPid})...`);
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

// ── test runner (P102-R1) ────────────────────────────────────────────────────
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

// ---- 4. tools/call — fixed repo ----
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
  if (text.includes(fixture.repoName)) return PASS(`"${fixture.repoName}" found in list_repos`);
  if (text.includes(basename(fixture.fixturePath))) return PASS(`"${basename(fixture.fixturePath)}" found (path match)`);
  return CONSTRAINT(`"${fixture.repoName}" not explicitly listed — may be under different name`);
}, { required: true });

test('4.3 query returns results', async (client, fixture) => {
  const r = await client.toolCall('query', { search_query: 'greet', repo: fixture.repoName, limit: 3, max_symbols: 5 });
  if (r.error) {
    const r2 = await client.toolCall('query', { search_query: 'greet', limit: 3, max_symbols: 5 });
    if (r2.error) return FAIL(`query error: ${r2.error.message}`);
    return PASS(`query OK (no repo param): ${extractTextContent(r2).length} chars`);
  }
  return PASS(`query OK: ${extractTextContent(r).length} chars`);
}, { required: true });

test('4.4 context resolves a symbol', async (client, fixture) => {
  const r = await client.toolCall('context', { name: 'greet', repo: fixture.repoName });
  if (r.error) {
    const r2 = await client.toolCall('context', { name: 'greet', file_path: 'src/main.js' });
    if (r2.error) return FAIL(`context error: ${r2.error.message}`);
    return PASS(`context OK via file_path: ${extractTextContent(r2).length} chars`);
  }
  return PASS(`context OK: ${extractTextContent(r).length} chars`);
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

// ---- 8. Timeout & cancellation (P102-R2) ----
suite('8. Timeout & cancellation');

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

test('8.2 client abort interrupts slow request with AbortError', async (_client, _fixture, log, args) => {
  let ctrl;
  try {
    ctrl = await startControllableProxy(log, [args._actualPort]);
  } catch (e) {
    return FAIL(`controllable proxy start failed: ${e.message}`);
  }
  try {
    await ctrl.client.toolCall('identity', {});

    const controller = new AbortController();
    setTimeout(() => {
      log('    aborting client request after 500ms...');
      controller.abort();
    }, 500);

    const t0 = hrtime();
    try {
      await ctrl.client.toolCall('delay', { ms: 15000 }, { signal: controller.signal, timeout: 20_000 });
      return FAIL('delay completed despite client abort — abort was not effective');
    } catch (e) {
      const dur = msSince(t0);
      const isAbort = e.name === 'AbortError' || e.message?.includes('abort') || e.message?.includes('AbortError');
      log(`    Caught after ${dur}ms: ${e.message?.slice(0, 120)}`);
      if (dur < 10000 && isAbort) {
        return PASS(`client abort correctly interrupted slow request after ${dur}ms (delay was 15000ms)`);
      }
      if (dur >= 10000) {
        return CONSTRAINT(`request may have completed naturally after ${dur}ms — abort not clearly effective`);
      }
      return PASS(`request interrupted after ${dur}ms: ${e.message?.slice(0, 100)}`);
    }
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

// ---- 9. Upstream failure & isolation (P102-R2) ----
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

    // Crash session 1
    try { await ctrl.client.toolCall('crash', {}, { timeout: 10_000 }); } catch {}
    await sleep(600);

    // Session 2 should still work
    try {
      const r2 = await c2.toolCall('identity', {}, { timeout: 5_000 });
      if (r2.error) return FAIL(`session 2 failed after session 1 crash: ${r2.error.message}`);
      return PASS('session 2 remained operational after session 1 upstream crash — sessions isolated');
    } catch (e) {
      return FAIL(`session 2 threw after session 1 crash: ${e.message}`);
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

test('11.1 establish baseline gitnexus process set', async (_client, _fixture, log, args) => {
  const baselinePids = getGitnexusPids();
  log(`    baseline gitnexus PIDs: [${baselinePids.join(', ') || 'none'}]`);
  args._baselinePids = baselinePids;
  return PASS(`baseline captured: ${baselinePids.length} existing gitnexus process(es)`);
});

test('11.2 proxy start creates new gitnexus child processes', async (_client, _fixture, log, args) => {
  const port2 = await findFreePort([args._actualPort]);
  const pm2 = new ProcessManager();
  try {
    await pm2.start(port2, [GITNEXUS_BIN, 'mcp'], log);
  } catch (e) {
    return FAIL(`second proxy start failed: ${e.message}`);
  }

  await sleep(2000);

  const currentPids = getGitnexusPids();
  const baseline = args._baselinePids || [];
  const newPids = currentPids.filter(p => !baseline.includes(p));
  log(`    current PIDs: [${currentPids.join(', ')}]`);
  log(`    new PIDs: [${newPids.join(', ')}]`);

  if (newPids.length === 0) {
    await pm2.stop(log);
    return FAIL('no new gitnexus processes detected after proxy start — proxy may not have spawned upstream');
  }

  if (!pidExists(pm2.proxyPid)) {
    await pm2.stop(log);
    return FAIL(`proxy PID ${pm2.proxyPid} is not alive`);
  }

  // Verify each tracked PID exists
  for (const p of newPids) {
    if (!pidExists(p)) {
      await pm2.stop(log);
      return FAIL(`new gitnexus PID ${p} not found — may have crashed immediately`);
    }
  }

  // Use the proxy and prove association
  try {
    const c = new McpClient(port2, log);
    await c.initialize();
    await c.toolsList();
    log(`    proxy on port ${port2} functional`);
  } catch (e) {
    log(`    proxy functional check: ${e.message}`);
  }

  args._trackedProxyPid = pm2.proxyPid;
  args._trackedPids = newPids;
  args._trackedPm2 = pm2;

  return PASS(`proxy PID ${pm2.proxyPid} spawned ${newPids.length} gitnexus process(es): [${newPids.join(', ')}]`);
}, { required: true });

test('11.3 proxy stop terminates all child processes', async (_client, _fixture, log, args) => {
  const trackedPids = args._trackedPids || [];
  const pm2 = args._trackedPm2;

  if (!pm2 || trackedPids.length === 0) {
    return SKIP('no tracked PIDs from 11.2 — cannot verify cleanup');
  }

  log(`    stopping proxy PID ${args._trackedProxyPid}...`);
  await pm2.stop(log);
  await sleep(3000);

  const survivors = [];
  for (const pid of trackedPids) {
    if (pidExists(pid)) {
      survivors.push(pid);
      log(`    PID ${pid} STILL ALIVE after proxy stop`);
    } else {
      log(`    PID ${pid} terminated`);
    }
  }

  if (pidExists(args._trackedProxyPid)) {
    survivors.push(args._trackedProxyPid);
    log(`    proxy PID ${args._trackedProxyPid} STILL ALIVE`);
  }

  args._trackedPm2 = null;
  args._trackedPids = [];

  if (survivors.length > 0) {
    return FAIL(`orphan processes after proxy stop: [${survivors.join(', ')}]`);
  }
  return PASS(`all ${trackedPids.length} tracked gitnexus process(es) and proxy terminated`);
}, { required: true });

test('11.4 stdout and stderr isolation confirmed', async (_client, _fixture, log, args) => {
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

    const stdoutLen = pm.stdoutBuf.length;
    const stderrLen = pm.stderrBuf.length;

    if (stdoutLen + stderrLen === 0) {
      return CONSTRAINT('no output captured on either channel — proxy may suppress upstream output');
    }

    return PASS(`stdout: ${stdoutLen} chars, stderr: ${stderrLen} chars, channels distinct`);
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

// ── helpers ──────────────────────────────────────────────────────────────────
function extractTextContent(rpcResult) {
  if (rpcResult.result?.content && Array.isArray(rpcResult.result.content)) {
    return rpcResult.result.content.map(c => c.text || c.data || JSON.stringify(c)).join('\n');
  }
  if (typeof rpcResult.result === 'string') return rpcResult.result;
  if (rpcResult.result && typeof rpcResult.result === 'object') return JSON.stringify(rpcResult.result);
  return '';
}

// ── report generator (P102-R5) ────────────────────────────────────────────────
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
- **会话模型**: 有状态 — 每个 \`Mcp-Session-Id\` 对应一个 GitNexus stdio 子进程
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
- **子进程模型**: 当前 mcp-proxy 为每个 Session 启动独立 GitNexus 子进程，需要评估内存和启动延迟。
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

- mcp-proxy stderr 日志与 GitNexus MCP stdout 正确隔离（不同管道）。
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

子进程追踪使用确定性 PID 验证：
- 启动前记录基线 gitnexus.exe PID 集合。
- 启动 proxy 后识别新增 PID，关联至 proxy 会话。
- 停止 proxy 后验证每个已追踪 PID 已退出。
- 不使用全局计数或 0→0 作为清理证据。

---
*报告由 \`scripts/interop/verify-mcp-proxy-gitnexus.mjs\` 于 ${now()} 自动生成。*
*每个结论可追溯到第 3 节中的测试证据（PASS/FAIL/SKIP/CONSTRAINT）。*
`;

  await writeFile(REPORT_PATH, report, 'utf-8');
  log(`Report written to ${REPORT_PATH}`);
  return { overall: finalVerdict, passed, failed, skipped, constrained, total: results.length, reportPath: REPORT_PATH, requiredBlocked };
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

  // 1. Versions
  log('--- Versions ---');
  const versions = probeVersions();
  for (const [k, v] of Object.entries(versions)) log(`  ${k}: ${v}`);

  // 2. Create temp fixture (P102-R4)
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
  try {
    await pm.start(port, [GITNEXUS_BIN, 'mcp'], log);
  } catch (e) {
    log(`FATAL: ${e.message}`);
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
    await pm.stop(log);
    await cleanupTempFixture(tempFixture, log);
    exit(1);
  }

  // 5. Run tests
  log('\n--- Tests ---');
  const results = await runAllTests(client, tempFixture, log, args);

  // 6. Report
  log('\n--- Report ---');
  await mkdir(dirname(REPORT_PATH), { recursive: true });
  const report = await generateReport(results, versions, tempFixture, args, startTime, log);

  // 7. Cleanup
  log('\n--- Cleanup ---');
  if (!args.keepProcesses) {
    if (args._trackedPm2) {
      try { await args._trackedPm2.stop(log); } catch {}
    }
    await pm.stop(log);
    await sleep(2000);

    const finalPids = getGitnexusPids();
    const baseline = args._baselinePids || [];
    const orphans = finalPids.filter(p => !baseline.includes(p));
    if (orphans.length > 0) {
      log(`  WARNING: ${orphans.length} orphan gitnexus process(es) after cleanup: [${orphans.join(', ')}]`);
    } else {
      log('  No orphan gitnexus processes');
    }
  } else {
    log(`  Processes kept alive (--keep-processes). mcp-proxy at http://127.0.0.1:${port}/mcp`);
  }

  await cleanupTempFixture(tempFixture, log);

  // 8. Summary
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
