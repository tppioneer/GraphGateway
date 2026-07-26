# GGW-P1-06 Workspace / Source 管理 REST 与 Rust Client

## 元数据

- State: `DRAFT`
- Default executor: Claude Code
- Depends on: P1-04
- Parallel with: P1-07（P1-07 的其他依赖也已满足后）
- Expected HEAD: `TO_BE_SET_AFTER_P1_04_INTEGRATION`
- Suggested branch: `codex/ggw-p1-06-management-api`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-06`
- Budget: 一次实现，最多两轮整改

## Objective

在现有本地控制面上提供 Workspace/Source CRUD、关联关系和状态读取 API，并在
`graphgateway-client` 中提供对应的强类型调用。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 6、14、19 节
- `docs/graphgateway-windows-tauri-product-design.zh-CN.md`
- P1-04 storage API

## Execution envelope

只实现本地管理面 REST 和 client；API 通过 storage/service 边界访问数据。节点
启动、MCP 查询和页面实现不属于本卡。

## Invariants

- 保持现有 loopback、Bearer token、Origin 和 request-id 规则。
- REST DTO 不泄漏 SQLite 细节或运行时进程句柄。
- Tauri 后续只能经 `graphgateway-client` 调用本 API。

## Allowed scope

- `crates/graphgateway-rest/`
- `crates/graphgateway-client/`
- `crates/graphgateway-server/` 中路由装配
- 直接相关的 API 测试与 OpenAPI/接口文档

## Forbidden scope

- `apps/graphgateway-desktop/`
- MCP server/client、proxy/GitNexus 进程
- 多源 QueryPlan、generation 切换
- 绕过认证的“仅开发”接口

## Acceptance criteria

- API 覆盖 Workspace/Source create/get/list/update/delete、Source 关联/解除关联。
- 请求和响应使用版本化 DTO；validation/storage 错误映射为稳定 HTTP 状态和
  错误 envelope。
- 条件更新或等价机制防止静默覆盖并发修改。
- `graphgateway-client` 对全部新 endpoint 提供强类型方法和错误映射。
- REST 集成测试覆盖认证、Origin、非法输入、冲突、not found 和成功路径。

## Verification commands

```powershell
cargo fmt --all -- --check
cargo clippy -p graphgateway-rest -p graphgateway-client -p graphgateway-server --all-targets --all-features -- -D warnings
cargo test -p graphgateway-rest -p graphgateway-client -p graphgateway-server --all-features
cargo test --workspace --all-features
```

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、endpoint 清单、变更文件、
验证结果、兼容性说明。不得同时实现页面或 MCP 查询。

