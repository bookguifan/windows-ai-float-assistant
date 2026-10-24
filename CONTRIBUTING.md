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

在 Windows 上准备 MSVC C++ 工具链、Windows SDK 和 WebView2 Runtime。Node.js 需满足 `^24.15.0`，pnpm 使用 `9.15.9`；已有本地构建使用 Rust `1.98.1`，不把 Cargo 元数据中的版本字段当作完整依赖图已经验证的最低版本。

```powershell
git clone https://github.com/bookguifan/windows-ai-float-assistant.git
cd windows-ai-float-assistant\deepseek-float
pnpm install --frozen-lockfile
pnpm tauri dev
```

前端资源需要先构建，随后再运行依赖内嵌资源的 Rust 检查：

```powershell
pnpm build
pnpm check
pnpm test:unit
pnpm exec playwright install chromium
pnpm test:js
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo check --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol
git diff --check
```

可根据改动先运行对应专项测试。记录实际命令、结果和未执行原因；不要把未运行测试写成通过，也不要降低断言或校验以绕过失败。自动化测试不能替代原生窗口、全新 Windows 或真实第三方站点验收。

使用独立测试数据与合成内容。涉及系统剪贴板、截图或账户的网站测试前，须明确测试范围和数据使用条件，不在日常资料上盲目运行。

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
