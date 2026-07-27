# GraphGateway Wave A 独立代码审查

- Review date: 2026-07-26
- Reviewer: Codex
- Base: `87afdaeb7c5579facb93edf15f786011ebb3c4ca`
- Scope: GGW-P1-01、GGW-P1-02、GGW-P1-03
- Mode: Review only；未修改实现分支

## GGW-P1-01

- Head: `9eeec40c7bf53c66f75f9e730c5f54286b3a9b5b`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-01`
- Verdict: `CHANGES_REQUIRED`

### P101-R1 — High / High confidence

- Status: RESOLVED at
  `9eeec40c7bf53c66f75f9e730c5f54286b3a9b5b`
- Location: `apps/graphgateway-desktop/src-tauri/src/lib.rs`,
  `apps/graphgateway-desktop/src-tauri/tests/packaged_smoke_test.rs`
- Closure evidence: 自动启动失败时可通过受控诊断文件取得脱敏
  `SidecarSnapshot`；缺失与损坏 Sidecar 两个场景都强制断言状态为 `Failed`、
  `last_error` 非空，并检查没有遗留 Sidecar。独立执行 packaged smoke 4/4
  通过，宿主退出后 Job Object 回收仍有效。

### P101-R2 — Medium / High confidence

- Status: RESOLVED at
  `8115f12247ea03f509d505e878369126776123e1`
- Location: `apps/graphgateway-desktop/README.md:119-152`
- Closure evidence: README 已明确 Job Object 创建/分配是硬启动条件；失败时
  Sidecar 被终止并回收、资源回滚、状态转为 Failed 且调用返回错误，并提供了
  Job Object 嵌套和运行环境的诊断步骤，与现有实现一致。

### P101-R3 — Medium / High confidence

- Status: RESOLVED at
  `9eeec40c7bf53c66f75f9e730c5f54286b3a9b5b`
- Location: `apps/graphgateway-desktop/scripts/build-sidecar.mjs`,
  `apps/graphgateway-desktop/scripts/copy-sidecar.mjs`,
  `apps/graphgateway-desktop/src-tauri/tauri.conf.json`
- Closure evidence: `beforeBuildCommand` 与 `npm run build:sidecar` 已统一到同一
  构建入口；目标三元组按 CLI、Tauri 环境、Cargo 环境和 host 顺序解析，并把
  同一个 triple 同时传给 Cargo `--target` 与复制步骤。复制只接受精确 target
  目录，确定性脚本测试验证非 host triple 与优先级，未使用 soft skip。

### P101-R4 — Medium / High confidence

- Status: RESOLVED on 2026-07-28（控制器方案 C，不改实现代码）
- Resolution: 控制者调整任务卡验证命令顺序（commit `3f22b7a`），先
  `npm run build:sidecar` 生成 external binary，再跑
  `cargo test --workspace`，使干净工作区验证流程可重复执行。
- Closure evidence: 控制器在 worktree
  `F:\develop\worktrees\GraphGateway-p1-01`（HEAD `9eeec40`）独立验证：
  删除 `binaries/*.exe` 模拟干净状态后按新顺序执行全部验证命令，
  `cargo test --workspace --all-features` 退出 0，全部测试通过。详见下方
  2026-07-28 Verification evidence。
- Location: `apps/graphgateway-desktop/src-tauri/tauri.conf.json:24-26`,
  `apps/graphgateway-desktop/package.json:8-12`
- Evidence: 在实现工作区尚无被忽略的 `src-tauri/binaries/graphgateway-*.exe`
  时，严格按任务卡顺序首先执行 `cargo test --workspace --all-features`，
  Tauri build script 因 external binary 不存在而退出 1：
  `resource path binaries\graphgateway-x86_64-pc-windows-msvc.exe doesn't exist`。
  先执行 `npm run build:sidecar` 后，同一 workspace 测试全部通过，证明失败来自
  未声明的制品前置条件，而不是测试代码。
- Violated item: 任务卡给出的 clean build 验证命令必须可重复执行，且无需手工
  准备 Sidecar 文件。
- Expected: 让任务卡规定的干净工作区验证流程自动生成所需 target sidecar，
  或使 workspace 测试不依赖预存且未纳入版本控制的 external binary；不得提交
  构建产物。修复后必须从无 `src-tauri/binaries/graphgateway*.exe` 的状态按卡内
  命令顺序验证。

### Verification evidence

- 累计差异 `aab3c4c..9eeec40`：单提交、仅修改任务允许的 8 个桌面端路径；
  实现工作区最终洁净
- `cargo fmt --all -- --check`: PASS
- 干净制品状态下 `cargo test --workspace --all-features`: FAIL，external binary
  不存在；执行 `npm run build:sidecar` 后重跑：PASS
- `npm ci`、`npm run build`、`npm run test:sidecar-build`: PASS
- workspace 测试包含 packaged smoke 4/4 PASS：正常启动、缺失、损坏与 token
  泄漏场景均通过
- `npx tauri build --no-bundle`: 已产出 host release 可执行文件；命令受 60 秒
  工具窗口截断，后台编译进程随后正常退出
- 复审结束后未发现属于该 worktree 的 `graphgateway.exe` 或
  `graphgateway-desktop.exe` 残留进程

#### 2026-07-28 控制器独立验证（P101-R4 关闭，方案 C）

- 干净状态：删除 `binaries/*.exe`（保留 `.gitkeep`），无 `graphgateway*` 残留进程
- `cargo fmt --all -- --check`: PASS
- `npm ci`: PASS（74 packages, 17s）
- `npm run build:sidecar`: PASS（`cargo build -p graphgateway-server --release
  --target x86_64-pc-windows-msvc` 9.40s，产出
  `graphgateway-x86_64-pc-windows-msvc.exe` 2.48MB）
- `cargo test --workspace --all-features`: PASS（所有测试通过）← R4 核心验证点
- `npm run build`: PASS（vite build 922ms）
- `npx tauri build --no-bundle`: PASS（`cargo build --release` 2m23s，产出
  `target/release/graphgateway-desktop.exe`）
- 命令差异：任务卡原写 `cargo tauri build`，实际环境 Tauri CLI 经 npm 安装
  （`@tauri-apps/cli`），须用 `npx tauri build`；任务卡已同步修正

## GGW-P1-02

- Head: `1d180f442115e2d47361e905785531bf2c92f141`
- Tested code commit: `414632dacc819d4c9c36fc863a3c608cec8fffd5`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-02`
- Review status: `READY_FOR_REVIEW`
- Last verdict: `CHANGES_REQUIRED`（round 2）
- Remediation round 3: AUTHORIZED on 2026-07-27；executor Claude Code，
  model `glm-5.2`，base
  `59676cd8cc2b1026c1b165a6ea6ab09f8b691e52`
- Remediation round 3 delivery: code
  `414632dacc819d4c9c36fc863a3c608cec8fffd5`，clean-gate report
  `1d180f442115e2d47361e905785531bf2c92f141`；等待独立复审

### P102-R1 — High / High confidence

- Status: OPEN after remediation round 2
  `59676cd8cc2b1026c1b165a6ea6ab09f8b691e52`
- Location: `scripts/interop/verify-mcp-proxy-gitnexus.mjs:281-411`,
  `:749-765`
- Evidence: fixture 初始化、Git 提交、GitNexus analyze 以及 query/context
  的 fail-fast 与 repo 参数绑定已补齐；但 canonical identity 的精确路径匹配失败后，
  `:392-404` 仍会仅凭 `path:` 行包含 fixture basename 选择 repo。不同目录下存在
  同名仓库时可绑定到错误索引，随后 4.2-4.4 仍可能把另一仓库的结果记为 PASS。
- Violated item: verdict 和每个结论必须可追溯到真实协议证据；失败时应返回非零。
- Accepted decision (2026-07-27): 禁止通过 repository basename 猜测 identity。
- Expected: 所有 fixture 初始化、Git 提交和 GitNexus analyze 命令必须
  fail-fast；精确确认临时 fixture 的 canonical identity 后才运行协议门禁。
  只接受规范化完整路径的唯一匹配；零匹配或多匹配都必须 FAIL。query/context
  必须始终绑定并断言该 identity，不允许 basename、无 repo 或 global fallback。

### P102-R2 — High / High confidence

- Status: OPEN after remediation round 2
  `59676cd8cc2b1026c1b165a6ea6ab09f8b691e52`
- Location: `scripts/interop/verify-mcp-proxy-gitnexus.mjs:875-962`,
  `:1001-1063`
- Evidence: disconnect 已进入必需门禁并记录共享上游约束；但 abort 测试仍只证明
  本地 `fetch` 收到 `AbortError`。它没有观察 20 秒 delay 是否在上游被终止，
  upstream PID 改变时只写 NOTE，且 `siblingAlive=false` 仍执行
  `return PASS(...)`。因此当前 PASS 没有如实测量和区分上游请求继续执行、
  Session/sibling 可用性以及相关资源行为。
- Violated item: 必须验证并记录上游异常、超时和客户端断开。
- Accepted decision (2026-07-27): 客户端断开后不要求 mcp-proxy 向 GitNexus
  冒泡传递中断信号；允许上游请求继续执行，但必须如实测量并记录。
- Expected: abort 后保持 proxy 运行并等待可控 delay 的最终结果，明确记录上游请求
  是继续完成、被取消还是状态未知；同时记录 upstream PID、原 Session、sibling
  Session 和相关进程的实际行为。不得因 `siblingAlive=false` 仍返回 PASS；不把
  “上游请求继续执行”本身判为失败。disconnect 场景继续作为共享上游约束记录。

### P102-R3 — High / High confidence

- Status: OPEN after remediation round 2
  `59676cd8cc2b1026c1b165a6ea6ab09f8b691e52`
- Location: `scripts/interop/verify-mcp-proxy-gitnexus.mjs:1283-1299`,
  `:1774-1782`
- Evidence: 全系统 PID 差值回退已删除，本轮独立运行可确定性追踪并回收两个
  descendant，报告的共享上游模型也已统一；但 11.2 的代理功能检查失败只写日志，
  仍会把任意存活 descendant 判为“服务 MCP 流量”。此外报告仍在临时 fixture
  cleanup 之前写入，cleanup 异常只记录 warning，不进入结果或 verdict。
- Violated item: 明确会话/进程映射、proxy 退出后的子进程行为，且结论来自证据。
- Accepted decision (2026-07-27): 黑盒验证边界只保留三项硬要求；不要求测试脚本
  直接观测 GitNexus 原始 stdout/stderr。
- Expected:
  1. MCP 功能检查失败必须 FAIL。
  2. 所有 cleanup 结果必须在生成报告之前进入 verdict。
  3. 进程归属必须通过 parent/child 关系证明；无法建立关联时 FAIL，不得使用
     全系统 PID 差值替代。

### P102-R4 — Medium / High confidence

- Status: REOPENED after remediation round 2
  `59676cd8cc2b1026c1b165a6ea6ab09f8b691e52`
- Location: `.gitignore:59`,
  `scripts/interop/verify-mcp-proxy-gitnexus.mjs:212-280`
- Evidence: runner 已改用 OS 临时 fixture，原 fixture ignore 规则也已删除；
  但从任务原始基线
  `87afdaeb7c5579facb93edf15f786011ebb3c4ca` 到最终 HEAD 的累计差异仍在根
  `.gitignore` 新增一个空行。即使不改变 ignore 语义，这仍是 Allowed scope
  之外的净修改，R4 的关闭条件未满足。
- Accepted decision (2026-07-27): 直接删除该范围外空行；最终累计差异中根
  `.gitignore` 必须与原始基线完全一致。

### P102-R5 — Medium / High confidence

- Status: RESOLVED at
  `59676cd8cc2b1026c1b165a6ea6ab09f8b691e52`
- Location: `docs/compatibility/mcp-proxy-gitnexus-interop-report.md:6,35`
- Closure evidence: 最终报告提交位于被测代码提交
  `8c29d71aa496bdd92b551d176eb56c95507e64a5` 之后，`Tested commit`
  精确记录该代码提交；fixture 使用仓库相对路径
  `tests\fixtures\mcp\sample-repo`，报告和互操作目录扫描未发现本机绝对路径。

### Verification evidence

- `node --check scripts/interop/verify-mcp-proxy-gitnexus.mjs`: PASS
- `node --version`: PASS，`v24.14.1`
- `mcp-proxy --version`: PASS，`6.5.4`
- `gitnexus --version`: PASS，`1.6.9`
- detached worktree 基于 tested code commit 独立完整运行：32 PASS、0 FAIL、
  0 SKIP、5 CONSTRAINT，报告结论 `PASS_WITH_CONSTRAINTS`
- 本轮运行中 fixture Git 提交和 canonical path 首选匹配成功；两个被测
  GitNexus descendant 与主 proxy descendant 均成功回收，运行结束后未发现属于
  detached review worktree 的残留进程
- 运行报告的 `Tested commit` 精确为
  `8c29d71aa496bdd92b551d176eb56c95507e64a5`，绝对路径扫描无匹配
- generation 能力缺失证据可信：P1-08 保持 BLOCKED
- P102-R5 关闭；P102-R1、P102-R2、P102-R3 继续保持 OPEN，P102-R4 重新打开

## GGW-P1-03

- Head: `d0d9b70f4378a918c78157a7d7c0bc9fb9e05bae`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-03`
- Verdict: `PASS`
- Integration: merged into `mcp` at
  `1dc08c3e811cbac141bbb06ae7df5e6eb8bbdc69`

### P103-R1 — High / High confidence

- Status: RESOLVED at
  `d0d9b70f4378a918c78157a7d7c0bc9fb9e05bae`
- Location: `crates/graphgateway-types/src/view.rs:63-150`,
  `crates/graphgateway-core/src/validation.rs:313-345`
- Closure evidence: `ViewMember` 与 `ResolvedGraphView` 字段保持私有，只暴露只读
  accessor；5 个 compile-fail 用例证明 generation、endpoint、capability 和成员
  集合不能原地修改。共享受校验构造路径拒绝
  `capability.source_id != source_id`，核心校验同时拒绝反序列化产生的错配对象，
  两条负向测试独立覆盖两个入口。

### P103-R2 — High / High confidence

- Status: RESOLVED at
  `247c87d5e4325b3909cbd0eab278109a8707aa43`
- Location: `crates/graphgateway-types/src/source.rs:123-126`,
  `crates/graphgateway-core/src/validation.rs:93-115`
- Closure evidence: endpoint 通过 `url::Url` 解析并按 host 类型精确识别 IPv4、
  IPv6 和 `localhost`；查询串、路径与 hostname 后缀绕过测试均通过，
  `SourceKind::Unknown` fail closed。

### P103-R3 — High / High confidence

- Status: RESOLVED at
  `247c87d5e4325b3909cbd0eab278109a8707aa43`
- Location: `crates/graphgateway-core/src/validation.rs:118-125`,
  `:143-177`
- Closure evidence: 单 Source 与 Workspace 两级校验已要求 writable Source
  同时满足 local、Primary、唯一且等于 `primary_source_id`；`AllReadOnly`
  禁止 writable Source，相关接受和拒绝测试通过。

### P103-R4 — Medium / High confidence

- Status: RESOLVED at
  `d0d9b70f4378a918c78157a7d7c0bc9fb9e05bae`
- Location: `crates/graphgateway-types/src/view.rs:93-150,470-549`
- Closure evidence: 新增 `ViewMember::new_unavailable`，与正常构造器复用相同的
  私有校验路径，且不暴露状态修改接口；`unavailable` 继续使用原 JSON 布尔字段。
  单元测试通过公共类型 API 构造 unavailable member 和 degraded view，不再通过
  反序列化绕过构造器。

### Verification evidence

- `cargo fmt --all -- --check`: PASS
- `cargo clippy -p graphgateway-types -p graphgateway-core --all-targets --all-features -- -D warnings`: PASS
- `cargo test -p graphgateway-types -p graphgateway-core --all-features`: PASS，
  51 core + 75 types + 1 组 trybuild（5 cases）
- 独立复审未发现新增 finding；P103-R1、P103-R2、P103-R3、P103-R4 全部关闭
- 合入 `mcp` 后重新执行以上三项检查：PASS，任务状态更新为 `INTEGRATED`

## Wave A summary

| Task | Verdict | Open findings |
| --- | --- | --- |
| GGW-P1-01 | `VERIFIED` | 无（P101-R4 方案 C 关闭） |
| GGW-P1-02 | `CHANGES_REQUIRED` | P102-R1、P102-R2、P102-R3、P102-R4 |
| GGW-P1-03 | `PASS` | 无 |

GGW-P1-03 已集成到 `mcp`。GGW-P1-01 经方案 C（控制器调整任务卡验证命令顺序，
commit `3f22b7a`）关闭 P101-R4，2026-07-28 控制器独立验证全部 PASS，状态
`VERIFIED`，待集成到 `mcp`。GGW-P1-02 仍为 `CHANGES_REQUIRED`，不得集成。
GGW-P1-02 原两轮整改预算已用尽；用户于 2026-07-27 显式批准第三轮例外，
仅允许处理 P102-R1/P102-R2/P102-R3/P102-R4，完成后必须重新独立复审。
