# GGW-P1-13 Windows 桌面应用图标设计与接入

## 元数据

- State: `DRAFT`
- Default executor: Claude Code
- Depends on: P1-01
- Parallel with: P1-08、P1-12
- Expected HEAD: `TO_BE_SET_AFTER_P1_01_INTEGRATION`
- Suggested branch: `codex/ggw-p1-13-desktop-icon`
- Suggested worktree: `F:\develop\worktrees\GraphGateway-p1-13`
- Budget: 一次实现，最多两轮整改

## Objective

为 GraphGateway Desktop 设计一套原创、可维护、适合小尺寸显示的正式应用图标，
替换当前占位资源，并接入 Windows 安装包、可执行文件、任务栏、系统托盘和窗口
标题栏。图标在 Windows 深色与浅色背景下都应保持清晰的轮廓和品牌辨识度。

## Source of truth

- `docs/graphgateway-windows-tauri-product-design.zh-CN.md` 第 10、17 节
- P1-01 Windows Owned Sidecar 发布基线
- `apps/graphgateway-desktop/src-tauri/tauri.conf.json`
- `apps/graphgateway-desktop/src-tauri/icons/`

## Visual direction

- 核心意象为“图节点/连接关系经过网关汇聚”，与 GraphGateway 名称直接关联。
- 使用简洁几何轮廓，不依赖文字、字母缩写、细线或照片式细节传达含义。
- 正式应用图标可使用有限品牌色；主轮廓在浅色和深色底上都必须可分辨。
- 系统托盘提供针对浅色任务栏和深色任务栏优化的高对比变体，不能只依赖
  Windows 自动缩放或反色。
- 16×16、20×20、24×24 等小尺寸需要独立像素校正，不得仅把大图机械缩小。

## Execution envelope

执行器负责图标视觉设计、源文件、派生资产、Tauri/Windows 接入和可重复生成方式。
可以对现有桌面启动代码做最小修改，以创建系统托盘图标并在系统主题变化后选择
正确的深浅色变体。不得借此改变窗口、Sidecar 或页面的产品行为。

## Invariants

- 图标源文件和全部派生资产必须可在仓库内追溯，禁止只提交不可编辑的最终位图。
- 图标必须为本项目原创或采用明确允许商业分发的组成元素；交付中记录来源与许可。
- Windows 可执行文件、安装包、任务栏和窗口标题栏使用同一品牌图形。
- 系统托盘深浅色变体保持同一轮廓和占位尺寸，切换主题时不能出现明显跳动。
- 托盘接入不得改变“关闭最后一个窗口后停止 Sidecar 并正常退出”的现有语义，
  不得使进程因托盘对象而意外驻留。
- 不新增网络权限，不改变 WebView、REST、认证或 Owned Sidecar 边界。

## Allowed scope

- `apps/graphgateway-desktop/src-tauri/icons/`
- `apps/graphgateway-desktop/src-tauri/tauri.conf.json`
- `apps/graphgateway-desktop/src-tauri/src/` 中与窗口/托盘图标接入直接相关的代码和测试
- 图标生成、尺寸检查和视觉验收所需的项目内脚本
- 与图标资产生成和验收直接相关的桌面应用 README

## Forbidden scope

- Workspace、Source、Router、REST、MCP 和 Sidecar 业务逻辑
- 页面布局、配色系统或完整品牌视觉体系重做
- 系统托盘菜单、通知、点击动作、最小化到托盘或关闭到托盘
- 修改窗口关闭、Sidecar 停止和进程回收语义
- Linux、macOS 专属图标适配和移动端图标
- 提交设计工具缓存、无授权字体、素材库资产或本机绝对路径

## Required deliverables

- 一份可编辑的矢量母版，明确画布、安全区、品牌色和透明背景规则。
- Windows 多分辨率 `icon.ico`，至少包含
  16、20、24、32、40、48、64、128 和 256 像素版本。
- Tauri bundle 所需的 PNG 派生资源；不得保留当前占位图。
- 系统托盘浅色背景版和深色背景版，以及其高 DPI 派生资源。
- 可重复的资产生成命令或脚本；从母版生成两次应得到相同输出。
- 一张视觉验收图，按实际显示尺寸同时展示正式图标和托盘变体在白色、浅灰、
  深灰和黑色背景上的效果。

## Acceptance criteria

- 打包后的 `GraphGateway Desktop.exe` 和安装包显示正式图标，不出现 Tauri 默认
  图标、空白图标或旧缓存占位图。
- 应用运行时，Windows 任务栏、主窗口标题栏左上角和系统托盘均显示 GraphGateway
  图标；三处图形语言一致。
- 在 Windows 浅色和深色模式下，任务栏、系统托盘和标题栏图标的主要轮廓均清晰，
  16×16 与 20×20 实际尺寸下无裁切、糊成色块或关键节点粘连。
- 系统主题在应用运行期间切换时，托盘图标在合理时间内切换为对应变体；若系统
  主题无法读取，则使用在两种背景上都可辨识的安全回退图标。
- 100%、125%、150% 和 200% DPI 缩放下，Windows 能选择合适的图标尺寸，且无
  明显插值模糊、透明边缘黑边或非预期背景色。
- 自动检查能验证必需尺寸、文件格式、透明通道、ICO 内嵌帧和 Tauri 配置引用；
  生成脚本与相关 Rust 测试通过。
- 创建托盘图标后，关闭最后一个窗口仍会停止 Sidecar，桌面进程和托盘图标均退出，
  不遗留后台进程。
- 交付材料记录视觉选择、颜色值、母版到派生资源的生成方式及第三方素材许可结论。

## Verification commands

```powershell
cargo fmt --all -- --check
cargo test --manifest-path apps/graphgateway-desktop/src-tauri/Cargo.toml --all-features
Set-Location apps/graphgateway-desktop
npm ci
npm run build
cargo tauri build --no-bundle
cargo tauri build --bundles nsis
```

还需运行任务新增的图标资产检查命令，并在 Windows 实机完成以下 smoke test：

1. 分别在浅色和深色模式启动打包后的应用。
2. 截取任务栏、系统托盘和窗口标题栏的实际显示效果。
3. 应用运行期间切换一次系统主题，确认托盘变体随之更新。
4. 至少在 100% 和 200% DPI 下检查小尺寸清晰度；其余 DPI 由资产检查覆盖。
5. 关闭最后一个窗口，确认 Sidecar、Desktop 进程和托盘图标全部退出。

## Delivery contract

返回 `AGENT_RESULT`：`READY_FOR_REVIEW|BLOCKED`、base/head 完整提交 SHA、
母版与派生资产清单、颜色和许可说明、变更文件、自动化命令与结果、浅色/深色及
DPI smoke 截图路径、关闭后进程回收证据、验收清单、范围偏差、开放问题和风险。
不得提交安装包、构建产物或仅存在于外部设计工具中的源文件。
