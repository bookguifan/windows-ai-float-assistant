# Changelog

## [Unreleased]

## [v0.1.0-beta.2] - 2026-11-05

- 智能划词增强默认开启，优先使用 UIA / 原生读取；改进记事本及已验证 WPS 场景。
- 可选自动复制回退默认关闭，仅本次运行原生授权后启用；可选 Ctrl+Insert / Ctrl+C，每次只发送所选一种。
- 受支持文本尽力快照和恢复；复杂格式经明确风险授权后可复制，但不备份、不恢复，可能被覆盖。保留目标、来源及并发写入保护。
- 新结果卡跟随本次划词位置，流式更新和追问保持拖动后的位置，并保留边缘避让。
- 修复球窗 Ready/显隐、工具条及设置交互、DeepSeek 附件检测和自动 OCR；截图提问与 OCR 可分别设置快捷键。
- 加强主动复制失败后的尽力恢复和备份输入校验；恢复不承诺对任意 Windows 剪贴板格式原子无损。
- 沿用一个 Windows CI、一套绿色版构建入口及必要许可证/对应源码。

[发行说明和附件](https://github.com/bookguifan/windows-ai-float-assistant/releases/tag/v0.1.0-beta.2) · [已知限制](deepseek-float/docs/beta2-known-issues.md)。

此前的逐轮实验、修复与未发布候选记录保留在[发布 Tag 中的原始 Changelog](https://github.com/bookguifan/windows-ai-float-assistant/blob/v0.1.0-beta.2/CHANGELOG.md)和[阶段记录索引](deepseek-float/docs/archive/beta2/README.md)，不再与当前发布状态混写。历史 FAIL/UNKNOWN 不改判，Issue #2 继续开放。

## [0.1.0] - 2026-09-14

首个可运行版本：Windows 桌面 DeepSeek 悬浮助手骨架。

### Added

- **悬浮球**：64×64 透明无边框置顶窗口，自由拖动、左键开面板、右键菜单。
- **DeepSeek 面板**：内嵌 `chat.deepseek.com`，路线 A 持久化登录（cookie 存于 WebView2 data dir，跨重启保留）；可缩放、可拖动、关闭即隐藏不销毁。
- **划词工具条**：全局鼠标钩子（`WH_MOUSE_LL`）检测拖动选中自动浮现；含 复制 / 解释 / 翻译 / 总结 / 问问DeepSeek / 自定义 / ⋯；动态宽度、贴光标、屏幕边缘翻边避让、点击外部自动隐藏。
- **统一提示词模型**：内置 4 项（可编辑 / 隐藏 / 恢复默认，不可删）+ 自定义项（可增删改隐藏，上限 20），持久化 `prompts.json`。
- **自动注入并发送**：三级注入面板输入框（原生 setter + input 事件 → `execCommand` → Ctrl+V 兜底）+ 读回校验 + 自动点发送（不自动回车）。
- **设置窗口**：划词预览、提示词管理、产品功能三开关。
- **系统托盘**：显示/隐藏球·面板、重新登录、退出；托盘图标与悬浮球一致的 DeepSeek 鲸鱼。
- **位置记忆**：启动恢复悬浮球上次位置并夹取回可见区；拖动去抖落盘；退出前 flush 最终位置。
- **开机自启**：写 HKCU Run 项；启动时与注册表真实状态对账（尊重用户在任务管理器里的禁用）。
- **全屏隐藏**：前台全屏时自动隐藏悬浮球，退出全屏恢复（仅恢复原本可见的球）。
- **提权划词**：可选优先使用 UI Automation TextPattern 直读选区，失败回退剪贴板法。
- **release 优化**：`[profile.release]` 开启 `opt-level="s" + lto + codegen-units=1 + strip`。

### Changed

- 前端由 React 工程收敛为原生 `poc/*.html` 三页（ball / popup / settings）。
- `withGlobalTauri` 收窄为 `false`，本地页面改用 `__TAURI_INTERNALS__` 构造最小 IPC 桥。
- 字符串事件（`toggle-panel` 等）改为类型化 `#[tauri::command]` + `invoke_handler`。
- 提示词增加内存缓存，消除高频读盘。
- `lib.rs::setup` 拆分辅助函数，降低单函数复杂度。

### Fixed

- **划词崩溃** `STATUS_CONTROL_C_EXIT (0xc000013a)`：合成 Ctrl+C 被前台终端转成 `CTRL_C_EVENT` → 进程忽略 Ctrl+C（`SetConsoleCtrlHandler`）。
- **NoNewCopy**：合成 Ctrl+C 前等待 Ctrl/Alt 松开，避免投递成 Ctrl+Alt+C。
- **点击悬浮球无反应**：弃用 `data-tauri-drag-region`（mousedown 阶段吞 click），改脚本手动 `startDragging` + 位移阈值判定。
- **面板创建失败 `0x8007139F`**：同一 WebView2 user-data 目录下多个 webview 的 browser args 必须一致，移除面板的 `--disable-gpu`。
- **windows 0.61 编译**：UI Automation 特性改为 `Win32_UI_Accessibility`；显示器 API 归 `Win32::Graphics::Gdi`；注册表函数返回裸 `WIN32_ERROR`；`Mutex<Instant>` 非 const 初始化改 `OnceLock`。
- **配置竞态**：`config::update` 改为单锁内原子读-改-写，避免位置保存与设置写入互相覆盖。
- **工具条偏宽**：恢复 `button padding` / `.bar gap|height`，使 Rust 宽度估算与前端一致。
- **开机自启失败回滚**：`config_set_autostart` 仅在注册表写入成功时落盘，设置页失败时回滚开关。

### Removed

- 全局热键（`Ctrl+Alt+D`）—— 改为纯鼠标划词触发。
- 遗留未使用的 React 前端与 `vitest` / `tsc` 工具链。

---

> 早期 PoC 阶段（PoC-1..5：透明 / 内嵌 / IME / 形变 / 毛玻璃）的逐次验证记录见 `deepseek-float/docs/` 与仓库根 `docs/`。
