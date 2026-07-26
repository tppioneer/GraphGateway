# GraphGateway Wave A 独立代码审查

- Review date: 2026-07-26
- Reviewer: Codex
- Base: `87afdaeb7c5579facb93edf15f786011ebb3c4ca`
- Scope: GGW-P1-01、GGW-P1-02、GGW-P1-03
- Mode: Review only；未修改实现分支

## GGW-P1-01

- Head: `aab3c4ca920f0c1d25975b3eee60f2acadb8f0ba`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-01`
- Verdict: `CHANGES_REQUIRED`

### P101-R1 — High / High confidence

- Status: OPEN after remediation
  `aab3c4ca920f0c1d25975b3eee60f2acadb8f0ba`
- Location:
  `apps/graphgateway-desktop/src-tauri/tests/packaged_smoke_test.rs:611-735`
- Partial closure: happy-path smoke 已改为只终止 desktop PID，不使用 `/t`；
  强制清理只在失败 guard 中运行。独立运行证明 Sidecar 在宿主退出后 0.1 秒内
  由 Job Object 回收，因此“宿主退出后的进程回收”子项已修复。
- Remaining evidence: missing-sidecar smoke 捕获到的 stderr 为 0 字节，
  `has_sidecar_msg`、`has_resolve_msg`、`has_failed_msg` 全为 false，但代码只打印
  NOTE，不断言任何诊断通道，随后仍输出“diagnosable error”并 PASS。测试也没有
  构造或执行损坏 Sidecar，只覆盖文件缺失。
- Violated item: “缺失/损坏 Sidecar 时给出可诊断错误，且不会遗留进程”验收
  标准；进程未启动本身不是可供用户定位原因的诊断信息。
- Expected: 增加损坏二进制场景，并通过确定性可观察通道断言错误，例如 Failed
  状态及 `last_error`、受控诊断日志或测试专用状态查询；诊断为空时测试必须失败，
  不得把“没有 Sidecar 进程”替代为错误证据。

### P101-R2 — Medium / High confidence

- Status: RESOLVED at
  `8115f12247ea03f509d505e878369126776123e1`
- Location: `apps/graphgateway-desktop/README.md:119-152`
- Closure evidence: README 已明确 Job Object 创建/分配是硬启动条件；失败时
  Sidecar 被终止并回收、资源回滚、状态转为 Failed 且调用返回错误，并提供了
  Job Object 嵌套和运行环境的诊断步骤，与现有实现一致。

### P101-R3 — Medium / High confidence

- Status: OPEN after remediation
  `aab3c4ca920f0c1d25975b3eee60f2acadb8f0ba`
- Location: `apps/graphgateway-desktop/src-tauri/tauri.conf.json:9`,
  `apps/graphgateway-desktop/package.json:8`,
  `apps/graphgateway-desktop/scripts/copy-sidecar.mjs:101-189`
- Partial closure: copy 脚本已读取 `TAURI_ENV_TARGET_TRIPLE`，并在非 host target
  artifact 不存在时拒绝回退和重命名 host binary；12 项脚本断言在当前环境通过。
- Remaining evidence: 实际 Tauri `beforeBuildCommand` 与 `npm run
  build:sidecar` 仍先执行不带 `--target` 的
  `cargo build -p graphgateway-server --release`，未调用已修复的
  `build-sidecar.ps1`，因此只生成 host `target/release/graphgateway.exe`。
  独立设置 `TAURI_ENV_TARGET_TRIPLE=aarch64-pc-windows-msvc` 后执行
  `npm run build:sidecar`，Cargo 仍成功完成 host release build，随后 copy 因
  `target/aarch64-pc-windows-msvc/release/graphgateway.exe` 不存在而退出 1。
  当前修改把错误打包变成了正确失败，但没有实现跨 target 自动构建。
- Violated item: clean build 必须准备“正确 target-triple 名称”的 Sidecar。
- Expected: 让 `beforeBuildCommand` 和 `npm run build:sidecar` 调用同一个构建
  入口，由其解析有效 triple、执行
  `cargo build ... --target <triple>`，再复制同一 target 目录的 artifact。
  测试必须验证 Cargo 收到 `--target`，且在干净目录中不依赖预存 host/cross
  artifact；当前 fail-closed 测试的 soft skip 也应改为确定性 fixture。

### Verification evidence

- `cargo fmt --all -- --check`: PASS
- `cargo test --workspace --all-features`: PASS，全部 workspace suites 与
  doctests 通过；desktop linker 输出 1 条非失败 warning
- `npm ci`、`npm run build`: PASS
- 本机未安装 `cargo-tauri`；等价执行 `npx tauri build --no-bundle`: PASS，
  仅覆盖 host `x86_64-pc-windows-msvc`
- 独立 packaged smoke: 3/3 PASS；Job Object 回收有效，但 missing-sidecar
  证据同时显示 stderr 为空且所有诊断标志为 false
- `node scripts/test-copy-sidecar.mjs`: 12/12 PASS；只验证解析/copy，
  未验证构建入口向 Cargo 传递 target
- `TAURI_ENV_TARGET_TRIPLE=aarch64-pc-windows-msvc npm run build:sidecar`:
  FAIL（exit 1）；Cargo 构建 host artifact 后找不到 target-specific artifact

## GGW-P1-02

- Head: `59676cd8cc2b1026c1b165a6ea6ab09f8b691e52`
- Tested code commit: `8c29d71aa496bdd92b551d176eb56c95507e64a5`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-02`
- Verdict: `CHANGES_REQUIRED`
- Remediation round 3: AUTHORIZED on 2026-07-27；executor Claude Code，
  model `glm-5.2`，base
  `59676cd8cc2b1026c1b165a6ea6ab09f8b691e52`

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
| GGW-P1-01 | `CHANGES_REQUIRED` | P101-R1、P101-R3 |
| GGW-P1-02 | `CHANGES_REQUIRED` | P102-R1、P102-R2、P102-R3、P102-R4 |
| GGW-P1-03 | `PASS` | 无 |

GGW-P1-03 已集成到 `mcp`；GGW-P1-01 与 GGW-P1-02 仍为
`CHANGES_REQUIRED`，不得集成。GGW-P1-01 已用尽两轮整改预算，不得自动发起
第三轮；剩余 P101-R1/P101-R3 必须重新拆卡或由用户批准并记录显式例外。
GGW-P1-02 原两轮整改预算已用尽；用户于 2026-07-27 显式批准第三轮例外，
仅允许处理 P102-R1/P102-R2/P102-R3/P102-R4，完成后必须重新独立复审。
