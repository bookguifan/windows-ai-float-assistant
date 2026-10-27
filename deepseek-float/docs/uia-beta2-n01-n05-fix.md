# Beta.2 N01 / N05 定向修复与待原生复测记录

日期：2026-10-27。分支 feature/uia-safe-clipboard-fallback；HEAD a4dee7d55a68f2439d75ab2a2057bbff3bf60f2d，构建包含未提交修改，不能视为该提交的干净构建。不提交、不推送、不启动 GUI、不操作系统剪贴板。

## 旧失败与根因证据

旧 EXE SHA256：3B14E14F654590F6D7AF953EB14AB945872932F597729E545ADBCE78EA3ECE1E。旧原生报告及其日志保持不变：N01 FAIL、N05 FAIL、N07 夹具缺少内置 explain 而 BLOCKED；不以本轮自动化结果改写它们。

旧 N05 日志 request 672：UIA_SUCCESS → SESSION_READY → 约 0.8 秒后 popup action 待发文本已丢失，无 USER_COPY_SUCCESS。旧 N01 多次 NativeEdit SESSION_READY，人工报告设置页预览间歇为空，存在时完整。旧日志没有逐条 HWND/状态版本/移交时间，不能补造完整原生事件重放。

源码确认：mousehook 的 foreground_changed 无条件 invalidate；popup 原先用严格 Session::usable(target(), generation, now) 取待发文本，target() 在应用自己前台时返回 None。工具条点击引起自身焦点变化可在 IPC 前撤销 Session。纯策略回归可重现此拒绝；旧日志不足以证明每次失败的精确焦点次序。

设置页还有独立的确定性问题：settings::show 仅 eval 一次条件调用 __dsfSettingsInit，页面尚未安装该函数时静默丢弃；前端 trim 原始文本；同版本迟到初始化可在撤销后恢复旧预览。前端回归重现上述三类问题。原生 Edit 和 UIA 原本都已创建统一 Session，未另造读取通道。

## 最小修复边界

- selection.rs / popup.rs：统一 ToolbarSelection，只有原生鼠标钩子确认实际 popup HWND 命中后才能为当前票据生成一次性动作资格；核对来源 HWND/PID/thread 存活、代次、原有 8 秒 Session 寿命、可见性及当前前台。动作资格 1 秒失效，不延长 Session。
- mousehook.rs：仅保留该显式交互导致的 popup/原来源焦点变化；其他窗口、旧请求仍撤销。普通 Session::usable 的前台要求不变。密码控件的上游捕获拒绝不变；身份存活判断本身不是密码证明。
- popup.html / ipc.rs / lifecycle.rs：渲染票据随动作提交；后端检查调用窗口标签、应用来源、页面路径和可见性。外部 AI 页面或无原生点击的后台 IPC 不可取得动作资格。
- settings.rs / selection.rs：显式动作转成一次性、目标设置窗口绑定的移交，不再错误要求来源窗口在设置打开时仍前台；原 60 秒设置委托时限、失焦/关闭清理保留。
- settings.html / settings_ready：处理器安装后请求当前快照；保留原始 Unicode、换行和缩进；修订号及撤销标记拒绝迟到回复。普通打开设置仍清空选区。
- popup.rs / windows.rs：旧票据拒绝不再隐藏新工具条；超时及 dismiss 在主线程实际执行时重新检查代次和票据。新增诊断只含会话、代次、HWND、时间和接受状态，不记录选区正文。

本轮源码文件：poc/popup.html、poc/settings.html、src-tauri/src/selection.rs、popup.rs、settings.rs、mousehook.rs、ipc.rs、lifecycle.rs、windows.rs；测试：src-tauri/tests/selection_core.rs、tests/safe-copy-ui.test.mjs；本报告。此前已有 16 项工作区修改保留，发行门禁及 native_selection 等既有修改未重新实施。

## 红绿验证

Rust RED：从旧严格消费/过期判据机械抽取测试接缝后，24 PASS / 2 FAIL；不是 Windows 原生事件重放。前端 RED：13 PASS / 4 FAIL。首轮 GREEN：26 / 17 PASS。补充票据、来源隔离和旧回复回归后最终结果如下。

|实际命令|结果|
|---|---|
|cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib --test selection_core --test copy_engine --test native_copy_proof --test uia_strategy --test release_selection_policy --test native_selection_policy|377 PASS、12 ignored、0 FAIL；selection_core 29 PASS；ignored 不算原生通过|
|pnpm test:unit|382 PASS、0 FAIL（71 + 161 自定义断言及 150 个 node:test 用例）|
|cargo fmt --manifest-path src-tauri/Cargo.toml --check|PASS|
|cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings|PASS|
|cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --features experimental-clipboard-fallback --test release_selection_policy|4 PASS；Debug 显式研究特性允许受限开发路径，不等同发行构建|
|cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol,experimental-clipboard-fallback --test release_selection_policy|4 PASS；即使误启研究特性，Release 仍拒绝自动复制|
|cargo check --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol|PASS|
|pnpm build / pnpm check|PASS / PASS|
|cargo build --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol --bin deepseek-float|PASS；未启动|

本轮未运行 Playwright、真实 GUI、系统剪贴板测试、截图/OCR、六站回归。自动化不能代替 N01/N05 原生复测。发行危险 Ctrl+C 路径仍隔离；R01–R03 历史风险/结论不变。

## 新 EXE 与复测门禁

新 EXE SHA256：258D2D91583DF20B8FC805D7DE38E6DF18C6489B322A274D61FAC1924038A3D2。

独立证据目录由本轮交付给出，包含 SOURCE-FILES.csv、SOURCE-SNAPSHOT.zip、BUILD-RECORD.json、原工作区差异及全部红绿日志。使用公开仓库现有构建缓存进行链接，再复制到独立 bin，未覆盖已发布程序、历史 release EXE/HASH 或旧冻结测试 EXE。构建是当前工作区快照，不是干净 HEAD；工具链和源码清单总摘要见 BUILD-RECORD.json。

待单独授权：使用独立配置、合成 Unicode Edit / WPF 夹具；实验性回退持续禁用。N01：冷设置窗口和已加载设置窗口分别从工具条更多进入，核对中文、Emoji、换行和两个前导空格；移交后切窗/关闭清空，普通菜单设置不继承旧选区。N05：在 WPF 及原生 Edit 选择完整合成文本，点复制一次，在合成检查框逐字符核对；重复点击/超过 8 秒/新选区替换/切换外部窗口不得消费旧选区。先核对新 EXE 摘要；只有用户主动复制项可改剪贴板，不读取未知原内容。

## 残余风险与状态

- 原生鼠标按下、WinEvent、WebView IPC 的实际先后仍需新 EXE 验收；旧日志缺少的信息不构成已验证证据。
- 原生命中证明不等于认证物理用户；同桌面输入注入的威胁边界未升级。本轮没有增加自动 SendInput。
- 1 秒动作投递预算可能保守拒绝卡顿或长按；8 秒 Session 不变。键盘激活路径未原生验证，不凭策略测试宣称完整可访问性。
- 后台策略测试证明非法窗口/过期/旧代次拒绝，不证明任意 Windows 提供者安全；密码拒绝依赖既有上游检查，仍需原生负向回归。
- N03 完整时序、N04 精确内容、N07 有效备份夹具验证仍待补；N01/N05 新版 NOT_TESTED，Beta.2 保持阻断，未合并、未发布。

## 2026-10-27 原生复测与开发合并决定（后续补记）

前文保留为当时的构建/验证记录。最新原生复测：N01 预览与撤销子项正常但入口仍需重试；N05 有主动复制成功日志但精确粘贴 NOT MATCHED，60/33 UTF-16 单位；WM_COPY 来源未确认。完整问题及 N03/N04/N07、R01–R03 未关闭状态见 [Known Issues](beta2-known-issues.md)。

维护者已允许在新 CI 通过、发行安全门禁不退化、问题完整披露时合并开发代码；不再把全部兼容性缺陷关闭作为开发合并前提。Beta.2 正式发布仍 NO-GO，内部 Beta 未整体放行。本轮只提交/推送功能分支和更新 Draft PR，不合并 main，不创建 Tag/Release。
