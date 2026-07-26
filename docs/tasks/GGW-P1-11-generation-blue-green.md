# GGW-P1-11 Generation 蓝绿切换与安全回收

## 元数据

- State: `DRAFT`
- Default executor: Claude Code
- Depends on: P1-07、P1-08
- Parallel with: P1-09
- Expected HEAD: `TO_BE_SET_AFTER_P1_07_AND_P1_08_INTEGRATION`
- Suggested branch: `codex/ggw-p1-11-generation-blue-green`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-11`
- Budget: 一次实现，最多两轮整改

## Objective

在 Source 出现新 generation 时先启动并验证新节点，再原子发布给后续请求；
旧 ResolvedGraphView 继续使用旧节点，直至引用归零或超时后安全回收。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 7、9、12、22 节
- P1-07 节点状态机
- P1-08 immutable ResolvedGraphView

## Execution envelope

扩展节点管理器实现 generation 级蓝绿激活和引用租约。只处理同一 Source 的
generation 切换，不涉及多源路由或查询结果融合。

## Invariants

- 新节点未通过 MCP readiness 和 capability 注册前不能成为 active。
- 已创建的 view 不重定向到新 endpoint。
- 新节点失败时继续保留可用旧节点，不发布半成品状态。
- 回收必须有引用计数/租约和最大宽限期，且最终回收整个进程树。

## Allowed scope

- `crates/graphgateway-nodes/`
- `crates/graphgateway-core/` 中 view lease 的最小接口
- 必要的存储状态记录
- 并发/故障注入测试和运行手册

## Forbidden scope

- 北向 MCP、Tauri UI、多源规划
- 原地修改已发布 generation 的 endpoint/capability
- 以强杀旧进程作为正常切换第一步
- 无上限保留历史节点

## Acceptance criteria

- 相同新 generation 的并发激活只启动一次。
- 新节点 ready 后 active 指针原子切换；新请求获得新 view，旧请求保持旧 view。
- 新节点启动/readiness 失败时 active 不变，错误可诊断。
- 旧节点在最后租约释放后回收；泄漏租约在宽限期后告警并按策略回收。
- 服务关闭期间同时存在蓝/绿节点时，两者均被确定性回收。

## Verification commands

```powershell
cargo fmt --all -- --check
cargo clippy -p graphgateway-nodes -p graphgateway-core --all-targets --all-features -- -D warnings
cargo test -p graphgateway-nodes blue_green --all-features
cargo test -p graphgateway-nodes lease --all-features
cargo test --workspace --all-features
```

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、状态转换/租约说明、故障注入
证据、变更文件、验证结果、回收策略风险。

