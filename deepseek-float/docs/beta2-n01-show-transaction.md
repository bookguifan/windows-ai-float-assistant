# Beta.2 N01 显示事务修复与冻结记录（2026-10-27）

## 基线与结论

公开仓库 `bookguifan/windows-ai-float-assistant`，分支 `fix/beta2-native-acceptance`，HEAD `f9c364071210afac131989c9080db335a8106636`。开始时工作区、暂存区为空；通过本地代理只读复核，远端功能分支仍为该 SHA，main 仍为 `922479b1a30c070b572dfbd252cccaa543837449`。本次为未提交的 dirty 源码构建，不是仅从 HEAD 构建的产物。

已修复并自动化验证：未发生原生点击时，经过严格绑定的工具条显示事务可暂时保留选区；旧显示任务不能归还新选区的焦点。**N01 原生结果尚未复测，历史 FAIL 保留；Beta.2 发布仍 NO-GO。** 本轮未启动 EXE、原生 GUI 或系统剪贴板测试，未提交、推送、合并或发布。原私有仓库的原有修改未被操作。

## 可确认的原因与证据边界

既有原生记录中，request 330 offer 后约 57ms 被撤销，generation 330→331；后续 show、点击时已无票据。request 338 也有 offer 后约 49ms 撤销的样本。WPF request 40 的 UIA/Session 均为精确的 54 UTF-16 单位，但 offer→撤销→show→no-ticket，读取成功不等于入口成功。原始证据及成功样本均保留在[前轮报告](beta2-postmerge-convergence.md)。

源码路径为 `accept_session_on_main → offer → show_at_cursor`；旧 offer 将 interacting 重置为 false，旧 `preserves_foreground` 要求窗口已可见且已发生原生点击。show 期间一旦前台转为工具条，WinEvent 可 invalidate generation；25ms supervisor 的 `expire_session → retains` 也会拒绝尚未交互的工具条前台。旧实现的真实回归测试在此状态下得到 `ticket=None`（应为 `Some(1)`），退出 101。这证明一个可触发路径，**不能反推每个历史失败的具体 HWND 或唯一原因**。

正常路径：offer → 开始本次 show 事务 → 原目标/已验证工具条前台 → 原生点击 arm（终止显示事务）→ 单次 action → 消费或显式设置交接。

失败路径：offer → 外部前台/身份失效/代次失效 → invalidate/expire → 清票据 → 点击拒绝。此负向路径继续保留。

## 最小修改与状态约束

- `selection.rs`：`PopupShow` 绑定 ticket、request generation、原目标 HWND/PID/thread、popup HWND/PID/thread、show serial。`begin_show/current_show/finish_show/can_return_focus` 控制显示资格，不能凭显示事务消费动作；仍须原生 pointer-down 和既有 1s 动作检查。Session 的 8s 期限没有延长。
- `popup.rs`：在 `win.show()` 之前登记事务；原目标须仍存活且仍为前台，popup 须为本进程的真实窗口。只在本次 lease 内保留该工具条，其他本程序窗口不获授权。显示失败、隐藏、替换、过期、消费均撤销资格。
- 原有立即/150ms/300ms 焦点归还改为绑定 `PopupShow`，延迟工作投递到主线程重新核验 serial、ticket、generation、完整窗口身份、源存活和当前前台。只有当前前台精确为该 popup 且尚未原生点击时才尝试归还。真实点击立刻终止后续归还；旧任务和旧收尾不操作新事务。
- 350ms 只是本次显示 lease 的上限，**不是对任意 HWND 的时间豁免**。约 300ms 本次收尾正常结束 lease；排队迟到亦不能突破身份、代次或期限限制。
- `SHOW_GEN` 在显示开始时预留，自动关闭复用此 serial，保留 ticket 守卫，避免旧定时器影响新显示。
- `mousehook.rs/selection_runtime.rs`：前台回调只用轻量 native 查询与 try_lock；拒绝时 CAS 撤销回调进入时观察到的 generation，避免一条旧处理无条件递增后来代次。无日志格式化、磁盘 I/O、哈希或 COM 工作进入 WinEvent 回调。
- `native_selection.rs/settings.rs`：仅调整诊断作用域与关联，不改文本、设置移交、备份或主动复制语义。

### 保守边界与焦点风险

前台事件对应 HWND 与处理时实际前台不一致时，继续拒绝，不按短时间自动放行。不能证明来自本次 show 的迟到事件，或 callback try_lock 失败，也会保守撤销；因此部分首次点击兼容性问题仍可能存在。尚未用新 EXE 验证，不能宣称所有 N01 时序已修复。

`GetForegroundWindow` 检查和 `SetForegroundWindow` 不是跨进程原子事务；已增加紧邻 API 的 generation/前台复检，但仍需原生测试外部切换、快速点击与延迟归还的临界点，不能承诺 Windows 不再发生任何瞬时焦点变化。归还失败不授权复制或旧票据复活。若无法证明安全归属，保持撤销。

Microsoft 文档说明 [OUTOFCONTEXT 事件采用队列异步投递且回调可能重入](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook)，[dwmsEventTime 是事件生成时刻](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nc-winuser-wineventproc)。因此日志落盘顺序不能直接当成实际前台变化顺序。

## RED / GREEN 与覆盖

RED：修改前实际运行 `cargo test --manifest-path deepseek-float/src-tauri/Cargo.toml --test selection_core show_time_popup_focus_must_not_expire_before_first_native_click -- --exact`，0 PASS、1 FAIL，旧 `expire` 清除了 ticket。GREEN 保留同一断言，在安排阶段加入生产调用所需的 `begin_show`，没有修改期望；最终 selection_core 为 40 PASS。不是以编译失败冒充 RED。

|要求场景|自动化覆盖（纯状态机，不是原生 GUI PASS）|
|---|---|
|正常显示、首次点击、工具条自身焦点|show_time_popup_focus_must_not_expire_before_first_native_click|
|WebView 导致已验证 popup 前台|show_survives_only_popup_or_source_not_unrelated_own_windows 的 popup 分支；不模拟真实 WebView2 进程|
|外部主动切换、无关本程序窗口|同上外部身份分支，仍清票据|
|旧 ticket 被替换、旧显示迟到、旧计时器|old_show_and_timer_cannot_change_replacement_or_restore_focus；保留原生产双守卫|
|同一 ticket 的新显示替换旧显示|same_ticket_new_show_rejects_old_return_and_old_completion|
|源关闭、身份变化、generation 变化|hide_failure_source_death_generation_change_and_expiry_end_show；runtime 原生完整身份检查|
|隐藏、show 失败、过期|上述 clear/clear_if/expire 分支及生产失败清理；未人为制造原生 win.show 失败|
|重复点击/消费、设置交接|click_and_settings_handoff_end_show_and_do_not_allow_repeat_consumption 与既有 settings 测试|
|无效目标与过期 show 启动|malformed_popup_and_stale_source_cannot_start_show|
|结束的事务不可复用|finished_show_cannot_be_reused_and_unarmed_popup_expires|

首次 Clippy 因新用 `is_none_or` 超出项目 MSRV 1.77.2 而失败，已换成等价兼容写法，随后严格 Clippy 通过。初次失败日志保留；未修改 MSRV 或放宽 lint。

## 诊断隐私与时序

诊断默认关闭，必须同时满足正常诊断开启及 `DSFLOAT_SYNTHETIC_SELECTION_TRACE=synthetic-only`。文本摘要和范围日志还须目标身份有效、窗口标题精确在固定合成夹具白名单内、文本为固定合成内容或其选区。普通窗口即使文本与合成内容相同，也不登记关联作用域；不记录其正文或指纹。正常发行环境不产生选区指纹。

原先无条件关联的 offer/action/settings-show/settings-ready 元数据已移入受合成 session/ticket 作用域约束的诊断。settings 保留源 session 与独立设置 revision/ticket 的关联；诊断判断不参与动作授权或消费。

前台观察记录事件 HWND、事件线程、OS 生成 tick、回调进入时 generation、判断后 generation、回调捕获时间、诊断线程写入时间、ticket、show serial、source_alive 和 preserve 结果。**回调进入 generation 不是 OS 生成事件时的历史 generation**，后者现有接口不能还原；不能混称。OS tick 与进程相对时间属于不同时间域，不能直接相减。

迟到事件即使因 HWND 不匹配而被拒绝，仍在当前有效合成作用域内保留其 ticket/show/source_alive 观察；try_lock 失败时不强行阻塞取证。点击和前台观察分别使用容量 32 的非阻塞 try_send，满队列丢诊断，返回值不用于业务分支；未进行原生诊断压力测试。涉及不同队列、不同线程与可能重入，落盘顺序不等于事件顺序。新增标题白名单测试与既有关闭状态/非夹具内容/UTF-16LE 摘要测试通过。固定标题和内容是本地合成诊断约束，不是第三方窗口不可伪造的来源证明。诊断有开销，可能降低临界超时场景的成功率，不宣称开关前后时序完全一致；授权与票据安全判定使用同一实现。

## 本次实际自动化结果

在应用目录使用 pnpm 9.15.9；Node v24.14.0 低于 package.json 的 ^24.15.0，保留 engine 警告。Windows 主机、rustc/cargo 1.98.1、x86_64-pc-windows-msvc、锁定 tauri 2.11.5。无依赖升级。

|命令（仓库根运行，pnpm 在应用目录）|实际结果|
|---|---|
|cargo test --offline --locked --manifest-path deepseek-float/src-tauri/Cargo.toml|397 PASS、12 ignored，0 FAIL；最终复跑退出 0|
|cargo test 同上，--features experimental-clipboard-fallback --test release_selection_policy|4 PASS（Debug 研究组合）|
|cargo test 同上，--release --features custom-protocol,experimental-clipboard-fallback --test release_selection_policy|4 PASS，Release 不能被研究特性解锁|
|cargo fmt --manifest-path deepseek-float/src-tauri/Cargo.toml --check|退出 0|
|cargo clippy --offline --locked --manifest-path deepseek-float/src-tauri/Cargo.toml --all-targets -- -D warnings|最终退出 0|
|cargo check --offline --locked --manifest-path deepseek-float/src-tauri/Cargo.toml --release --features custom-protocol|退出 0|
|pnpm build / pnpm check|均退出 0|
|pnpm test:unit|384 PASS、0 FAIL|
|pnpm test:js|本地 Chromium Playwright 22 PASS|
|cargo build --offline --locked --manifest-path deepseek-float/src-tauri/Cargo.toml --release --features custom-protocol --bin deepseek-float|链接完成，退出 0；未启动 EXE|
|git diff --check / git diff --cached --check|退出 0；暂存区为空|

397 是各 Rust 执行目标之和，存在模块复用，并非 397 个独立原生场景；12 ignored 未执行，不能计 PASS。新测试属于 CI 已有 selection_core/--lib 执行范围，workflow 未修改，未触发或宣称完成远端新 SHA CI。safe_copy.rs、capture.rs、release_policy.rs、两份锁文件和发行准入规则无修改；UIA 与用户主动复制保留，R01–R03 历史风险未改判。

## 冻结构建

本地证据目录：`%TEMP%/beta2-n01-show-transaction-20261027-final/`（不加入 Git）。

- 构建输入：752 文件的完整 SHA256 清单 `SOURCE-BUILD-INPUT-SHA256.json`；清单本身在仓库外，避免自引用。
- 清单 SHA256：`1640C7D87323712E144D13BBBB0BC1391C4C417F767B81A427FC6B3D24C69371`。
- 构建完成后逐文件复核这 752 个输入，未发生变化。最终构建包含当时的报告和 Known Issues；本次冻结后仅更新本报告的构建结果文字，编译源码未变化。
- 最终完整工作树另存 `SOURCE-FINAL-SHA256.json`，标明构建后仅报告类差异，避免把最终文档与构建输入混为一谈。
- 构建时间：2026-10-27 20:10:16 至 20:13:27（北京时间）；实际命令与工具链在 `BUILD-INPUT.json`、`BUILD-RECORD.json` 和 release-link.log。
- 特性：`custom-protocol`；未启用 experimental-clipboard-fallback。无签名、无打包或上传发布附件。
- 独立 EXE：证据目录下 `bin/deepseek-float.exe`。
- EXE SHA256：`2D09F1B7F03428A48171EB7A0FC245E9D8E438B19B663418882A399DC7DEEAC4`。
- 未覆盖任何历史 EXE/HASH、Beta.1 附件；没有启动或原生验收此 EXE，旧 PASS 不能套用。

前一轮未启动的候选 EXE（SHA256 `93B8D5B7EE16B8C77391026EF51C506A35382B9BE5ED2E9866D36B09F3C0A2CB`）完整保留；补齐拒绝事件关联后已重新执行 Rust/策略/Clippy/生产检查并重建本最终候选，不将前一候选混用。前端内容未变化，前端与 Playwright 使用本轮实际成功执行的日志。

一次冻结/构建审批因审批服务用量限制未执行；用户要求继续后，经相同审批流程重试才成功，没有绕过审批。

## 剩余事项与下一轮最小原生验证（需单独授权）

1. N01：冷/热设置页各一次首次 ⋯，独立 ticket，核对菜单、设置、精确预览；记录前台事件与显示 lease。再检查外部 Alt+Tab、真实点击期间延迟归还、旧 ticket/旧定时器及 source 关闭。失败停下保留现场，不反复划词掩盖首次失败。
2. N05：固定 Unicode Edit 完整 0..60 与明确子区间，先记真实 EM_GETSEL，再按 native-read→Session→ticket→write→paste 逐 UTF-16 核对；另行授权主动复制。历史 60→33 不匹配未闭环，已有 58 单位局部 PASS 保留。
3. N03：旧票据、超时、替换、重复消费的完整原生矩阵仍 PARTIAL；纯模型不能替代。
4. N04：此前 WPF 54 单位读取局部 PASS，设置入口历史失败；多行/中文/Emoji/空格矩阵和新 EXE 精确预览仍待测。
5. N07：前轮合法合成备份合并、覆盖均不能解锁的原生局部 PASS 保留；本 EXE 可做必要回归，不重用无效备份，不放宽解析器。IPC/编译门禁专项本轮自动化通过，未做新 GUI 操作。
6. WM_COPY：历史来源仍 UNKNOWN。新测试只记录合成控件与明确人工动作，不启用实验性自动 Ctrl+C、不做 R01–R03 真实竞争。
7. N02/N06/N08：必要的密码拒绝、禁用状态与正常退出清理回归，均须记录本 EXE 摘要。

尚未闭合的 N01 原生入口、N05 完整精确复制、N03/N04 剩余矩阵及最终发布包/来源门禁，继续阻止 Beta.2 发布。本轮完成本地修复、自动化与候选冻结后停止。


## 后续原生尝试中止（2026-10-28）

原临时冻结目录在复测前已不存在；从当前 Release 输出中找到字节摘要相同的 EXE，复制到全新隔离目录后再次核验为 `2D09F1B7F03428A48171EB7A0FC245E9D8E438B19B663418882A399DC7DEEAC4`，未重新编译。用户确认只操作合成资料后启动，采用独立配置及 WebView2 数据目录、Meta 档和合成诊断。

测试进程在首帧握手后约 4 秒异常退出。Windows Application Error 1000：异常码 `0xC0000409`，模块 `deepseek-float.exe`，偏移 `0x4383EB`。原因未定位，不能宣称与显示事务有或没有因果关系。用户未看到夹具窗口，夹具启动就绪未确认，已清理该隐藏测试进程。N01/N05 均为 NOT_TESTED，不计 PASS 或将其当作 N01 功能 FAIL；未执行主动复制。

此次启动失败列为 P1 发布阻断，后续需先定位启动故障和夹具未显示原因，再开展原生复测。历史失败、冻结摘要和前述自动化测试记录保留；Beta.2 发布仍 NO-GO。原始事件和日志仅在本地临时证据目录，不进入 Git。
