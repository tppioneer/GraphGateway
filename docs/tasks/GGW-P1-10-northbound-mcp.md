# GGW-P1-10 北向 Streamable HTTP MCP 单源垂直链路

## 元数据

- State: `DRAFT`
- Default executor: Claude Code
- Depends on: P1-05、P1-06、P1-09
- Parallel with: 无（阶段收口任务）
- Expected HEAD: `TO_BE_SET_AFTER_P1_05_P1_06_AND_P1_09_INTEGRATION`
- Suggested branch: `codex/ggw-p1-10-northbound-mcp`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-10`
- Budget: 一次实现，最多两轮整改

## Objective

把单源规划器通过 GraphGateway 北向 Streamable HTTP MCP 暴露，完成 Workspace
会话绑定、资源发现以及 query/context/impact 的端到端调用。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 5、6、14、15、16、22 节
- P1-05 MCP 协议边界、P1-06 管理面、P1-09 dispatcher

## Execution envelope

在 server 装配标准 MCP server。客户端会话显式绑定一个 Workspace；每次工具
调用创建 ResolvedGraphView 和 QueryPlan。只交付单源 read 工具，不做多源和写
工具。

## Invariants

- 北向是标准 MCP Streamable HTTP，管理 REST 不是 MCP transport。
- MCP session 与 Workspace 绑定明确、可审计且有生命周期上限。
- loopback/Bearer token/Origin/request-id 安全规则继续生效。
- GraphGateway 返回自身稳定 envelope，不透传 proxy 私有结构。

## Allowed scope

- `crates/graphgateway-mcp/` 中 server 侧
- `crates/graphgateway-server/` 装配和会话绑定
- `crates/graphgateway-rest/` 中仅共享认证/状态边界的最小调整
- MCP conformance fixture、端到端测试和协议文档

## Forbidden scope

- 多源 fan-out、结果合并、写工具
- Tauri 页面、WebView 直连
- 暴露 mcp-proxy endpoint/token
- 让客户端在一次 tool call 中绕过 Workspace 任意选择本地路径

## Acceptance criteria

- `/mcp` 完成 initialize、会话管理、tools/list、tools/call、
  resources/list/read 的标准交互。
- 提供 Workspace 列表/详情及 Source/generation/capability 只读资源。
- 会话绑定不存在/无 Source 的 Workspace 时返回稳定错误；会话过期后可回收。
- query/context/impact 经真实 server 边界到 mock/fixture 节点完成单源调用，
  响应含成员级 provenance。
- 集成测试覆盖认证、Origin、两个隔离会话、取消、超时、下游失败和服务关闭。

## Verification commands

```powershell
cargo fmt --all -- --check
cargo clippy -p graphgateway-mcp -p graphgateway-server -p graphgateway-rest --all-targets --all-features -- -D warnings
cargo test -p graphgateway-mcp -p graphgateway-server -p graphgateway-rest --all-features
cargo test --workspace --all-features
```

另运行本卡新增的 MCP 端到端 conformance test；命令须写入仓库文档并适用于
Windows PowerShell。

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、北向 MCP 方法/资源清单、
端到端证据、变更文件、验证结果、仍被明确排除的能力。

