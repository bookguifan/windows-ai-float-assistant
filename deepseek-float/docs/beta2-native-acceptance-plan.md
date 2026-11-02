# Beta.2.0 Windows 原生验收计划（Draft）

## 候选

- 源码：`feature/beta2-release-safety` / `2f2ab6b5cac9b8b650dd1a5c8fcd149c67bc249e`
- EXE：`deepseek-float.exe` / SHA256 `372196e9f6c2cdbc098768450126188b31ba519b50532c160e7d82aee77f5e01`
- PDB（仅诊断）：SHA256 `662d172e6e16a0dcb563e8fb6c4a647e93c0d91ecb3685ca1eda8c49479ec545` / GUID-Age `c57a5122-1d56-4fff-a6e1-883780a4c819 / 1`
- Build features：`custom-protocol`；不含 `beta2-isolation`、`experimental-clipboard-fallback`；Release 自动 Ctrl+C 回退关闭。
- 本计划尚未启动该 EXE，启动计数为 0。

## 启动隔离前置条件

普通 Release 使用正式 Tauri identifier，不能仅设置 `LOCALAPPDATA` 就声称测试数据隔离。先在独立 Windows 测试账户，或已安装并启用的 Windows Sandbox 内运行；确认配置、收藏、提示词、日志、WebView2 profile 和备份位置都归属于该测试环境。基线先保留在测试账户的独立证据目录，不接触维护者正式配置。候选不具备可用隔离环境时，不启动。

本机未发现 `WindowsSandbox.exe`，当前没有第二个已验证的隔离用户配置，因此本计划目前为 **BLOCKED**。不通过切换环境变量、重用旧 Known Folder 或创建新的 identifier 数据目录来绕过。

## 原生验收顺序

1. 验 EXE/PDB SHA256 与 GUID/Age，再记录测试配置下三份业务文件、自启项和原有用户数据基线。
2. 启动一次候选，记录 PID、时间、进程子树、WebView2 首个 HRESULT、实际数据路径、球窗 HWND/可见性和 `ball_ready`。
3. 验悬浮球拖动、显示/隐藏、设置页打开/保存/重读/关闭及应用正常退出；保存时只改测试账户配置。
4. 用独立测试窗口和模拟文本做 A01 合成选区，验工具条显示/计时隐藏、Alt+Tab 行为、旧 Session/Ticket 与菜单失效、快速重复选择及异步旧结果拒绝。不要点击工具条的 Copy：A01 选区到工具条路径不执行剪贴板操作，Copy 是另一条会写系统剪贴板的显式路径，应单独验收。
5. 真实 Windows 剪贴板验收须另行明确授权，并只在一次性 VM/Sandbox 中执行，关闭宿主剪贴板重定向。先在来宾环境内写入并核验已知测试哨兵，再覆盖为纯文本、中文/英文、多行、Unicode、HTML、RTF、图片及多格式组合；加入剪贴板占用和发布失败注入，核对成功内容、完整/部分恢复与失败报告。每个用例后只恢复来宾内已知哨兵；不得读取、保存或覆盖宿主未知剪贴板内容。无法证明宿主剪贴板共享关闭时，跳过真实剪贴板测试。
6. 在一次性测试账户或 VM 的临时目录做设置保存/重读、收藏导出、备份/取消、预览、合法恢复、失败回滚与非法文件拒绝；逐文件核验隔离数据。当前 `beta2-isolation` 诊断构建禁用了原生文件对话框，不能用它验收 Windows 文件选择器；文件选择器须在隔离账户/VM 中用普通 Release 候选验证。正式配置不得作为输入或输出。
7. 正常退出，记录真实退出码、WebView2 子进程清理和应用事件；对比测试账户的正式文件基线及自启项。异常时保留首个失败阶段，不自动重启。

## 当前状态

- 本机候选启动：**NOT RUN**。本轮没有操作真实系统剪贴板、没有改写真实用户配置、没有执行原生备份恢复。
- 自动化/内存 facade 只证明相应测试边界，不代表桌面窗口、原生剪贴板或文件选择器验收通过。
- 取得隔离测试账户/Windows Sandbox 后，应以此处冻结的 EXE SHA 为唯一候选；若候选身份变更，重新生成本计划并另作启动授权。
- Issue #2 保持 OPEN；有限次成功启动不证明间歇性异常已修复。

## 专项操作边界

- A01 验收限于选区、工具条、Session/Ticket、计时器及异步结果生命周期；不点击 Copy，不触发剪贴板写入。若流程意外进入 Copy，停止该用例并转入单独授权的剪贴板验收。
- 真实剪贴板测试仅允许在宿主剪贴板重定向已关闭的一次性 Windows VM/Sandbox 内进行。已知哨兵必须在来宾内创建和恢复；不读取或覆盖维护者宿主剪贴板中的未知内容。
- 设置保存、文件选择器、备份及恢复只使用一次性测试账户/VM。隔离诊断构建的文件对话框禁用行为意味着它不能证明普通 Release 的原生文件选择器可用。
- 后续原生回归应使用自动化测试后生成并冻结的候选身份；一次授权请求集中列明启动次数、允许的 UI 操作、剪贴板隔离方式和恢复方式。候选身份或隔离环境不匹配时不启动。

## 2026-11-02 隔离启动计划修正与单次回归

- 首次尝试（PID `32920`，约 15 ms，退出码 `78`）是启动计划错误：启动器清除继承的 `WEBVIEW2_*` 后，又注入 `WEBVIEW2_USER_DATA_FOLDER`；应用按既有 fail-closed 规则在 WebView2 初始化前拒绝启动。这不是 WebView2 故障证据。
- 修正后的启动计划不设置任何 `WEBVIEW2_*`、`TEMP` 或 `TMP`，只在候选子进程环境中清除继承的 `WEBVIEW2_*`；WebView2 路径由应用隔离校验后派生并传入 Tauri 窗口构建器。未修改生产代码。
- 启动器还修正了 JSON 计划缺少 `startup_id` / `started_at_utc` 属性时的写入错误。由于当前环境的 WMI `Win32_Process` 查询返回拒绝访问，进程树监控改用 Win32 Toolhelp 快照；短时子进程关联回归通过。
- 回归结果：启动器 no-launch 测试 PASS；隔离路径 `7/7` PASS；Tauri mock 窗口引导 `1/1` PASS；普通构建窗口配置 `1/1` PASS；`git diff --check` PASS。首次隔离测试因测试构建使用正式 identifier 得到 `5/7`，按项目测试专用 Tauri identifier 覆盖后重跑为 `7/7`。

### 冻结候选与本次启动

- 候选来自 `main` / `2ecaba3b8b0d71aa69e38e0975f9916055c13cde`，为本地隔离诊断 Release，不是正式发行候选；构建 features 为 `custom-protocol,beta2-isolation`。
- EXE SHA256：`16FBFAAF6CAFA51D4D99F847BA3F0204D1D8793F02A5D37928DA07DB45B8ADDA`。
- PDB SHA256：`E0E4328DB9C9AE7EF8258127478EC3F89C679514A698946D8B9EB908B28BCE7D`；GUID/Age：`3a63cd30-8dab-4c40-8fbe-daa4bffdde87 / 1`。
- 本次修正计划只启动一次：`2026-11-02 01:54:23 UTC`，startup ID `ddea9b0cbb344a338e86b24a0d06f253`，PID `46148`。隔离根为本机 `%TEMP%\float-beta2-native-acceptance-20261102-b0711db770564ea2876e0ed5e44bb3b8\native-data-corrected-82dcdfde19a04939b8f3764a96c4ac87`；正式配置基线保存在同目录下的本地验收证据中。
- 运行时实际创建了隔离配置、诊断、日志、临时和 `webview2\EBWebView` 目录。identifier 派生的 LocalAppData 目录未创建；本次启动前正式 config/favorites/prompts SHA256 分别为 `CC5EC62C4A89C09E0E26DB0F9DB327F18E7ECCEAA5462BF60EB2639B6B84CB6E`、`31090B94842B85AEAFA1AA218A68AD2C596B4F30FE8101E18D23C44B39ECE9FB`、`461D059D3B1EB536073C4107A42088E58F1501533462563C5A550F6710BFB2F6`；Run 自启值为空。运行后哈希及自启对比待主进程退出后补记。
- Win32 监控记录球窗 HWND `267448` 有效且可见；主进程仍响应。隔离 `poc2-state.log` 记录 setup 已进入。未观察到主进程后代子进程；未发现 Application Error 1000 / WER 1001。
- `d1-debug.log` 未生成：候选为 Release，启动环境未设置 `DSFLOAT_DIAG`，该构建默认关闭业务诊断落盘。因此没有 `[ball-ready]` 握手/HRESULT 的可审计日志；窗口也未出现在当前 `@oai/sky` 可交互清单中。UI Ready **UNKNOWN**，WebView2 初始化 **UNKNOWN**，球窗隐藏/恢复 **NOT TESTED**，设置页 **BLOCKED**。不能以 HWND 可见或 `poc2-state.log` 代替 UI Ready。
- 启动阶段先记录了正常退出和正式数据对比待补；最终退出及数据复核见下方“运行后结果”。证据目录为 `%TEMP%\float-beta2-native-acceptance-20261102-b0711db770564ea2876e0ed5e44bb3b8\native-run-ddea9b0cbb344a338e86b24a0d06f253`。
- 本次新授权启动计数为 `1`；不自动重试。A01、剪贴板、备份恢复、自启写入及正式配置修改均未执行。Beta.2.0 仍为 **NO-GO**。

### 运行后结果（最终记录）

- 用户报告无法通过应用正常退出入口关闭。经核对 PID `46148` 的映像路径后，按用户授权仅对该隔离测试进程执行强制终止；监控句柄记录退出时间 `2026-11-02 02:10:41 UTC`、退出码 `-1`。这是人工终止结果，正常退出 **FAIL**，不能解释为应用正常退出或崩溃。
- 进程监控在启动后记录到有效、可见球窗 HWND `267448`；另有隐藏窗口 HWND `983520`。`poc2-state.log` 记录进入 setup，但没有 UI Ready 信号，也没有有效的 WebView2 初始化完成/HRESULT 证据。UI Ready **UNKNOWN**，WebView2 初始化 **UNKNOWN**，设置页 **BLOCKED**；不能仅依据 HWND 判定页面可用。
- 退出监控追踪到 21 个候选后代 PID；主进程终止后 10 秒剩余后代为 0。子进程清理 **PASS（监控范围内）**。
- 正式文件启动前后 SHA256 一致：`config.json` `CC5EC62C4A89C09E0E26DB0F9DB327F18E7ECCEAA5462BF60EB2639B6B84CB6E`；`favorites.json` `31090B94842B85AEAFA1AA218A68AD2C596B4F30FE8101E18D23C44B39ECE9FB`；`prompts.json` `461D059D3B1EB536073C4107A42088E58F1501533462563C5A550F6710BFB2F6`。HKCU Run 项 `deepseek-float` 前后均不存在。正式业务数据与自启项 **PASS（未变化）**。
- 本次隔离根内记录 102 个文件；其中包含 config、诊断记录及 WebView2 `EBWebView` profile。候选 identifier 派生的 LocalAppData 目录不存在。测试数据未清理。
- UI Ready 观测缺口来自本次 Release 启动计划未启用 `DSFLOAT_DIAG`，而该候选默认不写 `d1-debug.log`；所以 `[ball-ready]` 证据无法采集。这是验收计划/观测配置缺口，现有证据不足以判定 WebView2 页面初始化失败。已启动到可见 HWND，不满足“无法启动”的回退条件；未回退生产代码，也未重启。
- 本次验收最终结论：启动进程与可见 HWND **PASS**；WebView2 初始化、UI Ready **UNKNOWN**；设置页 **BLOCKED**；正常退出 **FAIL（人工强制终止，退出码 `-1`）**；监控范围内子进程清理及正式数据/自启项未变化 **PASS**。本轮启动次数为 `1`，Issue #2 保持 OPEN，Beta.2.0 **NO-GO**。
- 证据目录：`%TEMP%\float-beta2-native-acceptance-20261102-b0711db770564ea2876e0ed5e44bb3b8\native-run-ddea9b0cbb344a338e86b24a0d06f253`；关键材料为 `preflight-baseline.json`、`post-run.json`、`process-window-monitor.jsonl`。禁止据此重启同一候选；后续若需原生验收，先修正诊断观测计划并另行取得启动授权。
- 证据 SHA256：`preflight-baseline.json` `03C4C0898C700B0382075EDE7B51FD8D850701B9141A11549D0A8E521F550358`；`post-run.json` `BCEA2A26A11A9C9BD4ABA2ABDA1D27C56E43D12CFABD82C1E2D0E251EBF89615`；`process-window-monitor.jsonl` `BEE003D32AAEF003D17B8B3A0E232BA579BD5A1BD4EA0B79E906071F2253C700`。

## 2026-11-02 DSFLOAT_DIAG 单次原生验收

### 后续证据复核

- 原始 d1-debug.log 明确记录约 3 秒握手超时且无 Rust 收到记录；同次 process-window-monitor.jsonl 却在约 2.6 秒将 ui_ready_signal_seen 置为 true。记录相互矛盾，最终 UI Ready 判为 **UNKNOWN**，不能采信监控布尔字段为 PASS。
- 页面/IPC 失败的实际阶段仍未确定。PID 16588 是本授权唯一启动，用户找不到正常退出入口后按本次授权人工终止；退出码 -1 不代表崩溃。正式配置哈希和自启项未变化。
- 本轮在隔离构建中加入页面导航、脚本/IPC/首帧/握手结果的脱敏阶段日志。诊断 IPC 只接受固定阶段名，普通构建不注册该命令，也不注入页面脚本。需要新候选和另一次单独启动授权才能观测这些阶段。
- 球页交互测试 9/9 通过。完整隔离 Rust 测试、重建 dist 后的 CSP、fmt 和严格 Clippy 仍需完成。Beta.2.0 保持 **NO-GO**。

- 未改应用源码或构建输入。启动计划为本次任务新建唯一隔离根，并只给候选子进程设置 `DSFLOAT_DIAG=1`；该值在 Release 中解析为脱敏 Meta 档。启动器静态预检查通过：`d1-debug.log` 精确指向 `<隔离根>\diagnostics\d1-debug.log`，不设置 `WEBVIEW2_*`、`TEMP` 或 `TMP`。隔离路径仍由应用校验后配置。
- 冻结候选未变：EXE SHA256 `16FBFAAF6CAFA51D4D99F847BA3F0204D1D8793F02A5D37928DA07DB45B8ADDA`；PDB SHA256 `E0E4328DB9C9AE7EF8258127478EC3F89C679514A698946D8B9EB908B28BCE7D`；GUID/Age `3a63cd30-8dab-4c40-8fbe-daa4bffdde87/1`。
- 唯一启动时间 `2026-11-02 02:29:49 UTC`，startup ID `bf657ba55f8f456bb6ffc00014632af1`，PID `16588`；启动器记录进程启动。窗口监控见有效 HWND `201992` 可见，另一个 HWND `398646` 隐藏。隔离目录记录了 `ball` 窗口的 `data_directory` 指向本次 `<隔离根>\webview2`。
- 诊断日志首行确认 `档位=Meta`，因此开关确实生效；用户正文按长度/哈希脱敏。日志记录 `ball-ready` 等待信号，约 3 秒后明确记载“等待 ball_ready 超时”。没有收到握手。故原生进程启动 **PASS**，可见原生窗口/HWND **PASS**，UI Ready/首帧握手 **FAIL**；WebView2 控件初始化和页面导航完成 **UNKNOWN**，因为没有可确认的成功回执或 HRESULT。不能把启动器旧布尔字段 `ui_ready_signal_seen=true` 当作证据：旧匹配器只搜 `[ball-ready]`，误把等待和超时行也计为 Ready。
- 发现并修正启动器 Ready 匹配器：仅接受含 `[ball-ready]` 且实际包含“收到 ball_ready”的行。无启动静态回归分别验证真实握手通过、超时日志不通过；启动器回归、PowerShell 解析及 `git diff --check` 均通过。修正不改变候选二进制。
- 因握手超时，球窗隐藏/恢复和设置页 **NOT TESTED**。用户无法找到托盘或退出入口，因此正常退出 **BLOCKED**；未把人工终止解释为崩溃。按授权核验候选映像后，终止 PID `16588`，进程句柄退出码 `-1`。
- 正式 config/favorites/prompts SHA256 前后与上一节记录值一致；HKCU Run 的 `deepseek-float` 值前后均不存在，数据隔离 **PASS**。候选 identifier Known Folder 未创建；隔离根记录 101 个文件。终止清理瞬时进程树未发现活跃后代；监控的两个剩余 PID 后经核验为已复用的 Codex `node.exe` 与 `mcp-server-fetch.exe`，其父 PID 不属于候选，未对其采取操作。因为监控只记录 PID、未锁定进程创建时间，子进程清理 **UNKNOWN**。
- 本次证据目录：`%TEMP%\float-beta2-native-acceptance-20261102-b0711db770564ea2876e0ed5e44bb3b8\diagnostic-run-72de58aab34f410fa4faf140eb8ea6d3\native-run-bf657ba55f8f456bb6ffc00014632af1`。`d1-debug.log` SHA256 `A1123F3BB2F998CE36C1386B9C58AE99B437B42BF81B85FC666CBAA8EE12C3C2`；`post-run.json` SHA256 `6410EB8510A39DC071855DFBA0E41CB338F49378C5119513DF21ACC90ECB80C2`；`process-window-monitor.jsonl` SHA256 `7A6AF0E88700817A4F955C15E7EF33B9AC4D10D9D329E6AC7A2BEDC90052C058`。
- 本轮启动次数为 1；不重试。未操作剪贴板、未保存设置、未执行备份/恢复、未改正式配置或自启。未发现可据以回退应用源码的证据；明确缺陷为 UI Ready 握手超时，具体页面/IPC 根因仍待有诊断回执的后续授权验收定位。Beta.2.0 **NO-GO**。

## 2026-11-02 诊断修复候选原生回归（单次）

- 候选基于 `fix/beta2-ball-ready-diagnostics`，Git HEAD `2ecaba3b8b0d71aa69e38e0975f9916055c13cde`；工作区有 11 项未提交修改，候选不是该 HEAD 的干净工作树构建。Release features 为 `custom-protocol,beta2-isolation`，Node `v24.22.1`、Rust `1.98.1`、WebView2 Runtime `156.0.4314.58`。
- 本次仅启动一次：`2026-11-02T04:17:29.014Z`，startup ID `88c00a7f9a944cbaaf213dbb5409c037`，PID `43652`。EXE SHA256 `22A1837B523FABD63D5376F59B3D82E7D16D76A3F94251C914A4A73020E176CC`；PDB SHA256 `08F1F8B95390DDAC3932A65F1B72DC4FA6FB1B6C35C2579B420DEEFEA7FD9EEB`，GUID/Age `1195a12b-9292-4085-9f4c-5ee3013ae619/1`，与 EXE CodeView 匹配。
- `DSFLOAT_DIAG=1` 仅传给候选子进程；未设置任何 `WEBVIEW2_*` 变量。诊断日志依次记录 navigation started/finished、document/script start、IPC available、DOMContentLoaded、ready invoke、Rust 收到 `ball_ready`（启动后约 171 ms）、首帧回调（约 173 ms）及 invoke 成功（约 198 ms）。WebView2 页面/IPC 初始化与 UI Ready **PASS**；未记录 HRESULT 错误。球窗 HWND `6622952` 有效且可见。
- 用户手动确认隐藏球窗、等待至少 5 秒后未自行重现、主动显示恢复；应用日志分别记录 `ball_hide` 与后续 `ball_show`，两事件相隔约 9.35 秒。基础隐藏/恢复 **PASS**。由于手动隐藏发生在启动约 42 分钟后，启动阶段迟到回调竞争未被本次运行实际触发，相关竞态修复仍不能据此单独宣称已验证。
- 用户确认设置页已打开并正常关闭，未保存设置；设置页 **PASS（人工确认）**。用户随后通过应用自身退出入口退出。应用日志记录主动 `run: ExitRequested`；进程句柄在 `2026-11-02T05:00:26.763Z` 记录真实退出码 `0`。监控发现 16 个 WebView2 后代，主进程退出 10 秒后剩余 0；正常退出与监控范围内子进程清理 **PASS**。运行区间未发现匹配该应用的 Application Error 1000 或 WER 1001。
- 隔离根为 `%TEMP%\float-beta2-ball-ready-diag-fixed-20261102-486f2353f8da497aad0ebeeedcba6ca1\native-run-root`；WebView2 数据位于其 `webview2` 子目录。运行后清单共 251 个文件，根外路径数为 0；候选 identifier 派生的 LocalAppData 目录未创建。正式 config/favorites/prompts SHA256 与启动前一致，HKCU Run 的 `deepseek-float` 项前后均不存在。数据隔离 **PASS**。
- 交互工具未返回球窗对象，设置页步骤由用户手动完成；没有操作剪贴板、备份恢复或设置保存。证据目录为 `%TEMP%\float-beta2-ball-ready-diag-fixed-20261102-486f2353f8da497aad0ebeeedcba6ca1\native-run-88c00a7f9a944cbaaf213dbb5409c037`。`d1-debug.log` SHA256 `42BC4AB6473DF27156E1A9AB17F365539AAC0D6A762D1D94BAE80B21A8EC8933`；`post-run.json` SHA256 `6B0B975C3AD44E5E47FA2AB64429680EAA0D4FBE680D21138CEC3D58F30402E6`；`process-window-monitor.jsonl` SHA256 `18E05DCFED8DEDA119B56E4B2FA9840460F0546ADF1C14B15797503E56B18C95`。
- 本次启动与握手、球窗显示、基础隐藏/恢复、设置页人工验收、应用正常退出、子进程清理、正式数据不变均通过；启动迟到回调竞态未由本次时序充分覆盖。Issue #2 保持 OPEN；单次启动不证明间歇性启动问题已根治。Beta.2.0 仍为 **NO-GO**。
