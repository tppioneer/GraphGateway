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

- Head: `8300780b505e5e9e68fcd5acdb09785634acdd7a`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-02`
- Verdict: `CHANGES_REQUIRED`

### P102-R1 — High / High confidence

- Status: OPEN after remediation
  `8300780b505e5e9e68fcd5acdb09785634acdd7a`
- Location: `scripts/interop/verify-mcp-proxy-gitnexus.mjs:88-110`,
  `:230-270`, `:606-632`
- Evidence: 结果模型虽已改为 `PASS|FAIL|SKIP|CONSTRAINT`，但必需前置命令仍通过
  吞掉异常的 `sh()` 执行。独立运行中 `git commit` 因 shell 将提交消息拆成
  pathspec 而失败，runner 只记录 note，继续索引并把固定仓库 tests 4.1-4.5
  全部记为 PASS。`gitnexus analyze` 同样使用 `sh()`，非零退出不会进入
  `catch`；repo 无法精确解析时又退回 basename。query/context 失败后还会改为
  不带 repo 的全局查询或仅凭 `file_path` 查询，并继续返回 PASS。
- Violated item: verdict 和每个结论必须可追溯到真实协议证据；失败时应返回非零。
- Expected: 所有 fixture 初始化、Git 提交和 GitNexus analyze 命令必须
  fail-fast；精确确认临时 fixture 的 canonical identity 后才运行协议门禁。
  query/context 必须始终绑定并断言该 identity，不允许无 repo/global fallback。

### P102-R2 — High / High confidence

- Status: OPEN after remediation
  `8300780b505e5e9e68fcd5acdb09785634acdd7a`
- Location: `scripts/interop/verify-mcp-proxy-gitnexus.mjs:741-808`,
  `tests/fixtures/mcp/controllable-server.mjs:139-146`
- Evidence: 可控 fixture 已能制造 delay/crash/disconnect，且上游 crash 场景已
  实际执行；但 cancellation test 只断言本地 `fetch` 在 500 ms 收到
  `AbortError`，随后立即强制终止整个 proxy，没有证明延迟中的上游请求被取消、
  对应 Session 被回收或同一 proxy 的其他 Session 不受影响。fixture 的
  `disconnect` tool 没有被任何测试调用，因此“客户端断开和 Session 清理”仍无
  端到端证据。
- Violated item: 必须验证并记录上游异常、超时和客户端断开。
- Expected: abort/disconnect 后保持 proxy 运行，分别观察并断言上游请求终止、
  被断开 Session 的后续行为、相关子进程回收，以及 sibling Session 的可用性；
  将 fixture 的 disconnect 场景纳入必需门禁。

### P102-R3 — High / High confidence

- Status: OPEN after remediation
  `8300780b505e5e9e68fcd5acdb09785634acdd7a`
- Location: `scripts/interop/verify-mcp-proxy-gitnexus.mjs:941-1045`,
  `:1133-1282`, `:1345-1383`;
  `docs/compatibility/mcp-proxy-gitnexus-interop-report.md:33,155,200`
- Evidence: 独立运行中 descendant 查询实际返回空，runner 转而把全系统 13 个
  新 PID 的前 8 个当作子进程；其中多个在关联验证前已退出，11.2 仍为 PASS，
  随后这些任意 PID 恰好消失又使 11.3 PASS。报告在 cleanup 前生成；最终 cleanup
  警告仍有 6 个新 PID，却不进入测试结果或 verdict。报告还同时断言“每 Session
  一个独立 GitNexus 子进程”和“共享 upstream 导致 sibling Session 失败”，
  会话/进程模型自相矛盾。stdout/stderr 测试也只比较两个 buffer 的字节数，
  未验证 stdout 只含协议消息或 stderr 日志不污染协议通道。
- Violated item: 明确会话/进程映射、proxy 退出后的子进程行为，且结论来自证据。
- Expected: 使用可靠的 parent/child 或 Job Object 关联证明每个被测 PID 的归属；
  无法建立关联时 FAIL，不得使用全系统 PID 差值替代。把所有 cleanup 结果纳入
  report/verdict 后再写报告，并由测量结果生成唯一一致的会话/进程模型结论；
  对 stdout 协议帧和 stderr 日志内容分别做断言。

### P102-R4 — Medium / High confidence

- Status: RESOLVED at
  `8300780b505e5e9e68fcd5acdb09785634acdd7a`
- Location: `.gitignore:59-63`,
  `scripts/interop/verify-mcp-proxy-gitnexus.mjs:212-280`
- Closure evidence: 累计整改差异只从根 `.gitignore` 删除原先越界的 fixture
  规则，没有新增仓库级规则；runner 将 fixture 源文件复制到 OS 临时目录，
  排除 `.git`、`.gitnexus`、`.claude`，运行结束后删除该临时目录。

### P102-R5 — Medium / High confidence

- Status: RESOLVED at
  `8300780b505e5e9e68fcd5acdb09785634acdd7a`
- Location: `docs/compatibility/mcp-proxy-gitnexus-interop-report.md:6,35`
- Closure evidence: 报告提交位于被测代码提交
  `16eb2d1131511816d8a81f3d35eb808d61f333a5` 之后，`Tested commit`
  精确记录该代码提交；fixture 使用仓库相对路径
  `tests\fixtures\mcp\sample-repo`，报告和互操作目录扫描未发现本机绝对路径。

### Verification evidence

- `node --check scripts/interop/verify-mcp-proxy-gitnexus.mjs`: PASS
- `node ... --help`: PASS
- `node --version`: PASS，`v24.14.1`
- `mcp-proxy --version`: PASS，`6.5.4`
- `gitnexus --version`: PASS，`1.6.9`
- 独立完整运行：31 PASS、0 FAIL、0 SKIP、4 CONSTRAINT，
  `PASS_WITH_CONSTRAINTS`；但实际同时出现 fixture commit 失败、无可靠 descendant
  关联和 cleanup 后 6 个新 PID 告警，均未反映到报告 verdict
- generation 能力缺失证据可信：P1-08 保持 BLOCKED
- P102-R4、P102-R5 关闭；P102-R1、P102-R2、P102-R3 继续保持 OPEN

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
| GGW-P1-02 | `CHANGES_REQUIRED` | P102-R1、P102-R2、P102-R3 |
| GGW-P1-03 | `PASS` | 无 |

GGW-P1-03 已集成到 `mcp`；GGW-P1-01 与 GGW-P1-02 仍为
`CHANGES_REQUIRED`，不得集成。后续整改必须基于各自当前完整
Implementation HEAD，保留未关闭的 finding ID，并限制为原任务范围内的一轮修复。
