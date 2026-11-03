# Beta.2 发布准备

更新：2026-11-03。计划发布 `v0.1.0-beta.2`；应用内四处版本继续为 `0.1.0`。
本文是当前流程，历史阶段报告保留在 [archive/beta2/](archive/beta2/)。

## 当前代码

PR #6 / #7 已合并，基线 main 为 `9f9d65d995a06cede71b4850288d324e5f1b8c35`。PR #9 正在加入用户主动授权的通用复制兼容模式，并保留 flow 中已有的首次工具条修复。旧冻结 EXE / ZIP 不包含新代码，不能继续作为新功能发行候选。

先完成 PR #9 自动化检查及授权后的专项原生验收，再由维护者决定合并；合并后核对确定 main SHA 与 CI，经唯一 `pnpm release` 构建最终候选。本次测试构建不等于发布放行。

## 开发和 CI

Windows 本机使用 Node **24.22.1**、pnpm **9.15.9**、已验证的 Rust **1.98.1**、Tauri 2 与本机 WebView2。
既有 Node engine 校验保留，无新增环境管理器或工具链文件。命令和临时 PATH 修正见 [贡献指南](../../CONTRIBUTING.md)。

日常用 `pnpm dev`；按改动选择 `pnpm test` / `pnpm test:js` / `pnpm test:rs` 与 `pnpm check`。
只改前端无需构建 Rust Release。PR 的一个 Windows CI 完成完整验证：前端构建、全部单元测试、
Playwright、聚合检查（CSP/内联 JS/版本/净化白名单）、Rust fmt、默认与隔离特性 Clippy、
普通 Release 构建及窗口引导、默认 Rust 测试、Debug/Release 隔离路径与窗口引导、
Debug/Release 实验复制策略。不同 profile/feature 的安全检查保留；同配置 Release `cargo check` 已由实际 build 覆盖。
CI 不启动桌面程序，不公开上传构建包，不要求第三方 Reviewer。

## 唯一打包入口

在 `deepseek-float/` 执行 **`pnpm release`**，调用 `scripts/build-release.mjs`。
`release.ps1` 已被既有说明使用，继续作为薄包装；不维护第二套构建配方。

1. 校验 Node/pnpm 配对，冻结安装依赖。
2. 构建前端，执行现有 `pnpm check`，检查版本一致性及 CSP。
3. 检查生产 features 恰为 `custom-protocol`，拒绝继承 `TAURI_CONFIG` 覆盖。
4. 执行普通 Release 授权、目标与会话门控策略回归，然后构建普通 Release。
   普通构建放在最后，避免策略测试的 feature 污染最终 EXE。
5. 检查所有 HTML 资源新鲜度，可选签名，然后生成 EXE SHA256。
6. 按既有许可 CHECKSUMS 逐文件核对并组装材料，生成完整包校验表、一个 Windows x64 ZIP 及 ZIP SHA256。

完整单元测试、fmt、Clippy 和 Debug 测试由 CI 执行，不在打包中重复运行。
`--no-check` 不再允许；`--no-frontend` 仍须通过资源检查；`--no-deliver` 仅构建而不刷新交付目录。
`--check-toolchain-only` 只验证工具链。本地构建不等于发布，不启动 EXE，不创建 Tag/GitHub Release。

输出只使用已有的 `deepseek-float/release/`：EXE、`HASH.txt`、原目录结构的许可/源码材料、
`CHECKSUMS.csv`、`windows-ai-float-assistant-v0.1.0-beta.2-windows-x64.zip` 和 `.zip.sha256`。
临时组装目录在系统 TEMP 自动清理；ZIP 只收录经校验的材料及新 EXE，不收录旧 release 目录内容。
PDB、用户配置、日志、开发缓存、Token、私钥不应进入包；不要手动压缩整个工作目录。
许可证文本、NOTICE、MPL 对应源码归档及索引完整保留，不能因为精简流程删除。

## 原生验收和剩余放行项

使用[普通 Release 简单清单](beta2-native-acceptance-plan.md)，一次授权会话验启动/UI Ready、球窗、
A01、设置、截图/OCR、主要 AI 请求和退出。隔离构建仅作可选诊断，不重复全部普通功能。
数据操作使用专用测试内容；不能确保安全的设置保存/备份恢复先不执行并记录原因。

当前尚未放行，真实剩余事项：

- PR #9 专项原生验收及必要修复完成，维护者合并后对应 main GitHub CI 通过。
- 新普通 Release 的主要功能、正常退出与剪贴板专项人工验收；本轮未授权真实剪贴板测试，不能宣称通过。
- brotli 文件级通知及原生组件范围已在现有第三方说明中澄清；最终 ZIP 仍按校验表检查，不要求企业级二进制审计。
- 若发现可稳定复现的严重故障或严重用户数据丢失，修复后再发布。

[Issue #2](https://github.com/bookguifan/windows-ai-float-assistant/issues/2) 可作为已知问题开放，
一次成功启动不说明根治；它的开放状态本身不是额外发布门槛。
历史所有原生矩阵、PDB 身份报告、真实备份恢复或特定 VM 验收不再是统一强制门槛。
升级时退出旧实例并保留用户配置、收藏和 WebView2 数据，不清空 AppData。

## 通用兼容模式专项清单

默认安全模式不自动复制；切到兼容模式需原生确认，本次运行有效，可切回安全模式或关闭。普通 `pnpm tauri dev` 与 Windows custom-protocol Release 均保留相同运行时门控，不需要研究 feature。自动复制只发一次 Ctrl+Insert，不回落到 Ctrl+C。

下表本轮均为 NOT TESTED。获单独授权后只用专用剪贴板和合成文本，在同一会话记录读取路径（UIA / NativeEdit / Clipboard）、快捷键是否实际发送、选区内容是否正确、原剪贴板是否恢复及焦点是否变化。不能从工具条出现推断自动复制成功。

|目标|状态|
|---|---|
|记事本 / 标准 Edit|NOT TESTED|
|WPS Writer|NOT TESTED|
|Microsoft Word（安装时）|NOT TESTED|
|Chrome / Edge|NOT TESTED|
|VS Code|NOT TESTED|
|PDF 阅读器（安装时）|NOT TESTED|

另验：空/Unicode/多文本格式快照、原剪贴板为 HTML/RTF/图片时发送前拒绝、无复制/超时/恢复失败、第三方并发写入、连续划词及快速切窗。检测到未知来源、未稳定或并发写入时可能保留新剪贴板而不恢复旧内容；恢复失败必须显示失败，不能记为 PASS。同应用隐藏窗口的复制来源仍属于关联证据而非绝对证明，敏感内容请使用安全模式。
