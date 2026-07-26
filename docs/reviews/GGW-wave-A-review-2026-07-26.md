# GraphGateway Wave A 独立代码审查

- Review date: 2026-07-26
- Reviewer: Codex
- Base: `87afdaeb7c5579facb93edf15f786011ebb3c4ca`
- Scope: GGW-P1-01、GGW-P1-02、GGW-P1-03
- Mode: Review only；未修改实现分支

## GGW-P1-01

- Head: `4d9542246c89dda336555d7416a4641c13d11419`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-01`
- Verdict: `CHANGES_REQUIRED`

### P101-R1 — High / High confidence

- Location: `crates/graphgateway-server/tests/smoke_test.rs:30-48`
- Evidence: 新 smoke test 直接启动 workspace 的
  `target/debug/graphgateway.exe`，把测试进程自身写为 `parent_pid`。它没有启动
  `cargo tauri build --no-bundle` 产生的 Tauri Product，也没有终止 Tauri 宿主来
  观察 Job Object 是否回收已打包 Sidecar。执行器只证明了 debug Sidecar 自身的
  ready、REST shutdown 和退出。
- Violated item: “打包产物能启动 Sidecar”以及“宿主退出后的进程回收”验收标准。
- Expected: 增加 Windows 端到端 smoke，启动实际构建的 desktop executable，
  验证其拉起打包 Sidecar，随后终止/退出宿主并证明完整子进程树被回收；损坏或
  缺失打包 Sidecar 的失败路径也必须无残留进程。

### P101-R2 — Medium / High confidence

- Location: `apps/graphgateway-desktop/README.md:119-123`
- Evidence: README 声称 Job Object assignment 失败时 Sidecar 仍会启动；实际
  `sidecar/mod.rs:155-177` 将 Job Object 创建/分配视为硬条件，失败后 rollback
  并返回错误。
- Violated item: 可重复发布文档必须准确描述既有 Job Object/rollback 不变量。
- Expected: 文档改为说明启动会失败且 Sidecar 被回收，并给出诊断方式。

### P101-R3 — Medium / High confidence

- Location: `apps/graphgateway-desktop/scripts/copy-sidecar.mjs:22-30`
- Evidence: Sidecar 文件名只根据 Node 进程的 host `arch()` 推导，源文件也固定为
  `target/release/graphgateway.exe`。当 Tauri/Rust 使用显式 `--target` 或
  `CARGO_BUILD_TARGET` 时，脚本仍复制 host 目录并生成 host triple 名称。
- Violated item: clean build 必须准备“正确 target-triple 名称”的 Sidecar。
- Expected: 从 Tauri/Cargo 的有效 target triple 获取名称，并让构建命令、target
  输出目录和复制路径使用同一个 triple；为非默认 target 添加测试。

### Verification evidence

- `cargo fmt --all -- --check`: PASS
- `cargo test --workspace --all-features`: PASS，全部 workspace tests 与 doctests 通过
- `npm run build`: PASS
- 执行器报告 `npx tauri build --no-bundle`: PASS
- 未获得真实 packaged desktop 启动/宿主退出 smoke 证据

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

- Head: `247c87d5e4325b3909cbd0eab278109a8707aa43`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-03`
- Verdict: `CHANGES_REQUIRED`

### P103-R1 — High / High confidence

- Status: OPEN after remediation round 1
- Location: `crates/graphgateway-types/src/view.rs:58-85`,
  `crates/graphgateway-core/src/validation.rs:313-336`
- Evidence: 字段私有化、只读 accessor 和 compile-fail 证据已经完成，但
  `ViewMember::new` 接受任意 `CapabilitySnapshot`，没有校验
  `capability.source_id == source_id`；`validate_view_member` 对反序列化对象也没有
  检查该关系，因此仍可把其他 Source 的能力快照绑定到当前成员。
- Violated item: immutable
  `Source + generation -> endpoint/capability` 绑定必须在构造和边界校验中成立。
- Expected: 构造器拒绝 capability Source 身份不匹配，核心校验同时覆盖
  反序列化输入，并添加构造器与反序列化负向测试；保留现有不可变 API。

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

- Status: OPEN
- Location: `crates/graphgateway-types/src/view.rs:58-85,429-454`
- Evidence: `ViewMember::new` 无条件设置 `unavailable=false`，没有正常类型 API
  构造不可用成员；`is_degraded_true` 测试明确通过 JSON 反序列化绕过受校验构造器。
- Violated item: 设计允许部分成员不可用，但响应必须标记降级和缺失成员；领域类型
  必须能在不破坏不可变性的前提下表达该状态。
- Expected: 增加显式、不可变且受校验的不可用成员构造路径，保持现有
  `unavailable` JSON 布尔格式，并用公共类型 API 测试 degraded view。

### Verification evidence

- `cargo fmt --all -- --check`: PASS
- `cargo clippy -p graphgateway-types -p graphgateway-core --all-targets --all-features -- -D warnings`: PASS
- `cargo test -p graphgateway-types -p graphgateway-core --all-features`: PASS，
  50 core + 73 types + 1 组 trybuild（5 cases）
- P103-R2、P103-R3 已关闭；P103-R1、P103-R4 进入第二轮整改

## Wave A summary

| Task | Verdict | Open findings |
| --- | --- | --- |
| GGW-P1-01 | `CHANGES_REQUIRED` | P101-R1、P101-R2、P101-R3 |
| GGW-P1-02 | `CHANGES_REQUIRED` | P102-R1、P102-R2、P102-R3、P102-R4、P102-R5 |
| GGW-P1-03 | `CHANGES_REQUIRED` | P103-R1、P103-R4 |

三项均未达到 `VERIFIED`，不得集成到 `mcp`。后续整改必须基于各自当前完整
Implementation HEAD，保留以上 finding ID，并限制为原任务范围内的一轮修复。
