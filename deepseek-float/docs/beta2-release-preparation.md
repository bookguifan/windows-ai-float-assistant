# 发布与维护

更新：2026-11-05。[v0.1.0-beta.2 Pre-release 已公开](https://github.com/bookguifan/windows-ai-float-assistant/releases/tag/v0.1.0-beta.2)，Tag 指向 `a67fa0d1c84989e995d023867a98bff304e0e1e9`。应用内部版本仍为 `0.1.0`。已发布 ZIP 和 Tag 不随后续文档清理修改，也不重新打包。

## 一套开发与 CI

工具链及日常命令统一见[贡献指南](../../CONTRIBUTING.md#本地开发与测试)：Windows、Node 24.22.1、pnpm 9.15.9、已验证 Rust 1.98.1、Tauri 2 和本机 WebView2。`pnpm dev` 是前端，`pnpm tauri dev` 才启动桌面程序。

一个 Windows CI 覆盖前端构建/单测/Playwright、聚合检查、Rust fmt/Clippy/测试、普通 Release 构建及独立 profile/feature 安全策略。日常只跑相关测试，PR 由 CI 完整验证；CI 不启动真实桌面程序，不自动公开发行包。

## 唯一正式打包入口

在 `deepseek-float/` 执行 `pnpm release`，唯一实现为 `scripts/build-release.mjs`；已有 `release.ps1` 只作薄包装。

脚本检查工具链、构建前端及版本/CSP 等策略、验证生产 features 为 `custom-protocol`，运行普通 Release 安全策略，最后构建 Rust Release，再核对资源新鲜度、组装许可和对应源码、生成 EXE/ZIP SHA256。输出在 `release/`，不混入 PDB、配置、日志和缓存。许可输入、NOTICE、MPL 对应源码及校验索引必须保留。

完整自动化由 CI 负责，打包不再重复整套测试。`--check-toolchain-only` 只查工具链；`--no-deliver` 保留构建检查而不刷新交付目录；其他参数见 `scripts/build-release.mjs`。构建不启动 EXE、不创建 Tag、不发布 Release。

## 原生验收与发布

普通 Release 按[简要清单](beta2-native-acceptance-plan.md)在一次授权会话中验收。剪贴板专项另见[现行计划](windows-clipboard-native-test-plan.md)；历史隔离工具仅用于可选诊断，不要求所有普通功能重复运行。

必须区分：CI 成功、Dev 实测、某个冻结 Release 实测、维护者合并授权和公开发布授权。不能用旧候选结果替代新候选。启动前检查其他实例、配置备份及正式 WebView2 数据影响；真实剪贴板、设置保存、备份恢复和自启修改需要对应许可。无法保证安全的操作记 BLOCKED/NOT TESTED。

发布门槛：对应 main CI 成功，普通 Release 核心功能/正常退出通过，无已确认重大数据破坏或敏感文件，许可完整，最终 EXE/ZIP 哈希与验收对象一致。一般兼容性问题记[已知限制](beta2-known-issues.md)，Issue #2 可继续开放；稳定复现的严重故障仍阻断发布。只在维护者明确授权后创建 Tag/Pre-release，上传同一冻结 ZIP 和 SHA256SUMS，不因再次取哈希而重编译。

[Beta.2 阶段记录](archive/beta2/README.md)保留在已发布 Tag 和 Git 历史中，不作为继续执行历史全量矩阵的要求。
