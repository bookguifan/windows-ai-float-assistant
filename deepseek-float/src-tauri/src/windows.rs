//! 窗口生命周期（面板 / 悬浮球位置记忆 / 划词常驻窗 + 鼠标钩子）。
//!
//! 从 `lib.rs` 拆出：仅包含窗口创建、显隐、位置记忆等职责单一的代码，改善 god-file 可读性。
//! 对外公开的项由 `lib.rs` 统一 `pub(crate) use windows::*` 再导出，故既有 `crate::show_panel`
//! 等调用点无需改动。

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::error::AppResult;

// P0 修复（2026-09-25 审计）：双层冲刷用到的 Win32 API（令 DWM 丢弃旧合成表面并立即重绘）。
// 注：`RedrawWindow` 与 `RDW_*` 常量在 windows 0.61 位于 `Graphics::Gdi`（非 `WindowsAndMessaging`）。
use windows::Win32::Graphics::Gdi::{RedrawWindow, RDW_INVALIDATE, RDW_UPDATENOW};
// 球窗显形专用：原生 `ShowWindow` 只补 `WS_VISIBLE`，**不经 tao** ⇒ 不会触发 tao 用其缓存的
// window-flags 重建 `GWL_STYLE`（那会把 `ballstyle::fix` 清掉的 `WS_CAPTION` 放回来）。
// ★ 2026-10-13（F4）：新增 `IsWindowVisible` —— `ball_show` 末尾用它打印**实测**可见性，
//    取代此前无条件写「球页恢复可见」的**谎报日志**（真机上球可能已被 tao 隐藏）。
use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, ShowWindow, SW_SHOWNOACTIVATE};

// PoC-2 / M1 登录态判断（路线 A：内嵌持久化登录）—— 基于 URL 路径：
//   - 路径含 /sign_in        → 未登录（需用户在窗口里手动登录，可能伴随数美风控图形验证）
//   - 否则（host 为当前配置的面板 host）→ 已登录（或加载中）
// Tauri v2 的 WebView data 目录默认按 app 持久化，登录后的 session cookie 跨重启保留，
// 「内嵌持久化登录」天然成立，无需手动抓/注入 cookie（session 为 HttpOnly，JS 也读不到）。
//
// ★ 2026-09-17 起对「域名」通用：原本 `url.contains("chat.deepseek.com")` 会随面板切到
//   `config::panel_url()` 指定的镜像/自托管而失效 ⇒ 改为与配置的 panel host 比对。
pub(crate) fn panel_state(url: &str) -> &'static str {
    // ★ 审核 P2 收敛：登录页判定统一走 `config::url_is_sign_in`（路径段匹配，去 query/fragment），
    //   与 `readback::url_needs_login` 同一判定源，杜绝 `/sign_in` 子串误判正常提问。
    if crate::config::url_is_sign_in(url) {
        "NEEDS_LOGIN"
    } else if crate::config::host_of(url) == crate::config::panel_host() {
        "LOGGED_IN"
    } else {
        "UNKNOWN"
    }
}

/// 记录用 URL 脱敏：只保留 host+path，丢弃 query / fragment。
///
/// DeepSeek 页面导航 URL 可能携带一次性 token/参数（query 或 fragment 均可），
/// 若整段落进诊断日志会明文泄露。`panel_state` / 登录判定仍用完整 URL（其对 query/fragment
/// 免疫），仅日志层脱敏。
///
/// ★ 2026-09-20 修正：原实现只按 `?` 截断，**未处理纯 fragment 形式**
/// （`https://x.com/a#token=...` 无 query 时 fragment 会原样泄露）。现同时截断 `?` 与 `#`。
pub(crate) fn redact_url(url: &str) -> String {
    // 取第一个 query(`?`) 或 fragment(`#`) 之前的部分；两者任一都可出现在 URL 末尾携带敏感参数。
    let cut = url.find(['?', '#']).unwrap_or(url.len());
    url[..cut].to_string()
}

// 把一行诊断写入落盘文件（sink 为 Arc<Mutex<Option<File>>>，打开失败则静默跳过）。
fn diag_write(sink: &Arc<Mutex<Option<std::fs::File>>>, line: &str) {
    if let Ok(mut g) = sink.lock() {
        if let Some(f) = g.as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

/// ★ 只保证「面板页已加载 DeepSeek」，**绝不显示窗口** —— 结果卡链路专用。
///
/// 面板窗口刻意用 `about:blank` 创建，由本函数（或在 `run()` 启动期的预热线程里）触发懒跳
/// DeepSeek（见 `create_panel`）。**自 R6 起，预热改在应用启动期完成**（见 `run()` 内的
/// `[warmup]` 线程），故正常使用时面板早已加载好；此处仍作为**兜底**：
/// 万一预热未跑（如极早期启动即划词），懒跳会在首次结果卡链路里补上 —— 只是那一次会偏慢。
///
/// 隐藏态下 WebView2 照样执行脚本与导航（W-V13 / R11 已实证），故这里**不需要** show()。
pub(crate) fn ensure_panel_loaded<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    let panel = app.get_webview_window("panel")?;
    // ★ 2026-09-17：不再「只要不是 chat.deepseek.com 就强制跳回」—— 那会**主动打回**用户
    //   配置的镜像/自托管网址。改为**仅初次/空白页**才跳到 `config::panel_url()`（配置可切换）；
    //   面板已在目标页（含用户面板内手动导航、登录跳转）就保持不动。
    let url = crate::config::panel_url();
    // ★ 审核 P2：URL 必须经 JSON 转义后再拼进 JS 字面量。`config::panel_url()` 在
    //   `config_set_panel_url` 命令路径有 http(s)+host 校验，但本地 config.json 的**读盘路径
    //   （`config::load`）不校验** —— 若文件被外部篡改为含引号/反斜杠/换行的串，裸拼接可逃逸
    //   出这段 eval，构成 panel 域内的任意脚本注入。转义后最坏只能导航到「恶意 URL」，不再
    //   是 JS 指令注入；且 `format!` 只解析格式串常量里的 `{lit}` 占位位，URL 值本身即使含
    //   `{`/`}` 也只是普通参数值，无法破坏解析、不会 panic。
    let lit = serde_json::to_string(&url).unwrap_or_else(|_| "\"\"".to_string());
    let _ = panel.eval(format!(
        "if(location.href==='about:blank'||location.href===''){{location.href={lit};}}"
    ));
    Some(panel)
}

/// 球的「逻辑隐藏」标志：true 表示用户语义上的隐藏。
///
/// ★ 实现关键（2026-09-25 二次修复）：隐藏**不走 `hide()`** —— WS_HIDE 会撕裂 WebView2
///   合成表面，`show()` 重建表面时 DWM 先呈现该屏幕区域的**陈旧缓存帧**（真机截图实证：
///   球出现瞬间背景闪出面板/结果卡的 DeepSeek 文字，且两次内容一致 = 同一帧缓存）。
///   改用「空区域裁剪（`SetWindowRgn` 零面积区域）+ 点击穿透」，窗口保持 `WS_VISIBLE`：表面持续合成、从不重建，
///   show 侧不存在「首帧重建」，从结构上消灭该闪现。
static BALL_LOGIC_HIDDEN: AtomicBool = AtomicBool::new(false);

/// 球当前是否处于「逻辑隐藏」。调用方（fullscreen / screenshot / tray）判定球的可见性
/// 一律用本函数，**不要**用 `is_visible()` —— 逻辑隐藏下窗口仍 WS_VISIBLE，恒返回 true。
pub(crate) fn ball_is_hidden() -> bool {
    BALL_LOGIC_HIDDEN.load(Ordering::SeqCst)
}

/// 冲刷球的合成表面：强制球窗重合成一次，丢弃残留在球区域上的陈旧帧。
///
/// **为什么需要（历史成因）**：WebView2 的 GPU 合成表面**不等同于窗口可见性**，`hide()` 只置
/// `WS_HIDE`，回收有滞后；早期球窗为透明窗（`transparent:true`）时，与球同为
/// `transparent + always_on_top` 的相邻窗（划词工具条 / 结果卡 / 截图遮罩，即
/// `popup` / `result` / `shot`）hide 后其陈旧合成帧仍可能留在 DWM 中、被球窗读到
/// （用户所报「首次点球，球背后透出工具条背景文字」）。
/// ★ 2026-09-26（方案 A-2）：球窗已改为**不透明窗**（`transparent:false`，见
/// [`crate::ballstyle::apply_ball_shape`]），**不再存在**「透过球看邻居旧帧」的问题；
/// 本冲刷保留为廉价、幂等的重绘兜底（对球自身旧表面的清理仍有效）。
///
/// **两层冲刷（治本 + 纵深）**：
///   ⓪ 无标题栏确认：经 [`crate::ballstyle::enforce_captionless`] 清除任何被恢复的 `WS_CAPTION`
///      并禁用 DWM 非客户区渲染（真机探针实测闪现内容 = 球窗自身标题，此为**结构性根因**所在）；
///   ① Win32 `RedrawWindow(INVALIDATE | UPDATENOW)`：直接令 DWM 丢弃球窗旧合成表面并立即重绘。
///
/// **★ 2026-09-26（方案 A-2）：删除原「① 内容级 reflow」层** —— 该层靠 toggle
/// `document.body.style.display` 强制 WebView2 重排，本就是「验证期临时」手段；且方案 A-2 后
/// 球窗已是**不透明窗**（`transparent:false`），不存在「透过球看邻居陈旧帧」的问题，reflow 层
/// 已无必要，故随本次修复一并移除，冲刷收敛为「无标题栏确认 + RedrawWindow」两层。
///
/// **此前仅 `eval("void 0")`** —— 无副作用空表达式，WebView2 可能判定「无需重绘」而**从未真正冲刷**
/// （2026-09-25 审计 P0：批次 A 在 popup/result/shot hide 与 show_panel/finish 补冲刷后仍复现残影，
/// 印证 `void 0` 疑似 no-op）。现由 `RedrawWindow` 确实触发重合成。
///
/// 球窗缺失时静默跳过（不 panic）。
pub(crate) fn flush_ball<R: Runtime>(app: &AppHandle<R>) {
    // ⓪ 结构性根因（2026-09-25 第三次修复）：真机探针实测「闪现内容 = 球窗自身标题 BALL9Q2」，
    //    即某次操作下标题栏被画了出来 —— 先确保球窗此刻「无标题栏 + DWM 不渲染非客户区」，
    //    令其**根本没有可画的标题栏**。内部仅在确实发现标题栏位重现时才记日志，故可安全热点调用。
    crate::ballstyle::enforce_captionless(app);
    if let Some(b) = app.get_webview_window("ball") {
        // Win32 层：直接令 DWM 丢弃旧表面并立即重绘呈现。
        if let Ok(hwnd) = b.hwnd() {
            unsafe {
                let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_UPDATENOW);
            }
        }
        crate::diag::log("flush_ball: 已对球做两层冲刷（无标题栏确认 + RedrawWindow）");
    }
}

/// 逻辑隐藏悬浮球：**空区域裁剪令整窗不可见 + 点击穿透**，不调用 `hide()`。
///
/// 为什么不用 `hide()`：WS_HIDE 会撕裂 WebView2 合成表面；`show()` 重建表面时 DWM 先
/// 呈现该屏幕区域的陈旧缓存帧（真机截图实证：球出现瞬间背景闪出面板/结果卡的 DeepSeek
/// 文字、两次内容一致 = 同一帧缓存），等球自身透明内容合成好才恢复 —— 即「背景一闪即逝」。
///
/// **★ 2026-09-26（方案 A-2）隐藏主力改为 [`crate::ballstyle::apply_ball_shape`]`(app, false)`**：
/// 球窗已改为**不透明窗**（`transparent:false`，见 `tauri.conf.json` 与 `ballstyle::apply_ball_shape`），
/// 此前的「页面级 `visibility:hidden`」在不透明白底窗上**仍会露出白圆**，不再是可靠的隐藏手段。
/// 现改由 `SetWindowRgn` 套一个**零面积空区域**：整窗既不可见、且**不拦截任何点击**
/// （比 `visibility:hidden` 更彻底）。窗口全程 `WS_VISIBLE`，WebView2 合成表面持续合成、
/// **从不撕裂** ⇒ show 侧无任何「首帧重建」。
/// ★★ 2026-10-13（F3）：**已移除** `set_ignore_cursor_events(true)` ——
/// 它会把 tao 缓存的 `VISIBLE=0` 重放成 `ShowWindow(SW_HIDE)`，把球窗**真正隐藏**，
/// 使 `reveal_ball` 无法恢复（详见 `docs/ball-cannot-show-rootcause-2026-10-13.md`）。
/// 穿透无损失：零面积空区域**本身就不参与命中测试**。
/// 页面级 `visibility:hidden` 保留 —— 无害，且能在区域操作意外失败时兜底。
///
/// 返回 `true` 表示球窗存在且已应用（screenshot 依此决定是否成对恢复）。
pub(crate) fn ball_hide<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(b) = app.get_webview_window("ball") else {
        return false;
    };
    // 纵深防御①：页面级透明（区域操作意外失败时的兜底；自包含脚本，不依赖球页预定义函数）。
    let _ = b.eval("document.documentElement.style.visibility='hidden';void 0;");
    // ★★★ 2026-10-13（F3）：**删除**了此处的 `set_ignore_cursor_events(true)`。
    //
    // 【原先为何有它】作为「空区域」之外的点击穿透**双保险**（纵深防御②）。
    //
    // 【为什么必须删（tao 0.35.3 源码级确诊）】该方法内部走 tao 的
    //   `WindowState::set_window_flags → WindowFlags::apply_diff`，而 `apply_diff` 末尾有一段
    //   **只依赖 `new`、与 diff 是否为空无关**的**无条件隐藏**：
    //       if !new.contains(WindowFlags::VISIBLE) { let _ = ShowWindow(window, SW_HIDE); }
    //   球窗以 `visible:false` 创建（`tauri.conf.json`）、且我们**从不调 tao 的 `show()`**
    //   （全走原生 `ShowWindow`，见 `ball_show`）⇒ tao 缓存的 `window_flags` 里
    //   `VISIBLE` 位**恒为 0** ⇒ **每次调用都会把球窗真正隐藏（`WS_HIDE`）**。
    //
    //   它「看起来帮了忙」（球确实不可见了），实则：① 掩盖了 region 的真实作用，
    //   让「隐藏生效」的归因一直错到 region 上；② 让球进入一种 `reveal_ball` 当时
    //   **无法恢复**的状态（`WS_HIDE` 的窗口套 region 无效），直接造成
    //   「隐藏后无法再显示」。详见 `docs/ball-cannot-show-rootcause-2026-10-13.md`。
    //
    // ★ 穿透**毫无损失**：`SetWindowRgn` 的零面积空区域**本身就不参与命中测试**，
    //   落在球区域的点击会直接交给下方窗口 —— 这一直是隐藏态的主要手段（见本函数文档）。
    BALL_LOGIC_HIDDEN.store(true, Ordering::SeqCst);
    // ★ 方案 A-2 隐藏主力（**必须是本函数的最后一步 Win32 操作**）：零面积空区域 ⇒
    //   整窗不可见 + 不吞点击（不透明窗上唯一切实可靠的隐藏手段）。顺序铁律见上方注释：
    //   region 会被后续 `SWP_FRAMECHANGED` 清掉，故只能「最后落笔」。
    crate::ballstyle::apply_ball_shape(app, false);
    crate::diag::log("ball_hide: 页面透明+点击穿透+**末尾**空区域裁剪(apply_ball_shape=false)（保持 WS_VISIBLE，表面不撕裂）");
    true
}

/// 逻辑显示悬浮球：恢复球页可见与可点击。
///
/// 表面从未撕裂，**无需 settle、无需重建** —— 「show 首帧呈现陈旧区域帧」的结构性成因
/// 已不存在。仍补一次 [`flush_ball`] 作为对邻居（popup/result/shot）陈旧帧的纵深防御。
/// **显形的实现约束（★ 2026-09-25 铁证）**：球窗以 `visible:false` 创建（见 `tauri.conf.json`），
/// 显形必须走 **Win32 原生 `ShowWindow(SW_SHOWNOACTIVATE)`** —— Tauri 的 `show()` 会令 tao 重建
/// `GWL_STYLE` 从而把 `WS_CAPTION` 恢复回来（并连带 `SM_CXMINTRACK` 夹宽），产生一帧标题栏闪现。
/// 详见函数内注释与 `docs/audit-2026-09-25.md` §11。
pub(crate) fn ball_show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(b) = app.get_webview_window("ball") {
        // ★★ 2026-09-25 第五次排查**铁证**：这里**绝不能**调用 Tauri 的 `show()`。
        //    真机日志实证（新构建首次启动即复现）：
        //      fix 清位后 STYLE=0x04000000 → 调 `show()` 后 STYLE=0x14CB0000
        //      （= 原样式 | WS_VISIBLE | WS_CAPTION|SYSMENU|MINBOX|MAXBOX）。
        //    即 `show()` 会让 tao **用它内部缓存的 window-flags 重建 `GWL_STYLE`**，把
        //    [`crate::ballstyle::fix`] 清掉的 `WS_CAPTION` 原样放回，并连带触发 `SM_CXMINTRACK`
        //    夹宽 ⇒ 一帧「球窗自身标题」的闪现（用户所报「一闪即逝的背景」，探针实测文字
        //    `BALL9Q2`）。这也解释了此前「运行期 0 次恢复」：旧配置球窗恒 `visible:true`，
        //    `show()` 从不被调用。
        //    改用 Win32 原生 `ShowWindow(SW_SHOWNOACTIVATE)`：只补 `WS_VISIBLE`，**不重算样式**。
        // ★★ 2026-09-26（方案 A-2）：**先**套椭圆区域、**再**显形 —— 球窗以不透明窗创建，
        //    圆形外观靠 `SetWindowRgn` 的椭圆区域维持；若先 ShowWindow 再设区域，会有一帧
        //    「白色方块」闪现。故在显形**之前**先重套区域，使窗口一露面即是圆形。
        // ★★★ 2026-10-13（region 时序缺陷修复,见 docs/ball-toggle-bug-2026-10-13.md）：
        //    原先「此处套椭圆 + 末尾 flush_ball」的顺序**是错的** —— `flush_ball` 会经
        //    `enforce_captionless → make_tool_window → SetWindowPos(SWP_FRAMECHANGED)` 把
        //    刚套好的椭圆区域**清掉**。日志铁证：运行期 `apply_ball_shape` 零调用、`ball_show:`/
        //    `ball_hide:` 零日志，而 `ball_contextmenu: 菜单已关闭` 出现 20 次 —— 即菜单点选后
        //    region 被「设完立刻清」，两个方向点击都看不出变化。
        //    故现改为**两次套区域**：①此处（显形前，保证「一露面即圆」、无白方块帧）；
        //    ②函数末尾（所有 EXSTYLE/SWP 操作之后，`flush_ball` 之后）**再套一次** ——
        //    这才是真正生效的那一次（「画家最后落笔」）。`apply_ball_shape` 幂等，多套无害。
        crate::ballstyle::apply_ball_shape(app, true);
        if !b.is_visible().unwrap_or(true) {
            match b.hwnd() {
                Ok(hwnd) => {
                    unsafe {
                        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    }
                    crate::diag::log(
                        "ball_show: 原生 ShowWindow(SW_SHOWNOACTIVATE) 显形（不经 tao ⇒ 不重建样式）",
                    );
                }
                // 兜底：真取不到 HWND 时才退回 Tauri `show()`（会恢复标题栏，随后由 flush 清掉）。
                Err(e) => {
                    crate::diag::warn(&format!(
                        "ball_show: 取 HWND 失败（{e}），退回 Tauri show()（可能恢复标题栏）"
                    ));
                    let _ = b.show();
                }
            }
        }
        let _ = b.eval("document.documentElement.style.visibility='';void 0;");
        // ★★★ 2026-10-13（F3）：**删除**了此处的 `set_ignore_cursor_events(false)`。
        //   它既多余（球可见时本就没有 `WS_EX_TRANSPARENT`、默认可点击），
        //   更有害：会走 tao 的 `apply_diff` 重放 `ShowWindow(SW_HIDE)`，把上面（本函数
        //   第 227 行附近）那次原生 `ShowWindow` **显形救援在同一函数内立即撤销**
        //   —— 这正是「显示悬浮球时好时坏、取决于消息时序」的直接成因。
        BALL_LOGIC_HIDDEN.store(false, Ordering::SeqCst);
        flush_ball(app);
        // ★★★ 2026-10-13（region 时序缺陷修复）：`flush_ball` 内部会经
        //   `enforce_captionless → make_tool_window → SetWindowPos(SWP_FRAMECHANGED)`
        //   把上面（函数开头）套好的椭圆区域清掉 —— 2026-09-26 的注释其实已预见
        //   「Tauri/tao 的某些样式操作可能把已设的 region 清掉」，但把重套放在了**开头**，
        //   于是被同一函数末尾的清操作立即抹掉，防御自相矛盾。
        //   现于**所有** EXSTYLE/SWP 操作之后**最后再套一次** —— 这一次谁也清不掉。
        //   幂等、可高频调用；这是「点显示悬浮球必生效」的关键一拍。
        // ★★ 2026-10-13（F4 闭环，独立验证补充）：**同时**如实上报「形态」是否真的套上。
        //   只报 `IsWindowVisible` 不够 —— 若 `SetWindowRgn` 失败（窗口 `WS_VISIBLE` 但
        //   region 未生效/为空），用户依旧看不见球，日志却会打「恢复可见」，又是一条谎报。
        let shape_ok = crate::ballstyle::apply_ball_shape(app, true);
        // ★★★ 2026-10-13（F4：日志诚实化）：此前这里**无条件**写「球页恢复可见」，
        //   而真机上球其实可能已被 tao 的 `apply_diff` 隐藏 —— 这条**谎报日志**
        //   是本轮排障跑偏的直接原因之一（另一条是 `reveal_ball` 的「球已可见」，已随 F1 移除）。
        //   现改为打印 `IsWindowVisible` 与 `apply_ball_shape` 返回值的**实测值**，让日志自证。
        //   注：`flush_ball`（上方）内的 `enforce_captionless` 已做可见性自愈，
        //   故正常情况下两项皆为 true；任一为 false 即说明自愈也没兜住 —— 必须告警。
        // ★ 另：`hwnd()` 取不到时必须**单独告警**，不可 `unwrap_or(false)` ——
        //   否则会把「取不到 HWND」误报成「球窗仍被隐藏」，制造新的谎报。
        let os_visible = match b.hwnd() {
            Ok(h) => unsafe { IsWindowVisible(h).as_bool() },
            Err(e) => {
                crate::diag::warn(&format!(
                    "ball_show: 取球窗 HWND 失败（{e}），无法读回可见性（region_ok={shape_ok}）"
                ));
                return;
            }
        };
        if os_visible && shape_ok {
            crate::diag::log(
                "ball_show: 球页恢复可见+可点击（表面常驻，无首帧重建；末尾已重套椭圆区域；IsWindowVisible=true region_ok=true）",
            );
        } else {
            crate::diag::warn(&format!(
                "ball_show: 收尾自检未通过（IsWindowVisible={os_visible} region_ok={shape_ok}）—— 球可能仍不可见，请附 d1-debug.log 排查"
            ));
        }
    }
}

/// 启动期球窗显形「首帧就绪握手」闸门 —— 消除「启动瞬间球空白」。
///
/// ## 问题
/// 球窗以 `visible:false` 创建（见 `tauri.conf.json`）。此前启动期**直接**调用 [`ball_show`]：
/// 在 **WebView2 尚未绘制首帧**时就把窗口显形 —— 用户先看到一个「还没画好」的窗口
/// （白块 / 无 logo 的空白圆），待首帧就绪后才补齐，即用户所报「启动瞬间球空白」。
///
/// ## 修法（三方握手）
/// 1. 本函数在启动期把球窗维持为**零面积空区域**（[`crate::ballstyle::apply_ball_shape`]`(app, false)`
///    ⇒ 整窗不可见、且不吞点击）；**同时**在 OS 层把它显形（原生 `ShowWindow(SW_SHOWNOACTIVATE)`）。
///    ★ 为何要「可见但裁空」：**隐藏窗口下 WebView2 的合成器不产生渲染帧**、页面
///    `requestAnimationFrame` 被暂停 —— 若让球窗一直真·隐藏，首帧永不到来，握手必然落空、
///    每次都只能靠超时兜底（握手形同虚设）。故令窗口「OS 层可见、被零面积区域裁到什么都看不到」：
///    渲染照常进行、用户却看不到任何东西。
/// 2. `poc/ball.html` 在**首个渲染帧之后**经 [`crate::ipc::ball_ready`] 命令送来回就绪信号。
/// 3. 本函数起后台线程等待该信号（**≤ [`crate::ballready::BALL_READY_TIMEOUT`]（~3s）兜底超时**），
///    收到或超时后调用 [`ball_show`]：套椭圆区域（`apply_ball_shape(true)`）令球「一露面即圆」，
///    而此刻首帧早已绘制完成 ⇒ **不存在任何「空白窗口期」**。
///
/// ## 为什么必须保留超时兜底
/// 前端握手可能因页面加载失败 / IPC 桥（`__TAURI_INTERNALS__`）不可用而**永不到达**；
/// 若无超时则球将**永久不可见**（远比「空白」严重）。故 ~3s 后无论是否收到信号都必须显形。
///
/// ## 与运行期显隐的关系
/// 本函数是启动期**一次性**入口；运行期显隐仍走 [`reveal_ball`] / [`ball_hide`]。
/// 等待期间 `BALL_LOGIC_HIDDEN` 置位（球此刻确为隐藏态），由 [`ball_show`] 复位为可见。
pub(crate) fn gate_ball_show<R: Runtime>(app: &AppHandle<R>) {
    // 1) 隐藏主力：零面积空区域 ⇒ 整窗不可见、不吞点击（此刻起用户看不到球）。
    crate::ballstyle::apply_ball_shape(app, false);
    BALL_LOGIC_HIDDEN.store(true, Ordering::SeqCst);

    // 2) OS 层显形（区域已裁空 ⇒ 视觉上仍完全不可见）：驱动 WebView2 合成/绘制首帧，
    //    让球页的 rAF 能真正跑起来（否则握手永不到达、只能靠超时兜底）。
    //    走原生 ShowWindow（不经 tao，避免其用缓存的 window-flags 重建 WS_CAPTION —— 与
    //    ball_show 同一铁律；见该函数内 2026-09-25 铁证注释）。
    if let Some(b) = app.get_webview_window("ball") {
        if !b.is_visible().unwrap_or(true) {
            match b.hwnd() {
                Ok(hwnd) => unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                },
                Err(e) => crate::diag::warn(&format!(
                    "ball-ready: 取球窗 HWND 失败（{e}），无法预先显形以驱动首帧渲染（转由超时兜底）"
                )),
            }
        }
    }

    crate::diag::log(
        "[ball-ready] 已维持零面积空区域并预先显形（驱动首帧合成），等待 ball_ready 握手（≤3s 兜底）",
    );

    // 3) 后台等待「首帧就绪」/ 超时，随后正式显形（套椭圆区域）。
    let app2 = app.clone();
    thread::spawn(move || {
        let ready = crate::ballready::wait_ball_ready(crate::ballready::BALL_READY_TIMEOUT);
        if ready {
            crate::diag::log("[ball-ready] 收到 ball_ready（首帧已绘制）→ 套椭圆区域显形");
        } else {
            crate::diag::warn(
                "[ball-ready] 等待 ball_ready 超时（~3s 兜底）→ 强制显形（请检查球页加载 / IPC 桥）",
            );
        }
        ball_show(&app2);
    });
}

/// 显隐悬浮球统一入口（fullscreen / tray / single_instance 调用）。
///
/// **P0 二次修复（2026-09-25 真机复测）**：上一版 `show() → settle → flush` 只能把陈旧帧
/// 「闪一下后擦掉」（flush 在 settle 之后才跑），防不住「闪」本身 —— 陈旧帧在 show 首帧
/// 就已呈现。现改为逻辑显隐（见 [`ball_hide`]）：表面常驻，show 侧不再有任何重建。
pub(crate) fn reveal_ball<R: Runtime>(app: &AppHandle<R>) {
    // ★★★ 2026-10-13（F1：**取消按标志位分支**，无条件走完整恢复路径）
    //
    // 【原实现的致命缺陷】原代码为
    //     if ball_is_hidden() { ball_show(app) } else { flush_ball + 重套 region }
    //   —— else 分支**只重套 region**，从不 `ShowWindow`、不恢复页面 `visibility`、不恢复穿透。
    //   而 `ball_is_hidden()` 只是**逻辑态**，会与实际窗口状态脱节。真机日志铁证：
    //     `#79 flush_ball` → `#80 apply_ball_shape visible=true` → `#81 reveal_ball: 球已可见`
    //   这一拍 region 确实被套成椭圆了，**可球仍然不可见** —— 因为 `WS_VISIBLE` 已被
    //   tao 的 `apply_diff` 清掉（见 `ballstyle::enforce_captionless` 的 F2 注释），
    //   而 region 对 `WS_HIDE` 的窗口毫无作用。
    //   ⇒ else 分支既无从恢复、日志又谎报「球已可见」⇒ **球永久卡在不可见**（只能重启进程）。
    //   这正是用户所报「悬浮球隐藏后无法再显示」。
    //
    // 【现实现】「让球可见」的语义就是**无条件完整恢复**。`ball_show` 本身**幂等**：
    //   原生显形 + `eval` 恢复页面可见性 + 重套椭圆 region + 冲刷；且其内部
    //   `flush_ball → enforce_captionless` 还会做 **region / 可见性双重读回自愈**。
    //   故「已可见时重复执行」无害，而「状态一旦脱节」则必然被救回。
    //   ⇒ 判据不再依赖任何可能过期的标志位（与 P0-5 / F2「按结果校验」同一理念）。
    //
    // 【行为变化评估】仅 else 分支（`ball_is_hidden()==false`）的语义由「只补 region」
    //   变为「完整恢复」；隐藏态（flag==true）行为与原先**完全一致**。
    //   调用方 `fullscreen`（已由 `BALL_WAS_VISIBLE` 守卫）与 `single_instance` 不受影响。
    ball_show(app);
}

// 唤出面板并（首次）懒加载 DeepSeek。抽出来给划词链路复用；
// 与「球态点击 / 托盘 / 右键菜单」里那段既有逻辑等价，但**不改动那些已验证的调用点**。
pub(crate) fn show_panel<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    let panel = ensure_panel_loaded(app)?;
    // ★ 先解除最小化（真机缺陷 2026-09-14「结果卡弹窗后面板无法显示」的头号成因）：
    //   `show()` 走的是 ShowWindow(SW_SHOW)，它对**已最小化**的窗口**无效**；而最小化时
    //   WS_VISIBLE 仍在 ⇒ `is_visible()` 恒为 true ⇒ 上层「点球切换显示」会走进「收起」分支，
    //   用户于是「怎么点都唤不回面板」。必须先 SW_RESTORE 再 SHOW。
    if panel.is_minimized().unwrap_or(false) {
        let _ = panel.unminimize();
        crate::diag::log("show_panel: 面板此前处于最小化，已先恢复");
    }
    let _ = panel.show();
    // ★ 越界回收：若面板已整体跑到所有显示器之外（显示器拓扑 / 分辨率变化，或系统建议矩形
    //   被 WM_DPICHANGED 改写），show() 会「成功」但用户仍然看不见 —— 挪回主显示器工作区。
    ensure_panel_on_screen(app, &panel);
    // ★ 球不再参与「隐现舞步」（2026-09-25 二次修复）：此前「开面板先 hide 球再 show 回」
    //   恰恰制造了「hide 撕裂合成表面 → show 首帧呈现该区域陈旧缓存帧」的闪现（真机截图实证：
    //   闪出内容为面板/结果卡文字、两次一致）。球表面现已常驻（见 `ball_hide`/`ball_show`），
    //   开面板完全不触碰球的可见性；这里仅补一次冲刷，作为对邻居陈旧帧的纵深防御。
    flush_ball(app);
    Some(panel)
}

/// 隐藏面板的统一收口（替代各调用点散落的 `p.hide()`）。
///
/// 为什么要收口：panel 是 `always_on_top` 的 WebView2 窗（**不透明** —— `setup_panel` 未设
/// `transparent`），`hide()` 只是 WS_HIDE —— WebView2 的 GPU 合成表面**不等同于窗口可见性**，
/// 回收有滞后。历史上真机报过「悬浮球背景透出『DeepSeek…好的』一类面板文字」；其成因是
/// **球自身区域上的旧合成表面**未及时被合成器丢弃。这里在 hide 后**强制触发 ball 重绘**
/// （`flush_ball`）让合成器刷掉旧表面。属低风险加固 —— 不影响可见性语义，只多一次 InvalidateRect。
/// 注：panel 并非透明窗，故此处的残影来源是「球自身旧表面」，而非「透视到 panel」。
pub(crate) fn hide_panel<R: Runtime>(app: &AppHandle<R>) {
    let t_step = Instant::now();
    let _ = if let Some(p) = app.get_webview_window("panel") {
        p.hide()
    } else {
        Ok(())
    };
    // 冲刷可能残留在透明球表面的 WebView 旧帧（消「悬浮球背景渗出面板文字」）。
    // Tauri v2 未暴露 `InvalidateRect`；用 `eval("void 0")` 触发一次页面重绘/重合成即可，
    // 等效于让合成器重读当前表面。无害、极轻。（与 `show_panel` 隐现第二拍复用同一 `flush_ball`。）
    flush_ball(app);
    crate::diag::log(&format!(
        "hide_panel: 面板已 hide 并冲刷球合成表面（耗时={}ms）",
        t_step.elapsed().as_millis()
    ));
}

/// 面板越界回收：当前左上角已不在任何显示器内 → 挪回主显示器工作区（留 60px 边距）。
///
/// 只做这一件事：**不**纠正「部分越界」—— 用户可能刻意把面板拖到只露一角，
/// 那种情况下强行纠偏反而是和用户抢窗口。
fn ensure_panel_on_screen<R: Runtime>(app: &AppHandle<R>, panel: &WebviewWindow<R>) {
    let Ok(pos) = panel.outer_position() else {
        return;
    };
    // 取「左上角往里缩 24px」的点判定：面板被整体推出屏幕时，该点必不在任何显示器内。
    if app
        .monitor_from_point(pos.x as f64 + 24.0, pos.y as f64 + 24.0)
        .ok()
        .flatten()
        .is_some()
    {
        return;
    }
    if let Ok(Some(m)) = app.primary_monitor() {
        let a = m.work_area();
        let _ = panel.set_position(PhysicalPosition::new(a.position.x + 60, a.position.y + 60));
        crate::diag::warn(&format!(
            "show_panel: 面板位置 ({},{}) 已不在任何显示器内，回收到主显示器工作区",
            pos.x, pos.y
        ));
    }
}

/// 面板窗口（路线 A：内嵌 DeepSeek 网页端，默认隐藏，经球态/托盘/划词唤出）。
pub(crate) fn setup_panel(
    app: &AppHandle,
    sink: &Arc<Mutex<Option<std::fs::File>>>,
) -> AppResult<()> {
    let sink_c = sink.clone();
    let panel_url = WebviewUrl::External("about:blank".parse().unwrap());
    match WebviewWindowBuilder::new(app, "panel", panel_url)
        .title("DeepSeek 悬浮面板")
        .position(420.0, 140.0)
        .inner_size(440.0, 600.0)
        .min_inner_size(320.0, 360.0)
        .resizable(true) // D2：面板可缩放
        .always_on_top(true)
        .decorations(true) // 保留原生标题栏以便自由拖动；关闭按钮隐藏面板而非销毁
        .visible(false) // 默认隐藏，由球态/托盘切换
        .initialization_script(include_str!("dsfloat_init.js"))
        .on_navigation({
            let sink_c = sink_c.clone();
            move |url| {
                let state = panel_state(url.as_str());
                let line = format!(
                    "[M1] on_navigation url={} state={}\n",
                    redact_url(url.as_str()),
                    state
                );
                crate::diag::log(line.trim_end());
                diag_write(&sink_c, &line);
                true
            }
        })
        .on_page_load({
            let sink_c = sink_c.clone();
            move |webview, payload| {
                let ev = payload.event();
                if matches!(ev, tauri::webview::PageLoadEvent::Finished) {
                    // 新文档不继承上一页的JS代次；只初始化为当前有效任务，旧队列仍会被guard拒绝。
                    let token = crate::workflow::current();
                    let _ = webview.eval(format!(
                        "if((window.__dsfloatRequest||0)<={id}){{window.__dsfloatRequest={id};window.__dsfloatNavigationPending=false;}}",
                        id=token.0
                    ));
                }
                let state = panel_state(payload.url().as_str());
                let line = format!(
                    "[M1] on_page_load url={} event={:?} state={}\n",
                    redact_url(payload.url().as_str()),
                    ev,
                    state
                );
                crate::diag::log(line.trim_end());
                diag_write(&sink_c, &line);
            }
        })
        .build()
    {
        Ok(w) => {
            crate::diag::log("[M1] panel window created (route A: embedded persistent login)");
            crate::diag::log(&format!(
                "[M1] panel configured_url={} visible=false",
                crate::config::panel_url()
            ));
            let handle_evt = app.clone();
            w.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    crate::hide_panel(&handle_evt);
                }
            });
        }
        Err(e) => {
            crate::diag::error(&format!("[M1] panel builder failed: {e}"));
        }
    }
    Ok(())
}

/// D1 划词链路的常驻窗口（工具条 + 设置 + 提示条）：常驻创建后启动全局鼠标钩子（类豆包）。
///
/// 三个窗口都在 setup 内**常驻创建一次**（visible=false），之后只 show / hide，
/// 绝不每次弹窗都创建或销毁 —— 反复建/销毁 webview 是 Windows 上
/// WebView2 窗口类反注册 abort（Error 1411/1412）的主要来源。
pub(crate) fn setup_popup_and_mousehook(app: &AppHandle) -> AppResult<()> {
    crate::popup::ensure(app);
    crate::settings::ensure(app);
    let hm = app.clone();
    if !crate::mousehook::start(move |ev| match ev {
        crate::mousehook::MouseEvent::Selection(request) => {
            crate::popup::trigger_selection(&hm, request)
        }
        crate::mousehook::MouseEvent::Dismiss(generation) => {
            crate::popup::dismiss_generation(&hm, generation);
        }
        crate::mousehook::MouseEvent::DismissMenu(generation) => {
            crate::context_menu::close(&hm, generation)
        }
    }) {
        crate::diag::error("[M2] 鼠标划词钩子启动失败（已降级，划词功能不可用）");
    }
    Ok(())
}

/// 启动期诊断：把球态 / 面板态窗口的存在性与关键属性落盘到诊断日志。
pub(crate) fn setup_diag_windows(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("ball") {
        crate::diag::log("[M1] ball window created");
        crate::diag::log(&format!("[M1] ball is_decorated={:?}", w.is_decorated()));
        crate::diag::log(&format!(
            "[M1] ball is_always_on_top={:?}",
            w.is_always_on_top()
        ));
        crate::diag::log(&format!("[M1] ball size={:?}", w.inner_size()));
    } else {
        crate::diag::log("[M1] ball window missing");
    }
    if let Some(w) = app.get_webview_window("panel") {
        crate::diag::log("[M1] panel window created");
        crate::diag::log(&format!("[M1] panel is_resizable={:?}", w.is_resizable()));
        crate::diag::log(&format!("[M1] panel is_visible={:?}", w.is_visible()));
    } else {
        crate::diag::log("[M1] panel window missing");
    }
}

/// 悬浮球边长（逻辑像素，用于恢复位置时的越界夹取；物理边长按显示器缩放换算）。
const BALL_SIZE: i32 = 64;

/// 位置去抖落盘的轮询间隔（ms）—— 拖动期间把最新位置写进内存、由本线程低频消费刷盘，
/// 避免每次 Moved 都写盘。400ms 保证「最终静止位置」一定被保存（不会被节流窗口吞掉）。
const POS_FLUSH_POLL_MS: u64 = 400;
/// 启动期后台预热面板前的静默（ms）—— 稍候几帧，避免与首屏窗口创建抢占 WebView2 资源。
/// 0.6s 后隐藏态导航至 DeepSeek，结果卡首用不再等待加载。
pub(crate) const PANEL_WARMUP_DELAY_MS: u64 = 600;

/// 待落盘的球位置（拖动期间只写这里，不刷盘；由去抖线程消费）。
static PENDING_POS: Mutex<Option<(i32, i32)>> = Mutex::new(None);

/// 立即把内存中待落盘的球位置写入配置。去抖线程按 400ms 轮询消费它；
/// 退出前也必须显式调用一次（各退出路径硬调 `process::exit`，会绕过事件循环，
/// 若在最后一次拖动后 400ms 内退出，否则终点位置会丢）。
///
/// 返回**本轮是否真的写盘了**（有待落盘位置且已写入 ⇒ `true`；无待写位置 ⇒ `false`）。
/// 供去抖线程判定「这一拍发生了拖动落位」，以便随之补一次 [`crate::flush_ball`]（见下）。
/// 既有退出路径的调用点忽略返回值即可（`bool` 非 `#[must_use]`，不产生告警）。
pub(crate) fn flush_pending_pos() -> bool {
    let pos = PENDING_POS.lock().ok().and_then(|mut g| g.take());
    match pos {
        Some((x, y)) => {
            crate::config::update(|c| {
                c.ball_x = Some(x);
                c.ball_y = Some(y);
            });
            true
        }
        None => false,
    }
}

/// 把球位置夹取到某显示器的工作区内，避免拔掉显示器 / 分辨率或 DPI 变化后把球恢复到屏幕外。
/// 取不到该点所在显示器时退回主显示器；再取不到则原样返回。
fn clamp_ball_to_monitor(app: &AppHandle, x: i32, y: i32) -> (i32, i32) {
    let monitor = app
        .monitor_from_point(x as f64, y as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let area = m.work_area();
        let (ax, ay) = (area.position.x, area.position.y);
        let (aw, ah) = (area.size.width as i32, area.size.height as i32);
        // 物理球边长 = 逻辑边长 × 显示器缩放（Moved/工作区均为物理像素）
        let ball_px = (BALL_SIZE as f64 * m.scale_factor()).round() as i32;
        let max_x = (ax + aw - ball_px).max(ax);
        let max_y = (ay + ah - ball_px).max(ay);
        return (x.clamp(ax, max_x), y.clamp(ay, max_y));
    }
    (x, y)
}

/// §7 位置记忆：启动期恢复悬浮球上次位置，并挂载拖动时的「Moved → 持久化」监听。
///
/// 拖动由前端 `poc/ball.html` 经 `startDragging` 交给系统模态循环，JS 的 mouseup 会被吞掉，
/// 故**不**依赖前端回调，而在 Rust 侧监听球态窗口的 `Moved` 事件来记忆位置。
pub(crate) fn setup_ball_position(app: &AppHandle) -> AppResult<()> {
    // 1) 恢复上次记忆的位置（物理像素；与 Moved 事件坐标空间一致），并夹取回可见区域。
    let cfg = crate::config::load();
    if let (Some(x), Some(y)) = (cfg.ball_x, cfg.ball_y) {
        if let Some(w) = app.get_webview_window("ball") {
            let (cx, cy) = clamp_ball_to_monitor(app, x, y);
            let _ = w.set_position(PhysicalPosition::new(cx, cy));
            crate::diag::log(&format!(
                "位置记忆：恢复球位置 ({x},{y}) → 夹取后 ({cx},{cy})"
            ));
        }
    }

    // 2) 拖动时只把最新位置写入内存（Moved 事件期间不刷盘）。
    if let Some(w) = app.get_webview_window("ball") {
        let ev_app = app.clone();
        w.on_window_event(move |event| match event {
            tauri::WindowEvent::Moved(pos) => {
                if let Ok(mut g) = PENDING_POS.lock() {
                    *g = Some((pos.x, pos.y));
                }
            }
            // 2b) 焦点 / 尺寸变化是「标题栏被恢复」的高发时刻（真机探针 2026-09-25：闪出内容实测为
            //     球窗自身标题 `BALL9Q2`）。每次这类事件都重新确认「无标题栏 + DWM 不渲染非客户区」，
            //     并纠正被 `SM_CXMINTRACK` 连带夹宽的尺寸。幂等：标题栏位已清时为空操作，
            //     不会自激（`set_size` 仅在确实清掉了标题栏位时才做，下一拍即收敛）。
            //
            //     ★★★ 2026-10-13（region 时序缺陷修复 P0-4）：本分支是「隐藏被悄悄打回」的
            //     **真凶**。菜单关闭 → 焦点回到球 → 本分支跑 `enforce_captionless` →
            //     `clear_caption_bits` 下发 `SetWindowPos(SWP_FRAMECHANGED)` →
            //     **清掉 `ball_hide` 刚设的零面积空区域** ⇒ 球重新露出来。
            //     真机日志（d1-debug.log）：`ball_hide` 完成于 +9394ms，仅 7ms 后即
            //     「运行时检测到球窗标题栏位被恢复，已重新清除」。
            //     现已由 [`crate::ballstyle::enforce_captionless`] 内部**末尾重套 region** 收口
            //     （按其逻辑态 `ball_is_hidden()` 决定套椭圆还是套空区域）—— 故此处无需额外处理，
            //     只需保证调用的仍是那个已收口的函数。
            tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Resized(_) => {
                crate::ballstyle::enforce_captionless(&ev_app);
            }
            _ => {}
        });
    }

    // 3) 去抖落盘线程：每 400ms 检查一次，有变化才写盘。
    //    相比「按时间戳节流」的写法，去抖保证拖动**最终静止位置**一定会被保存（不会被节流窗口吞掉）。
    //    ★ 2026-09-25 第八次排查：拖动结束、球落位后的第一拍，合成表面可能尚未刷新，球窗会
    //    瞬时只剩「空描边矩形」（该现象已被实拍录到帧，bbox=(1791,519,1885,664)，见
    //    docs/audit-2026-09-25.md §13.4）—— 起因是拖动进入 Windows 模态移动循环期间不重绘。
    //    故每轮**确实写盘**后补一次 flush_ball（改造后它廉价且幂等，可安全高频调用），抹掉该残影。
    let flush_app = app.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(POS_FLUSH_POLL_MS));
        if flush_pending_pos() {
            crate::flush_ball(&flush_app);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_url_strips_query_and_fragment() {
        // query 形式
        assert_eq!(
            redact_url("https://chat.deepseek.com/a/chat/s/x?token=ABC&redirect=/y"),
            "https://chat.deepseek.com/a/chat/s/x"
        );
        // 纯 fragment（无 query）：原实现会漏掉，2026-09-20 修正后应截断。
        assert_eq!(
            redact_url("https://chat.deepseek.com/a#access_token=SECRET"),
            "https://chat.deepseek.com/a"
        );
        // query + fragment：两者都丢弃。
        assert_eq!(redact_url("https://x.com/p?a=1#b=2"), "https://x.com/p");
        // 无 query/fragment：原样保留。
        assert_eq!(
            redact_url("https://chat.deepseek.com/"),
            "https://chat.deepseek.com/"
        );
        // 裸 host（防御）
        assert_eq!(redact_url("https://x.com"), "https://x.com");
    }

    // ---- P0：panel_state（登录态判定）—— 审计标「纯逻辑可测却零测试」，本轮补齐 ----
    //
    // 说明（为何这里不是「纯函数」但依然可测）：`panel_state` 依赖 `config::panel_host()`，
    // 后者读全局配置（磁盘/缓存）。**不对配置做任何写入**（避免污染用户真实 config.json），
    // 而是**以当前配置的 host 为基准构造 URL** —— 于是断言与「默认官方 host」还是「用户配置的
    // 镜像 host」无关，天然自适应（这也正是 2026-09-17「对域名通用」改动的语义）。
    // 登录页分支则完全不依赖 host，可硬编码断言。
    #[test]
    fn panel_state_needs_login_for_sign_in_paths() {
        // 登录页判定**优先于** host：即便 host 与面板一致，也必须报 NEEDS_LOGIN。
        let host = crate::config::panel_host();
        assert_eq!(
            panel_state(&format!("https://{host}/sign_in")),
            "NEEDS_LOGIN",
            "面板 host 自身的登录页应判 NEEDS_LOGIN"
        );
        // 非面板 host 的登录页同样是 NEEDS_LOGIN（判定先于 host 比较）。
        assert_eq!(
            panel_state("https://mirror.example.com/app/sign_in/"),
            "NEEDS_LOGIN"
        );
        assert_eq!(
            panel_state("https://anything.invalid/sign_in"),
            "NEEDS_LOGIN"
        );
    }

    #[test]
    fn panel_state_logged_in_when_host_matches_configured_panel_host() {
        // 与「当前配置的面板 host」一致的 URL → LOGGED_IN（域名无关：官方/镜像同判据）。
        let host = crate::config::panel_host();
        assert_eq!(panel_state(&format!("https://{host}/")), "LOGGED_IN");
        assert_eq!(
            panel_state(&format!("https://{host}/a/chat/s/abc?x=1#frag")),
            "LOGGED_IN",
            "path/query/fragment 不影响 host 判定"
        );
    }

    #[test]
    fn panel_state_unknown_when_host_differs_or_blank() {
        let host = crate::config::panel_host();
        // 与面板 host 不同的 host → UNKNOWN（不误报已登录）。
        assert_eq!(
            panel_state(&format!("https://not-{host}.invalid/")),
            "UNKNOWN"
        );
        // about:blank（面板创建时的初始页）：host_of 解析为 "about"，不等于任何面板 host。
        assert_eq!(panel_state("about:blank"), "UNKNOWN");
    }

    #[test]
    fn panel_state_query_sign_in_is_not_login_page() {
        // query/fragment 里出现 /sign_in 不算登录页（审核 P2：不误掐正常提问）→ 落到 host 判定。
        let host = crate::config::panel_host();
        assert_eq!(
            panel_state(&format!("https://{host}/?redirect=/sign_in")),
            "LOGGED_IN"
        );
        assert_eq!(panel_state("https://x.invalid/?next=/sign_in"), "UNKNOWN");
    }
}
