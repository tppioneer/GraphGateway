#!/usr/bin/env node
/**
 * controllable-server.mjs — Controllable MCP stdio fixture for P102-R2
 *
 * Implements a minimal MCP server over stdio (JSON-RPC, one message per line).
 * Exposes tools that produce deterministic delay, crash, and disconnect
 * scenarios so the interop runner can verify mcp-proxy error handling without
 * depending on GitNexus internal behaviour.
 *
 * Tools:
 *   echo       — returns the input unchanged
 *   delay      — responds after a configurable delay (timeout testing)
 *   crash      — exits the process with code 1 (upstream crash testing)
 *   disconnect — closes stdin/stdout without exiting (connection-loss testing)
 *   identity   — returns server PID, uptime, and platform (sanity check)
 *   delay_status — returns state of the most recent delay request (P102-R2)
 *
 * Logs to stderr so the MCP transport (stdout) stays clean.
 */

import { createInterface } from 'node:readline';

const log = (msg) => process.stderr.write(`[ctrl-svr] ${msg}\n`);

const TOOLS = [
  {
    name: 'echo',
    description: 'Returns the input arguments unchanged. Used for basic connectivity verification.',
    inputSchema: {
      type: 'object',
      properties: { text: { type: 'string', description: 'Text to echo back' } },
    },
  },
  {
    name: 'delay',
    description: 'Responds after a configurable delay in milliseconds. Use for client/server timeout testing.',
    inputSchema: {
      type: 'object',
      properties: { ms: { type: 'number', description: 'Delay in milliseconds (max 120000)' } },
      required: ['ms'],
    },
  },
  {
    name: 'crash',
    description: 'Causes the server process to exit with code 1 after sending the response. Use for upstream crash testing.',
    inputSchema: { type: 'object', properties: {} },
  },
  {
    name: 'disconnect',
    description: 'Closes stdin and stdout streams without exiting the process. Use for connection-loss testing.',
    inputSchema: { type: 'object', properties: {} },
  },
  {
    name: 'identity',
    description: 'Returns the server PID, uptime, and platform. Always succeeds.',
    inputSchema: { type: 'object', properties: {} },
  },
  {
    name: 'delay_status',
    description: 'Returns the state of the most recent delay request (none, pending, completed). Used by the interop runner to observe whether an aborted client request completed upstream.',
    inputSchema: { type: 'object', properties: {} },
  },
];

function respond(id, result) {
  const msg = JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n';
  process.stdout.write(msg);
}

function rpcError(id, code, message) {
  const msg = JSON.stringify({ jsonrpc: '2.0', id, error: { code, message } }) + '\n';
  process.stdout.write(msg);
}

let initialized = false;

// P102-R2: track the most recent delay request state so the runner can
// observe whether an aborted client request completed upstream.
let lastDelayState = 'none';   // 'none' | 'pending' | 'completed'
let lastDelayMs = 0;

const rl = createInterface({ input: process.stdin, terminal: false });

rl.on('line', (raw) => {
  raw = raw.trim();
  if (!raw) return;

  let req;
  try { req = JSON.parse(raw); } catch {
    log(`unparseable input: ${raw.slice(0, 120)}`);
    return;
  }

  const { id, method, params } = req;

  switch (method) {
    case 'initialize': {
      initialized = true;
      log(`initialize from ${params?.clientInfo?.name || 'unknown'}`);
      respond(id, {
        protocolVersion: '2025-03-26',
        capabilities: { tools: {} },
        serverInfo: { name: 'controllable-fixture', version: '1.0.0' },
      });
      break;
    }

    case 'notifications/initialized':
      log('initialized notification received');
      break;

    case 'tools/list':
      respond(id, { tools: TOOLS });
      break;

    case 'tools/call': {
      const toolName = params?.name;
      const args = params?.arguments || {};

      switch (toolName) {
        case 'echo':
          respond(id, {
            content: [{ type: 'text', text: `echo: ${args.text || '(empty)'}` }],
          });
          break;

        case 'delay': {
          const ms = Math.min(Math.max(0, args.ms || 0), 120_000);
          lastDelayState = 'pending';
          lastDelayMs = ms;
          log(`delay ${ms}ms starting`);
          setTimeout(() => {
            lastDelayState = 'completed';
            log(`delay ${ms}ms complete`);
            respond(id, {
              content: [{ type: 'text', text: `delayed response after ${ms}ms` }],
            });
          }, ms);
          break;
        }

        case 'crash':
          log('crash requested — will exit(1) after response');
          respond(id, {
            content: [{ type: 'text', text: 'crashing now — server process exiting with code 1' }],
          });
          setTimeout(() => {
            log('exiting with code 1');
            process.exit(1);
          }, 100);
          break;

        case 'disconnect':
          log('disconnect requested — will close streams after response');
          respond(id, {
            content: [{ type: 'text', text: 'disconnecting streams — stdout and stdin closing' }],
          });
          setTimeout(() => {
            log('closing stdout and stdin');
            try { process.stdout.end(); } catch {}
            try { process.stdin.end(); } catch {}
            try { process.stdin.destroy(); } catch {}
            // Keep the event loop alive so the process doesn't exit
            setInterval(() => {}, 60_000).unref();
          }, 100);
          break;

        case 'identity':
          respond(id, {
            content: [{
              type: 'text',
              text: JSON.stringify({
                pid: process.pid,
                uptime: Math.round(process.uptime()),
                platform: process.platform,
                arch: process.arch,
                nodeVersion: process.version,
              }),
            }],
          });
          break;

        case 'delay_status':
          respond(id, {
            content: [{ type: 'text', text: JSON.stringify({ state: lastDelayState, ms: lastDelayMs, pid: process.pid }) }],
          });
          break;

        default:
          rpcError(id, -32601, `Unknown tool: ${toolName}`);
      }
      break;
    }

    case 'resources/list':
      respond(id, { resources: [] });
      break;

    case 'resources/read':
      rpcError(id, -32602, 'No resources available on controllable fixture');
      break;

    default:
      if (id !== undefined && id !== null) {
        rpcError(id, -32601, `Method not found: ${method}`);
      }
  }
});

rl.on('close', () => {
  log('stdin closed — server shutting down');
  process.exit(0);
});

process.on('SIGTERM', () => {
  log('SIGTERM received — exiting');
  process.exit(0);
});

process.on('SIGINT', () => {
  log('SIGINT received — exiting');
  process.exit(0);
});

log(`started (pid ${process.pid}, node ${process.version}, ${process.platform})`);
