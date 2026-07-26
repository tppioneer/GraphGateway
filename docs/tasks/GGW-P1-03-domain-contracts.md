# GGW-P1-03 Workspace / Source / Generation 领域契约

## 元数据

- State: `READY`
- Default executor: Claude Code
- Depends on: 无
- Parallel with: P1-01、P1-02
- Expected HEAD: `87afdaeb7c5579facb93edf15f786011ebb3c4ca`
- Suggested branch: `codex/ggw-p1-03-domain-contracts`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-03`
- Budget: 一次实现，最多两轮整改

## Objective

定义第一阶段所需的 Workspace、Source、Generation、ResolvedGraphView、能力快照
和稳定错误契约，为存储、REST、节点和路由提供唯一共享类型。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 4、6、10、11、17 节
- 现有 `crates/graphgateway-types`、`crates/graphgateway-core`

## Execution envelope

只实现纯领域类型、校验、序列化和单元测试。MVP 中 endpoint 坚持
`Source + generation -> 单实例`，ResolvedGraphView 一旦创建即不可变。

## Invariants

- Workspace 决定候选 Source，QueryPlan 决定实际使用 Source。
- 查询前必须解析 immutable generation。
- SourceId、WorkspaceId、GenerationId 等标识不得混用裸字符串。
- 错误码保持机器可判定，不以展示文本作为协议。

## Allowed scope

- `crates/graphgateway-types/`
- `crates/graphgateway-core/` 中纯领域模块
- 对应单元测试和必要的 crate manifest
- 与字段契约直接相关的开发文档

## Forbidden scope

- SQLite、REST、进程管理、MCP transport、Tauri UI
- 多源 QueryPlan 或 fan-out
- 读写真实仓库、网络、环境变量
- 为下游实现临时重复 DTO

## Acceptance criteria

- 类型覆盖 Workspace、Source、SourceKind、branch/ref selector、Generation、
  Endpoint、CapabilitySnapshot、ResolvedGraphView 和 provenance 基础结构。
- 明确字段必填性、默认值、枚举未知值策略和序列化格式。
- 校验拒绝空 ID、非法本地/远程配置、重复 Source、无 generation 的可查询视图。
- ResolvedGraphView 创建后不能原地改变 generation 或 endpoint。
- 稳定错误包含 code、message、可选 details/cause，且有序列化快照测试。

## Verification commands

```powershell
cargo fmt --all -- --check
cargo clippy -p graphgateway-types -p graphgateway-core --all-targets --all-features -- -D warnings
cargo test -p graphgateway-types -p graphgateway-core --all-features
```

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、公开类型清单、变更文件、
验证结果、兼容性风险。禁止顺带实现存储或 HTTP 层。

