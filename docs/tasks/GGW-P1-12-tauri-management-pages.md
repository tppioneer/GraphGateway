# GGW-P1-12 Tauri Workspace / Source 管理页面

## 元数据

- State: `DRAFT`
- Default executor: Claude Code
- Depends on: P1-06
- Parallel with: P1-08；也可与 P1-07/P1-09/P1-11 并行
- Expected HEAD: `TO_BE_SET_AFTER_P1_06_INTEGRATION`
- Suggested branch: `codex/ggw-p1-12-tauri-management`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-12`
- Budget: 一次实现，最多两轮整改

## Objective

在现有 Tauri Product 中提供 Workspace/Source 的列表、创建、编辑、删除、关联和
状态查看页面，由薄 Tauri command 通过 `graphgateway-client` 调用 Sidecar。

## Source of truth

- `docs/graphgateway-windows-tauri-product-design.zh-CN.md`
- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 6、14、19 节
- P1-06 management client

## Execution envelope

实现本地管理 UI 和必要的薄命令桥接。沿用现有 SidecarManager 与安全上下文；
页面不直接发 HTTP，不依赖 Router/storage 内部 crate。

## Invariants

- `WebView -> Tauri command -> graphgateway-client -> localhost Sidecar`。
- token 不进入 DOM、前端状态、localStorage 或可复制错误信息。
- 页面只管理 Workspace/Source，不嵌入 MCP 调试器或节点进程控制台。
- Sidecar 未就绪/崩溃时页面显示可恢复状态，不绕过宿主直接连接端口。

## Allowed scope

- `apps/graphgateway-desktop/src/`
- `apps/graphgateway-desktop/src-tauri/`
- `graphgateway-client` 中仅修正 P1-06 已定义 API 的必要兼容问题
- UI/command 测试、截图或手工验收说明

## Forbidden scope

- Router/core/storage 业务逻辑
- WebView 直接 fetch Sidecar、暴露 token/port
- Remote Shared Service、In-process Library 模式
- query/context/impact 查询工作台、多源拓扑编辑器

## Acceptance criteria

- Workspace/Source 列表具有 loading/empty/error/retry 状态。
- 支持创建、编辑、删除和 Source 关联/解除关联；危险删除有确认和后端冲突反馈。
- 表单校验与后端稳定错误能映射为清晰字段/页面提示。
- Source 详情展示类型、branch/ref、已知 generation、capability 和节点状态；
  不支持的数据明确显示 unavailable。
- Tauri command 为 async、可取消/超时，不持有全局锁跨 await。
- 前端构建、Rust 测试和至少一套组件或端到端 UI 测试通过。

## Verification commands

```powershell
Set-Location apps/graphgateway-desktop
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --all-features
cargo tauri build --no-bundle
```

还需运行仓库中新增的 UI 测试命令，并在交付结果中记录一次 Windows 手工 smoke
test：启动、创建 Workspace/Source、关联、编辑、冲突删除、Sidecar 重启后刷新。

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、页面/command 清单、变更文件、
自动化验证结果、手工 smoke 证据、UI 已知限制。不得提交构建产物或本机配置。

