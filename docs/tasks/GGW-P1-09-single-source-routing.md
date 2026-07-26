# GGW-P1-09 单源 QueryPlan、一致性门禁与分发

## 元数据

- State: `DRAFT`
- Default executor: Claude Code
- Depends on: P1-08
- Parallel with: P1-11
- Expected HEAD: `TO_BE_SET_AFTER_P1_08_INTEGRATION`
- Suggested branch: `codex/ggw-p1-09-single-source-routing`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-09`
- Budget: 一次实现，最多两轮整改

## Objective

基于 ResolvedGraphView 生成单 Source QueryPlan，完成 capability/consistency/write
policy 门禁、query/context/impact 分发和成员级 provenance 归一化。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 6、9、11、12、13、16 节
- P1-08 immutable ResolvedGraphView

## Execution envelope

只实现单源规划与内部 dispatcher。当前阶段 read 工具为 query/context/impact；
写策略只建立“仅 local primary 可写”的可复用门禁，不在本卡暴露新北向写工具。

## Invariants

- Workspace 只提供候选 Source；实际 Source 必须写入 QueryPlan。
- QueryPlan 必须引用已解析 generation，执行期不得切换 endpoint。
- capability 不满足时在调用下游前失败。
- 任意写意图只有 local primary Source 可通过；remote 和 secondary 一律拒绝。
- provenance 至少达到返回成员/结果项粒度。

## Allowed scope

- `crates/graphgateway-core/` 的 planner、policy、dispatcher、normalizer
- 与 MCP client 的既有抽象集成
- mock 下游、单元/集成测试和内部契约文档

## Forbidden scope

- 北向 MCP/REST endpoint、Tauri UI
- 多源 fan-out、结果融合、跨源 ranking
- 暴露 rename/detect_changes 等写工具
- 在结果缺少来源时填充猜测 provenance

## Acceptance criteria

- query/context/impact 各能生成包含 tool、Source、generation、endpoint、timeout、
  consistency 的单源 QueryPlan。
- 缺 capability、generation 非 ready、endpoint 不匹配和 policy 拒绝均在下游
  调用前返回稳定错误。
- dispatcher 只调用计划指定 endpoint，传播 timeout/cancellation/request-id。
- 下游结果被归一化为稳定 envelope，每个成员包含 Source/generation/tool
  provenance；无法归属的结果明确失败或标记 unresolved，不静默伪造。
- 测试覆盖成功、能力不足、超时、取消、下游错误、provenance 缺失和写策略。

## Verification commands

```powershell
cargo fmt --all -- --check
cargo clippy -p graphgateway-core --all-targets --all-features -- -D warnings
cargo test -p graphgateway-core single_source --all-features
cargo test -p graphgateway-core provenance --all-features
cargo test --workspace --all-features
```

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、QueryPlan/envelope 摘要、
变更文件、验证结果、明确未实现的多源与写工具范围。

