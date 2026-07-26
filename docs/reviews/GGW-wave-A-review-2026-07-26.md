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

### Verification evidence

- `node --check scripts/interop/verify-mcp-proxy-gitnexus.mjs`: PASS
- `node ... --help`: PASS
- 执行器报告真实链路运行退出码为 0、报告为 `PASS_WITH_CONSTRAINTS`
- generation 能力缺失证据可信：P1-08 保持 BLOCKED
- 由于 runner 和必需场景存在上述缺陷，28/28 PASS 不能作为验收证据

## GGW-P1-03

- Head: `c5e3813aeeb7558b3c4f0ddd83f9cb07a8763ae6`
- Worktree: `F:\develop\worktrees\GraphGateway-p1-03`
- Verdict: `CHANGES_REQUIRED`

### P103-R1 — High / High confidence

- Location: `crates/graphgateway-types/src/view.rs:17-60`
- Evidence: `ResolvedGraphView.members`、`ViewMember.generation_id` 等字段全部公开，
  调用方可在创建后直接替换成员或 generation。ViewMember 也没有 endpoint 或
  capability snapshot，因此无法表达任务要求的不可变
  `Source + generation -> endpoint/capability` 绑定。
- Violated item: ResolvedGraphView 创建后不能原地改变 generation 或 endpoint。
- Expected: 通过私有字段和受校验构造器建立不可变视图，仅提供只读 accessor；
  member 包含已解析 endpoint 与同 generation 的 capability snapshot。添加
  compile-fail/API 测试或等价证据证明调用方不能原地修改。

### P103-R2 — High / High confidence

- Location: `crates/graphgateway-types/src/source.rs:123-126`,
  `crates/graphgateway-core/src/validation.rs:93-115`
- Evidence: loopback 校验使用字符串 `contains`。例如
  `https://example.invalid/?localhost` 或
  `http://127.0.0.1.example.invalid/mcp` 会被 Local Source 接受。
  `SourceKind::Unknown` 还会完全跳过 loopback/TLS 校验。
- Violated item: 校验必须拒绝非法本地/远程配置；本地 endpoint 不得逃逸 loopback。
- Expected: 使用 URL parser 解析 scheme/host/port，按 IP/hostname 精确判断
  loopback；Unknown 必须 fail closed，至少按 remote TLS/read-only 处理。

### P103-R3 — High / High confidence

- Location: `crates/graphgateway-core/src/validation.rs:118-125`,
  `:143-177`
- Evidence: writable 校验只要求 `location == Local`，没有要求 SourceRole::Primary，
  也没有要求 writable Source 等于 Workspace.primary_source_id。因此任意 local
  baseline/dependency/member 都可设置 `writable=true` 并通过验证。
- Violated item: 写操作只能指向 Workspace 的 local primary Source。
- Expected: Workspace 级校验保证最多一个 writable Source，且它必须同时是
  local、role=Primary、source_id=primary_source_id；AllReadOnly 等 write policy
  必须禁止 writable Source，并添加拒绝测试。

### Verification evidence

- `cargo fmt --all -- --check`: PASS
- `cargo clippy -p graphgateway-types -p graphgateway-core --all-targets --all-features -- -D warnings`: PASS
- `cargo test -p graphgateway-types -p graphgateway-core --all-features`: PASS，83 tests
- 测试通过但没有覆盖上述不变量，且 P103-R1 可由公开 API 直接复现

## Wave A summary

| Task | Verdict | Open findings |
| --- | --- | --- |
| GGW-P1-01 | `CHANGES_REQUIRED` | P101-R1、P101-R2、P101-R3 |
| GGW-P1-02 | `CHANGES_REQUIRED` | P102-R1、P102-R2、P102-R3 |
| GGW-P1-03 | `CHANGES_REQUIRED` | P103-R1、P103-R2、P103-R3 |

三项均未达到 `VERIFIED`，不得集成到 `mcp`。后续整改必须基于各自当前完整
Implementation HEAD，保留以上 finding ID，并限制为原任务范围内的一轮修复。
