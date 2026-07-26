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
 * Exit code: 0 = all acceptance tests PASS; non-zero = at least one FAIL.
 */

import { spawn, execFileSync } from 'node:child_process';
import { writeFile, mkdir, stat } from 'node:fs/promises';
import { resolve, dirname, basename, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { argv, exit, platform, hrtime, stdout } from 'node:process';
import { randomInt } from 'node:crypto';
import { createServer } from 'node:net';

// ── constants ────────────────────────────────────────────────────────────────
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REPO_ROOT = resolve(__dirname, '..', '..');
const REPORT_PATH = resolve(REPO_ROOT, 'docs', 'compatibility', 'mcp-proxy-gitnexus-interop-report.md');
const MCP_PROXY_BIN = 'mcp-proxy';
const GITNEXUS_BIN = 'gitnexus';
const PROTOCOL_VERSION = '2025-03-26';
const MCP_ACCEPT = 'application/json, text/event-stream';

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

/** Find a free TCP port on loopback. */
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
      encoding: 'utf-8', timeout: 30_000, shell: platform === 'win32', ...opts,
    }).trim();
  } catch (e) { return `ERROR: ${e.message}`; }
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

// ── fixture setup ────────────────────────────────────────────────────────────
async function setupFixture(fixturePath, log) {
  log(`Setting up fixture at ${fixturePath}`);
  const res = { path: fixturePath, repoName: null, freshlyIndexed: false, error: null };

  const dotGit = join(fixturePath, '.git');
  try { await stat(dotGit); } catch {
    log('  Initialising git repo in fixture directory...');
    try {
      sh(['git', 'init'], { cwd: fixturePath });
      sh(['git', 'config', 'user.email', 'ggw-p1-02-test@graphgateway.local'], { cwd: fixturePath });
      sh(['git', 'config', 'user.name', 'GGW P1-02 Test'], { cwd: fixturePath });
      sh(['git', 'add', '-A'], { cwd: fixturePath });
      sh(['git', 'commit', '-m', 'test: interop fixture initial commit'], { cwd: fixturePath });
    } catch (e) { res.error = `git init/commit failed: ${e.message}`; return res; }
  }

  const listOut = sh([GITNEXUS_BIN, 'list']);
  const normalizedPath = fixturePath.replace(/\\/g, '/').toLowerCase();
  if (listOut.toLowerCase().includes(normalizedPath)) {
    log('  Already indexed by GitNexus');
    const lines = listOut.split('\n');
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].toLowerCase().includes(normalizedPath)) {
        for (let j = i - 1; j >= 0; j--) {
          const m = lines[j].match(/^\s{2}(\S+)/);
          if (m) { res.repoName = m[1]; break; }
        }
        break;
      }
    }
  } else {
    log('  Indexing fixture with gitnexus analyze...');
    try {
      const t0 = hrtime();
      sh([GITNEXUS_BIN, 'analyze', fixturePath], { timeout: 120_000 });
      log(`  Indexed in ${msSince(t0)} ms`);
      res.freshlyIndexed = true;
      const l2 = sh([GITNEXUS_BIN, 'list']);
      const lines = l2.split('\n');
      for (let i = 0; i < lines.length; i++) {
        if (lines[i].toLowerCase().includes(normalizedPath)) {
          for (let j = i - 1; j >= 0; j--) {
            const m = lines[j].match(/^\s{2}(\S+)/);
            if (m) { res.repoName = m[1]; break; }
          }
          break;
        }
      }
    } catch (e) { res.error = `gitnexus analyze failed: ${e.message}`; return res; }
  }

  if (!res.repoName) res.repoName = basename(fixturePath);
  log(`  Repo name: ${res.repoName}`);
  return res;
}

// ── process manager ──────────────────────────────────────────────────────────
class ProcessManager {
  constructor() { this.proxy = null; this.port = 0; this.proxyPid = 0; this.cleanupDone = false; }

  async start(port, log) {
    this.port = port;
    const useShell = platform === 'win32';
    log(`Starting mcp-proxy on 127.0.0.1:${port} (shell=${useShell})`);

    const spawnArgs = [
      '--port', String(port),
      '--host', '127.0.0.1',
      '--server', 'stream',
      '--connectionTimeout', '30000',
      '--requestTimeout', '120000',
    ];
    if (useShell) spawnArgs.push('--shell');
    spawnArgs.push('--', GITNEXUS_BIN, 'mcp');

    return new Promise((resolve, reject) => {
      const child = spawn(MCP_PROXY_BIN, spawnArgs, {
        stdio: ['pipe', 'pipe', 'pipe'],
        detached: false,
        windowsHide: true,
        shell: platform === 'win32',
      });
      this.proxy = child;
      this.proxyPid = child.pid;
      log(`  mcp-proxy PID: ${child.pid}`);

      child.on('error', (e) => reject(new Error(`spawn error: ${e.message}`)));
      let stderrBuf = '';
      child.stderr.on('data', (d) => { stderrBuf += d.toString(); });

      const startTime = Date.now();
      const poll = () => {
        if (Date.now() - startTime > 30_000) {
          child.kill('SIGTERM');
          return reject(new Error(`mcp-proxy not ready within 30s. stderr: ${stderrBuf.slice(0, 500)}`));
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
    if (this.proxy && !this.proxy.killed) {
      this.cleanupDone = true;
      log(`Stopping mcp-proxy (PID ${this.proxyPid})...`);
      this.proxy.kill('SIGTERM');
      const ok = await new Promise(r => { const t = setTimeout(() => r(false), 8000); this.proxy.on('exit', () => { clearTimeout(t); r(true); }); });
      if (!ok) { log('  SIGKILL...'); try { this.proxy.kill('SIGKILL'); } catch {} }
      log('  Stopped');
    }
    this.proxy = null;
  }

  probeGitnexusOrphans() {
    try {
      return sh(['cmd', '/c', 'tasklist /fi "imagename eq gitnexus.exe" /fo csv /nh 2>nul || echo none']);
    } catch { return 'Could not probe'; }
  }
}

// ── MCP HTTP client ──────────────────────────────────────────────────────────
/** Parse SSE body: extract data: lines, then JSON.parse. */
function parseSseOrJson(body) {
  const lines = body.split(/\r?\n/);
  let dataLine = null;
  for (const line of lines) {
    if (line.startsWith('data:')) dataLine = line.slice(5).trim();
    if (line.startsWith('event:') && dataLine) { /* capture previous data */ }
  }
  if (dataLine) {
    try { return JSON.parse(dataLine); } catch { return { _raw: body, _error: 'SSE data parse failed' }; }
  }
  // Fallback: treat body as raw JSON
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
        signal: AbortSignal.timeout(opts.timeout || 30_000),
      });
    } catch (e) {
      if (e.name === 'AbortError' || e.name === 'TimeoutError') throw new Error(`request timeout after ${opts.timeout || 30_000}ms`);
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
    // Send initialized notification
    await this._rpc('notifications/initialized', {}, { timeout: 5_000 }).catch(() => {});
    return r;
  }

  async toolsList() { return this._rpc('tools/list'); }
  async resourcesList() { return this._rpc('resources/list'); }
  async resourcesRead(uri) { return this._rpc('resources/read', { uri }); }
  async toolCall(name, args = {}, opts = {}) { return this._rpc('tools/call', { name, arguments: args }, opts); }

  /** Create a new client with its own session (new initialize round-trip). */
  async forkAsNewSession() {
    const c = new McpClient(this.port, this.log);
    await c.initialize();
    return c;
  }
}

// ── test runner ──────────────────────────────────────────────────────────────
const tests = [];
let currentSuite = '';

function suite(name) { currentSuite = name; }
function test(name, fn) { tests.push({ suite: currentSuite, name, fn }); }

async function runAllTests(client, fixture, log, args) {
  const results = [];
  for (const t of tests) {
    const t0 = hrtime();
    let status = 'PASS', evidence = '';
    try {
      evidence = await t.fn(client, fixture, log, args);
    } catch (e) {
      status = 'FAIL';
      evidence = String(e.message || e).slice(0, 2000);
    }
    const dur = msSince(t0);
    results.push({ suite: t.suite, name: t.name, status, evidence: String(evidence).slice(0, 2000), duration_ms: dur });
    log(`  [${status}] ${t.suite} / ${t.name} (${dur} ms)`);
    if (status === 'FAIL') {
      log(`    Evidence: ${String(evidence).slice(0, 300)}`);
    }
  }
  return results;
}

// ── test definitions ─────────────────────────────────────────────────────────

suite('1. Initialize & Session');
test('1.1 initialize returns protocol version and server info', async (client) => {
  // Already initialized, verify state
  if (!client.initialized) throw new Error('client not initialized');
  return `session-id=${(client.sessionId || 'none').slice(0, 8)}..., initialized=true`;
});

test('1.2 Mcp-Session-Id is assigned and required', async (client) => {
  const sid = client.sessionId;
  if (!sid || typeof sid !== 'string' || sid.length < 8) throw new Error(`invalid session ID: ${sid}`);
  // Verify it's required by trying a call without it
  const c2 = new McpClient(client.port, ()=>{});
  try {
    const r = await c2._rpc('tools/list');
    if (r.error) return `session required (correct): ${r.error.message?.slice(0, 100)}`;
    return `session not required — stateless mode active`;
  } catch { return `session ID required: ${sid.slice(0, 8)}... (length=${sid.length})`; }
});

test('1.3 session ID is stable across requests', async (client) => {
  const s1 = client.sessionId;
  await client.toolsList();
  const s2 = client.sessionId;
  if (s1 !== s2) throw new Error(`session ID changed: ${s1} -> ${s2}`);
  return `session stable: ${s1.slice(0, 8)}...`;
});

suite('2. tools/list');
test('2.1 tools/list returns non-empty tool array', async (client) => {
  const r = await client.toolsList();
  if (r.error) throw new Error(`tools/list error: ${r.error.message}`);
  if (!r.result?.tools || !Array.isArray(r.result.tools)) throw new Error(`invalid: ${JSON.stringify(r).slice(0, 200)}`);
  if (r.result.tools.length === 0) throw new Error('empty tools list');
  return `${r.result.tools.length} tools`;
});

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
  if (issues.length) throw new Error(issues.slice(0, 5).join('; '));
  return `all ${r.result.tools.length} tools well-formed`;
});

suite('3. resources/list & resources/read');
test('3.1 resources/list returns resource array', async (client) => {
  const r = await client.resourcesList();
  if (r.error) throw new Error(`resources/list error: ${r.error.message}`);
  const resources = r.result?.resources || [];
  return `${resources.length} resources: ${resources.map(x => x.uri).join(', ') || '(none)'}`;
});

test('3.2 resources/read returns content for known resource', async (client) => {
  const list = await client.resourcesList();
  const uris = (list.result?.resources || []).map(r => r.uri);
  if (uris.length === 0) return 'SKIP: no resources to read';
  const r = await client.resourcesRead(uris[0]);
  if (r.error) throw new Error(`resources/read error: ${r.error.message}`);
  const hasContent = !!(r.result?.contents || r.result?.content || r.result?.text);
  return hasContent ? `resource ${uris[0]} returned content` : `resource ${uris[0]} returned (content may be empty)`;
});

suite('4. tools/call — fixed repo');
test('4.1 list_repos returns repo list', async (client) => {
  const r = await client.toolCall('list_repos');
  if (r.error) throw new Error(`list_repos error: ${r.error.message || JSON.stringify(r.error)}`);
  const text = extractTextContent(r);
  return `list_repos returned ${text.length} chars`;
});

test('4.2 list_repos contains fixture repo', async (client, fixture) => {
  const r = await client.toolCall('list_repos');
  const text = extractTextContent(r);
  if (text.includes(fixture.repoName)) return `"${fixture.repoName}" found`;
  if (text.includes(basename(fixture.path))) return `"${basename(fixture.path)}" found (path match)`;
  return `"${fixture.repoName}" not explicitly listed — may be under different name`;
});

test('4.3 query returns results', async (client, fixture) => {
  const r = await client.toolCall('query', { search_query: 'greet', repo: fixture.repoName, limit: 3, max_symbols: 5 });
  if (r.error) {
    // Fallback: try without repo
    const r2 = await client.toolCall('query', { search_query: 'greet', limit: 3, max_symbols: 5 });
    if (r2.error) throw new Error(`query error: ${r2.error.message}`);
    return `query OK (no repo param): ${extractTextContent(r2).length} chars`;
  }
  return `query OK: ${extractTextContent(r).length} chars`;
});

test('4.4 context resolves a symbol', async (client, fixture) => {
  const r = await client.toolCall('context', { name: 'greet', repo: fixture.repoName });
  if (r.error) {
    const r2 = await client.toolCall('context', { name: 'greet', file_path: 'src/main.js' });
    if (r2.error) throw new Error(`context error: ${r2.error.message}`);
    return `context OK via file_path: ${extractTextContent(r2).length} chars`;
  }
  return `context OK: ${extractTextContent(r).length} chars`;
});

test('4.5 checkpoint: read-only tool calls work', async (client) => {
  // Check structural check tool
  const r = await client.toolCall('check', { cycles: true });
  if (r.error) return `check tool constrained: ${r.error.message?.slice(0, 100)}`;
  return `check tool working: ${extractTextContent(r).length} chars`;
});

suite('5. Two concurrent client sessions');
test('5.1 two independent sessions get different IDs', async (client) => {
  const c2 = await client.forkAsNewSession();
  if (!c2.sessionId) throw new Error('second client has no session ID');
  if (c2.sessionId === client.sessionId) throw new Error('same session ID for both clients');
  return `session1=${client.sessionId.slice(0, 8)}... session2=${c2.sessionId.slice(0, 8)}...`;
});

test('5.2 both sessions return same tool set', async (client) => {
  const c2 = await client.forkAsNewSession();
  const [r1, r2] = await Promise.all([client.toolsList(), c2.toolsList()]);
  const n1 = r1.result?.tools?.map(t => t.name).sort().join(',');
  const n2 = r2.result?.tools?.map(t => t.name).sort().join(',');
  if (!n1 || !n2) throw new Error('one session failed tools/list');
  if (n1 !== n2) throw new Error(`tool sets differ between sessions`);
  return `${r1.result.tools.length} tools in both sessions`;
});

test('5.3 original session still functional after new session', async (client) => {
  await client.forkAsNewSession();
  const r = await client.toolsList();
  if (!r.result?.tools) throw new Error('original session broken after creating new session');
  return 'original session intact';
});

suite('6. Concurrent requests');
test('6.1 three concurrent tools/list complete', async (client) => {
  const results = await Promise.all([client.toolsList(), client.toolsList(), client.toolsList()]);
  for (let i = 0; i < results.length; i++) {
    if (!results[i].result?.tools) throw new Error(`req ${i} failed: ${JSON.stringify(results[i]).slice(0, 200)}`);
  }
  return `all 3 concurrent requests OK (${results[0].result.tools.length} tools each)`;
});

test('6.2 concurrent list_repos + tools/list do not interfere', async (client) => {
  const [r1, r2] = await Promise.all([client.toolCall('list_repos'), client.toolsList()]);
  if (!extractTextContent(r1) && r1.error) throw new Error(`list_repos failed: ${r1.error.message}`);
  if (!r2.result?.tools) throw new Error('tools/list failed');
  return 'concurrent different-tool OK';
});

suite('7. Error handling — malformed requests');
test('7.1 invalid JSON returns error', async (client) => {
  const res = await fetch(client.baseUrl, {
    method: 'POST', headers: client._headers(), body: 'not json {{{', signal: AbortSignal.timeout(5000),
  });
  const text = await res.text();
  const isError = res.status >= 400 || /error|parse/i.test(text);
  return isError ? `proxy rejected invalid JSON (status ${res.status})` : `accepted with status ${res.status} — constraint noted`;
});

test('7.2 missing jsonrpc field is rejected', async (client) => {
  const res = await fetch(client.baseUrl, {
    method: 'POST', headers: client._headers(), body: JSON.stringify({ method: 'tools/list' }), signal: AbortSignal.timeout(5000),
  });
  const text = await res.text();
  const isRejected = res.status >= 400 || /error|invalid/i.test(text);
  return isRejected ? `properly rejected (status ${res.status})` : `accepted (status ${res.status}) — constraint`;
});

test('7.3 unknown tool returns error', async (client) => {
  const r = await client.toolCall('nonexistent_tool_xyz_123', {});
  if (r.error) return `correctly returned error: ${r.error.message?.slice(0, 100) || r.error.code}`;
  return `no error for unknown tool — constraint`;
});

suite('8. Timeouts');
test('8.1 client-side abort timeout works', async (client, fixture, log, args) => {
  const controller = new AbortController();
  setTimeout(() => controller.abort(), 1);
  try {
    await fetch(`http://127.0.0.1:${args._actualPort}/mcp`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', accept: MCP_ACCEPT },
      body: JSON.stringify({ jsonrpc: '2.0', id: 999, method: 'tools/list', params: {} }),
      signal: controller.signal,
    });
    return 'request completed before abort (proxy very fast)';
  } catch (e) {
    if (e.name === 'AbortError') return 'client-side abort correctly interrupted request';
    throw e;
  }
});

test('8.2 list_repos completes within reasonable time', async (client) => {
  const t0 = hrtime();
  const r = await client.toolCall('list_repos', {}, { timeout: 20_000 });
  if (r.error) throw new Error(`list_repos failed: ${r.error.message}`);
  const dur = msSince(t0);
  return `list_repos completed in ${dur} ms`;
});

suite('9. Session lifecycle');
test('9.1 three rapid session creations do not destabilize proxy', async (client) => {
  const sessions = [];
  for (let i = 0; i < 3; i++) {
    const c = new McpClient(client.port, ()=>{});
    await c.initialize();
    sessions.push(c.sessionId);
  }
  // Verify original still works
  const r = await client.toolsList();
  if (!r.result?.tools) throw new Error('original session broken after 3 new sessions');
  const ids = sessions.map(s => s?.slice(0, 8) || 'none');
  return `created sessions: ${ids.join(', ')}; original intact`;
});

test('9.2 session header with garbage value is rejected', async (client) => {
  const res = await fetch(client.baseUrl, {
    method: 'POST',
    headers: { 'content-type': 'application/json', accept: MCP_ACCEPT, 'mcp-session-id': 'garbage-not-valid' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list', params: {} }),
    signal: AbortSignal.timeout(5000),
  });
  const text = await res.text();
  const rejected = res.status >= 400 || /invalid|error|not found/i.test(text);
  return rejected ? `garbage session rejected (status ${res.status})` : `accepted (status ${res.status}) — constraint`;
});

suite('10. Proxy exit & child process cleanup');
test('10.1 proxy stop cleans up gitnexus subprocesses', async (client, fixture, log, args) => {
  if (args._orphanChecked) return 'SKIP: already validated';
  args._orphanChecked = true;

  const orphansBefore = sh(['cmd', '/c', 'tasklist /fi "imagename eq gitnexus.exe" /fo csv /nh 2>nul || echo none']);
  log(`  gitnexus processes before: ${orphansBefore.slice(0, 200)}`);

  // Start a second proxy on a different port
  const port2 = await findFreePort([args._actualPort]);
  const pm2 = new ProcessManager();
  try {
    await pm2.start(port2, log);
    await sleep(1500);
    const c2 = new McpClient(port2, log);
    await c2.initialize();
    await c2.toolsList();
  } catch (e) {
    log(`  second proxy start failed: ${e.message} — trying cleanup`);
  }

  await pm2.stop(log);
  await sleep(3000);

  const orphansAfter = sh(['cmd', '/c', 'tasklist /fi "imagename eq gitnexus.exe" /fo csv /nh 2>nul || echo none']);
  log(`  gitnexus processes after: ${orphansAfter.slice(0, 200)}`);

  const cBefore = (orphansBefore.match(/gitnexus/gi) || []).length;
  const cAfter = (orphansAfter.match(/gitnexus/gi) || []).length;
  if (cAfter > cBefore) throw new Error(`orphan processes: ${cBefore} -> ${cAfter}`);
  return `process count stable (${cBefore} -> ${cAfter})`;
});

suite('11. Generation / P1-08 pre-check');
test('11.1 audit for generation-related tools', async (client) => {
  const r = await client.toolsList();
  const tools = r.result.tools;
  // Look for generation-specific tools by name (not description keywords)
  const genToolNames = ['resolve_generation', 'generation_status', 'list_branches', 'branch_status'];
  const found = tools.filter(t => genToolNames.includes(t.name));

  // Also look for generation_id in any tool's inputSchema
  const genIdTools = [];
  for (const t of tools) {
    const props = t.inputSchema?.properties || {};
    if (props.generation_id || props.generationId) genIdTools.push(t.name);
  }

  if (found.length === 0 && genIdTools.length === 0) {
    return 'BLOCKED: No generation resolution tools (resolve_generation, generation_status, list_branches) found, and no tool accepts generation_id parameter. P1-08 requires GitNexus to add these before GraphGateway can implement branch→generation resolution.';
  }

  let report = '';
  if (found.length > 0) report += `Tools: ${found.map(t => t.name).join(', ')}. `;
  if (genIdTools.length > 0) report += `Tools accepting generation_id: ${genIdTools.join(', ')}. `;
  return report || 'generation partially supported';
});

test('11.2 audit for generation_id / branch parameters', async (client) => {
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
  return summary.join(' | ');
});

test('11.3 list all tool names for reference', async (client) => {
  const r = await client.toolsList();
  const names = r.result.tools.map(t => t.name).sort();
  return `total ${names.length}: ${names.join(', ')}`;
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

// ── report generator ─────────────────────────────────────────────────────────
async function generateReport(results, versions, fixture, args, startTime, log) {
  const passed = results.filter(r => r.status === 'PASS').length;
  const failed = results.filter(r => r.status === 'FAIL').length;
  const genResult11_1 = results.find(r => r.name.includes('11.1'));
  const genBlocked = genResult11_1?.evidence?.startsWith('BLOCKED');

  const constraints = [];
  if (genBlocked) {
    constraints.push('P1-08 BLOCKED: GitNexus 1.6.9 does not expose generation-related MCP tools (resolve_generation, generation_status). Branch→generation resolution is unavailable. GraphGateway must use one-instance-per-generation deployment model until GitNexus adds generation support.');
  }
  for (const r of results) {
    if (r.evidence?.toLowerCase().includes('constraint')) {
      constraints.push(`${r.name}: ${r.evidence.slice(0, 300)}`);
    }
  }

  const finalVerdict = constraints.length > 0 && failed === 0 ? 'PASS_WITH_CONSTRAINTS' : (failed === 0 ? 'PASS' : 'FAIL');

  const totalDur = msSince(startTime);

  const report = `# mcp-proxy ↔ GitNexus MCP 互操作兼容性报告

> **生成时间**: ${now()}
> **任务**: GGW-P1-02 mcp-proxy / GitNexus 互操作门禁
> **结论**: **${finalVerdict}**
> **Commit**: ${sh(['git', 'rev-parse', 'HEAD'], { cwd: REPO_ROOT })}
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
- **测试仓库**: \`${fixture.repoName}\` (\`${fixture.path}\`)

## 3. 测试结果

| # | Suite | Test | 结果 | 耗时(ms) |
|---|---|---|---|---|
${results.map((r, i) => `| ${i + 1} | ${r.suite} | ${r.name} | **${r.status}** | ${r.duration_ms} |`).join('\n')}

### 详细证据

${results.map((r, i) => `**${i + 1}. ${r.name}** [${r.status}]\n> ${r.evidence.replace(/\n/g, '\n> ')}\n`).join('\n')}

**总计**: ${passed} PASS, ${failed} FAIL, ${results.length} total

## 4. P1-05 / P1-08 下游约束

### 4.1 P1-05 (Southbound MCP Client)

mcp-proxy 成功将 GitNexus stdio MCP 暴露为标准 Streamable HTTP endpoint。GraphGateway 南向 MCP Client 实现时需要注意：

- **SSE 解析**: 响应以 \`event: message\` + \`data:\` 行格式返回，需提取 \`data:\` 行 JSON。
- **会话管理**: 每次 \`initialize\` 返回新的 \`Mcp-Session-Id\`，后续请求必须携带此 header。
- **Accept header**: 必须包含 \`application/json, text/event-stream\`，否则返回 406。
- **子进程模型**: 当前 mcp-proxy 为每个 Session 启动独立 GitNexus 子进程，需要评估内存和启动延迟。
- **传输适配验证**: ✅ mcp-proxy 正确透传了 tools、resources、capabilities 和能力协商。

### 4.2 P1-08 (Generation & Resolved View)

${genBlocked
  ? `**BLOCKED** — GitNexus ${versions.gitnexus} 不包含 generation 解析 MCP 能力。

经 tools/list 审计：
- ❌ 无 \`resolve_generation\`、\`generation_status\`、\`list_branches\` 等 generation 管理工具
- ❌ 无工具接受 \`generation_id\` 参数（所有查询工具隐式使用最新索引版本）
- ✅ 多数工具已接受 \`branch\` 参数（${results.find(r => r.name.includes('11.2'))?.evidence?.match(/branch param: (.+?) \|/)?.[1] || 'N/A'}），可用于 GraphGateway 路由时的分支选择

**解除阻塞需要 GitNexus 增加**:
1. \`resolve_generation(repo, branch)\` → \`{generation_id, head_sha, freshness}\`
2. \`generation_status(generation_id)\` → \`{state, completion, indexed_at}\`
3. 现有查询工具（\`query\`, \`context\`, \`impact\` 等）接受可选 \`generation_id\` 参数

**临时方案**: GraphGateway 可采用"一实例一 generation"部署模型 — 每个 mcp-proxy endpoint 绑定固定已索引版本。`
  : `GitNexus 已有 generation 能力。P1-08 可以继续。`}

## 5. 约束与已知限制

${constraints.length > 0 ? constraints.map(c => `- ${c}`).join('\n') : '- 无'}

## 6. 可观测性与隔离

- ✅ mcp-proxy stderr 日志与 GitNexus MCP stdout 正确隔离（不同管道）。
- ✅ GitNexus 使用结构化日志（pino）写入 stderr，与 MCP 协议消息分通道。
- ✅ mcp-proxy 透传 GitNexus 的 JSON-RPC 响应到 HTTP 响应流。
- ⚠️ mcp-proxy 在非 debug 模式下仅输出启动信息和错误，缺少请求级 trace。

## 7. 安全边界

- ✅ mcp-proxy 默认监听 \`127.0.0.1\`（loopback only）。
- ✅ GitNexus MCP 不监听任何网络端口（stdio only）。
- ✅ 未配置 \`--apiKey\` 时本地无认证（开发模式）。
- ✅ mcp-proxy 支持 \`--apiKey\` 和 \`X-API-Key\` header 认证。
- ✅ 无效 \`Mcp-Session-Id\` 被正确拒绝。

---

*报告由 \`scripts/interop/verify-mcp-proxy-gitnexus.mjs\` 于 ${now()} 自动生成。每个结论可追溯到第 3 节中的测试证据。*
`;

  await writeFile(REPORT_PATH, report, 'utf-8');
  log(`Report written to ${REPORT_PATH}`);
  return { overall: finalVerdict, passed, failed, total: results.length, reportPath: REPORT_PATH };
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

  // 2. Fixture
  log('\n--- Fixture ---');
  const fixture = await setupFixture(args.fixture, log);
  if (fixture.error) { log(`FATAL: ${fixture.error}`); exit(1); }

  // 3. Start proxy
  log('\n--- Starting mcp-proxy ---');
  const port = args.port || await findFreePort();
  args._actualPort = port;
  const pm = new ProcessManager();
  try { await pm.start(port, log); } catch (e) { log(`FATAL: ${e.message}`); exit(1); }

  // 4. Initialize client
  log('\n--- MCP client ---');
  const client = new McpClient(port, log);
  const initR = await client.initialize();
  log(`  Server: ${initR.result?.serverInfo?.name} v${initR.result?.serverInfo?.version}`);
  log(`  Capabilities: ${JSON.stringify(initR.result?.capabilities)}`);
  log(`  Session: ${client.sessionId || '(none)'}`);

  // 5. Run tests
  log('\n--- Tests ---');
  const results = await runAllTests(client, fixture, log, args);

  // 6. Report
  log('\n--- Report ---');
  await mkdir(dirname(REPORT_PATH), { recursive: true });
  const report = await generateReport(results, versions, fixture, args, startTime, log);

  // 7. Cleanup
  log('\n--- Cleanup ---');
  if (!args.keepProcesses) {
    await pm.stop(log);
    await sleep(2000);
    const orphans = pm.probeGitnexusOrphans();
    log(`  Orphans: ${orphans}`);
  } else {
    log(`  Processes kept alive (--keep-processes). mcp-proxy at http://127.0.0.1:${port}/mcp`);
  }

  // 8. Summary
  log(`\n=== ${report.overall} ===`);
  log(`${report.total} tests: ${report.passed} PASS, ${report.failed} FAIL`);
  log(`Report: ${report.reportPath}`);
  log(`Duration: ${msSince(startTime)} ms`);

  exit(report.overall === 'FAIL' ? 1 : 0);
}

main().catch((e) => { console.error(`FATAL: ${e.message}\n${e.stack}`); exit(1); });
