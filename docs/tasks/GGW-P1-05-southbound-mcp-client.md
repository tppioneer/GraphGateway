# GGW-P1-05 南向 MCP 客户端与能力协商

## 元数据

- State: `DRAFT`
- Default executor: Claude Code
- Depends on: P1-02、P1-03
- Parallel with: P1-04
- Expected HEAD: `TO_BE_SET_AFTER_P1_02_AND_P1_03_INTEGRATION`
- Suggested branch: `codex/ggw-p1-05-southbound-mcp`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-05`
- Budget: 一次实现，最多两轮整改

## Objective

实现 GraphGateway 到下游节点的标准 Streamable HTTP MCP 客户端，支持会话、
能力协商、超时/取消、结构化错误和受控重连。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 5、8、9、15 节
- P1-02 兼容性报告
- P1-03 领域契约

## Execution envelope

新增 transport/client 边界，只处理 MCP 协议，不进行 Workspace 选择、generation
解析、QueryPlan 或进程拉起。测试使用可控的本地 mock MCP server。

## Invariants

- GraphGateway 只调用标准 MCP 能力，不调用 mcp-proxy 私有 API。
- 下游 capability 以 initialize 和 list 响应为准，不靠配置猜测。
- 超时、取消和断线不会无限重试或泄漏会话。

## Allowed scope

- 新增 `crates/graphgateway-mcp/`
- Workspace `Cargo.toml` / `Cargo.lock`
- MCP client 的 mock、协议 fixture 和测试
- P1-02 报告要求的兼容约束实现

## Forbidden scope

- mcp-proxy/GitNexus 进程管理
- 北向 MCP server
- Workspace/Source 持久化、QueryPlan、Tauri UI
- 针对 GitNexus 返回体的未记录私有解析

## Acceptance criteria

- 支持 initialize、initialized、tools/list、tools/call、resources/list、
  resources/read 及 Streamable HTTP 会话头。
- 能力快照包含协议版本、工具/资源及其 schema，且能转换为 P1-03 类型。
- 每请求支持 timeout/cancellation；断线、无效 JSON-RPC、协议错误和 HTTP 错误
  映射为稳定错误。
- 会话关闭和 client drop 不遗留请求任务；重连有次数/退避上限。
- mock server 测试覆盖成功、能力变化、超时、取消、断线和错误响应。

## Verification commands

```powershell
cargo fmt --all -- --check
cargo clippy -p graphgateway-mcp --all-targets --all-features -- -D warnings
cargo test -p graphgateway-mcp --all-features
cargo test --workspace --all-features
```

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、支持的 MCP 方法、变更文件、
验证结果、与 P1-02 的偏差。发现互操作报告不足时应阻塞，不得加入私有旁路。

