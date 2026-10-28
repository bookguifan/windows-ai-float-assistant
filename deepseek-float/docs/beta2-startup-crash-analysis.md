# Beta.2 启动崩溃 P1 调查（2026-10-28）

## 结论与状态

**根因仍 UNKNOWN；没有声称修复。Beta.2 继续 NO-GO。** 已用故障 EXE 匹配的 PDB 将故障 RVA 定位至 `std::panicking::panic_with_hook+0x3B0`，但原现场无转储，不能恢复首个 panic 的业务调用栈。本轮经单独授权运行同一 EXE，一次隔离调试观察 45 秒未捕获目标异常，随后用户菜单退出；不能据一次未复现宣称稳定性恢复。

本轮未改生产逻辑、测试期望、发行准入、Session 8 秒期限、PopupShow 授权、图标或锁文件。未提交、推送、合并或发布；原私有仓库未修改。N01/N05 原生验收继续暂缓。

## Git 基线与审查范围

- 公开仓库：`bookguifan/windows-ai-float-assistant`。
- 工作分支：`fix/beta2-native-acceptance`。
- 本地及远端分支 HEAD：`d7e7afd17b64a9989fa6cfa03f41ef6bbf0477e5`。
- 前一提交：`f9c364071210afac131989c9080db335a8106636`。
- 本地/远端 main：`922479b1a30c070b572dfbd252cccaa543837449`，未移动。
- 开始时工作区、暂存区干净；本轮仅新增本报告并追加 Known Issues 调查结论。
- 已复核 d7e7afd 的 10 个变更文件，以及 lifecycle/lib/main/windows 启动编排。历史 N01/N05 FAIL、WM_COPY UNKNOWN、R01–R03 研究风险及本次原始启动失败均保留。

## 故障现场及符号证据

|项目|实测值/证据边界|
|---|---|
|故障 EXE SHA256|`2D09F1B7F03428A48171EB7A0FC245E9D8E438B19B663418882A399DC7DEEAC4`|
|Windows 事件|Application Error 1000，时间 2026-10-28 09:29:05（北京时间）；WER BEX64 对应同一报告及进程|
|ExceptionCode|`0xC0000409`|
|故障模块/RVA|`deepseek-float.exe` / `0x4383EB`|
|机器类型/PE 时间戳|AMD64 `0x8664` / `0x6AE09567`|
|首选 ImageBase|`0x140000000`；不是原现场 ASLR 后的实际加载基址|
|文件偏移|按 PE section 映射为 `0x4377EB`，不能与 RVA 混用|
|运行时 ExceptionAddress|原模块加载基址缺失，UNKNOWN；不把首选基址相加结果冒充运行时 VA|
|CodeView / PDB 匹配|GUID `8278c2f6-9e4f-4177-b364-b81cb031ffda`、Age 1 均一致|
|PDB SHA256|`FA51DAB070E3BC7464358BBBB45B0ECF84B6BC64CDFBC1CAA09D61503F5949C2`|
|离线符号|本机 DbgHelp、仅本地匹配 PDB：`std::panicking::panic_with_hook+0x3B0`；符号起始 RVA `0x43803B`|
|WER 异常数据|`7`，与本机 Windows SDK winnt.h 的 `FAST_FAIL_FATAL_APP_EXIT` 常量相符；WER 字段不能替代完整 ExceptionInformation 数组|
|异常线程及线程栈|UNKNOWN；现有归档只有 Report.wer，未找到该现场转储，不能凭故障指令推断调用者|
|FailFast 原因|不能仅凭该异常码认定栈/缓冲区溢出、FFI panic、panic hook 二次 panic 或具体业务错误|

最早可信故障位置只收敛到 Rust panic 处理函数内的故障指令，**尚未收敛到首个业务故障节点**。`panic_with_hook` 可服务不同 panic/abort 分支，缺少现场栈时不能确定上游分支。PDB 已单独复制保全，未使用新构建的 PDB 强行解释旧 EXE。

原日志从球窗首帧握手继续到面板、右键菜单和热键初始化完成，最后为菜单页面就绪；没有 popup 创建成功、settings 创建成功、mousehook::start 或启动完成标记。它只限定最后可观察阶段，不能证明异常必在主线程或精确发生在下一行。

## 实际初始化调用图

```text
main -> lib 再导出 lifecycle::run
  -> console handler / single_instance::acquire
  -> launcher 信标 / 安装全局 panic hook
  -> Tauri 建配置 ball 窗口 -> setup
     -> backup::recover_interrupted_restore
     -> 可选日志插件 / diag 初始化
     -> ballstyle::fix / 球位置配置加载 / gate_ball_show
     -> setup_panel / tray / context_menu::ensure / hotkey::start
     -> panels::setup_popup_and_mousehook
        -> popup::ensure（创建隐藏 popup，登记 HWND、auto-close 线程）
        -> settings::ensure（隐藏设置页）
        -> mousehook::start
           -> selection_trace::start（显式 synthetic-only 时初始化两个有界队列）
           -> MouseEvent 控制线程
           -> pump: WH_MOUSE_LL -> SetWinEventHook -> 消息泵
     -> screenshot::ensure / result::ensure
     -> 隐藏面板预热 / 自启对账 / fullscreen / 窗口诊断 / boot PID
     -> single_instance::watch -> setup 返回 -> 事件循环

拖选事件后（不是无划词启动时立即运行）：
  mousehook -> selection_runtime::submit -> START.call_once
  -> UIA/Native Edit worker + 25ms supervisor
  -> accept_session_on_main -> offer -> show_at_cursor -> PopupShow
```

本次成功观察的日志先后记录 popup、settings、mousehook、WH_MOUSE_LL OK、shot/result、boot。原失败记录尚未到这些完成检查点；因此没有证据把新 foreground_changed 定为当次直接触发者，也不排除其他初始化线程的异步错误。没有在相同环境启动前一版本，不能宣称已证明 d7e7afd 相对 f9c3640 的确定回归。

## 定向静态审查

|范围|当前代码证据|未排除事项|
|---|---|---|
|WinEvent callback|`foreground_changed` 没有 unwrap/expect；状态用 try_lock，失败返回保守拒绝；invalidate_if 为 CAS；没有 callback 内格式化、磁盘 I/O 或 COM|没有异常现场栈，无法证明不存在所有 Windows 回调重入/FFI panic 场景|
|Popup 状态|HWND 初始 0；创建成功才登记；native_identity 用 IsWindow、PID/thread 查询，不解引用 HWND；无 Session 时不能 begin_show；隐藏/失败/消费终止 show|IsWindow 与后续操作不是跨进程原子承诺；原生竞态仍待专项验收，不改宽身份策略|
|延迟任务|主线程投递，检查 ticket/generation/show serial、源身份存活及前台；不持状态锁调用 SetForegroundWindow|新 EXE 的 N01 焦点时序未验证；不能用本次只启动替代|
|诊断|selection_trace::start 在 popup/settings ensure 后且注册 hook 前调用；OnceLock 队列固定容量 32，try_send 丢诊断；STOP 原子退出，关诊断时不建队列|后台诊断线程未 join 的既有退出模型不能仅凭一次干净残留检查认定所有退出竞态通过|
|runtime|worker/supervisor 按首个 submit 惰性启动；无划词启动不能直接归因到 supervisor 的选区处理|单次无拖选启动未覆盖 provider 阻塞、票据或原生文本链路|
|panic 记录|现有 hook 在写 panic.log 前 force_capture、格式化、eprintln；原现场没有 panic.log|原 panic 文本丢失原因 UNKNOWN；不得凭未写日志就认定 hook 是根因，更不擅自 catch/吞所有 panic|

没有发现足以解释本次故障并能用现有证据证明的生产逻辑缺陷，因此没有猜测性回滚 PopupShow、禁用全部诊断、放宽窗口校验或加入全局焦点豁免。

## 本轮单独授权的最小实验

用户明确批准后才通过 Win32 Debug API 启动同一故障 EXE：`DEBUG_ONLY_THIS_PROCESS`，独立 LOCALAPPDATA/APPDATA/WebView2 目录，配置 autostart=false、privileged_capture=false、回退 false、hotkey=null；面板为本地未提供服务地址，不使用真实 AI 站点。没有启动合成划词夹具，没有剪贴板读写。

本地捕获器对目标异常准备记录 ExceptionInformation、异常线程、StackWalk64 与 MiniDumpWriteDump；未设置系统 WER、未下载远端符号、未上传任何内容。**45 秒内没有捕获到指定异常，因此没有异常栈或转储生成，不能把捕获能力当作已取得栈。** 观察到的初始化日志全部越过原停点。调试器 45 秒后脱离；程序继续运行，用户随后菜单退出。

- 用户确认菜单退出；应用日志末尾为 `run: ExitRequested` 主动退出路径。
- 从诊断相对时间看本次运行约 443 秒（7 分 23 秒）。
- 本轮主 PID 与携带该隔离 profile 的 WebView2 进程均无残留。
- 捕获器脱离后没有保留等待应用退出的句柄，**未取得精确退出码，不写成 exit 0**。
- 这只是一次未复现，不是多次启动稳定性 PASS；调试器、独立目录和未启动旧夹具改变了条件，可能影响时序。
- 原失败日志中 Meta/synthetic-only 开启；本次同样开启仍未复现。普通诊断关闭组合尚未原生测试，不能宣布通过；未通过关闭诊断伪装修复。

## RED/GREEN 与代码处置

本轮没有能复现首个 panic 的测试，没有 RED/GREEN 修复结果；状态为 **N/A（根因证据不足）**。已有 N01 真实 RED/GREEN 及显示事务负向测试保留，不改期望，不代替启动回归。本轮重新执行的纯自动化均为基线健全性验证。

尚未添加生产诊断检查点：现有日志已能区分“popup ensure 完成之前”与后续钩子阶段；在未取得首个 panic 栈前，本轮优先保持故障字节不变。若下一次仍无栈，可先提出在 ensure 前后及 shutdown 添加固定启动阶段标记的最小补丁，使用现有诊断开关、不在 callback 中落盘，不改变授权/线程同步；不把这类取证补丁叫作崩溃修复。

## 本轮实际自动化

仓库根 Cargo 命令均指定 `--manifest-path deepseek-float/src-tauri/Cargo.toml`，pnpm 在应用目录运行。工具链：rustc/cargo 1.98.1、Node v24.14.0、项目 pnpm 9.15.9；Node 低于要求 ^24.15.0，保留 engine 警告，未擅自升级环境。

|命令|退出码|实际结果|
|---|---|---|
|cargo test --offline --locked（完整目标）|0|397 PASS，12 ignored，0 FAIL；各目标执行数合计，非独立 GUI 场景数|
|cargo test --offline --locked --test selection_core|0|40 PASS；显示事务及既有票据检查|
|cargo test --offline --locked --features experimental-clipboard-fallback --test release_selection_policy|0|4 PASS，Debug 研究组合，无系统剪贴板操作|
|cargo test --offline --locked --release --features custom-protocol,experimental-clipboard-fallback --test release_selection_policy|0|4 PASS，发行构建不可被研究 feature 解锁|
|cargo fmt --check|0|格式通过|
|cargo clippy --offline --locked --all-targets -- -D warnings|0|严格检查通过|
|cargo check --offline --locked --release --features custom-protocol|0|生产特性检查通过|
|pnpm build / pnpm check|0 / 0|前端构建及检查通过|
|pnpm test:unit|0|384 PASS（71+161 个自定义断言及152个 Node tests），0 FAIL|
|pnpm test:js|0|本地 Chromium Playwright 22 PASS，不是 Windows 工具窗口验收|

Release 链接和最终文件检查在下节记录。没有以这些 PASS 宣称启动问题关闭，也没有触发远端 CI。

## 构建、隐私与待办

旧故障 EXE 和匹配 PDB 已独立保全；PDB/转储仅本地，不作为 Release 附件。原 EXE 静态扫描命中 178 个 ASCII 用户目录样式编译路径、0 个 UTF-16 用户目录模式；只记数量，不抄用户名/路径。模式扫描不等于完整秘密审计，不能视为已消除个人路径公开风险。调试符号包含构建路径，不应上传。

### 独立构建验证产物（未修复，不用于发布）

- 实际命令：`CARGO_INCREMENTAL=0 cargo build --offline --locked --manifest-path deepseek-float/src-tauri/Cargo.toml --release --features custom-protocol --bin deepseek-float`；退出 0，链接耗时 4 分 03 秒。
- 编译业务源码仍为 HEAD d7e7afd；构建期间新增/修改的只有调查文档，构建没有任何崩溃修复补丁。不是声称可重复构建或修复后的新版本。
- 独立本地目录：本轮临时取证目录下 `baseline-link-validation/`，未覆盖旧故障 EXE。
- EXE SHA256：`4544237B9520F53F155E931556B40FC2B3D37A4F52422BF934A6B1E4FF8B3E9A`。
- PDB SHA256：`A747953BD094040A6925991DBCD0C614BCA87C41868BDA02C89D9E7FCE6AF6FF`。
- 新 EXE/PDB CodeView GUID：`ece85ac6-8451-4876-845e-79c55c7b57ee`、Age 1，一致。该 PDB 只对应新验证产物，不拿它解释旧故障。
- 签名 NotSigned，custom-protocol，未启用实验性 feature；本轮未启动该新产物。因未修改生产逻辑，**不得标记为 FIXED 或通过原生启动验收**。
- `CARGO_INCREMENTAL=0` 是本次明确记录的构建环境；旧故障构建未记录同一强制值，字节差异不能归因于业务修复。
- 新产物仍可能含编译路径；未进行完整发布秘密审计，不向外分发 EXE/PDB/日志。
- 最终源码文件 SHA256 清单及本轮验证记录保存在仓库外；清单不参与自身哈希。文档更新不构成新业务版本。
- 本轮相对链接检查、文档敏感模式检查、`git diff --check`、`git diff --cached --check` 均退出 0；暂存区为空。


## 下一次最小实验与放行条件

1. 保留同一故障 EXE/PDB，以可复核的工具继续捕获首个 panic 与致命异常；捕获器须保留进程句柄到菜单退出，以取得真实退出码。优先只有助手、无夹具、无剪贴板；必要时再单独授权对照原启动父进程/句柄环境，一次只改变一个条件。
2. 如需要再次运行、调试或新增转储设置，按本轮边界先取得下一次明确授权。不得默认继承为无限次重试或启动 N01/N05。
3. 得到首个可信业务栈后，建立真实 RED，再修最小生命周期/FFI/初始化问题；未定位前不通过禁用钩子、诊断或减少校验来“修复”。
4. 新修复候选需要独立摘要、匹配符号、连续启动和用户菜单退出证据；本次单次未复现不能放行 P1。
5. 启动 P1 关闭后才恢复 N01 冷/热首次点击、N05 真实选区→复制精确链、N03 旧票据/替换/超时/重复消费、N04 UIA 完整文本矩阵；N07 仅保留前轮合并/覆盖局部 PASS。
6. WM_COPY 历史来源 UNKNOWN，R01–R03 保留原结果。发行危险自动 Ctrl+C 门禁及密码/目标/过期检查不变；Beta.2 仍 NO-GO，不合并、不发布。
