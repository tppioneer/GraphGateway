# GGW-P1-02 mcp-proxy / GitNexus 互操作门禁

## 元数据

- State: `DRAFT`
- Default executor: Claude Code
- Depends on: 无
- Parallel with: P1-01、P1-03
- Expected HEAD: `TO_BE_SET_AFTER_TASK_CARD_COMMIT`
- Suggested branch: `codex/ggw-p1-02-mcp-interop`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-02`
- Budget: 一次实现，最多两轮整改

## Objective

用可重复的黑盒验证证明 mcp-proxy 能否把 GitNexus stdio MCP 透明暴露为
Streamable HTTP，并明确 generation、会话、并发和关闭语义，作为后续实现门禁。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 8、15、22 节
- mcp-proxy 与 GitNexus 当前安装版本的标准 CLI/MCP 行为

## Execution envelope

这是技术 spike，只产出测试脚本、固定输入和兼容性报告。通过标准 MCP
initialize、tools/list、resources/list/read、tools/call 观察行为，不读取或依赖
mcp-proxy 私有控制接口。

## Invariants

- mcp-proxy 只做传输适配，不承载业务路由。
- 结论必须来自命令和协议报文证据；未知能力标记为未知/不支持。
- 若 generation 能力不足，记录阻塞条件，不发明私有补丁协议。

## Allowed scope

- `scripts/interop/`
- `tests/fixtures/mcp/`
- `docs/compatibility/`
- 本任务所需的开发依赖锁定文件

## Forbidden scope

- 所有生产 Rust crate 和 Tauri 业务代码
- 通过源码猜测替代黑盒验证
- 修改或 fork mcp-proxy/GitNexus
- 将本机索引、用户数据或完整 MCP 敏感载荷提交仓库

## Acceptance criteria

- 报告记录 OS、Node、mcp-proxy、GitNexus 的精确版本和完整启动参数。
- 自动化脚本验证 initialize、会话 ID、tools/list、resources/list/read 和至少一次
  对固定仓库的只读 tools/call。
- 验证并记录：两个客户端会话、并发请求、上游异常、超时、客户端断开和
  proxy 退出后的 GitNexus 子进程行为。
- 列出 GitNexus 暴露的 generation 相关工具、参数、返回值；若不存在，明确
  P1-08 的 `BLOCKED` 条件。
- 报告给出 `PASS`、`PASS_WITH_CONSTRAINTS` 或 `FAIL`，且每个结论可追溯到
  脱敏证据。

## Verification commands

```powershell
node --version
mcp-proxy --version
gitnexus --version
node scripts/interop/verify-mcp-proxy-gitnexus.mjs --fixture tests/fixtures/mcp/sample-repo
```

验证脚本必须可重复运行，负责启动和回收自身创建的进程，并在失败时返回非零码。

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、兼容性结论、版本矩阵、
验证输出、证据文件、对 P1-05/P1-08 的约束。不得把“进程能启动”等同于 MCP
互操作通过。

