//! 应用生命周期：启动编排（[`run`]）+ 调试构建专用自检 / 稳定性压测入口。
//!
//! 从 `lib.rs` 拆出。`run` 是 crate 入口（由 `main.rs` 经 `lib.rs` 再导出），
//! 编排窗口创建、托盘、热键、划词、回读等全部子系统；`setup_selftest` / `setup_stress`
//! 为 `#[cfg(debug_assertions)]` 排障入口，release 构建中不存在。

use std::fs::File;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::LevelFilter;
use tauri::{Builder, RunEvent};
// `Manager`（`get_webview_window`）仅被 `#[cfg(debug_assertions)]` 的 `setup_selftest` /
// `setup_stress` 中的 `get_webview_window` 使用，故随 debug 门控，避免 release 下的 unused 告警。
#[cfg(debug_assertions)]
use tauri::Manager;
use tauri_plugin_log::Builder as LogBuilder;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ★ 第一层止血：忽略本进程控制台的 Ctrl+C（CTRL_C_EVENT）。
    // 历史：划词抓取曾用 SendInput 合成 **Ctrl+C**。dev 下二进制是 console 子系统
    // （main.rs 只在 release 才设 windows_subsystem="windows"），当前台是终端/conhost 时，
    // 合成按键会被转成 CTRL_C_EVENT，默认处理器以 STATUS_CONTROL_C_EXIT(0xc000013a) 直接杀进程
    // （随之出现的 WebView2 "Failed to unregister class Chrome_WidgetWin_0" 只是猝死时的
    // 无害反注册日志，不是根因）。
    // ★★ 第二层根治（2026-09-14 真机事故）：复制和弦已从 Ctrl+C 换成 **Ctrl+Insert**
    // （见 `capture::send_copy_chord`）。因为上面这个 HANDLER **只保护本进程** —— 同一个控制台里的
    // `pnpm` / `node`(vite) / `tauri` CLI 没有这层保护，会被同一次 CTRL_C_EVENT 打断，
    // 使 `pnpm tauri dev` 整棵树「无声消失」（无 panic.log / 无 WER / 无退出日志）。
    // 此 HANDLER 保留作**纵深防御**：万一将来有别的路径再合成 Ctrl+C，本进程至少不会自己打死自己。
    // release 为 GUI 子系统无控制台，此调用返回 Err 且无副作用。
    unsafe {
        let _ = windows::Win32::System::Console::SetConsoleCtrlHandler(None, true);
    }

    // ★ 单实例判定必须满足两个「早于」：
    //   1) 早于任何窗口 / WebView2 / 鼠标钩子 / 托盘创建（否则第二实例会装第二个 WH_MOUSE_LL、
    //      第二个托盘图标，并并发写 config.json 与 Run 注册表项）；
    //   2) 早于下面那份「清空并写入」poc2-state.log 的启动信标（否则第二实例会把首实例的
    //      登录态诊断日志抹掉）。
    if let Some(crate::single_instance::Acquire::Secondary) = crate::single_instance::acquire() {
        // ★★ 这条路径**必须留痕**（真机教训 2026-09-14）：「本进程是第二实例」在用户看来
        //   就是「窗口一闪、什么都没了」，而此前这里**一行日志都不写**、且发生在诊断日志
        //   初始化之前 → 既无 `panic.log`、也无 WER 记录、连 `d1-debug.log` 都没有 ——
        //   与「软件崩溃」**完全无法区分**（用户实测报障：「读取失败并软件崩溃」）。
        //   已知触发路径（本次已复现）：已有实例在跑时再次启动（含 `pnpm tauri dev` 重新拉起
        //   `target\debug\deepseek-float.exe`）—— 现场证据是旧实例占着 vite 5173 与 cargo
        //   构建锁，新实例退到 5174 并「启动后立刻退出」。
        //   `crate::diag` 是**纯文件 I/O**（惰性打开、不依赖 Tauri 运行时），此处调用安全
        //   —— 单实例模块自身在失败路径上也已在用它。
        crate::diag::warn(
            "single-instance: 检测到已有实例在运行 → 本进程（第二实例）已唤醒首实例并退出。             这是**设计行为、不是崩溃**：请确认是否重复启动了本程序（或 `tauri dev` 正在重启应用）。",
        );
        crate::single_instance::wake_primary(); // 让首实例把悬浮球显示出来
        std::process::exit(0);
    }

    // 启动信标：在任何 Tauri/WebView2 运行时初始化之前落盘。
    // 即便当前环境（headless / 非交互会话）无法初始化 WebView2 导致 setup() 不执行，
    // 也能确认二进制已启动、路线 A 代码路径已进入。真正的登录态由 setup() 内的
    // on_navigation/on_page_load 写入同一文件（会先清空再写 setup 标记）。
    // ★ 带上版本指纹 + 时间戳：用户报障时可据此溯源产物（构建号/打包档位），
    //   否则一根日志行无法判断他跑的是哪个版本的 exe。
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let dir = std::path::Path::new(&local).join("com.deepseekfloat.desktop");
            let _ = std::fs::create_dir_all(&dir);
            let ts = crate::diag::now_rfc3339();
            let line = format!(
                "[M1] launcher started v{}{} at {ts} (route A: embedded persistent login + ball/panel)\n",
                env!("CARGO_PKG_VERSION"),
                if cfg!(not(feature = "custom-protocol")) || cfg!(debug_assertions) {
                    " [dev]"
                } else {
                    ""
                },
            );
            let _ = std::fs::write(dir.join("poc2-state.log"), line);
        }
    }

    // 全局 panic hook：把 panic 信息 + backtrace 落盘 panic.log。
    // 点击崩溃多为 WebView2 原生层 abort（Tauri v2 不隔离 webview 进程 → 宿主直接 exit -1），
    // 这种不会走 Rust 的 Result/if-let，只会静默终止进程；此 hook 便于在用户桌面捕获真实根因。
    std::panic::set_hook(Box::new(|info| {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let dir = std::path::Path::new(&local).join("com.deepseekfloat.desktop");
            let _ = std::fs::create_dir_all(&dir);
            let bt = std::backtrace::Backtrace::force_capture();
            let msg = format!(
                "[PANIC] {}\nlocation: {:?}\nbacktrace:\n{}\n",
                info,
                info.location(),
                bt
            );
            eprintln!("{}", msg);
            let _ = std::fs::write(dir.join("panic.log"), msg);
            // O1：另写结构化崩溃摘要（最近事件 + 栈），便于用户/agent 快速定位根因。
            let loc = info.location().map(|l| l.to_string());
            crate::diag::crash_summary(&format!("{info}"), loc.as_deref(), &format!("{bt}"));
        }
    }));

    Builder::default()
        .setup(|app| {
            // Recover an interrupted multi-file restore before any config or prompt cache is read.
            crate::backup::recover_interrupted_restore().map_err(std::io::Error::other)?;

            // ---- R17-P1：release 也提供诊断通道（档位约定见 `diag.rs` 模块头）----
            //  · debug：恒开，Info 级（与历史行为一致）；
            //  · release：默认**不初始化** —— 仅在 `DSFLOAT_DIAG` 置位时开，且降为 Warn：
            //    只收 Tauri / wry / WebView2 侧的异常，不被依赖库的 INFO 刷屏。
            //  为什么之前只给 debug：release 用户线程里不需要库日志，且能省体积。但代价是
            //  用户报障时我们**一无所有** —— R1–R15 的真机缺陷全靠日志定位，故改为「可选开」。
            //  与 `diag` 的分工：`diag` 记我方业务里程碑（d1-debug.log，带相对时间戳与序号），
            //  plugin-log 记依赖与运行时异常（LogDir / stdout），两者互补、互不覆盖。
            if crate::diag::enabled() {
                let plugin_level = if cfg!(debug_assertions) {
                    LevelFilter::Info
                } else {
                    LevelFilter::Warn
                };
                if let Err(e) = app
                    .handle()
                    .plugin(LogBuilder::default().level(plugin_level).build())
                {
                    crate::diag::warn(&format!("[M1] log plugin init failed (non-fatal): {e}"));
                }
            }

            // R17-P1：把「诊断档位」写进日志 —— 用户报障时先看这一行，就知道手上这份档案
            // 记了什么（`Meta` 档正文已隐去，只留长度与哈希）。
            crate::diag::log(&format!(
                "[diag] 档位={:?}（Meta=正文隐去｜Full=含正文；由环境变量 DSFLOAT_DIAG 或同目录 diag.flag 标记文件控制，见 diag.rs 模块头）",
                crate::diag::level()
            ));

            // ---- 路线 A 登录态落盘日志（面板窗口 = 内嵌 DeepSeek 网页端）----
            let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
            let diag_path = std::path::Path::new(&local)
                .join("com.deepseekfloat.desktop")
                .join("poc2-state.log");
            let _ =
                std::fs::create_dir_all(diag_path.parent().unwrap_or(std::path::Path::new(".")));
            let _ = std::fs::write(&diag_path, "[M1] setup entered; route A active\n"); // 清空并写入 setup 标记
            let sink: Arc<Mutex<Option<File>>> = Arc::new(Mutex::new(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&diag_path)
                    .ok(),
            ));
            crate::diag::log(&format!("[M1] state_log={}", diag_path.display()));

            // =====================================================================
            // ★ (#004-B) 悬浮球显形路径**前移**：消掉「启动 ~4.6s 才见球」
            // ---------------------------------------------------------------------
            // 真机证据（2026-09-26 release 日志毫秒线）：`ball_ready` 在 +676ms（球页首帧已画好），
            // 但 `ball_show` 拖到 +4602ms —— 原因**不是握手**，而是本显形闸门原排在 5 个窗口创建
            // **之后**（panel +0.8s / popup +2.75s / settings +3.27s / shot +3.77s / result +4.43s），
            // 闸门 +4.52s 才就绪（握手信号早已 latch ⇒ 净开销≈2ms）。
            // ⇒ 把这三步作为**一个整体**提到闭包最开头（先于 setup_panel / Hotkey / 其余窗口创建）：
            //   球窗由 `tauri.conf.json` 在 `setup()` **之前**创建（见下方 setup_panel 前的注释），
            //   故此刻 ball 已存在；三者**只碰 ball 窗口**，不依赖其它窗口已创建 ——
            //     · `ballstyle::fix`          仅 `get_webview_window("ball")`；
            //     · `setup_ball_position`      仅 ball 窗口 + 显示器枚举（`monitor_from_point`）；
            //     · `windows::gate_ball_show`  仅 ball 窗口 + 就绪闸门 + 后台 thread。
            //   预期：闸门 ~+100ms 就绪 → `ball_show` 落在 ~+700ms。
            //   ★ 组内顺序**不可颠倒**：先清标题栏位 → 再按尺寸做边界夹取 → 最后套椭圆区域显形。
            // =====================================================================

            // ---- 悬浮球窗口样式修正：清掉 Tauri 未移除的 WS_CAPTION ----
            // 否则 Windows 按 SM_CXMINTRACK(本机 136) 把 64px 宽的球窗夹成 136px 宽，
            // 球的右侧会多出 72px 宽的不可见区域吞掉鼠标点击（真机确认）。
            // ★ 必须在 setup_ball_position 之前 —— 后者用窗口尺寸做边界夹取。
            // ★★ 2026-09-25 第五次修复：真机日志证实「启动期清位成功、运行期从未被恢复」，
            //    说明残影不在运行期，而在**启动窗口期** —— `fix()` 早先要到 +1370ms 才跑，而球窗
            //    在它之前就已创建，于是启动期球窗带着 WS_CAPTION（且被 SM_CXMINTRACK 夹成 136px 宽），
            //    其文字即用户所报「一闪即逝的背景」。⇒ 球窗**隐藏态创建**（`tauri.conf.json` ball
            //    `visible:false`），先 `fix()` 清位、再显形，启动期不再有任何带标题栏的帧。
            //    ★ #004-B：本步现与位置记忆 / 显形闸门一起**前移**到此处，使显形不再被后续窗口拖到 +4.6s。
            crate::ballstyle::fix(app.handle());

            // ---- §7 位置记忆：恢复球位置 + 挂载拖动记忆 ----
            crate::panels::setup_ball_position(app.handle())?;

            // ---- ★ 球窗显形（必须在 `fix()` 之后；球窗以隐藏态创建）----
            // 显形前的这一小段时间窗口内，球窗从未被绘制过 ⇒ 不可能闪现标题栏。
            // ★ #7：改为**首帧就绪握手闸门** —— 不再「创建后立即显形」，而是先维持零面积空区域
            //    并等待 `poc/ball.html` 在首个渲染帧后经 `ipc::ball_ready` 送来的就绪信号
            //    （≤3s 兜底超时），之后才套椭圆区域显形，从根上消除「启动瞬间球空白」。
            //    详见 `windows::gate_ball_show`。
            crate::panels::gate_ball_show(app.handle());

            // ---- 面板窗口（路线 A：内嵌 DeepSeek 网页端，默认隐藏，经球态/托盘/划词唤出）----
            // 默认隐藏但 webview 仍会加载（会话在隐藏期间保持），唤出时无需重载 → 持久化登录不丢。
            //
            // ⚠️ 严禁给面板单独设置 additional_browser_args（例如 --disable-gpu）——这会直接导致面板创建失败：
            //   1) tauri.conf.json 中的 ball 窗口先于本 setup() 创建（tauri::app::setup 源码顺序：
            //      先 WebviewWindowBuilder::from_config(...).build() 建 config 窗口，再执行 setup hook）；
            //   2) ball 已用「默认 browser args」在共享的 WebView2 user data 目录上创建 CoreWebView2Environment；
            //   3) WebView2 规则：同一 user data 目录上的多个 Environment 必须 CoreWebView2EnvironmentOptions 一致，
            //      否则第二次 CreateCoreWebView2EnvironmentWithOptions 失败（wry::WebContext::new 文档明确警告，
            //      失败 HRESULT = 0x8007139F / ERROR_INVALID_STATE）；
            //   4) ball 与 panel 的 data_directory 均为 None → 同一个目录 → 面板若带 --disable-gpu 必然创建失败
            //      → get_webview_window("panel") 返回 None → 左键提示「面板窗口未创建」。
            // 结论：ball / panel 必须共用同一套默认参数，才能共享 WebView2 环境与持久化登录 cookie。
            // 若日后仍遇 WebView2 窗口类反注册 abort（Error 1411/1412，仅见于退出/销毁路径），
            // 正确解法是「单 webview 形变架构」（一个 webview 在 64×64 与 440×600 间 resize + 切内容），
            // 而不是给某个 webview 加 browser args 造成环境不一致。
            // 面板窗口（路线 A）：抽出为 setup_panel（见 windows 模块），与内联逻辑完全一致，
            // 仅把 builder 搬进职责单一的函数，改善可读性。
            crate::panels::setup_panel(app.handle(), &sink)?;

            // 球态 ↔ 面板态切换、右键菜单、工具条动作、设置保存/运行，均已改为
            // 类型化 #[tauri::command]（见 ipc 模块），不再走字符串事件。
            // 球态右键经 ball_contextmenu 命令打开自绘菜单，首次使用懒建；托盘仍用原生菜单。

            // ---- 系统托盘（W2）----
            crate::tray::setup_tray(app.handle())?;
            // WebView2 禁止从同步事件回调懒建窗口；右键只更新并显示常驻菜单。
            crate::context_menu::ensure(app.handle());

            // ---- R13-S4：截图全局热键（决策 D1：默认 `Ctrl+Alt+1`，设置页可改）----
            // ★ 注册失败**不阻塞启动**（只落 `diag`）：托盘与右键菜单的「截图提问」始终可用。
            //   回调只发信号，真正的截图流程在 hotkey 模块的 worker 线程上跑（同 mousehook 纪律）。
            {
                let hk_app = app.handle().clone();
                let spec = crate::config::load().hotkey;
                let ready = crate::hotkey::start(
                    move || {
                        crate::screenshot::begin_with_mode(&hk_app, crate::screenshot::ShotMode::Ask);
                    },
                    spec.as_deref(),
                );
                crate::diag::log(&format!(
                    "hotkey: 初始化完成（泵就绪={ready}，配置 spec={:?}，实际生效={:?}）",
                    spec,
                    crate::hotkey::current()
                ));
            }

            // ---- D1 划词工具条：常驻创建 + 事件监听 + 全局鼠标钩子（Task #9 核心链路）----
            crate::panels::setup_popup_and_mousehook(app.handle())?;

            // ---- 「截图提问」S1：常驻创建第 5 个窗口「遮罩」（visible=false，只 show/hide）----
            crate::screenshot::ensure(app.handle());

            // ---- 「结果回流」R2：常驻创建第 6 个窗口「结果卡」（visible=false，只 show/hide）----
            // 由 R1 回读的交付点触发（`readback::deliver` 是唯一交付点），setup 里只创建不显示。
            // 放在这里（而非更晚）是为了让后续步骤看到的窗口集合尽早稳定（结果卡窗口一经创建即常驻、
            // 只 show/hide，供预热与回读交付点直接复用）。
            // ★ #004-B 校正：原注释称「让 `ballstyle::fix` / 位置记忆看到稳定窗口集合」，但二者已
            //   **前移**至闭包最开头、且**只碰 ball 窗口**，与此处的窗口集合无关；该理由不再成立。
            crate::result::ensure(app.handle());

            // ---- ★ R6 修复①/②：启动即后台预热面板（隐藏态导航至 DeepSeek）----
            // 用户真机反馈：①「第一次使用结果卡片启动很慢」②「工具条窗口需要提前打开一次面板」——
            // 两者同源：结果卡链路依赖隐藏面板已加载 DeepSeek，而面板刻意用 about:blank 创建、
            // 此前要等用户首次走结果卡时才懒跳。把这次加载**前移**到启动期，首次划词时面板早已就绪，
            // 卡片立即拿到回答；「先手动开一次面板」的临时 workaround 不再需要。
            // 面板保持隐藏（本函数只 eval 导航、绝不 show），对用户不可见；若尚未登录，
            // 页面停在 /sign_in，与「手动开面板」表现一致（仍需登录一次）。
            {
                let warm = app.handle().clone();
                std::thread::spawn(move || {
                    // S2：后台线程启动标记 —— 崩溃取证第一线索（线程何时起、何时止）。
                    crate::diag::log("[warmup] 后台预热线程启动");
                    // 稍候，避免与首屏窗口创建抢占 WebView2 资源。
                    std::thread::sleep(Duration::from_millis(
                        crate::panels::PANEL_WARMUP_DELAY_MS,
                    ));
                    match crate::panels::ensure_panel_loaded(&warm) {
                        Some(_) => crate::diag::log(
                            "[warmup] 面板已后台预热（隐藏态导航至 DeepSeek，结果卡首用不再等待加载）",
                        ),
                        None => crate::diag::warn("[warmup] 面板窗口未创建，跳过预热"),
                    }
                    // S2：线程结束标记（与启动成对，便于判断是否"预热线程卡死/提前退出"）。
                    crate::diag::log("[warmup] 后台预热线程结束");
                });
            }

            // ---- （悬浮球 `fix` / 位置记忆 / 显形闸门 已**前移**至闭包最开头，见上方 #004-B）----

            // ---- §8 开机自启：与注册表**真实状态**对账 ----
            // ★★ 2026-09-24 真机缺陷（「开机自启后悬浮球显示错乱」）后的两处关键修正：
            //   ① **只有能独立运行的构建**（release + custom-protocol）才有资格被登记为自启；
            //      dev 构建被登记后，开机时 vite dev server 不在 → 球窗加载 devUrl 失败 →
            //      WebView2 连接错误页（白底 + 巨字 + 原生滚动条）。详见 `autostart.rs` 模块头。
            //   ② 旧逻辑在「配置为开 && 注册表有值」时**无条件** `set_autostart(true)` 重写 exe 路径
            //      （本意是升级自愈），于是**反复把 dev 构建路径写回注册表** —— 用户即使手动改也会被
            //      覆盖回去。故 dev 构建下**不再重写**；若发现一条指向本开发产物的脏项则**主动清理**。
            if crate::autostart::registration_supported() {
                // 可独立运行的构建：沿用原对账逻辑 ——
                // 配置为开：一致 → 重写一次以刷新 exe 路径（升级 / 移动安装目录后自愈）；
                // 配置为开但注册表中已不存在（用户已在任务管理器 / 系统设置里禁用）→
                // 尊重用户意图，回落为关闭，**不再**自动写回（否则会静默撤销用户的禁用）。
                if crate::config::load().autostart {
                    if crate::autostart::is_autostart() {
                        if crate::autostart::set_autostart(true) {
                            crate::diag::log("开机自启：与注册表一致，已刷新写入 exe 路径");
                        } else {
                            crate::diag::warn("开机自启：与注册表一致，但刷新写入失败");
                        }
                    } else {
                        crate::config::update(|c| c.autostart = false);
                        crate::diag::log("开机自启：注册表项已被外部移除，尊重用户意图回落为关闭");
                    }
                }
            } else {
                // 开发构建：**不支持**开机自启。
                match crate::autostart::dirty_self_registration() {
                    // ① 注册表里有一条**指向本开发产物**的脏项 → 主动清理（它注定失效，且旧逻辑会反复写回）。
                    Some(target) => {
                        let removed = crate::autostart::set_autostart(false);
                        crate::config::update(|c| c.autostart = false);
                        crate::diag::warn(&format!(
                            "开机自启：当前为开发构建、无法独立运行，不支持开机自启；\
                             已清理指向本开发产物的脏注册表项（{target}，删除{}）。\
                             如需开机自启，请改用 release 版（`pnpm release`）并在设置页开启。",
                            if removed {
                                "成功"
                            } else {
                                "失败 —— 请在「任务管理器 → 启动」里禁用它"
                            }
                        ));
                    }
                    // ② 无脏项：注册表里没有该项，或有但指向**别处**（如另一份 release 安装）→ 一律不改动，
                    //    仅在「配置为开」时落一条说明（避免用户以为 dev 下开启过、实际不生效又无提示）。
                    None => {
                        if crate::config::load().autostart {
                            crate::diag::warn(
                                "开机自启：当前为开发构建，已跳过自启对账（不写回注册表）；\
                                 如需开机自启，请改用 release 版（`pnpm release`）。",
                            );
                        }
                    }
                }
            }

            // ---- §9 全屏隐藏：启动后台轮询线程（实时读取 config.hide_on_fullscreen）----
            crate::fullscreen::start(app.handle().clone());

            // ---- 诊断：球态 / 面板态 窗口存在性（抽出为 setup_diag_windows）----
            crate::panels::setup_diag_windows(app.handle());

            // ---- §11 单实例：首实例起等待线程，被第二实例唤醒时显示悬浮球（此时球窗口已存在）----
            // ★ 启动即落盘进程号（崩溃取证的**第一路证据**）：`d1-debug.log` 是**追加**写入的
            //   多个进程生命周期，一旦出现 pid 变化、或 `+xxms` 计数**回退**，就证明「发生过重启」——
            //   与 `run: Exit*`（优雅退出）缺失互补：两者一起才能区分
            //   「原生 abort / 被外部终止（如 dev 文件监视器重启）」与「正常退出」。
            let pid = std::process::id();
            crate::diag::log(&format!("[boot] pid={pid}"));

            crate::single_instance::watch(app.handle().clone());

            // ---- ★ 调试构建专用自检入口（`DSFLOAT_SELFTEST=<秒>`；release 下无此函数）----
            #[cfg(debug_assertions)]
            setup_selftest(app.handle().clone());

            // ---- ★ 稳定性压测入口（`DSFLOAT_SELFTEST_STRESS=<轮数>`；release 下无此函数）----
            #[cfg(debug_assertions)]
            setup_stress(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::ipc::toggle_panel,
            crate::ipc::ball_contextmenu,
            crate::ipc::context_menu_ready,
            crate::ipc::context_menu_switch_site,
            crate::ipc::context_menu_action,
            crate::ipc::context_menu_close,
            crate::ipc::ball_ready,
            crate::ipc::popup_action,
            crate::ipc::settings_save,
            crate::ipc::settings_run,
            crate::ipc::config_get,
            crate::ipc::autostart_supported,
            crate::ipc::config_set_autostart,
            crate::ipc::config_set_fullscreen,
            crate::ipc::config_set_privileged,
            crate::ipc::config_set_clipboard_fallback,
            crate::ipc::config_set_panel_url,
            crate::ipc::panel_sites_save,
            crate::ipc::set_hotkey,
            crate::ipc::hotkey_status,
            crate::ipc::shot_commit,
            crate::ipc::shot_cancel,
            crate::ipc::shot_preview_ready,
            crate::ipc::shot_confirm,
            crate::ipc::shot_reselect,
            crate::ipc::result_favorite_save,
            crate::ipc::result_export,
            crate::ipc::favorites_list,
            crate::ipc::favorites_delete,
            crate::ipc::favorites_export,
            crate::ipc::backup_export,
            crate::ipc::backup_import_preview,
            crate::ipc::backup_import_cancel,
            crate::ipc::backup_restore,
            crate::ipc::result_open_exports_dir,
            crate::ipc::result_close,
            crate::ipc::result_resume,
            crate::ipc::result_retry_send,
            crate::ipc::result_copy,
            crate::ipc::result_ask,
            crate::ipc::result_drag_start,
            crate::ipc::result_resize,
            crate::ipc::result_new_session,
            crate::ipc::result_open_external
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // 退出前清理鼠标钩子（泵线程收 WM_QUIT 后自动 Unhook）。
            // ★ 退出事件同时落盘（崩溃取证）：进程若静默消失（WebView2 原生 abort 不经 panic hook），
            //   日志里**缺少**这两行即证明「不是优雅退出」，而是原生 abort / 被外部终止 ——
            //   这是与 `panic.log` 互补的第二路证据。
            match event {
                RunEvent::ExitRequested { .. } => {
                    crate::diag::log("run: ExitRequested（优雅退出路径）");
                    // 标记进程退出：让在途的回读 worker 线程尽快结束，
                    // 不再对已析构的 panel 窗口继续 eval（见 readback::mark_exiting）。
                    crate::readback::mark_exiting();
                    crate::mousehook::stop();
                    // 热键泵线程同样收 WM_QUIT（注销必须发生在注册它的线程上，见 hotkey.rs）。
                    crate::hotkey::shutdown();
                    crate::panels::flush_pending_pos(); // 优雅退出路径同样落盘最后一次拖动位置
                }
                RunEvent::Exit => {
                    crate::diag::log("run: Exit（事件循环已结束）");
                }
                _ => {}
            }
        });
}

/// ★ P1 修复（2026-09-25 审计）：退出前的统一清理 + 取证日志。
///
/// 此前托盘「退出」直接 `process::exit(0)`，**绕过**事件循环的 `RunEvent::ExitRequested` 清理
/// （mark_exiting / mousehook::stop / hotkey::shutdown / flush_pending_pos），且**不写任何日志**
/// → 用户主动退出在 `d1-debug.log` 中**与** WebView2 原生 abort 无法区分（崩溃取证第二路失效）。
///
/// 抽出本函数：在硬退出前显式执行与优雅退出等价的清理并落盘日志，使「主动退出」可被取证识别。
/// （第二实例退出发生在 Tauri 应用创建前、无 app/hook，取证由 `run()` 内既有的 `diag::warn` 承担，
/// 故不在此调用。）
pub(crate) fn prepare_exit() {
    crate::diag::log(
        "run: ExitRequested（硬退出路径，等价于优雅路径——主动退出，非 WebView2 abort）",
    );
    crate::readback::mark_exiting();
    crate::mousehook::stop();
    crate::hotkey::shutdown();
    crate::panels::flush_pending_pos();
}

/// ★ 调试构建专用自检入口（**release 下不存在** —— `#[cfg(debug_assertions)]` 门控）：
/// `DSFLOAT_SELFTEST=<秒>` 启动 N 秒后**自动**执行一次托盘「诊断：发送并回读一轮」。
///
/// 两个用途：
///   1. 免手工点托盘即可复现 / 验证 R1 的 W-V3 / W-V4 / W-V5（以及 R11 的隐藏面板态）；
///   2. **崩溃取证**：WebView2 原生 abort 不走 Rust panic hook（`panic.log` 会是空的），
///      此时「心跳停在第几行」+「有没有 `run: Exit*` 行」是判定「谁死的、怎么死的」的唯一线索：
///      心跳继续、只是回读日志停了 → 死的是回读线程（进程还活着）；
///      心跳与所有日志一起停、且没有 `run: Exit*` → 整个进程被原生 abort / 外部终止。
#[cfg(debug_assertions)]
fn setup_selftest(app: tauri::AppHandle) {
    let Ok(raw) = std::env::var("DSFLOAT_SELFTEST") else {
        return;
    };
    let Ok(delay) = raw.trim().parse::<u64>() else {
        crate::diag::warn(&format!(
            "[selftest] DSFLOAT_SELFTEST 不是秒数：{raw:?}，已忽略"
        ));
        return;
    };
    let show_first = std::env::var("DSFLOAT_SELFTEST_SHOW").as_deref() == Ok("1");
    crate::diag::log(&format!(
        "[selftest] 已装载：{delay}s 后自动执行一次回读诊断（先展开面板以加载远端页={show_first}）"
    ));
    std::thread::spawn(move || {
        // S2：后台自检线程启动标记（debug-only；与 warmup 同等崩溃取证价值）。
        crate::diag::log("[selftest] 后台自检线程启动");
        // ★ `DSFLOAT_SELFTEST_SHOW=1`：先按**用户路径**展开面板（`show_panel` = show + 懒跳
        //   DeepSeek）把远端页加载起来，等就绪后**再收起**，然后才跑诊断。
        //
        //   为什么需要它：面板刻意用 `about:blank` + 展示时懒跳（见 show_panel 注释），
        //   所以「面板从未展开过」时页面里没有 `__DSFLOAT__` → 回读只能报 `未注入`
        //   → W-V13（「先收起面板再触发」）在自动化下**永远拿不到现场**。
        //   注意语义：W-V13 要求的是「**曾展开过、然后收起**」，不是「从未展开」——
        //   本前置正是照此构造（先展开加载 → 收起 → 隐藏态下发诊断）。
        if std::env::var("DSFLOAT_SELFTEST_SHOW").as_deref() == Ok("1") {
            crate::diag::log("[selftest] 先展开面板以加载 DeepSeek（随后收起，模拟 W-V13 路径）");
            crate::panels::show_panel(&app);
            std::thread::sleep(Duration::from_secs(15)); // 等远端页加载 + 注入脚本就位
            if let Some(p) = app.get_webview_window("panel") {
                let _ = p.hide();
                crate::diag::log(&format!(
                    "[selftest] 面板已收起（is_visible={:?}）——此后回读应在隐藏态完成",
                    p.is_visible()
                ));
            }
            std::thread::sleep(Duration::from_secs(3)); // 让收起后的布局稳定
        }
        std::thread::sleep(Duration::from_secs(delay));
        // ★ R13-S5：探针分发 —— `DSFLOAT_SELFTEST_PROBE=upload|conversation` 时**只跑探针**，
        //   不发测试消息。三个排障探针从托盘菜单撤下后，这里是它们的**唯一入口**：
        //   用户可见菜单不再有被误触的风险（尤其「发送并回读一轮」会真往会话里发消息），
        //   而需要在「站点 DOM 变了」时定位上传入口 / 会话容器时，带上环境变量即可复现。
        match std::env::var("DSFLOAT_SELFTEST_PROBE").as_deref() {
            Ok("upload") => {
                crate::diag::log("[selftest] 仅触发 inspect_upload（不发测试消息）");
                crate::inject::inspect_upload(&app);
                return;
            }
            Ok("conversation") => {
                crate::diag::log("[selftest] 仅触发 inspect_conversation（不发测试消息）");
                crate::inject::inspect_conversation(&app);
                return;
            }
            _ => {}
        }
        crate::diag::log("[selftest] 触发 begin_watch_diagnostic");
        crate::readback::begin_watch_diagnostic(&app);
        // 心跳：60 次 × 2s = 120s，足够覆盖「回读 60s 超时降级」与「中途猝死」两种情况。
        for i in 1..=60u32 {
            std::thread::sleep(Duration::from_secs(2));
            crate::diag::log(&format!("[selftest] heartbeat #{i}（进程存活）"));
        }
        crate::diag::log("[selftest] 心跳结束");
        // S2：自检线程结束标记。
        crate::diag::log("[selftest] 后台自检线程结束");
    });
}

/// ★ R15-C2：稳定性压测（`DSFLOAT_SELFTEST_STRESS=<每窗轮数>`）。
///
/// 判据来自 `docs/m3-single-webview-plan.md:45` —— 反复 show/hide 各常驻窗后，
/// 不得出现 `exit -1` / `Error 1411/1412` / 新增 `panic.log`。
///
/// ★ 为什么要真的跑：**M3（单 webview 形变）能否「冻结」，唯一依据就是这份压测**，
///   而它自 2026-09-13 立项起**一直没被执行过** —— 在没有压测的情况下，
///   「没复现」不构成结论，只是一句推测。
///
/// 覆盖范围与边界（先说清它测不到什么）：
/// - **覆盖**：进程内的窗生命周期 —— `show()/hide()` 各 N 轮 × 5 个常驻功能窗，
///   这是 1411/1412 的历史现场；
/// - **不覆盖**：跨进程的反复冷启/退出 —— 由**外部循环多次启动本程序**完成
///   （每次冷启都会重新创建 6 个 webview、再走一次退出路径）。
///
/// `DSFLOAT_SELFTEST_STRESS_EXIT=graceful` ⇒ 结束后走 `app.exit(0)`
/// （触发 `RunEvent::ExitRequested` + **真实的 webview 析构**，正是历史上出
/// 1411/1412 的那条路径）；默认走**生产路径**（`std::process::exit(0)`，跳过析构，
/// 与托盘「退出」一致）。两条都跑，才能区分「问题是析构引起的」还是「别处引起的」。
#[cfg(debug_assertions)]
fn setup_stress(app: tauri::AppHandle) {
    let Ok(raw) = std::env::var("DSFLOAT_SELFTEST_STRESS") else {
        return;
    };
    let Ok(cycles) = raw.trim().parse::<u32>() else {
        crate::diag::warn(&format!(
            "[stress] DSFLOAT_SELFTEST_STRESS 不是轮数：{raw:?}，已忽略"
        ));
        return;
    };
    let graceful = std::env::var("DSFLOAT_SELFTEST_STRESS_EXIT").as_deref() == Ok("graceful");
    // 5 个常驻功能窗。`ball` 刻意不参与：它本就常显（show/hide 语义与其他窗不同），
    // 且它是用户直接看着的窗口 —— 反复闪它只增加干扰、不增加信息。
    const WINDOWS: [&str; 5] = ["panel", "settings", "popup", "shot", "result"];
    crate::diag::log(&format!(
        "[stress] 已装载：{cycles} 轮 × {} 窗 show/hide；退出方式={}",
        WINDOWS.len(),
        if graceful {
            "graceful(app.exit，含真实析构)"
        } else {
            "hard(process::exit，生产路径)"
        }
    ));
    std::thread::spawn(move || {
        // S2：后台压测线程启动标记（debug-only）。
        crate::diag::log("[stress] 后台压测线程启动");
        // 等 setup 全部完成 + WebView2 出首帧：过早 show/hide 测的是「半初始化状态」，
        // 那不是现场（现场是「用户在使用中反复开关面板」）。
        std::thread::sleep(Duration::from_secs(3));
        let mut ops = 0u32;
        let mut fails = 0u32;
        let mut missing = 0u32;
        for round in 1..=cycles {
            for label in WINDOWS {
                let Some(w) = app.get_webview_window(label) else {
                    missing += 1;
                    continue;
                };
                if let Err(e) = w.show() {
                    fails += 1;
                    crate::diag::warn(&format!("[stress] {label} show 失败: {e}"));
                }
                ops += 1;
                std::thread::sleep(Duration::from_millis(25));
                if let Err(e) = w.hide() {
                    fails += 1;
                    crate::diag::warn(&format!("[stress] {label} hide 失败: {e}"));
                }
                ops += 1;
                std::thread::sleep(Duration::from_millis(15));
            }
            if round % 10 == 0 || round == cycles {
                crate::diag::log(&format!(
                    "[stress] 进度 {round}/{cycles} 轮（累计 {ops} 次 show/hide · 失败 {fails} · 缺窗 {missing}）"
                ));
            }
        }
        // ★ 判定行：事后只要 grep 这一行 + 看进程退出码，就能给 M3 一个**结论**（而不是推测）。
        crate::diag::log(&format!(
            "[stress] **完成**（{cycles} 轮 × {} 窗 · {ops} 次 show/hide · 失败 {fails} · 缺窗 {missing}）\
             ⇒ 判据：无 1411/1412、无 panic.log 新增、退出码 0",
            WINDOWS.len()
        ));
        // S2：压测线程结束标记（与启动成对）。
        crate::diag::log("[stress] 后台压测线程结束");
        std::thread::sleep(Duration::from_millis(300));
        crate::panels::flush_pending_pos();
        if graceful {
            crate::diag::log("[stress] 走 graceful 退出（app.exit(0)，含真实 webview 析构）");
            app.exit(0);
        } else {
            crate::diag::log("[stress] 走 hard 退出（process::exit(0)，与托盘「退出」一致）");
            std::process::exit(0);
        }
    });
}
