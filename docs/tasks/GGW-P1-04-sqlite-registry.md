# GGW-P1-04 SQLite 配置与注册表

## 元数据

- State: `IMPLEMENTING`
- Default executor: Claude Code
- Executor model: `glm-5.2`
- Depends on: P1-03
- Parallel with: 无。P1-05 与本任务都会修改根 `Cargo.toml` / `Cargo.lock`，
  必须在独立 worktree 中串行执行和集成
- Expected HEAD: `03de937be89b90eb15f990cf1ae339eefc45d4c3`
- Suggested branch: `codex/ggw-p1-04-sqlite-registry`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-04`
- Budget: 一次实现，最多两轮整改

## Objective

提供 SQLite 持久化基础，保存 Workspace、Source、关联关系和已知 generation/
capability 元数据，形成管理面与节点层共享的事务性注册表。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 6、10、14、17 节
- P1-03 已集成的领域契约

## Execution envelope

实现独立 storage crate、迁移、repository 接口和测试。运行态进程句柄、token、
session 绑定不进入 SQLite。

## Invariants

- SQLite 是配置和可恢复元数据存储，不是 MCP 会话或进程真相来源。
- 删除被 Workspace 引用的 Source 必须有显式策略，不能产生悬空引用。
- 迁移必须单调、事务化、可重复启动。

## Allowed scope

- 新增 `crates/graphgateway-storage/`
- 根 `Cargo.toml`
- 根 `Cargo.lock`
- 存储测试、迁移文件和存储层说明

## Forbidden scope

- REST/MCP endpoint、进程拉起、Tauri 页面
- 保存明文 token、子进程 PID、MCP session ID
- 多租户或远程数据库
- 修改 P1-03 类型以掩盖映射问题；需要变更时返回阻塞说明

## Acceptance criteria

- 空数据库可自动迁移到当前 schema，并记录 schema version。
- Workspace/Source 具备 create/get/list/update/delete 和关联关系事务。
- 唯一性、外键、删除策略和并发写冲突有明确测试。
- generation/capability 快照可按 Source 写入和读取，历史记录不会被静默覆盖。
- 数据库损坏、迁移失败和约束冲突映射为稳定领域错误。

## Verification commands

```powershell
cargo fmt --all -- --check
cargo clippy -p graphgateway-storage --all-targets --all-features -- -D warnings
cargo test -p graphgateway-storage --all-features

Push-Location apps/graphgateway-desktop
npm ci
npm run build:sidecar
npx tauri build --no-bundle
Pop-Location

cargo test --workspace --all-features
```

全工作区测试对 Tauri 的 external binary 和已构建 desktop executable 有前置依赖。
上述命令必须从仓库根目录开始按顺序执行，并能在无预存构建产物的干净 worktree
中重复通过。

## Delivery contract

返回且只返回一个完整 `AGENT_RESULT` 块：

- `status` 只能是 `READY_FOR_REVIEW` 或 `BLOCKED`
- `task_id`、执行器、完整 base/head commit SHA
- schema/迁移摘要和变更文件清单
- 每条验收标准的 `PASS|FAIL|NOT_RUN` 结果
- 每条验证命令、结果和简明证据
- 范围偏差、开放问题和数据兼容风险；没有时明确写 `NONE`

测试数据库必须使用临时目录并自动清理。缺少提交、验证证据不完整、出现范围外
修改或无法确认完整 base/head SHA 时，不得返回 `READY_FOR_REVIEW`。
