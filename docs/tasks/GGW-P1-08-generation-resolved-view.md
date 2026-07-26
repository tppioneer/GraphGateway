# GGW-P1-08 Generation 解析与 ResolvedGraphView

## 元数据

- State: `DRAFT`
- Default executor: Claude Code
- Depends on: P1-04、P1-05、P1-07；受 P1-02 generation 门禁约束
- Parallel with: P1-12
- Expected HEAD: `TO_BE_SET_AFTER_P1_04_P1_05_AND_P1_07_INTEGRATION`
- Suggested branch: `codex/ggw-p1-08-resolved-view`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-08`
- Budget: 一次实现，最多两轮整改

## Objective

把 Workspace 中 Source 的 branch/ref 选择解析为可查询的 immutable generation，
生成一次请求生命周期内固定的 ResolvedGraphView。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 7、9、10、12 节
- P1-02 的 generation 能力结论
- P1-03/P1-04/P1-05/P1-07 已集成契约

## Execution envelope

实现 generation resolver、状态等待和视图组装。若 P1-02 没有证明下游能通过
标准 MCP 解析/检查 generation，本卡必须 `BLOCKED`；不得用目录时间戳、分支名
或“当前节点”冒充 immutable generation。

## Invariants

- QueryPlan 创建前必须获得 generation。
- 同一 ResolvedGraphView 内 Source 到 generation/endpoint 映射不可改变。
- 请求执行中发生新 generation 只影响后续请求。
- capability 与 endpoint 必须属于同一 generation 实例。

## Allowed scope

- `crates/graphgateway-core/` 中 resolver/view 服务
- `crates/graphgateway-storage/` 必要查询接口
- `crates/graphgateway-nodes/` 必要只读状态接口
- mock/集成测试和 generation 行为文档

## Forbidden scope

- 单源工具分发、北向 MCP、Tauri UI
- 多 Source fan-out 或跨源一致性
- 蓝绿启动/回收（P1-11）
- 私有 proxy API、猜测 generation、静默降级为 latest

## Acceptance criteria

- exact generation 可立即解析；branch/ref 能解析到确定 generation 或稳定错误。
- indexing/pending 状态按策略有界等待；超时、失败和能力缺失可诊断。
- 并发解析可复用同一 ready generation，不发布半初始化 endpoint。
- ResolvedGraphView 包含 Workspace、Source、generation、endpoint、能力快照及
  构建时间/一致性元数据。
- 测试证明请求期间 generation 变化不会改变既有 view。

## Verification commands

```powershell
cargo fmt --all -- --check
cargo clippy -p graphgateway-core -p graphgateway-storage -p graphgateway-nodes --all-targets --all-features -- -D warnings
cargo test -p graphgateway-core generation --all-features
cargo test -p graphgateway-nodes generation --all-features
cargo test --workspace --all-features
```

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、generation 能力证据、
变更文件、验证结果、一致性限制。若阻塞，必须指出缺失的标准 MCP 方法/字段。

