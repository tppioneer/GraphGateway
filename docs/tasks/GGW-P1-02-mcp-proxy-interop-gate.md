# GGW-P1-02 mcp-proxy / GitNexus 互操作门禁

## 元数据

- State: `VERIFIED`
- Implementation HEAD: `1d180f442115e2d47361e905785531bf2c92f141`
- Tested code commit: `414632dacc819d4c9c36fc863a3c608cec8fffd5`
- Remediation base: `59676cd8cc2b1026c1b165a6ea6ab09f8b691e52`
- Remediation round: `3/3`（用户于 2026-07-27 显式授权例外轮次）
- Review: `docs/reviews/GGW-wave-A-review-2026-07-26.md#ggw-p1-02`
- Open findings: 无
- Closed findings: `P102-R1`、`P102-R2`、`P102-R3`、`P102-R4`、`P102-R5`（2026-07-28 round 3 独立复审，reviewer: Claude Code）
- Next action: 待集成到 `mcp`（需用户确认）
- Executor: Claude Code
- Model: `glm-5.2`
- Delivery receipt: Claude Code JSON envelope
  `is_error=false`、`terminal_reason=completed`、零 permission denial
- Depends on: 无
- Parallel with: P1-01、P1-03
- Expected HEAD: `87afdaeb7c5579facb93edf15f786011ebb3c4ca`
- Suggested branch: `codex/ggw-p1-02-mcp-interop`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-02`
- Budget: 一次实现，最多两轮整改

## 已接受的复审决策

- `P102-R1`: 禁止通过 repository basename 猜测 identity；只接受规范化完整路径
  的唯一匹配，零匹配或多匹配都必须 FAIL。
- `P102-R2`: 客户端断开后不要求向 GitNexus 上游传播中断；必须测量并记录上游
  请求继续完成、被取消或状态未知，以及原 Session、sibling Session、upstream PID
  和相关进程的实际行为。
- `P102-R3`: 黑盒验证只保留三项硬要求：MCP 功能检查失败必须 FAIL；所有 cleanup
  结果必须在报告生成前进入 verdict；进程归属必须通过 parent/child 关系证明。
  不要求测试脚本直接观测 GitNexus 原始 stdout/stderr。
- `P102-R4`: 直接删除根 `.gitignore` 的范围外空行，使最终累计差异与原始基线一致。

## 第三轮整改契约

- Base: `59676cd8cc2b1026c1b165a6ea6ab09f8b691e52`
- Resolve only: `P102-R1`、`P102-R2`、`P102-R3`、`P102-R4`
- `P102-R1`: 删除 basename identity fallback；完整路径规范化后必须唯一匹配，
  零匹配或多匹配均 FAIL。
- `P102-R2`: 不传播客户端 abort 也可接受；必须等待并记录可控上游请求最终是
  继续完成、被取消还是状态未知，并记录原 Session、sibling Session、upstream PID
  和相关进程行为。`siblingAlive=false` 不得返回 PASS。
- `P102-R3`: MCP 功能检查失败必须 FAIL；所有 cleanup 结果必须在报告生成前进入
  verdict；进程归属必须仅通过 parent/child 关系证明。无需直接观测 GitNexus
  原始 stdout/stderr。
- `P102-R4`: 唯一允许的原任务范围外修改是删除根 `.gitignore` 中该任务引入的
  空行；最终 `.gitignore` 必须与原始基线
  `87afdaeb7c5579facb93edf15f786011ebb3c4ca` 完全一致。
- 禁止修改本任务卡、审查文档、设计文档、生产 Rust/Tauri 代码或其他无关文件。
- 完整门禁必须从 clean worktree 运行；报告提交必须位于被测代码提交之后，
  `Tested commit` 必须精确指向被测代码提交且不得包含本机绝对路径。

## 第三轮交付

- Code commit: `414632dacc819d4c9c36fc863a3c608cec8fffd5`
- Initial report commit:
  `8164baa0e24f8442f25ed5cc9200b43fd70a76b7`
- Clean-gate report commit:
  `1d180f442115e2d47361e905785531bf2c92f141`
- Clean detached gate: 38 tests，32 PASS、0 FAIL、0 SKIP、6 CONSTRAINT，
  `PASS_WITH_CONSTRAINTS`
- Report `Tested commit`:
  `414632dacc819d4c9c36fc863a3c608cec8fffd5`
- Delivery state: 等待 Codex 对原始基线到最终 HEAD 的累计差异进行独立复审；
  `P102-R1`～`P102-R4` 尚未关闭。

## Objective

用可重复的黑盒验证证明 mcp-proxy 能否把 GitNexus stdio MCP 透明暴露为
Streamable HTTP，并明确 generation、会话、并发和关闭语义，作为后续实现门禁。

## Source of truth

- `docs/graph-workspace-mcp-router-design.zh-CN.md` 第 8、15、22 节
- mcp-proxy 与 GitNexus 当前安装版本的标准 CLI/MCP 行为

## Execution envelope

这是技术 spike，只产出测试脚本、固定输入和兼容性报告。通过标准 MCP
initialize、tools/list、resources/list/read、tools/call 观察行为，不读取或依赖
mcp-proxy 私有控制接口。

## Invariants

- mcp-proxy 只做传输适配，不承载业务路由。
- 结论必须来自命令和协议报文证据；未知能力标记为未知/不支持。
- 若 generation 能力不足，记录阻塞条件，不发明私有补丁协议。

## Allowed scope

- `scripts/interop/`
- `tests/fixtures/mcp/`
- `docs/compatibility/`
- 本任务所需的开发依赖锁定文件

## Forbidden scope

- 所有生产 Rust crate 和 Tauri 业务代码
- 通过源码猜测替代黑盒验证
- 修改或 fork mcp-proxy/GitNexus
- 将本机索引、用户数据或完整 MCP 敏感载荷提交仓库

## Acceptance criteria

- 报告记录 OS、Node、mcp-proxy、GitNexus 的精确版本和完整启动参数。
- 自动化脚本验证 initialize、会话 ID、tools/list、resources/list/read 和至少一次
  对固定仓库的只读 tools/call。
- 验证并记录：两个客户端会话、并发请求、上游异常、超时、客户端断开和
  proxy 退出后的 GitNexus 子进程行为。
- 列出 GitNexus 暴露的 generation 相关工具、参数、返回值；若不存在，明确
  P1-08 的 `BLOCKED` 条件。
- 报告给出 `PASS`、`PASS_WITH_CONSTRAINTS` 或 `FAIL`，且每个结论可追溯到
  脱敏证据。

## Verification commands

```powershell
node --version
mcp-proxy --version
gitnexus --version
node scripts/interop/verify-mcp-proxy-gitnexus.mjs --fixture tests/fixtures/mcp/sample-repo
```

验证脚本必须可重复运行，负责启动和回收自身创建的进程，并在失败时返回非零码。

## Delivery contract

返回 `AGENT_RESULT`：`DONE|BLOCKED`、完整提交 SHA、兼容性结论、版本矩阵、
验证输出、证据文件、对 P1-05/P1-08 的约束。不得把“进程能启动”等同于 MCP
互操作通过。
