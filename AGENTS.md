# AGENTS.md

GraphGateway 多代理开发规范。规则来自 Wave A 审查证据，仅记录反复出现的问题。

## Tauri 桌面任务验证命令顺序

`cargo test --workspace` 对 Tauri 构建产物有隐含依赖，验证命令必须按顺序执行：

1. `npm run build:sidecar` — 生成 `src-tauri/binaries/graphgateway-{target}.exe`，满足 `build.rs` 的 `externalBin` 检查
2. `npx tauri build --no-bundle` — 生成 `target/release/graphgateway-desktop.exe`，满足 `packaged_smoke` 测试前置条件（`beforeBuildCommand` 幂等地跑 `npm run build && npm run build:sidecar`）
3. `cargo test --workspace --all-features` — 两个构建产物依赖均已满足

Tauri CLI 通过 npm 安装（`@tauri-apps/cli`），用 `npx tauri build`，不是 `cargo tauri build`。

依据：P1-01 P101-R4（build.rs external binary 依赖）+ 集成验证（packaged_smoke desktop exe 依赖）。

## 任务卡验证命令必须可重复

干净 worktree（无构建产物）按任务卡验证命令顺序必须完整通过，不得依赖预存且未纳入版本控制的构建产物。

## 范围控制

只修改任务卡 `Allowed scope` 内的路径。范围外文件的任何净修改（包括空行、格式化变更）都会被审查标记为 finding。提交前用 `git diff --stat <base>..HEAD` 自检。
