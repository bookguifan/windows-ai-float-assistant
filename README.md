# Windows AI Float Assistant

**Windows AI 桌面悬浮助手**：通过悬浮球、划词工具条和 AI 网页面板，把提问、追问与回答收藏放到桌面上。

[![Windows x64](https://img.shields.io/badge/Windows-x64-0078D4)](#系统要求)
[![MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![Beta](https://img.shields.io/badge/Beta-v0.1.0--beta.2-orange)](https://github.com/bookguifan/windows-ai-float-assistant/releases/tag/v0.1.0-beta.2)
[![CI](https://github.com/bookguifan/windows-ai-float-assistant/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/bookguifan/windows-ai-float-assistant/actions/workflows/ci.yml)

> v0.1.0-beta.2 已发布。智能划词增强默认开启，优先 UIA / 原生读取；自动复制回退默认关闭，开启或更换快捷键需要本次运行的原生风险确认。复杂格式剪贴板可能被覆盖且无法恢复；请先阅读[已知限制](deepseek-float/docs/beta2-known-issues.md)。

## 下载 Windows 版

**[下载 v0.1.0-beta.2 · Windows x64 ZIP](https://github.com/bookguifan/windows-ai-float-assistant/releases/download/v0.1.0-beta.2/windows-ai-float-assistant-v0.1.0-beta.2-windows-x64.zip)**

[查看 Release 说明与已知限制](https://github.com/bookguifan/windows-ai-float-assistant/releases/tag/v0.1.0-beta.2) · [反馈普通使用问题](https://github.com/bookguifan/windows-ai-float-assistant/issues)

这是 **Beta 测试版本，不是稳定版**。EXE 未签名，Windows 可能显示来源或 SmartScreen 提示。请先核对下载来源与 SHA256，不建议关闭系统安全防护。

<details>
<summary>查看 SHA256 校验值与检查命令</summary>

Windows ZIP：

```text
B75304160B0D2EA01FF91FF57AA4777359111AD3919FC0AC13B092769ACB1D59
```

包内 `deepseek-float.exe`：

```text
26B046FDAB0C39A210F3AF8285CCDFC8470D16BB967A84D96ACE37EE7F4E7754
```

在下载目录运行 PowerShell：

```powershell
Get-FileHash .\windows-ai-float-assistant-v0.1.0-beta.2-windows-x64.zip -Algorithm SHA256
```

ZIP 内也提供文件校验清单。普通 SHA256 用于核对文件一致性，不能代替代码签名或发布者身份认证。

</details>

## 能做什么

| 功能 | 使用方式 |
| --- | --- |
| 桌面悬浮球 | 拖动调整位置，左键打开面板，右键进入功能菜单 |
| UI Automation 划词 | 在支持的程序中读取选区，通过工具条复制或结合提示词提问 |
| 截图提问 | 框选屏幕区域，尝试将图片贴入当前 AI 网页面板；需站点支持图片上传，不自动发送问题 |
| AI 网页面板 | 使用所选第三方 AI 网站，在菜单中切换站点 |
| 多轮追问 | 在结果卡继续提问，回答回流依赖当前站点的适配情况 |
| 结果卡 | 新划词定位到当前选区附近；流式更新和追问保持拖动位置 |
| 收藏与 Markdown 导出 | 保存回答、搜索收藏、导出 Markdown，并打开导出目录 |
| 配置及收藏备份恢复 | 导出备份，导入时预览并选择合并或覆盖恢复 |

**AI 服务来自你选择的第三方网站**，本项目不提供模型服务，也不承诺所有网站、软件或 Windows 环境兼容。截图上传、消息发送和回答回读可能受网站版本、登录状态及网络影响。

开发、打包和验收入口见[发布与维护](deepseek-float/docs/beta2-release-preparation.md)。

## 快速上手

1. 下载 ZIP，完整解压到本地目录，保留随包的许可证和对应源码材料。
2. 运行 `deepseek-float.exe`。若提示缺少运行组件，按[系统要求](#系统要求)安装相应依赖。
3. 左键点击悬浮球打开 AI 面板，选择站点并按网站要求登录。
4. 右键悬浮球，打开“提示词与设置”。“智能划词增强”默认开启，在支持的程序中选中文本后通过工具条操作。UIA / 原生读取不可用时，可主动授权“自动复制回退”，根据目标选择 Ctrl+Insert（默认）或 Ctrl+C。
5. 需要图片提问时，从菜单选择“截图提问”并框选，检查面板附件后再提问；也可从“收藏夹”查找或导出已保存的回答。
6. 备份入口在设置页的“数据备份与恢复”；退出程序使用右键菜单中的“退出助手”。

工具条中的**主动复制**是明确操作。自动复制回退默认关闭，授权仅本次运行有效，重启后须重新确认；旧配置和备份不能代替授权。纯文本尽力恢复，HTML/RTF/图片/文件等复杂内容不备份、不恢复，可能被覆盖且无法找回。

## 系统要求

- **Windows x64**：当前对应的有限原生验收在 Windows 10 19045 x64 上完成；其他 Windows 版本、DPI、多显示器与受限账户环境未完整验证。
- **[Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)**：若系统已有可用版本，可直接使用；缺失时按微软官方说明安装。
- **[Microsoft VC++ x64 Runtime](https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist)** 及兼容的 UCRT；本 ZIP 不捆绑运行库安装器。
- 使用第三方 AI 网站需要网络，部分站点需要自行登录。

## 已知限制

- 自动划词依赖目标控件和复制快捷键支持；已验证本机记事本 NativeEdit、WPS Writer Ctrl+Insert 和 WPS PDF Ctrl+C，不能据此保证所有软件兼容。
- 第三方 AI 网站可能调整页面或交互，影响输入、图片上传及回答回流。
- 本次冻结 Release 已完成核心功能、DeepSeek 合成问答、截图/OCR 和退出冒烟；其他 AI 站点、浏览器/编辑器兼容矩阵及多屏不同 DPI 未完整验证。
- 本次发行从 Tag 对应 main 构建，公开下载与实测文件哈希一致；不承诺不同工具链或环境的逐字节可重复构建。
- EXE 未签名；私密漏洞报告渠道尚未建立或完成验证，见[安全政策](SECURITY.md)。

## 本地开发与构建

开发使用 Windows、MSVC C++ 构建工具与 Windows SDK。本机使用 Node.js `24.22.1`（既有 `^24.15.0` 校验保留），pnpm 固定为 `9.15.9`；已有本地构建使用 Rust `1.98.1`，更低版本的完整依赖兼容性尚未单独验证。

```powershell
git clone https://github.com/bookguifan/windows-ai-float-assistant.git
cd windows-ai-float-assistant\deepseek-float
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm dev` 只启动前端；运行完整桌面应用使用 `pnpm tauri dev`。测试另一实例前先退出当前程序，避免单实例互斥唤醒旧实例。

本地绿色版构建：

```powershell
pnpm release
```

唯一构建逻辑是 `scripts/build-release.mjs`；`release.ps1` 只做兼容包装。脚本验证工具链、构建前端并运行 `pnpm check`、验证安全策略，最后构建普通 `custom-protocol` Rust Release。默认在 `deepseek-float/release/` 组装 EXE、必要许可及对应源码材料、一个 Windows x64 ZIP 和 EXE/ZIP SHA256；不启动程序，不创建 Tag 或 GitHub Release。完整测试由一个 Windows CI 执行，打包不重复整套 CI。保留构建和检查、但不刷新交付目录时使用：

```powershell
node scripts/build-release.mjs --no-deliver
```

不要省略生产构建所需的 `custom-protocol`。自行分发构建产物时，仍需附带适用的第三方许可、通知及对应源码。测试命令与 PR 要求见[贡献指南](CONTRIBUTING.md)。

## 技术架构

| 层 | 技术与职责 |
| --- | --- |
| 桌面应用 | Tauri v2、Rust：窗口、工作流、配置、收藏与备份 |
| Windows 集成 | Win32、UI Automation：悬浮窗口、选区读取与截图 |
| 界面 | 原生 HTML/CSS/JavaScript，Vite 构建，WebView2 渲染 |
| AI 服务 | 第三方 AI 网页：提供模型回答，本项目负责面板与适配 |

开发细节见[前后端契约](deepseek-float/docs/frontend-rust-contract.md)和[备份恢复说明](deepseek-float/docs/backup-restore-qa.md)。

## 隐私与安全

请在发送前确认选中文本、截图和问题可以交给当前 AI 网站处理。网站的登录、数据处理和隐私政策由对应服务提供者决定。

配置、收藏、备份及 Markdown 导出可能含有个人内容。备份不包含网站登录态，但用户填写的内容仍可能敏感；请妥善保管，不要提交真实配置、WebView2 用户目录、剪贴板、对话或原始日志。

安全渠道当前**暂缓、未验证**，不宣称 GitHub Private Vulnerability Reporting 已启用。请勿在公开 Issue/PR 披露敏感漏洞细节，详见 [SECURITY.md](SECURITY.md)。

## 贡献与反馈

欢迎提交[普通 Bug 或功能建议](https://github.com/bookguifan/windows-ai-float-assistant/issues)，以及小范围、可验证的改进。提交 PR 前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 开源许可

有权授权的项目原创代码采用 [MIT License](LICENSE)。第三方依赖、素材及商标继续适用各自条款，见[第三方说明](THIRD_PARTY_NOTICES.md)及 [Windows 许可材料](deepseek-float/distribution/windows/README.md)。
