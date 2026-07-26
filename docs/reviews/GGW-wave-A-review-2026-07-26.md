# GraphGateway Wave A 独立代码审查

- Review date: 2026-07-26
- Reviewer: Codex
- Base: `87afdaeb7c5579facb93edf15f786011ebb3c4ca`
- Scope: GGW-P1-01、GGW-P1-02、GGW-P1-03
- Mode: Review only；未修改实现分支

## GGW-P1-01

- Head: `8115f12247ea03f509d505e878369126776123e1`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-01`
- Verdict: `CHANGES_REQUIRED`

### P101-R1 — High / High confidence

- Status: OPEN after remediation
  `8115f12247ea03f509d505e878369126776123e1`
- Location:
  `apps/graphgateway-desktop/src-tauri/tests/packaged_smoke_test.rs:378-421`
- Evidence: 新测试确实启动了 Tauri desktop 和 Sidecar，但它通过
  `taskkill /f /t /pid <desktop>` 终止宿主；`/t` 本身会终止该 PID 的整个子进程
  树，因此即使 Job Object 的 `KILL_ON_JOB_CLOSE` 完全失效，Sidecar 也会被
  `taskkill` 回收。若 Sidecar 15 秒后仍存在，测试还会主动调用
  `kill_process_tree(sidecar_pid)`，再以清理后的 PID 集合执行最终断言，未断言
  `reclaimed == true`，会把真实的回收失败变成 PASS。
- Violated item: “打包产物能启动 Sidecar”以及“宿主退出后的进程回收”验收标准。
- Expected: 仅终止 desktop 宿主进程，不使用会递归杀子进程的 `/t`；在任何测试
  清理动作之前断言 Sidecar 已由 Job Object 回收。测试失败后的强制清理只能放在
  guard/finally 中，不能参与成功判定；缺失或损坏 Sidecar 的场景也不得以
  spawn 失败或缺少构建产物为成功跳过。

### P101-R2 — Medium / High confidence

- Status: RESOLVED at
  `8115f12247ea03f509d505e878369126776123e1`
- Location: `apps/graphgateway-desktop/README.md:119-152`
- Closure evidence: README 已明确 Job Object 创建/分配是硬启动条件；失败时
  Sidecar 被终止并回收、资源回滚、状态转为 Failed 且调用返回错误，并提供了
  Job Object 嵌套和运行环境的诊断步骤，与现有实现一致。

### P101-R3 — Medium / High confidence

- Status: OPEN after remediation
  `8115f12247ea03f509d505e878369126776123e1`
- Location: `apps/graphgateway-desktop/src-tauri/tauri.conf.json:9`,
  `apps/graphgateway-desktop/scripts/copy-sidecar.mjs:72-129`,
  `apps/graphgateway-desktop/scripts/build-sidecar.ps1:91-116`
- Evidence: Tauri 为 `beforeBuildCommand` 提供实际目标
  `TAURI_ENV_TARGET_TRIPLE`，但脚本只读取自定义 `--target`、
  `CARGO_BUILD_TARGET` 和 host rustc/Node 架构；配置中的 before-build Cargo
  命令也未把 Tauri `--target` 转交给 Sidecar 构建。独立复现设置
  `TAURI_ENV_TARGET_TRIPLE=aarch64-pc-windows-msvc`、清除
  `CARGO_BUILD_TARGET` 后执行 `node .../copy-sidecar.mjs --dry-run`，仍输出
  `x86_64-pc-windows-msvc`。此外两个复制脚本在目标目录不存在时均回退
  `target/release/graphgateway.exe`，可能把 host 二进制复制并命名为非 host
  target。
- Violated item: clean build 必须准备“正确 target-triple 名称”的 Sidecar。
- Expected: before-build 流程以 `TAURI_ENV_TARGET_TRIPLE` 为目标来源，并将同一
  triple 显式传给 Cargo 构建和复制脚本；请求非默认 target 时不得回退或重命名
  host artifact。新增测试必须覆盖 Tauri hook 环境变量以及目标 artifact 缺失时
  fail closed，而不只验证 `--dry-run` 输出。

### Verification evidence

- 独立 `cargo fmt --all -- --check`: PASS
- 独立 `cargo test -p graphgateway-desktop --all-features -- --test-threads=1`:
  PASS，7 unit + 3 packaged smoke；但 P101-R1 所述测试逻辑不能证明 Job Object
  回收
- 独立 `node apps/graphgateway-desktop/scripts/test-copy-sidecar.mjs`: PASS，
  6/6；但未覆盖 Tauri hook 的实际 target 环境
- 独立 Tauri target 环境复现：FAIL，期望 aarch64，实际输出 x86_64
- 执行器报告 workspace tests、npm build 和 `cargo tauri build --no-bundle`
  均 PASS；这些 host-target 结果不能关闭 P101-R1/P101-R3

## GGW-P1-02

- Head: `cbfdee98b47d8ec6cf9601a77231d4e5a9eb9d39`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-02`
- Verdict: `CHANGES_REQUIRED`

### P102-R1 — High / High confidence

- Location: `scripts/interop/verify-mcp-proxy-gitnexus.mjs:319-336`
- Evidence: runner 默认每项为 `PASS`，只有 callback 抛异常才失败。返回
  `SKIP`、`constraint` 或未证明目标行为的文本仍计为 PASS。报告因此把
  `unknown tool` 未报错和请求在 abort 前完成都计入 28/28 PASS。
- Violated item: verdict 和每个结论必须可追溯到真实协议证据；失败时应返回非零。
- Expected: 建模 `PASS|FAIL|SKIP|CONSTRAINT`，每项断言目标行为；必需验收项为
  SKIP/CONSTRAINT 时总体不得成为 PASS，且进程退出码反映门禁失败。

### P102-R2 — High / High confidence

- Location: `scripts/interop/verify-mcp-proxy-gitnexus.mjs:495-570`
- Evidence: 没有注入 GitNexus 上游异常，也没有断开一个已初始化客户端并验证
  Session/子进程回收。所谓 timeout 测试调用快速 `tools/list`；实际报告明确写着
  “request completed before abort”，没有发生 timeout/cancellation。
- Violated item: 必须验证并记录上游异常、超时和客户端断开。
- Expected: 使用可控 fixture MCP 子进程实现延迟、崩溃和断线场景，分别断言
  HTTP/MCP 错误、超时/取消、Session 清理以及其他会话不受影响。

### P102-R3 — High / High confidence

- Location: `scripts/interop/verify-mcp-proxy-gitnexus.mjs:574-603`,
  `:715-770`
- Evidence: cleanup 测试在第二个 proxy 启动失败时也继续；只比较名为
  `gitnexus.exe` 的进程数量，实际证据为 `0 -> 0`。它没有证明被测 GitNexus
  子进程曾存在、属于哪个 Session 或在 proxy 退出后消失。报告仍断言
  “每 Session 一个子进程”以及 stdout/stderr 正确隔离，但没有对应测量。
- Violated item: 明确会话/进程映射、proxy 退出后的子进程行为，且结论来自证据。
- Expected: 记录被测 proxy 和各 Session 对应的实际子进程 PID/启动标记，先证明
  存在再证明退出；启动失败必须 FAIL。捕获 stdout/stderr 并对通道隔离做断言。

### P102-R4 — Medium / High confidence

- Location: `.gitignore:63-69`,
  `docs/tasks/GGW-P1-02-mcp-proxy-interop-gate.md:39-45`
- Evidence: Implementation HEAD 在仓库根 `.gitignore` 增加了 fixture Git、
  GitNexus 与代理元数据规则；任务卡允许范围只包含 `scripts/interop/`、
  `tests/fixtures/mcp/`、`docs/compatibility/` 及任务所需的开发依赖锁文件，
  未授权修改根目录 `.gitignore`。
- Violated item: 交付必须遵守任务卡的 Allowed scope；仓库级忽略规则属于范围外
  变更，且会影响其他任务和开发者看到的工作树状态。
- Expected: 从整改提交移除根目录 `.gitignore` 变更；若 fixture 会产生运行时
  文件，应由互操作脚本在受控临时目录内创建并清理，或先通过任务卡变更显式扩大
  允许范围后再修改仓库级规则。

### P102-R5 — Medium / High confidence

- Location: `docs/compatibility/mcp-proxy-gitnexus-interop-report.md:6,35`
- Evidence: 报告的 `Commit` 写成任务基线
  `87afdaeb7c5579facb93edf15f786011ebb3c4ca`，而被审实现 HEAD 为
  `cbfdee98b47d8ec6cf9601a77231d4e5a9eb9d39`，无法追溯报告实际测试的实现；
  “测试仓库”还包含
  `F:\develop\worktrees\GraphGateway-p1-02\tests\fixtures\mcp\sample-repo`
  这一审查者本机绝对路径。
- Violated item: 兼容性结论必须来自可复现、已脱敏且可追溯到被测版本的证据。
- Expected: 在已提交的实现版本上重新运行门禁，报告明确记录实际被测 commit；
  路径使用仓库相对路径（如 `tests/fixtures/mcp/sample-repo`）或稳定占位符，
  不得写入开发者机器的绝对路径。

### Verification evidence

- `node --check scripts/interop/verify-mcp-proxy-gitnexus.mjs`: PASS
- `node ... --help`: PASS
- 执行器报告真实链路运行退出码为 0、报告为 `PASS_WITH_CONSTRAINTS`
- generation 能力缺失证据可信：P1-08 保持 BLOCKED
- 由于 runner 和必需场景存在上述缺陷，28/28 PASS 不能作为验收证据

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
| GGW-P1-02 | `CHANGES_REQUIRED` | P102-R1、P102-R2、P102-R3、P102-R4、P102-R5 |
| GGW-P1-03 | `PASS` | 无 |

GGW-P1-03 已集成到 `mcp`；GGW-P1-01 与 GGW-P1-02 仍为
`CHANGES_REQUIRED`，不得集成。后续整改必须基于各自当前完整
Implementation HEAD，保留未关闭的 finding ID，并限制为原任务范围内的一轮修复。
