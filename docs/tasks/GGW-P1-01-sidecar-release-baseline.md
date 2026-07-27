# GGW-P1-01 Windows Owned Sidecar 发布基线

## 元数据

- State: `CHANGES_REQUIRED`
- Implementation HEAD: `9eeec40c7bf53c66f75f9e730c5f54286b3a9b5b`
- Review: `docs/reviews/GGW-wave-A-review-2026-07-26.md#ggw-p1-01`
- Open findings: `P101-R4`
- Closed findings: `P101-R1`、`P101-R2`、`P101-R3`
- Remediation round: 3
- Remediation budget: `EXHAUSTED_AFTER_USER_OVERRIDE`（2026-07-27）
- Next action: 等待用户批准第 4 轮，仅处理 `P101-R4`
- Default executor: Codex（GPT-5.6）
- Depends on: 无
- Parallel with: P1-02、P1-03
- Original base: `87afdaeb7c5579facb93edf15f786011ebb3c4ca`
- Expected HEAD: `aab3c4ca920f0c1d25975b3eee60f2acadb8f0ba`
- Suggested branch: `codex/ggw-p1-01-sidecar-release`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-01`
- Budget: 一次实现，最多两轮整改

## Objective

把现有 Tauri Owned Sidecar 从“源码环境可运行”收敛为 Windows 上可重复验证的
构建、打包和启动基线，不改变 Sidecar 的产品边界。

## Source of truth

- `docs/graphgateway-windows-tauri-product-design.zh-CN.md`
- `docs/adr/001-sidecar-handshake-and-job-object.md`
- `apps/graphgateway-desktop/`

## Execution envelope

执行器只处理 Windows Owned Sidecar 的构建、资源打包和发布验证。保留现有
loopback、token、Origin、job object、watch/rollback 语义。

## Invariants

- Tauri Product 仍通过本地 HTTP/CLI 边界集成，不直接链接路由内部业务 crate。
- Sidecar 只绑定 loopback，ready 校验仍验证 PID、端口和启动令牌。
- 不恢复 shell 权限，不把 token 写入日志或持久化配置。

## Allowed scope

- `apps/graphgateway-desktop/`
- Windows 构建/打包脚本
- 与打包验证直接相关的 README 或发布文档
- 针对打包和启动路径的测试

## Forbidden scope

- Router、MCP、Workspace/Source 业务实现
- 新部署模式或远程服务模式
- 放宽 Tauri capabilities、安全头或本地认证
- 修改主设计中的架构决策

## Acceptance criteria

- Windows clean build 能自动准备正确 target-triple 名称的 Sidecar 文件，无需手工复制。
- Tauri 配置 schema、external binary、资源路径与当前 Tauri 版本一致。
- 打包产物能启动 Sidecar，执行 readiness 检查，并在宿主退出时回收子进程。
- 缺失/损坏 Sidecar 时给出可诊断错误，且不会遗留进程。
- 文档给出开发构建和发布构建的可重复命令。

## Verification commands

```powershell
cargo fmt --all -- --check
Push-Location apps/graphgateway-desktop
npm ci
npm run build:sidecar
Pop-Location
cargo test --workspace --all-features
Push-Location apps/graphgateway-desktop
npm run build
cargo tauri build --no-bundle
Pop-Location
```

先执行 `npm run build:sidecar` 生成 `src-tauri/binaries/graphgateway-{target}.exe`，
再跑 `cargo test --workspace`，避免 build.rs 因 external binary 缺失而失败（关闭 P101-R4）。

另执行卡内新增的 Windows Sidecar smoke test；该测试必须校验启动、ready、
宿主退出后的进程回收以及日志不含启动 token。

## Delivery contract

返回 `AGENT_RESULT`：`READY_FOR_REVIEW|BLOCKED`、base/head 完整提交 SHA、
变更文件、P101-R1/P101-R3 处理证据、上述命令输出摘要、smoke test 证据、
范围偏差、开放问题和已知风险。不得提交打包产物、token 或本机绝对路径。
