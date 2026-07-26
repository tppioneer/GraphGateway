# mcp-proxy ↔ GitNexus MCP 互操作兼容性报告

> **生成时间**: 2026-07-26 18:34:14.812 UTC
> **任务**: GGW-P1-02 mcp-proxy / GitNexus 互操作门禁
> **结论**: **PASS_WITH_CONSTRAINTS**
> **Tested commit**: 414632dacc819d4c9c36fc863a3c608cec8fffd5
> **总耗时**: 182620 ms

---

## 1. 版本矩阵

| 组件 | 版本 |
|---|---|
| OS | win32 x64 |
| Node.js | v24.14.1 |
| mcp-proxy | 6.5.4 |
| GitNexus | 1.6.9 |

## 2. 启动参数与传输

```sh
mcp-proxy \
  --port 24955 --host 127.0.0.1 \
  --server stream \
  --connectionTimeout 30000 --requestTimeout 120000 \
  --shell -- gitnexus mcp
```

- **MCP 协议版本**: `2025-03-26`
- **Transport**: Streamable HTTP (`POST /mcp`)
- **响应格式**: SSE (`event: message\ndata: <json>\n\n`)
- **会话模型**: 有状态 — 每个 Mcp-Session-Id 是独立会话标识，但所有会话共享一个 GitNexus stdio 上游进程。一个会话的上游崩溃会影响其他会话。
- **子进程启动**: mcp-proxy 使用 `--shell` 模式启动 gitnexus
- **Fixture repository**: `sample-repo` (source: `tests\fixtures\mcp\sample-repo`)

## 3. 测试结果

| # | Suite | Test | 结果 | 耗时(ms) |
|---|---|---|---|---|
| 1 | 1. Initialize & Session | 1.1 initialize returns protocol version and server info | **PASS** | 0 |
| 2 | 1. Initialize & Session | 1.2 Mcp-Session-Id is assigned and required | **PASS** | 1 |
| 3 | 1. Initialize & Session | 1.3 session ID is stable across requests | **PASS** | 7 |
| 4 | 2. tools/list | 2.1 tools/list returns non-empty tool array | **PASS** | 4 |
| 5 | 2. tools/list | 2.2 each tool has name + description + inputSchema | **PASS** | 4 |
| 6 | 3. resources/list & resources/read | 3.1 resources/list returns resource array | **PASS** | 3 |
| 7 | 3. resources/list & resources/read | 3.2 resources/read returns content for known resource | **PASS** | 64 |
| 8 | 4. tools/call — fixed repo | 4.1 list_repos returns repo list | **PASS** | 60 |
| 9 | 4. tools/call — fixed repo | 4.2 list_repos contains fixture repo | **PASS** | 57 |
| 10 | 4. tools/call — fixed repo | 4.3 query returns results — bound to fixture repo | **PASS** | 5 |
| 11 | 4. tools/call — fixed repo | 4.4 context resolves a symbol — bound to fixture repo | **PASS** | 3 |
| 12 | 4. tools/call — fixed repo | 4.5 checkpoint: read-only tool calls work | **PASS** | 3 |
| 13 | 5. Two concurrent client sessions | 5.1 two independent sessions get different IDs | **PASS** | 3 |
| 14 | 5. Two concurrent client sessions | 5.2 both sessions return same tool set | **PASS** | 7 |
| 15 | 5. Two concurrent client sessions | 5.3 original session still functional after new session | **PASS** | 4 |
| 16 | 6. Concurrent requests | 6.1 three concurrent tools/list complete | **PASS** | 6 |
| 17 | 6. Concurrent requests | 6.2 concurrent list_repos + tools/list do not interfere | **PASS** | 64 |
| 18 | 7. Error handling — malformed requests | 7.1 invalid JSON returns error | **PASS** | 2 |
| 19 | 7. Error handling — malformed requests | 7.2 missing jsonrpc field is rejected | **PASS** | 1 |
| 20 | 7. Error handling — malformed requests | 7.3 unknown tool returns error | **CONSTRAINT** | 3 |
| 21 | 8. Timeout, cancellation & disconnect | 8.1 delay tool: configurable slow request eventually completes | **PASS** | 13786 |
| 22 | 8. Timeout, cancellation & disconnect | 8.2 client abort: observe upstream final state and session behavior | **CONSTRAINT** | 17575 |
| 23 | 8. Timeout, cancellation & disconnect | 8.3 other session responsive during slow request (isolation) | **PASS** | 14754 |
| 24 | 8. Timeout, cancellation & disconnect | 8.4 upstream disconnect: session invalidated, proxy stays alive | **CONSTRAINT** | 11523 |
| 25 | 9. Upstream failure & isolation | 9.1 upstream crash detected — subsequent request returns error | **PASS** | 11737 |
| 26 | 9. Upstream failure & isolation | 9.2 crashed session cannot make further requests | **CONSTRAINT** | 10725 |
| 27 | 9. Upstream failure & isolation | 9.3 other session remains operational after sibling crash | **CONSTRAINT** | 11360 |
| 28 | 10. Session lifecycle | 10.1 three rapid session creations do not destabilize proxy | **PASS** | 16 |
| 29 | 10. Session lifecycle | 10.2 session header with garbage value is rejected | **PASS** | 1 |
| 30 | 11. Proxy exit & child process cleanup | 11.1 verify process tracking primitives | **PASS** | 707 |
| 31 | 11. Proxy exit & child process cleanup | 11.2 deterministic descendant tracking — no system-wide baseline | **PASS** | 22187 |
| 32 | 11. Proxy exit & child process cleanup | 11.3 proxy stop terminates all tracked child processes | **PASS** | 13238 |
| 33 | 11. Proxy exit & child process cleanup | 11.4 stdout and stderr content isolation | **PASS** | 12690 |
| 34 | 12. Generation / P1-08 pre-check | 12.1 audit for generation-related tools | **CONSTRAINT** | 5 |
| 35 | 12. Generation / P1-08 pre-check | 12.2 audit for generation_id / branch parameters | **PASS** | 3 |
| 36 | 12. Generation / P1-08 pre-check | 12.3 list all tool names for reference | **PASS** | 3 |
| 37 | 13. Main proxy cleanup | 13.1 main proxy stop terminates all child processes | **PASS** | 28273 |
| 38 | 13. Main proxy cleanup | 13.4 temporary fixture cleanup | **PASS** | 11 |

### 详细证据

**1. 1.1 initialize returns protocol version and server info** [PASS]
> session-id=217c2a88..., initialized=true

**2. 1.2 Mcp-Session-Id is assigned and required** [PASS]
> session required (correct): Bad Request: No valid session ID provided

**3. 1.3 session ID is stable across requests** [PASS]
> session stable: 217c2a88...

**4. 2.1 tools/list returns non-empty tool array** [PASS]
> 17 tools

**5. 2.2 each tool has name + description + inputSchema** [PASS]
> all 17 tools well-formed

**6. 3.1 resources/list returns resource array** [PASS]
> 2 resources: gitnexus://repos, gitnexus://setup

**7. 3.2 resources/read returns content for known resource** [PASS]
> resource gitnexus://repos returned content

**8. 4.1 list_repos returns repo list** [PASS]
> list_repos returned 4474 chars

**9. 4.2 list_repos contains fixture repo** [PASS]
> "sample-repo" found in list_repos

**10. 4.3 query returns results — bound to fixture repo** [PASS]
> query OK (repo="sample-repo"): 285 chars

**11. 4.4 context resolves a symbol — bound to fixture repo** [PASS]
> context OK (repo="sample-repo"): 285 chars

**12. 4.5 checkpoint: read-only tool calls work** [PASS]
> check tool working: 322 chars

**13. 5.1 two independent sessions get different IDs** [PASS]
> session1=217c2a88... session2=9c1755cf...

**14. 5.2 both sessions return same tool set** [PASS]
> 17 tools in both sessions

**15. 5.3 original session still functional after new session** [PASS]
> original session intact

**16. 6.1 three concurrent tools/list complete** [PASS]
> all 3 concurrent requests OK (17 tools each)

**17. 6.2 concurrent list_repos + tools/list do not interfere** [PASS]
> concurrent different-tool OK

**18. 7.1 invalid JSON returns error** [PASS]
> proxy rejected invalid JSON (status 400)

**19. 7.2 missing jsonrpc field is rejected** [PASS]
> properly rejected (status 400)

**20. 7.3 unknown tool returns error** [CONSTRAINT]
> unknown tool returned no error — proxy passes through backend response

**21. 8.1 delay tool: configurable slow request eventually completes** [PASS]
> delayed response received after 3013ms (requested 3000ms)

**22. 8.2 client abort: observe upstream final state and session behavior** [CONSTRAINT]
> upstream continued and completed the aborted request (abort not propagated to upstream). abort interrupted local request; upstream PID 98364=alive; upstream request state=completed; original session alive=true; sibling session alive=true

**23. 8.3 other session responsive during slow request (isolation)** [PASS]
> session 2 responded in 4ms while session 1 delayed 4000ms — sessions isolated

**24. 8.4 upstream disconnect: session invalidated, proxy stays alive** [CONSTRAINT]
> disconnected session invalidated successfully; new session constrained: new session created but identity failed: Not connected — proxy may share upstream process across sessions

**25. 9.1 upstream crash detected — subsequent request returns error** [PASS]
> proxy detected upstream failure: subsequent request returned error (Not connected)

**26. 9.2 crashed session cannot make further requests** [CONSTRAINT]
> some post-crash requests succeeded — session may not be fully invalidated

**27. 9.3 other session remains operational after sibling crash** [CONSTRAINT]
> session 2 affected by session 1 crash: Not connected — mcp-proxy uses shared upstream process, sessions not process-isolated

**28. 10.1 three rapid session creations do not destabilize proxy** [PASS]
> created sessions: 9a184e74, 3b6ea47d, 8772fb0b; original intact

**29. 10.2 session header with garbage value is rejected** [PASS]
> garbage session rejected (status 404)

**30. 11.1 verify process tracking primitives** [PASS]
> tracking primitives functional: pidExists=ok, portPid=100184, childrenFound=1

**31. 11.2 deterministic descendant tracking — no system-wide baseline** [PASS]
> proxy PID 98596, 2 tracked alive descendant(s): [92184, 73664]

**32. 11.3 proxy stop terminates all tracked child processes** [PASS]
> all 2 tracked descendant(s) and proxy terminated

**33. 11.4 stdout and stderr content isolation** [PASS]
> stdout: 30 chars (proxy log), stderr: 464 chars (no protocol leakage)

**34. 12.1 audit for generation-related tools** [CONSTRAINT]
> BLOCKED: No generation resolution tools (resolve_generation, generation_status, list_branches) found, and no tool accepts generation_id parameter. P1-08 requires GitNexus to add these before GraphGateway can implement branch→generation resolution.

**35. 12.2 audit for generation_id / branch parameters** [PASS]
> branch param: query, cypher, context, detect_changes, check, rename, impact, explain, pdg_query, route_map, tool_map, shape_check, api_impact, trace | generation_id param: NONE

**36. 12.3 list all tool names for reference** [PASS]
> total 17: api_impact, check, context, cypher, detect_changes, explain, group_list, group_sync, impact, list_repos, pdg_query, query, rename, route_map, shape_check, tool_map, trace

**37. 13.1 main proxy stop terminates all child processes** [PASS]
> main proxy cleanup: all 2 tracked descendant(s) terminated

**38. 13.4 temporary fixture cleanup** [PASS]
> temp fixture removed: ggw-p1-02-1785090672643


**总计**: 32 PASS, 0 FAIL, 0 SKIP, 6 CONSTRAINT, 38 total

> Each conclusion maps to PASS, FAIL, SKIP, or CONSTRAINT.
> A CONSTRAINT or SKIP on a required acceptance item prevents the overall verdict from becoming PASS.
> Required items with CONSTRAINT/SKIP: 8.2 client abort: observe upstream final state and session behavior, 8.4 upstream disconnect: session invalidated, proxy stays alive, 9.2 crashed session cannot make further requests, 9.3 other session remains operational after sibling crash, 12.1 audit for generation-related tools

## 4. P1-05 / P1-08 下游约束

### 4.1 P1-05 (Southbound MCP Client)

mcp-proxy 成功将 GitNexus stdio MCP 暴露为标准 Streamable HTTP endpoint。GraphGateway 南向 MCP Client 实现时需要注意：

- **SSE 解析**: 响应以 `event: message` + `data:` 行格式返回，需提取 `data:` 行 JSON。
- **会话管理**: 每次 `initialize` 返回新的 `Mcp-Session-Id`，后续请求必须携带此 header。
- **Accept header**: 必须包含 `application/json, text/event-stream`，否则返回 406。
- **子进程模型**: 有状态 — 每个 Mcp-Session-Id 是独立会话标识，但所有会话共享一个 GitNexus stdio 上游进程。一个会话的上游崩溃会影响其他会话。
- **传输适配验证**: mcp-proxy 正确透传了 tools、resources、capabilities 和能力协商。

### 4.2 P1-08 (Generation & Resolved View)

**BLOCKED** — GitNexus 1.6.9 不包含 generation 解析 MCP 能力。

经 tools/list 审计：
- 无 `resolve_generation`、`generation_status`、`list_branches` 等 generation 管理工具
- 无工具接受 `generation_id` 参数（所有查询工具隐式使用最新索引版本）
- 多数工具已接受 `branch` 参数（query, cypher, context, detect_changes, check, rename, impact, explain, pdg_query, route_map, tool_map, shape_check, api_impact, trace），可用于 GraphGateway 路由时的分支选择

**解除阻塞需要 GitNexus 增加**:
1. `resolve_generation(repo, branch)` → `{generation_id, head_sha, freshness}`
2. `generation_status(generation_id)` → `{state, completion, indexed_at}`
3. 现有查询工具（`query`, `context`, `impact` 等）接受可选 `generation_id` 参数

**临时方案**: GraphGateway 可采用"一实例一 generation"部署模型 — 每个 mcp-proxy endpoint 绑定固定已索引版本。

## 5. 约束与已知限制

- P1-08 BLOCKED: GitNexus 1.6.9 does not expose generation-related MCP tools (resolve_generation, generation_status). Branch→generation resolution is unavailable. GraphGateway must use one-instance-per-generation deployment model until GitNexus adds generation support.
- 7.3 unknown tool returns error: unknown tool returned no error — proxy passes through backend response
- 8.2 client abort: observe upstream final state and session behavior: upstream continued and completed the aborted request (abort not propagated to upstream). abort interrupted local request; upstream PID 98364=alive; upstream request state=completed; original session alive=true; sibling session alive=true
- 8.4 upstream disconnect: session invalidated, proxy stays alive: disconnected session invalidated successfully; new session constrained: new session created but identity failed: Not connected — proxy may share upstream process across sessions
- 9.2 crashed session cannot make further requests: some post-crash requests succeeded — session may not be fully invalidated
- 9.3 other session remains operational after sibling crash: session 2 affected by session 1 crash: Not connected — mcp-proxy uses shared upstream process, sessions not process-isolated
- 12.1 audit for generation-related tools: BLOCKED: No generation resolution tools (resolve_generation, generation_status, list_branches) found, and no tool accepts generation_id parameter. P1-08 requires GitNexus to add these before GraphGateway can implement branch→generation resolution.

## 6. 可观测性与隔离

- mcp-proxy stderr 日志与 GitNexus MCP stdout 通过不同管道隔离。
- GitNexus 使用结构化日志（pino）写入 stderr，与 MCP 协议消息分通道。
- mcp-proxy 透传 GitNexus 的 JSON-RPC 响应到 HTTP 响应流。
- mcp-proxy 在非 debug 模式下仅输出启动信息和错误，缺少请求级 trace。

## 7. 安全边界

- mcp-proxy 默认监听 `127.0.0.1`（loopback only）。
- GitNexus MCP 不监听任何网络端口（stdio only）。
- 未配置 `--apiKey` 时本地无认证（开发模式）。
- mcp-proxy 支持 `--apiKey` 和 `X-API-Key` header 认证。
- 无效 `Mcp-Session-Id` 被正确拒绝。

## 8. 进程清理验证 (P102-R3)

子进程追踪使用确定性 PID 父子关系验证：
- 启动 proxy 后通过监听端口解析实际 PID。
- 使用 parent/child 系统调用（WMI / pgrep）递归查找所有子进程。
- 通过代理功能运行验证子进程确实服务 MCP 流量（所有权证明）。
- 停止 proxy 后，使用 pidExists 逐个验证已追踪 PID 已退出。
- 不使用全系统 PID 快照差值作为关联证据。
- 清理结果（含孤儿 PID）计入测试结果和整体 verdict。

---
*报告由 `scripts/interop/verify-mcp-proxy-gitnexus.mjs` 于 2026-07-26 18:34:14.812 UTC 自动生成。*
*每个结论可追溯到第 3 节中的测试证据（PASS/FAIL/SKIP/CONSTRAINT）。*
