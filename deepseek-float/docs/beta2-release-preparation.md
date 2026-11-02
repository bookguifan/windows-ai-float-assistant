# Beta.2 发布准备

更新：2026-11-02。计划发布 `v0.1.0-beta.2`；应用内四处版本继续为 `0.1.0`。
本文是当前流程，历史阶段报告保留在 [archive/beta2/](archive/beta2/)。

## 当前代码

main 为 `2ecaba3`（PR #5 已合并）。[PR #6](https://github.com/bookguifan/windows-ai-float-assistant/pull/6)
仍是隔离球页诊断草稿，head `d72198c`，该 head 的 CI 已通过。本次流程精简从 main 独立分支开始，
不重复实现 PR #5 的统一构建入口、Node/pnpm 配对校验和文档归档，也不隐式合并 PR #6。

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
4. 执行 Release 实验特性无法解锁复制的策略回归，然后构建普通 Release。
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

- PR #7 再 PR #6 完成人工合并，合并后的 main GitHub CI 通过。
- 新普通 Release 的主要功能及正常退出人工验收；本次没有启动权限，均不能宣称通过。
- brotli 文件级通知及原生组件范围已在现有第三方说明中澄清；最终 ZIP 仍按校验表检查，不要求企业级二进制审计。
- 若发现可稳定复现的严重故障或严重用户数据丢失，修复后再发布。

[Issue #2](https://github.com/bookguifan/windows-ai-float-assistant/issues/2) 可作为已知问题开放，
一次成功启动不说明根治；它的开放状态本身不是额外发布门槛。
历史所有原生矩阵、PDB 身份报告、真实备份恢复或特定 VM 验收不再是统一强制门槛。
升级时退出旧实例并保留用户配置、收藏和 WebView2 数据，不清空 AppData。
