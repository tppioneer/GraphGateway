# GGW-P1-07 proxy / GitNexus 节点生命周期与能力注册

## 元数据

- State: `DRAFT`
- Default executor: Claude Code
- Depends on: P1-04、P1-05
- Parallel with: P1-06
- Expected HEAD: `TO_BE_SET_AFTER_P1_04_AND_P1_05_INTEGRATION`
- Suggested branch: `codex/ggw-p1-07-node-lifecycle`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-07`
- Budget: 一次实现，最多两轮整改

## Objective

管理 `Source + generation` 对应的单个 mcp-proxy/GitNexus 节点实例，完成启动、
MCP readiness、能力注册、健康检查和确定性回收。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 7、8、9、10、22 节
- P1-02 互操作约束
- P1-04 registry、P1-05 MCP client

## Execution envelope

新增节点管理边界。MVP 坚持一个 endpoint 对应一个 Source/generation；只做单
实例启动停止和异常恢复，本卡不做新旧 generation 蓝绿切换。

## Invariants

- readiness 必须完成 MCP initialize/能力读取，不能只判断端口可连接。
- endpoint 仅绑定 loopback；进程参数按结构化参数传递，禁止 shell 拼接。
- 运行态 PID/session/token 只存在内存，持久化层只保存可恢复元数据。
- 能力注册来自实际 MCP 响应。

## Allowed scope

- 新增 `crates/graphgateway-nodes/`
- Workspace manifest/lock
- 节点进程测试 fixture、集成测试和必要文档
- 与 storage/MCP client 的最小装配代码

## Forbidden scope

- 北向 MCP、QueryPlan、Tauri UI
- 蓝绿切换和旧 generation 回收策略（P1-11）
- 修改 mcp-proxy/GitNexus 或使用私有控制 API
- 接受任意远程 host、执行用户拼接的命令字符串

## Acceptance criteria

- 同一 Source/generation 的并发启动被合并，不产生重复节点。
- 启动过程分配 loopback endpoint，完成 MCP readiness 后才发布能力快照。
- 启动失败、readiness 超时、运行中崩溃和显式停止都能回收整个进程树。
- 日志区分 proxy 与 GitNexus，包含 Source/generation/request correlation，
  且不泄漏 token。
- registry 可查询 Starting/Ready/Degraded/Stopped/Failed 状态及最近错误。

## Verification commands

```powershell
cargo fmt --all -- --check
cargo clippy -p graphgateway-nodes --all-targets --all-features -- -D warnings
cargo test -p graphgateway-nodes --all-features
cargo test --workspace --all-features
```

Windows 集成测试还必须验证父进程退出/任务取消后不遗留由 fixture 启动的子进程。

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、状态机说明、变更文件、
验证结果、进程回收证据。真实工具不满足 P1-02 约束时返回阻塞，不得弱化门禁。

