# 贡献指南

欢迎为 Windows AI Float Assistant 提交普通问题反馈、功能建议和 PR。项目已公开，当前版本为 **v0.1.0-beta.1（Pre-release）**；测试版本的兼容性和体验仍有改进空间。

## Bug 反馈

通过 [Issues](https://github.com/bookguifan/windows-ai-float-assistant/issues) 反馈普通使用问题，建议提供：

- 应用版本、Windows 版本与架构、系统缩放比例；相关时提供 WebView2 版本。
- 操作步骤、预期结果、实际结果及是否能稳定复现。
- 如涉及 AI 网页，说明站点和登录状态，不附账户、凭据或真实对话。
- 尽量使用合成文本；日志和截图须先脱敏，只提供定位所需的最少信息。

请先搜索已有 Issue，避免重复。敏感漏洞细节不得放入公开 Issue/PR，私密渠道当前暂缓且未验证，见 [SECURITY.md](SECURITY.md)。

## 功能建议

说明要解决的使用场景、当前操作中的困难、期望行为以及可以接受的替代方案。较大的改动建议先在 Issue 中讨论范围；功能建议不代表维护者已承诺实现。

如提供界面截图，请使用真实运行画面和合成资料，确认可公开；不要把图标或生成画面当作实际功能截图。

## 本地开发与测试

在一套本机 Windows 环境准备 MSVC C++ 工具链、Windows SDK 和 WebView2 Runtime。
使用 Node.js **24.22.1**、pnpm **9.15.9** 和已验证的 Rust **1.98.1**；Tauri 2 沿用锁文件。
不要求 Docker、VM、独立 Dev/Test/Release 环境或新版本管理器。

在 deepseek-float/ 下：

| 命令 | 用途 |
| --- | --- |
| `pnpm install --frozen-lockfile` | 安装已有锁定依赖 |
| `pnpm dev` | 日常前端开发；完整桌面开发用 `pnpm tauri dev` |
| `pnpm test` / `pnpm test:unit` | 同一组前端和构建脚本单元测试，二选一 |
| `pnpm test:js` | Playwright；先 `pnpm build`，首次需 `pnpm exec playwright install chromium` |
| `pnpm test:rs` | 默认 Rust 测试；先构建前端资源，可追加测试过滤器 |
| `pnpm check` | CSP、内联脚本、版本、净化白名单；先 `pnpm build` |
| `pnpm release` | 唯一正式打包入口；普通 Release EXE、许可、ZIP 和哈希 |

日常只运行相关测试，例如 `node tests/settings-workflows.test.mjs` 或
`pnpm test:rs selection`。不要为每次前端修改编译 Rust Release。
PR 后由一个 Windows GitHub CI 完整验证，包括 fmt、Clippy 和独立的 Debug/Release 安全策略。
没有 verify/check-all 等第二套聚合入口。CI 不发布构建包，不要求第三方 Reviewer。

### pnpm 调用旧 Node 时

先检查 `node --version`、`pnpm --version`、`pnpm exec node -p process.version`。
使用已安装的正确 Node 和 Corepack，不修改全局 PATH。若 pnpm 是携带旧 Node 的独立可执行文件，
可只在当前 PowerShell 会话生成 Corepack 入口（把两处路径换成自己已有的安装路径）：

```powershell
$nodeExe = 'C:\path\to\node-24.22.1\node.exe'
$corepackJs = 'C:\path\to\node_modules\corepack\dist\corepack.js'
$pnpmShim = Join-Path $env:TEMP 'float-pnpm-session'
New-Item -ItemType Directory -Force -Path $pnpmShim | Out-Null
& $nodeExe $corepackJs enable pnpm --install-directory $pnpmShim
$env:Path = $pnpmShim + ';' + (Split-Path $nodeExe) + ';' + $env:Path
pnpm --version
pnpm exec node -p process.version
node scripts/build-release.mjs --check-toolchain-only
```

Corepack 读取已有 packageManager 固定 pnpm 9.15.9；发布入口继续验证 pnpm 子进程和入口 Node 一致。
关闭终端即可丢弃 PATH 调整。没有增加持久环境配置文件。

原生验收使用[普通 Release 清单](deepseek-float/docs/beta2-native-acceptance-plan.md)，
一次会话记录 PASS/FAIL/NOT TESTED；不要求重复跑隔离诊断版。
未经授权不启动 EXE、操作真实剪贴板、覆盖正式配置或执行真实备份恢复。
无法保证用户数据安全的操作先跳过并说明原因；自动化通过不代替原生验收。

## PR 提交要求

- 保持改动范围清晰，说明具体问题、最终行为、修改原因及验证结果。
- 大规模重构、依赖升级或功能变化先讨论，避免混入无关格式化和构建产物。
- 保留冻结依赖和锁文件；发布构建继续使用 `custom-protocol`，本地构建说明见 [README](README.md)。
- 不移除发送不确定时禁止自动重发、会话隔离、Markdown 净化及备份恢复保护。
- 公开 Beta 的自动复制回退固定禁用，不要在无单独风险评审的情况下恢复危险路径。
- UI 改动应提供真实、脱敏的效果说明；普通问题可附小型合成复现。

## 数据与许可证

不得提交真实用户配置、收藏、备份、AI 对话、剪贴板、截图、WebView2 用户目录、Cookie、令牌、签名私钥或未经脱敏的日志；EXE、ZIP、依赖缓存和其他本地构建产物也不应混入源码 PR。

提交者应有权提供相应代码或资源。项目原创代码使用 [MIT](LICENSE)，第三方许可、版权、NOTICE 与商标权利不因贡献而被替换或删除。新增依赖或复制素材需说明精确版本、来源和许可；涉及 MPL 对应源码等分发义务时，需保留并补齐相应材料，不能统一改成 MIT。见[第三方说明](THIRD_PARTY_NOTICES.md)。
