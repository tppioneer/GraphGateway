# sample-repo

Minimal JavaScript repository used as a deterministic fixture for the
**mcp-proxy ↔ GitNexus MCP interop verification suite** (GGW-P1-02).

## Purpose

- Provides a small, repeatable codebase for GitNexus to index.
- Used by `scripts/interop/verify-mcp-proxy-gitnexus.mjs` to execute
  read-only MCP tools (`list_repos`, `query`, `context`, etc.) through the
  `mcp-proxy` → `gitnexus mcp` transport chain.

## Contents

| File | Purpose |
|---|---|
| `src/main.js` | Entry-point exports: `greet`, `calculateSum`, `isValidEmail`, `delay` |
| `src/utils.js` | Utility exports: `formatDate`, `parseJson`, `deepClone`, `sleep`, `fibonacci` |

## Notes

- This directory is initialised as a standalone git repository by the
  verification script (`git init` + `git commit`).
- It is indexed with `gitnexus analyze` on first run.
- Do **not** commit its `.git` directory or `.gitnexus/` index into the
  enclosing GraphGateway repository.
