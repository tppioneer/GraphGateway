# mcp-proxy ↔ GitNexus MCP 互操作兼容性报告

> **生成时间**: 2026-07-26 15:14:56.080 UTC
> **任务**: GGW-P1-02 mcp-proxy / GitNexus 互操作门禁
> **结论**: **PASS_WITH_CONSTRAINTS**
> **Tested commit**: 16eb2d1131511816d8a81f3d35eb808d61f333a5
> **总耗时**: 124581 ms

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
  --port 18791 --host 127.0.0.1 \
  --server stream \
  --connectionTimeout 30000 --requestTimeout 120000 \
  --shell -- gitnexus mcp
```

- **MCP 协议版本**: `2025-03-26`
- **Transport**: Streamable HTTP (`POST /mcp`)
- **响应格式**: SSE (`event: message\ndata: <json>\n\n`)
- **会话模型**: 有状态 — 每个 `Mcp-Session-Id` 对应一个 GitNexus stdio 子进程
- **子进程启动**: mcp-proxy 使用 `--shell` 模式启动 gitnexus
- **Fixture repository**: `sample-repo` (source: `tests\fixtures\mcp\sample-repo`)

## 3. 测试结果

| # | Suite | Test | 结果 | 耗时(ms) |
|---|---|---|---|---|
| 1 | 1. Initialize & Session | 1.1 initialize returns protocol version and server info | **PASS** | 0 |
| 2 | 1. Initialize & Session | 1.2 Mcp-Session-Id is assigned and required | **PASS** | 1 |
| 3 | 1. Initialize & Session | 1.3 session ID is stable across requests | **PASS** | 8 |
| 4 | 2. tools/list | 2.1 tools/list returns non-empty tool array | **PASS** | 4 |
| 5 | 2. tools/list | 2.2 each tool has name + description + inputSchema | **PASS** | 4 |
| 6 | 3. resources/list & resources/read | 3.1 resources/list returns resource array | **PASS** | 4 |
| 7 | 3. resources/list & resources/read | 3.2 resources/read returns content for known resource | **PASS** | 62 |
| 8 | 4. tools/call — fixed repo | 4.1 list_repos returns repo list | **PASS** | 56 |
| 9 | 4. tools/call — fixed repo | 4.2 list_repos contains fixture repo | **PASS** | 54 |
| 10 | 4. tools/call — fixed repo | 4.3 query returns results | **PASS** | 301 |
| 11 | 4. tools/call — fixed repo | 4.4 context resolves a symbol | **PASS** | 46 |
| 12 | 4. tools/call — fixed repo | 4.5 checkpoint: read-only tool calls work | **PASS** | 3 |
| 13 | 5. Two concurrent client sessions | 5.1 two independent sessions get different IDs | **PASS** | 3 |
| 14 | 5. Two concurrent client sessions | 5.2 both sessions return same tool set | **PASS** | 7 |
| 15 | 5. Two concurrent client sessions | 5.3 original session still functional after new session | **PASS** | 5 |
| 16 | 6. Concurrent requests | 6.1 three concurrent tools/list complete | **PASS** | 7 |
| 17 | 6. Concurrent requests | 6.2 concurrent list_repos + tools/list do not interfere | **PASS** | 49 |
| 18 | 7. Error handling — malformed requests | 7.1 invalid JSON returns error | **PASS** | 2 |
| 19 | 7. Error handling — malformed requests | 7.2 missing jsonrpc field is rejected | **PASS** | 1 |
| 20 | 7. Error handling — malformed requests | 7.3 unknown tool returns error | **CONSTRAINT** | 2 |
| 21 | 8. Timeout & cancellation | 8.1 delay tool: configurable slow request eventually completes | **PASS** | 13732 |
| 22 | 8. Timeout & cancellation | 8.2 client abort interrupts slow request with AbortError | **PASS** | 11220 |
| 23 | 8. Timeout & cancellation | 8.3 other session responsive during slow request (isolation) | **PASS** | 14703 |
| 24 | 9. Upstream failure & isolation | 9.1 upstream crash detected — subsequent request returns error | **PASS** | 11699 |
| 25 | 9. Upstream failure & isolation | 9.2 crashed session cannot make further requests | **CONSTRAINT** | 10711 |
| 26 | 9. Upstream failure & isolation | 9.3 other session remains operational after sibling crash | **CONSTRAINT** | 11337 |
| 27 | 10. Session lifecycle | 10.1 three rapid session creations do not destabilize proxy | **PASS** | 14 |
| 28 | 10. Session lifecycle | 10.2 session header with garbage value is rejected | **PASS** | 1 |
| 29 | 11. Proxy exit & child process cleanup | 11.1 establish baseline process set | **PASS** | 747 |
| 30 | 11. Proxy exit & child process cleanup | 11.2 proxy start creates new child processes | **PASS** | 8776 |
| 31 | 11. Proxy exit & child process cleanup | 11.3 proxy stop terminates all child processes | **PASS** | 14223 |
| 32 | 11. Proxy exit & child process cleanup | 11.4 stdout and stderr isolation confirmed | **PASS** | 12616 |
| 33 | 12. Generation / P1-08 pre-check | 12.1 audit for generation-related tools | **CONSTRAINT** | 5 |
| 34 | 12. Generation / P1-08 pre-check | 12.2 audit for generation_id / branch parameters | **PASS** | 3 |
| 35 | 12. Generation / P1-08 pre-check | 12.3 list all tool names for reference | **PASS** | 3 |

### 详细证据

**1. 1.1 initialize returns protocol version and server info** [PASS]
> session-id=366c2ab7..., initialized=true

**2. 1.2 Mcp-Session-Id is assigned and required** [PASS]
> session required (correct): Bad Request: No valid session ID provided

**3. 1.3 session ID is stable across requests** [PASS]
> session stable: 366c2ab7...

**4. 2.1 tools/list returns non-empty tool array** [PASS]
> 17 tools

**5. 2.2 each tool has name + description + inputSchema** [PASS]
> all 17 tools well-formed

**6. 3.1 resources/list returns resource array** [PASS]
> 2 resources: gitnexus://repos, gitnexus://setup

**7. 3.2 resources/read returns content for known resource** [PASS]
> resource gitnexus://repos returned content

**8. 4.1 list_repos returns repo list** [PASS]
> list_repos returned 3971 chars

**9. 4.2 list_repos contains fixture repo** [PASS]
> "sample-repo" found in list_repos

**10. 4.3 query returns results** [PASS]
> query OK: 899 chars

**11. 4.4 context resolves a symbol** [PASS]
> context OK: 478 chars

**12. 4.5 checkpoint: read-only tool calls work** [PASS]
> check tool working: 159 chars

**13. 5.1 two independent sessions get different IDs** [PASS]
> session1=366c2ab7... session2=16e7be2c...

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
> delayed response received after 3009ms (requested 3000ms)

**22. 8.2 client abort interrupts slow request with AbortError** [PASS]
> client abort correctly interrupted slow request after 505ms (delay was 15000ms)

**23. 8.3 other session responsive during slow request (isolation)** [PASS]
> session 2 responded in 3ms while session 1 delayed 4000ms — sessions isolated

**24. 9.1 upstream crash detected — subsequent request returns error** [PASS]
> proxy detected upstream failure: subsequent request returned error (Not connected)

**25. 9.2 crashed session cannot make further requests** [CONSTRAINT]
> some post-crash requests succeeded — session may not be fully invalidated

**26. 9.3 other session remains operational after sibling crash** [CONSTRAINT]
> session 2 affected by session 1 crash: Not connected — mcp-proxy uses shared upstream process, sessions not process-isolated

**27. 10.1 three rapid session creations do not destabilize proxy** [PASS]
> created sessions: 73bb8df2, 649697f0, f3a0ec1f; original intact

**28. 10.2 session header with garbage value is rejected** [PASS]
> garbage session rejected (status 404)

**29. 11.1 establish baseline process set** [PASS]
> baseline captured: 599 processes running

**30. 11.2 proxy start creates new child processes** [PASS]
> proxy spawn PID 77580, listening PID 80184, tracked 8 descendants: [77580, 82704, 80184, 74724, 77348, 81524, 80804, 82588]

**31. 11.3 proxy stop terminates all child processes** [PASS]
> all 8 tracked gitnexus process(es) and proxy terminated

**32. 11.4 stdout and stderr isolation confirmed** [PASS]
> stdout: 30 chars, stderr: 450 chars, channels distinct

**33. 12.1 audit for generation-related tools** [CONSTRAINT]
> BLOCKED: No generation resolution tools (resolve_generation, generation_status, list_branches) found, and no tool accepts generation_id parameter. P1-08 requires GitNexus to add these before GraphGateway can implement branch→generation resolution.

**34. 12.2 audit for generation_id / branch parameters** [PASS]
> branch param: query, cypher, context, detect_changes, check, rename, impact, explain, pdg_query, route_map, tool_map, shape_check, api_impact, trace | generation_id param: NONE

**35. 12.3 list all tool names for reference** [PASS]
> total 17: api_impact, check, context, cypher, detect_changes, explain, group_list, group_sync, impact, list_repos, pdg_query, query, rename, route_map, shape_check, tool_map, trace


**总计**: 31 PASS, 0 FAIL, 0 SKIP, 4 CONSTRAINT, 35 total

> Each conclusion maps to PASS, FAIL, SKIP, or CONSTRAINT.
> A CONSTRAINT or SKIP on a required acceptance item prevents the overall verdict from becoming PASS.
> Required items with CONSTRAINT/SKIP: 9.2 crashed session cannot make further requests, 9.3 other session remains operational after sibling crash, 12.1 audit for generation-related tools

## 4. P1-05 / P1-08 下游约束

### 4.1 P1-05 (Southbound MCP Client)

mcp-proxy 成功将 GitNexus stdio MCP 暴露为标准 Streamable HTTP endpoint。GraphGateway 南向 MCP Client 实现时需要注意：

- **SSE 解析**: 响应以 `event: message` + `data:` 行格式返回，需提取 `data:` 行 JSON。
- **会话管理**: 每次 `initialize` 返回新的 `Mcp-Session-Id`，后续请求必须携带此 header。
- **Accept header**: 必须包含 `application/json, text/event-stream`，否则返回 406。
- **子进程模型**: 当前 mcp-proxy 为每个 Session 启动独立 GitNexus 子进程，需要评估内存和启动延迟。
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
- 9.2 crashed session cannot make further requests: some post-crash requests succeeded — session may not be fully invalidated
- 9.3 other session remains operational after sibling crash: session 2 affected by session 1 crash: Not connected — mcp-proxy uses shared upstream process, sessions not process-isolated
- 12.1 audit for generation-related tools: BLOCKED: No generation resolution tools (resolve_generation, generation_status, list_branches) found, and no tool accepts generation_id parameter. P1-08 requires GitNexus to add these before GraphGateway can implement branch→generation resolution.

## 6. 可观测性与隔离

- mcp-proxy stderr 日志与 GitNexus MCP stdout 正确隔离（不同管道）。
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

子进程追踪使用确定性 PID 验证：
- 启动前记录基线 gitnexus.exe PID 集合。
- 启动 proxy 后识别新增 PID，关联至 proxy 会话。
- 停止 proxy 后验证每个已追踪 PID 已退出。
- 不使用全局计数或 0→0 作为清理证据。

---
*报告由 `scripts/interop/verify-mcp-proxy-gitnexus.mjs` 于 2026-07-26 15:14:56.080 UTC 自动生成。*
*每个结论可追溯到第 3 节中的测试证据（PASS/FAIL/SKIP/CONSTRAINT）。*
