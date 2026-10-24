//! §3：划词工具条窗口（类豆包）。
//!
//! 生命周期：**setup 内常驻创建一次**，之后只 show / hide，
//! 绝不每次弹窗都创建或销毁 —— 反复建/销毁 webview 是 Windows 上
//! WebView2 窗口类反注册 abort（Error 1411/1412）的主要来源。
//!
//! 工具条按钮由**统一提示词列表**（prompts::load）动态渲染：
//!   首行：`[复制] + 前 ≤5 项提示词（内置优先，自定义可占位）+ [⋯]`；
//!   第 6 项起的提示词收进「⋮ 更多」浮层（点 ⋮ 就地扩高展开，选中即关）。
//! 「复制」是特殊动作（写回剪贴板）；其余都是提示词（内置/自定义统一，id 即数据）。
//!
//! 关闭条件：
//!   1) 点了任一动作（复制 / 提示词）→ 动作后立即关闭
//!   2) 点了「⋮」→ 就地扩高展开浮层（不关闭；点浮层项/设置才关）
//!   3) 点了「设置」→ 打开设置窗口后关闭工具条
//!   4) 点击工具条外的任意处（鼠标钩子 Dismiss）→ 立即关闭
//!   5) 8s 超时自动关闭
//!   6) 显式调用 `hide()`（新的划词覆盖旧的、或抓词失败）

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::{capture, inject};

use windows::Win32::Foundation::HWND;

pub const POPUP_LABEL: &str = "popup";
/// 首行最多常驻的提示词数量（**复制 + ≤4 项提示词 = 共 5 个按钮**）；
/// 第 5 项起并入「⋮ 更多」浮层。顺序 = 设置里 prompts.json 的列表顺序（用户可排序）。
const INLINE_MAX: usize = 4;
/// 「⋮ 更多」浮层：每行逻辑高度（行按钮 30 + 间距 2）。
const FLY_ROW_H: f64 = 32.0;
/// 浮层单屏最多显示行数（超出则容器内纵向滚动，避免高度失控顶出屏幕）。
const FLY_MAX_ROWS: usize = 8;

/// 自动关闭时间（team-lead 规定 8s）
pub const POPUP_TIMEOUT_MS: u64 = 8_000;
/// 工具条逻辑高度（紧凑：胶囊上下各留 6px，减少文字外白边）。
const POPUP_LOGICAL_H: f64 = 42.0;
/// 创建窗口时的初始宽度（每次 show 前会 set_size 到按内容估算的宽度）。
const POPUP_DEFAULT_W: f64 = 320.0;
/// 与光标之间的距离（逻辑像素）
const CURSOR_GAP: f64 = 6.0;
/// 工具条宽度范围（逻辑像素）——按内容收缩，超出则横向滚动。
/// MAX_W 收敛到 460，避免提示词较多时工具条「右侧过长」拖到屏幕外。
const MIN_W: f64 = 150.0;
const MAX_W: f64 = 460.0;

/// 本次 show 时的布局现场：供「⋮ 更多」扩高时复用锚点 / 工作区 / scale 重新定位（翻边）。
#[derive(Clone)]
struct LastShow {
    /// 光标锚点（物理像素）。
    anchor: (i32, i32),
    /// 显示器缩放因子。
    scale: f64,
    /// 工作区（物理像素）。
    work: (i32, i32, i32, i32),
    /// show 时的窗口宽（逻辑像素）。
    logical_w: f64,
}
static SHOW_LAST: Mutex<Option<LastShow>> = Mutex::new(None);

/// 每次 show 自增：用于让「在途的 8s 超时」失效，避免旧超时把新弹窗关掉。
static SHOW_GEN: AtomicU64 = AtomicU64::new(0);
/// 触发时抓到的选区文本，等用户在工具条上点按钮时取用。
static PENDING_TEXT: Mutex<Option<String>> = Mutex::new(None);
/// 抓词串行锁：多个触发源（鼠标钩子回调等）可能并发触发，剪贴板读/写不可重入，
/// 用 try_lock 让并发中的第二次触发直接跳过（而不是去抢剪贴板）。
static TRIGGER_GUARD: Mutex<()> = Mutex::new(());
/// 工具条当前是否可见（供 mousehook 判断「点击是否落在工具条外」以自动隐藏）。
static POPUP_VISIBLE: AtomicBool = AtomicBool::new(false);
/// 工具条的屏幕矩形（物理像素）(left, top, right, bottom)，show 时更新。
static POPUP_RECT: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);

/// 自动关闭定时器：单一常驻线程 + 消息驱动，取代原先「每次 show 起一个 8s 线程」的做法
/// （原实现每个工具条弹窗都 spawn 一个线程，频繁划词会堆积大量短命线程）。
/// show 时投递 `ArmMsg{generation,deadline}`；线程到点检查 SHOW_GEN 是否仍是该 generation，
/// 是则 hide（期间有新的 show 会发新 ArmMsg，旧消息的 generation 已失效，自然作废，不会误关新弹窗）。
static AUTO_CLOSE_TX: Mutex<Option<Sender<ArmMsg>>> = Mutex::new(None);

struct ArmMsg {
    generation: u64,
    deadline: Instant,
}

/// 启动自动关闭常驻线程（在 ensure 首次创建 popup 时调用一次）。
fn start_auto_close(app: AppHandle) {
    let (tx, rx) = channel::<ArmMsg>();
    if let Ok(mut g) = AUTO_CLOSE_TX.lock() {
        *g = Some(tx);
    }
    std::thread::spawn(move || {
        let mut pending: Option<ArmMsg> = None;
        loop {
            if let Some(msg) = pending.take() {
                let now = Instant::now();
                if now >= msg.deadline {
                    if SHOW_GEN.load(Ordering::SeqCst) == msg.generation {
                        crate::diag::log("popup 自动关闭：超时到达，隐藏工具条");
                        hide(&app);
                    }
                    continue;
                }
                match rx.recv_timeout(msg.deadline - now) {
                    Ok(new) => pending = Some(new),
                    Err(RecvTimeoutError::Timeout) => pending = Some(msg),
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            } else {
                // 首项 ArmMsg 不能丢：必须用 Ok(new) 收下，否则常驻线程会一直阻塞在 recv 上
                // 导致工具条永不自动关闭（原实现此处写成 `else if let Err(_) = rx.recv() { break }`，丢弃了首条消息）。
                match rx.recv() {
                    Ok(new) => pending = Some(new),
                    Err(_) => break,
                }
            }
        }
    });
}

/// 工具条当前是否可见。
pub fn popup_visible() -> bool {
    POPUP_VISIBLE.load(Ordering::SeqCst)
}

/// 判断某屏幕坐标（物理像素）是否落在工具条内。不可见时返回 false。
pub fn point_in_popup(x: i32, y: i32) -> bool {
    if !POPUP_VISIBLE.load(Ordering::SeqCst) {
        return false;
    }
    POPUP_RECT
        .lock()
        .ok()
        .and_then(|g| *g)
        .map(|(l, t, r, b)| x >= l && x <= r && y >= t && y <= b)
        .unwrap_or(false)
}

/// setup 内调用：常驻创建一次（`visible=false`）。
pub fn ensure(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(POPUP_LABEL) {
        return Some(w);
    }
    match WebviewWindowBuilder::new(app, POPUP_LABEL, WebviewUrl::App("poc/popup.html".into()))
        .title("DeepSeek 划词工具条")
        .inner_size(POPUP_DEFAULT_W, POPUP_LOGICAL_H)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        // 不给 resize 子窗口类任何机会：undecorated + resizable 正是 panel 上
        // 那个 TAURI_DRAG_RESIZE_* 子窗口类（Error 1411/1412）的成因。
        .resizable(false)
        .shadow(false)
        // 免打断工具条：显示时不抢走当前应用的焦点（否则用户刚划的选区会因失焦而丢）
        .focusable(false)
        .visible(false)
        .build()
    {
        Ok(w) => {
            crate::diag::log(&format!(
                "popup 窗口创建成功（常驻，visible=false，初始宽={} h={}）",
                POPUP_DEFAULT_W, POPUP_LOGICAL_H
            ));
            // 启动常驻自动关闭线程（线程复用：取代原先「每次弹窗起一个 8s 线程」的做法）
            start_auto_close(app.clone());
            Some(w)
        }
        Err(e) => {
            crate::diag::error(&format!("popup 窗口创建失败: {e}"));
            None
        }
    }
}

/// 光标屏幕坐标（物理像素）。
pub fn cursor_pos() -> (i32, i32) {
    let mut p = windows::Win32::Foundation::POINT::default();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

/// ★ 纯函数：把一个尺寸为 `size` 的浮窗放在 `prefer`（首选左上角，物理像素）。
///
/// 越界时**单轴翻转**：右越界 → 翻到锚点左侧；下越界 → 翻到锚点上方；最后夹取到工作区
/// （工作区比窗口还小时以工作区左上角为准，保证不会跑到屏幕外）。
///
/// 抽成**共享纯函数**供工具条（[`show_at_cursor`]）与结果卡（`result::show`）复用：
/// 两处的翻边规则必须逐字一致，否则同一块屏幕边缘上两窗行为会分叉（设计文档 §5.2 明确
/// 要求「不复制粘贴」）。无 Tauri 依赖 ⇒ 四边与极窄工作区都可单测。
///
/// 参数：`prefer=(首选左上角)`、`anchor=(锚点矩形 x,y,w,h)`、`size=(窗宽,窗高)`、
/// `work=(工作区 x,y,w,h)`、`gap=与锚点的间隙`。全部为**物理像素**。
pub fn place_near(
    prefer: (i32, i32),
    anchor: (i32, i32, i32, i32),
    size: (i32, i32),
    work: (i32, i32, i32, i32),
    gap: i32,
) -> (i32, i32) {
    let (px, py) = prefer;
    let (ax, ay, _, _) = anchor;
    let (w, h) = size;
    let (wx, wy, ww, wh) = work;
    let mut x = px;
    let mut y = py;
    // 右边放不下 → 翻到锚点左侧；下边放不下 → 翻到锚点上方。
    if x + w > wx + ww {
        x = ax - w - gap;
    }
    if y + h > wy + wh {
        y = ay - h - gap;
    }
    // 极端情况（窗口比工作区还大）也要保证可见。
    x = x.clamp(wx, (wx + ww - w).max(wx));
    y = y.clamp(wy, (wy + wh - h).max(wy));
    (x, y)
}

/// 按文字估算按钮宽度（逻辑像素）。13px 字体：CJK ≈13px/字，ASCII ≈7.5px/字；
/// 按钮 = 左右 padding 18 + 图标 14 + 间距 5 + 文字。
fn text_px(s: &str) -> f64 {
    s.chars()
        .map(|c| if c.is_ascii() { 7.5 } else { 13.0 })
        .sum()
}
fn button_px(label: &str) -> f64 {
    31.0 + text_px(label) // 内边距 12(6×2) + 图标 14 + 图标/文字间距 5
}
/// 「复制」「⋯」按钮的固定宽度。
const COPY_PX: f64 = 63.0;
const MORE_PX: f64 = 30.0;
/// .bar 左右内边距合计 8px（4×2），按钮间距 3px，安全余量 8px。
const BAR_PAD: f64 = 8.0;
const BTN_GAP: f64 = 3.0;
const WIDTH_MARGIN: f64 = 8.0;

/// 工具条总宽：复制 + **首行内置提示词** + ⋯。自定义并入「⋮ 更多」浮层，不参与宽度估算，
/// 因此首行宽度稳定（不会随自定义数量增长）。clamp 到 [MIN_W, MAX_W]。
fn toolbar_width(labels: impl Iterator<Item = String>) -> f64 {
    let mut w = BAR_PAD + COPY_PX;
    let mut n = 1.0; // copy 按钮
    for label in labels {
        w += button_px(&label);
        n += 1.0;
    }
    w += MORE_PX;
    n += 1.0; // ⋯ 按钮
    w += BTN_GAP * (n - 1.0);
    (w + WIDTH_MARGIN).clamp(MIN_W, MAX_W)
}

/// 在光标附近弹出工具条：先把**未隐藏**的提示词推给前端渲染，再按内容收缩宽度 + 物理定位 + 翻边。
pub fn show_at_cursor(app: &AppHandle) {
    let Some(win) = ensure(app) else {
        return;
    };

    // 推「未隐藏」的提示词列表（顺序 = 设置里的列表顺序，用户可排序）给前端：
    // 前 ≤4 项进首行、其余进「⋮ 更多」浮层。
    let prompts = crate::prompts::load(app);
    let visible: Vec<_> = prompts.into_iter().filter(|p| !p.hidden).collect();
    let list_json = serde_json::to_string(&visible).unwrap_or_else(|_| "[]".into());
    let _ = win.eval(format!(
        "if(window.__dsfSetPrompts)window.__dsfSetPrompts({list_json});"
    ));

    // 按首行前 ≤4 项（复制之后的提示词）估算宽度；第 5 项起进浮层，不占首行宽。
    let logical_w = toolbar_width(visible.iter().take(INLINE_MAX).map(|p| p.label.clone()));
    let logical_h = POPUP_LOGICAL_H;
    if let Err(e) = win.set_size(LogicalSize::new(logical_w, logical_h)) {
        crate::diag::log(&format!(
            "popup set_size({logical_w},{logical_h}) 失败: {e}"
        ));
    }

    let (cx, cy) = cursor_pos();
    let monitor = app.monitor_from_point(cx as f64, cy as f64).ok().flatten();
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let w = (logical_w * scale).round() as i32;
    let h = (logical_h * scale).round() as i32;
    let gap = (CURSOR_GAP * scale).round() as i32;

    // 首选光标右下方；越界则单轴翻转，最后夹取到工作区。
    // ★ 与结果卡共用 `place_near`（同一纯函数）—— 光标点即「宽高为 0 的锚点矩形」，
    //   故这里的行为与抽取前的内联实现**逐字等价**（含 clamp 口径）。
    let (x, y) = match monitor.as_ref() {
        Some(m) => {
            let area = m.work_area();
            let work = (
                area.position.x,
                area.position.y,
                area.size.width as i32,
                area.size.height as i32,
            );
            // 存下布局现场，供「⋮ 更多」扩高时复用锚点/工作区/scale 重新定位（翻边）。
            if let Ok(mut g) = SHOW_LAST.lock() {
                *g = Some(LastShow {
                    anchor: (cx, cy),
                    scale,
                    work,
                    logical_w,
                });
            }
            place_near((cx + gap, cy + gap), (cx, cy, 0, 0), (w, h), work, gap)
        }
        // 取不到显示器信息：保持原行为（不 clamp）。
        None => (cx + gap, cy + gap),
    };

    let _ = win.set_position(PhysicalPosition::new(x, y));
    // ★ 划词工具条不能抢走用户正在编辑的输入框的键盘焦点：
    //   Tauri `show()` 对 always_on_top 无边框窗即便 focusable(false) 仍可能让原窗口失焦，
    //   导致用户划词后在原输入框按 Backspace 无法删除（焦点已不在输入框）。
    //   故显示前记录当前前台窗口，显示后立即把键盘焦点归还给原窗口。
    let prev_fg = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    if let Err(e) = win.show() {
        crate::diag::warn(&format!("popup show 失败: {e}"));
        return;
    }
    // 归还前台：不判断 hwnd 有效性——对已失效/不存在窗口 SetForegroundWindow 会静默失败，无害。
    let prev_fg = prev_fg.0 as isize;
    unsafe {
        let _ =
            windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(HWND(prev_fg as *mut _));
    }
    // ★ 防 WebView2 **异步**抢焦点（2026-09-19）：`show()` 后 WebView2 浏览器进程仍可能通过
    //   MoveFocus 把输入焦点搬进工具条内容（即便窗口带 WS_EX_NOACTIVATE），即时归还会被它随后覆盖
    //   —— 这是「划词后在原输入框按 Backspace 删不掉」此前的遗留根因。
    //   延迟再抢两次（150ms/300ms），**仅当「当前前台仍是工具条」时**才归还 prev_fg，
    //   避免把用户已切走的窗口抢回来（守卫式，不打扰）。
    //   注：`*mut c_void` 非 Send，跨线程只传 `isize` 原子值，在 for 循环内重建 HWND。
    let ph = win.hwnd().ok().map(|h| h.0 as isize);
    if let Some(ph) = ph {
        std::thread::spawn(move || {
            for _ in 0..2 {
                std::thread::sleep(Duration::from_millis(150));
                unsafe {
                    let fg = windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
                    if fg.0 as isize == ph {
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(HWND(
                            prev_fg as *mut _,
                        ));
                    }
                }
            }
        });
    }
    // 记录可见状态与屏幕矩形，供 mousehook 判定「点击在工具条外 → 自动隐藏」
    if let Ok(mut g) = POPUP_RECT.lock() {
        *g = Some((x, y, x + w, y + h));
    }
    POPUP_VISIBLE.store(true, Ordering::SeqCst);
    crate::diag::log(&format!(
        "popup 已 show（宽 {} 逻辑像素，首行 {} 个按钮，位于 ({x},{y}) scale={scale}）",
        logical_w,
        visible.len().min(INLINE_MAX) + 2
    ));
    // 每次弹窗成功显示后，复位前端的一次性锁 fired + 收起浮层（popup.html 暴露的全局函数）。
    let _ = win.eval("if (window.__dsfResetFired) window.__dsfResetFired(); if (window.__dsfHideMore) window.__dsfHideMore();");
    arm_auto_close(app, POPUP_TIMEOUT_MS);
}

/// 「⋮ 更多」：把常驻工具条窗口**就地扩高**展开自定义浮层，并按「新高度」重新翻边定位。
///
/// 不关闭、不消费待发文本 —— 用户点完浮层里的某项（或点“设置”）才真正关（走 on_action）。
/// 复用 show 时存下的锚点/工作区/scale，扩高后用 `place_near` 对新高度重新夹取（防止顶出屏幕）。
fn expand_more(app: &AppHandle) {
    let Some(win) = app.get_webview_window(POPUP_LABEL) else {
        return;
    };
    let Some(last) = SHOW_LAST.lock().ok().and_then(|g| (*g).clone()) else {
        crate::diag::warn("popup: open-more 但缺少 show 布局现场，忽略");
        return;
    };
    let list = crate::prompts::load(app);
    let labels: Vec<String> = list
        .iter()
        .filter(|p| !p.hidden)
        .map(|p| p.label.clone())
        .collect();
    let n = labels.len().saturating_sub(INLINE_MAX).min(FLY_MAX_ROWS);
    // ★ 顶栏保留：窗口宽 = 顶栏宽；高度 = 顶栏 42 + 右下窄下拉（窄宽由 CSS 的 max-content 决定）。
    let new_h =
        POPUP_LOGICAL_H + 12.0 + (n as f64 + 1.0) * FLY_ROW_H + if n > 0 { 7.0 } else { 0.0 };
    if let Err(e) = win.set_size(LogicalSize::new(last.logical_w, new_h)) {
        crate::diag::log(&format!(
            "popup 展开 set_size({}, {new_h:.0}) 失败: {e}",
            last.logical_w
        ));
    }
    let gap = (CURSOR_GAP * last.scale).round() as i32;
    let w = (last.logical_w * last.scale).round() as i32;
    let h = (new_h * last.scale).round() as i32;
    let (cx, cy) = last.anchor;
    let (x, y) = place_near((cx + gap, cy + gap), (cx, cy, 0, 0), (w, h), last.work, gap);
    let _ = win.set_position(PhysicalPosition::new(x, y));
    if let Ok(mut g) = POPUP_RECT.lock() {
        *g = Some((x, y, x + w, y + h));
    }
    POPUP_VISIBLE.store(true, Ordering::SeqCst);
    let _ = win.eval("if (window.__dsfShowMore) window.__dsfShowMore();");
    crate::diag::log(&format!(
        "popup 已展开「更多」（宽 {} 高 {new_h:.0} 逻辑像素，{} 项进下拉，位于 ({x},{y})）",
        last.logical_w,
        labels.len().saturating_sub(INLINE_MAX)
    ));
}

/// 复用常驻自动关闭线程（start_auto_close 在 ensure 内启动一次）：
/// 通过 AUTO_CLOSE_TX 投递 ArmMsg{generation, deadline} 取代「每次弹窗起一个线程」，
/// 避免频繁划词堆积大量短命线程（线程复用）。常驻线程到点再按 SHOW_GEN 判定是否仍有效。
fn arm_auto_close(app: &AppHandle, timeout_ms: u64) {
    let generation = SHOW_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let msg = ArmMsg {
        generation,
        deadline: Instant::now() + Duration::from_millis(timeout_ms),
    };
    // 常驻线程未就绪（极端竞态：ensure 尚未调用 start_auto_close）时降级为一次性线程兜底，
    // 保证 8s 超时仍生效，不会因重构引入「永不关闭」的回归。
    let sent = AUTO_CLOSE_TX
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|tx| tx.send(msg).is_ok()))
        .unwrap_or(false);
    if !sent {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(timeout_ms));
            if SHOW_GEN.load(Ordering::SeqCst) == generation {
                hide(&app);
            }
        });
    }
}

/// 关闭工具条：让在途超时失效 + 清可见标记 + hide。
///
/// ★ 修「首次点悬浮球，球背后透出划词工具条（白底「复制 … ⋮」）」：工具条同为
///   `transparent + always_on_top` 窗，`hide()` 只置 `WS_HIDE`，其合成表面回收有滞后 ——
///   透明球会读到工具条的**陈旧合成帧**（Click-through 的观感）。与 `hide_panel` 同机制：
///   hide 后强制球重绘一次，冲刷残留旧帧。这正是「**第二次点击就不出现**」里 `hide_panel`
///   已验证有效的那一步 —— 此前**只**加在 panel 侧，工具条侧缺失。
pub fn hide(app: &AppHandle) {
    SHOW_GEN.fetch_add(1, Ordering::SeqCst);
    POPUP_VISIBLE.store(false, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(POPUP_LABEL) {
        let _ = w.hide();
    }
    // 冲刷残留在透明球表面的工具条旧帧（消「悬浮球背景渗出工具条」）。
    crate::flush_ball(app);
    crate::diag::log("popup 已 hide 并冲刷球合成表面");
}

pub fn set_pending_text(text: String) {
    if let Ok(mut g) = PENDING_TEXT.lock() {
        *g = Some(text);
    }
}

fn take_pending_text() -> Option<String> {
    PENDING_TEXT.lock().ok().and_then(|mut g| g.take())
}

/// 仅读取不消费——供设置窗口展示预览而不会清空。
pub fn peek_pending_text() -> Option<String> {
    PENDING_TEXT.lock().ok().and_then(|g| g.clone())
}

/// 清空待发选区文本（不读不消费）。用于「本应用窗口内划词」守卫：
/// 该分支会跳过抓词并提前返回，若之前残留了上一次成功抓词的 `PENDING_TEXT`，
/// 不清空会被后续动作（如设置页运行 / 工具条按钮）误消费陈旧文本。
pub(crate) fn clear_pending_text() {
    if let Ok(mut g) = PENDING_TEXT.lock() {
        *g = None;
    }
}

/// 检查光标当前是否落在本应用**自己的某个窗口**内（settings / result / panel / popup / ball …）。
///
/// ★ 2026-09-19 修「设置页输入框划词后 Backspace 失效」：用户在**本应用自己的输入框**里
///   拖选（如设置页「面板网址」）时，Selection 也会被鼠标钩子触发 → 弹起工具条。此时无论
///   工具条是否抢焦点，都会干扰正在编辑的输入框。故**本应用窗口内的划词一律不弹工具条**，
///   （用户在自己的设置/面板里改文本是真编辑，不是「划词查询」，弹条既无意义又可能抢焦点）。
///
/// 用 `WindowFromPoint` 取光标处最顶层窗口，再沿祖先链（GetAncestor GA_ROOT）看是否等于
/// 本应用任一窗口的 HWND。纯 Win32，可在鼠标钩子的 worker 线程调用。
fn cursor_in_own_window(app: &AppHandle) -> bool {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, WindowFromPoint, GA_ROOT};
    let (cx, cy) = cursor_pos();
    let hwnd = unsafe { WindowFromPoint(POINT { x: cx, y: cy }) };
    if hwnd.is_invalid() {
        return false;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    for win in app.webview_windows().values() {
        if let Ok(h2) = win.hwnd() {
            if root.0 == h2.0 {
                return true;
            }
        }
    }
    false
}

/// 划词抓取的唯一入口（由全局鼠标钩子的 Selection 事件触发）：
/// 抓当前选区 → 成功则弹工具条；失败只落诊断日志（用户已要求不要浮层提示）。
///
/// ⚠️ 本函数会阻塞约 400ms（剪贴板轮询），**只能从鼠标钩子的 worker 线程调用**，不要放主线程。
pub fn trigger_selection(app: &AppHandle) {
    // 串行化：并发触发时第二次直接跳过（剪贴板操作不可重入）
    let Ok(_guard) = TRIGGER_GUARD.try_lock() else {
        crate::diag::log("trigger_selection 跳过（上一次抓词仍在进行）");
        return;
    };
    // ★ 前置自身窗口守卫（2026-09-19 修复强化）：在任何抓词前先判定光标是否落在本应用自身窗口
    //   （悬浮球 / 工具条 / 结果卡 / 设置页 / 面板）内。命中则直接跳过 —— 拖动 ball、在设置页
    //   或面板输入框里拖选都是「真编辑」，并非「划词查询」，无需合成 Ctrl+Insert 去临时劫持剪贴板。
    //   原实现把这一判断放「抓词成功后」，代价是自身窗口内动作仍会执行一次剪贴板读改写。
    if cursor_in_own_window(app) {
        crate::diag::log("划词发生在本应用窗口内，跳过抓词（避免劫持剪贴板）");
        // ★ MEDIUM-3 修复（2026-09-21）：跳过抓词前先清空残留的 PENDING_TEXT，
        //   避免上一次成功抓词留下的旧文本被后续动作误消费。
        clear_pending_text();
        hide(app);
        return;
    }
    crate::diag::log("trigger_selection 进入（鼠标钩子 worker 线程）");

    let cfg = crate::config::load();
    let _ = capture::take_last_raw();
    let outcome = selection_with_policy(
        cfg.privileged_capture,
        cfg.clipboard_fallback_enabled,
        crate::uia::uia_selection_text,
        capture::set_last_raw,
        || capture::capture_selection(capture::POLL_TIMEOUT_MS, capture::MAX_CHARS),
    );

    match outcome {
        capture::CaptureOutcome::Ok(text) => {
            // 自身窗口守卫已前置到函数开头（见 L509），能走到这里即非自身窗口动作，直接弹工具条。
            crate::diag::log(&format!(
                "抓词成功，{} 字符，准备弹工具条",
                text.chars().count()
            ));
            set_pending_text(text);
            show_at_cursor(app);
        }
        other => {
            crate::diag::log(&format!("抓词失败/非成功: {other:?}"));
            // 抓词失败：不弹工具条。用户已明确要求「不要浮层提示」→ 只落诊断日志（降级矩阵仍可见于 d1-debug.log）
            if let Some(msg) = other.user_message() {
                crate::diag::warn(&format!("划词失败：{msg}"));
            }
            hide(app);
        }
    }
}

/// UIA 始终先于获准的回退；关闭回退时不调用任何自动复制 API。
fn selection_with_policy(
    use_uia: bool,
    allow_fallback: bool,
    uia: impl FnOnce() -> Option<String>,
    remember_raw: impl FnOnce(String),
    fallback: impl FnOnce() -> capture::CaptureOutcome,
) -> capture::CaptureOutcome {
    if use_uia {
        if let Some(raw) = uia() {
            let text = capture::normalize_text(&raw);
            if text.chars().count() >= capture::MIN_CHARS {
                remember_raw(raw);
                return capture::CaptureOutcome::Ok(
                    text.chars().take(capture::MAX_CHARS).collect(),
                );
            }
        }
    }
    if crate::release_policy::automatic_copy_allowed(allow_fallback) {
        fallback()
    } else {
        // 不将禁用回退误报为一次成功捕获。
        crate::diag::log("自动复制回退已关闭，UIA 未提供有效选区，跳过抓词");
        capture::CaptureOutcome::NoNewCopy
    }
}

/// 工具条上按钮被点击（`popup-action` 事件）的统一入口。
///
/// preset_name 的合法取值（与 `poc/popup.html` 的 `data-preset` 一一对应）：
///   - `copy`：特殊动作，写回剪贴板；
///   - `open-settings`：打开设置窗口；
///   - 其余：统一提示词列表里的 id（内置 explain/translate/summarize/ask 与自定义 custom_*），
///     查 prompts 后 compose_custom + 注入面板。
///
/// 硬约束：绝不发送空内容。
pub fn on_action(app: &AppHandle, preset_name: &str) {
    if preset_name == "open-more" {
        // 「⋮ 更多」：若有溢出项才就地扩高展开浮层；无溢出则直接打开设置（避免空浮层「点了没东西」）。
        let overflow = {
            let list = crate::prompts::load(app);
            list.iter()
                .filter(|p| !p.hidden)
                .count()
                .saturating_sub(INLINE_MAX)
        };
        if overflow > 0 {
            expand_more(app);
        } else {
            crate::settings::open(app);
            hide(app);
        }
        return;
    }

    if preset_name == "open-settings" {
        // 打开设置窗口：保留 pending text（设置页要预览/运行都要用到）
        crate::settings::open(app);
        hide(app);
        return;
    }

    // 常规动作：先关闭工具条，再消费选区文本
    hide(app);
    let Some(text) = take_pending_text() else {
        crate::diag::warn("[M2] popup action 触发但待发文本已丢失");
        return;
    };

    // 复制：写回系统剪贴板（不注入面板）
    if preset_name == "copy" {
        // ★ 排版保真：优先用抓词时旁路留存的**原文**，以保留换行 / 缩进 / 制表符。
        //   `text`（PENDING_TEXT）是 normalize_text 的产物，所有空白已被折叠为单空格，
        //   直接复制会丢掉 Markdown 的列表 / 段落 / 代码块结构。
        //   取不到原文（如 UIA 直读路径）时退化为 `text`，行为与改动前一致，绝不失败。
        //
        // ★ 自校验（防止 LAST_RAW 与 PENDING_TEXT 错配）：仅当原文**确属本次划词**时才用 raw，
        //   否则视为陈旧缓存，退化为 `text`。
        //   判据：`normalize(raw)` 以 `text` 为前缀。两种合法形态都满足——
        //     ① 正常：两者全等，前缀恒成立；
        //     ② 超长：`text` 是 `normalize(raw)` 截断到 MAX_CHARS 的前缀（原文不截断，见方案取舍 1）。
        //   而「上一条划词的原文」其归一化结果**不会**以当前 text 为前缀（内容不同），故被挡住。
        //   代价：复制时多算一次 normalize（O(n)，n ≤ 一次选区长度），可忽略。
        let (raw, source) = match capture::take_last_raw() {
            Some(r) if capture::raw_matches_pending(&r, &text) => (r, "raw"),
            Some(_) => (text, "normalized(缓存不匹配)"),
            None => (text, "normalized(无原文)"),
        };
        if capture::set_clipboard_text(&raw) {
            crate::diag::log(&format!(
                "popup: 已复制 {} 字符到剪贴板（来源={source}）",
                raw.chars().count()
            ));
        } else {
            crate::diag::warn("popup: 复制失败（剪贴板被占用）");
        }
        return;
    }

    // 其余都是提示词 id：内置 + 自定义统一，查列表拿指令
    let list = crate::prompts::load(app);
    let Some(p) = list.iter().find(|p| p.id == preset_name) else {
        crate::diag::warn(&format!("[M2] popup 收到未知 preset: {preset_name}"));
        return;
    };
    let prompt = capture::compose_custom(&p.instruction, &text);
    send_prompt(app, prompt, p.label.clone(), text, p.show_card);
}

/// 设置窗口里点「运行」某条提示词时调用（与 on_action 的提示词分支等价，走 settings-run 事件）。
pub fn run_prompt(app: &AppHandle, id: &str) {
    let Some(text) = take_pending_text() else {
        crate::diag::warn("settings-run: 待发文本已丢失（需重新划词）");
        return;
    };
    let list = crate::prompts::load(app);
    let Some(p) = list.iter().find(|p| p.id == id) else {
        crate::diag::warn(&format!("settings-run: 提示词不存在或已删除: {id}"));
        return;
    };
    let prompt = capture::compose_custom(&p.instruction, &text);
    let label = p.label.clone();
    let show_card = p.show_card;
    send_prompt(app, prompt, label.clone(), text, show_card);
    crate::diag::log(&format!(
        "settings-run: 已运行「{label}」（交付方式={}）",
        if show_card { "结果卡" } else { "面板" }
    ));
}

/// 共用的「发送」动作：硬约束空内容、**按提示词的交付方式分流**、注入。
///
/// ★ `show_card`（`Prompt::show_card`，设置页逐条可改）决定本条提示词走哪条路：
///   - `true`  = **显示结果卡片**：**不弹面板**（只保证远端页已加载），注入后起回读把回答搬上卡片；
///   - `false` = **弹出面板**：唤出面板并聚焦，注入后**不起回读**（用户自己在面板里看回答）。
///
/// ★ `label` / `quote` 只用于结果卡的 UI 文案（标题与引用原句），不参与任何判据。
fn send_prompt(app: &AppHandle, prompt: String, label: String, quote: String, show_card: bool) {
    if prompt.trim().is_empty() {
        crate::diag::warn("send_prompt: 待发送内容为空，已取消");
        return;
    }
    if !show_card {
        // ---- 交付方式 = 面板：弹出来让用户自己看，不起回读、不出卡片 ----
        let Some(panel) = crate::show_panel(app) else {
            crate::diag::warn("send_prompt: 面板窗口未创建（见 poc2-state.log）");
            return;
        };
        let _ = panel.show();
        if let Err(e) = panel.set_focus() {
            crate::diag::log(&format!(
                "send_prompt: 面板 set_focus 失败（注入可能因未聚焦而不发 Ctrl+V 兜底）: {e}"
            ));
        }
        inject::inject_and_send_async(panel, prompt);
        crate::diag::log(&format!(
            "send_prompt: 「{label}」已注入面板并唤起（交付方式=面板，不起回读）"
        ));
        return;
    }

    // ---- 交付方式 = 结果卡：**不弹面板**，只保证远端页已加载 ----
    let Some(panel) = crate::ensure_panel_loaded(app) else {
        crate::diag::warn("send_prompt: 面板窗口未创建（见 poc2-state.log）");
        return;
    };
    // ★ P1 修复（2026-09-25 审计）：结果卡路径此前只 `ensure_panel_loaded`（绝不 show/unminimize）。
    //   若面板处于**最小化**态，WebView2 渲染被节流、`__DSFLOAT__` 未就绪 → 注入 8 次耗尽 →
    //   静默放弃 + 超时交付**空结果卡**（用户实测「面板里明明回答了，卡片却空白」）。
    //   最小化 ≠ 隐藏（隐藏态 WebView2 仍渲染，见 `ensure_panel_loaded` 注释）；此处先恢复渲染。
    //   注：恢复后面板会短暂可见（罕见路径：用户把面板最小化后触发结果卡），属可接受权衡。
    if panel.is_minimized().unwrap_or(false) {
        let _ = panel.unminimize();
        crate::diag::log("send_prompt(结果卡): 面板此前最小化，已先 unminimize 以保注入成功");
    }
    inject::inject_and_send_async(panel, prompt.clone());
    crate::diag::log(
        "send_prompt: **不弹面板**（交付方式=结果卡）；若日志随后出现「没找到输入框」，说明本轮尚未加载 DeepSeek，请先展开一次面板完成登录",
    );
    // ★ 结果卡闭环（R3）：**此前这条主链路根本没接回读** —— 只有托盘「诊断：发送并回读一轮」
    //   与卡片上的「继续提问」会 begin_watch，于是用户划词 → 点工具条 → 面板里回答了，
    //   却**永远等不到结果卡**。注入与回读必须成对出现，这才是完整链路。
    crate::readback::begin_watch(
        app,
        prompt,
        None,
        crate::readback::WatchOpts {
            tag: "popup".to_string(),
            source: format!("划词 · {label}"),
            title: format!("这里是我的{label}"),
            quote,
            ..Default::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_fallback_policy_never_copies_without_opt_in() {
        for use_uia in [false, true] {
            for raw in [None, Some(String::new()), Some("x".into())] {
                let outcome = selection_with_policy(
                    use_uia,
                    false,
                    || raw,
                    |_| panic!("invalid UIA must not save raw text"),
                    || panic!("automatic copy must not run"),
                );
                assert!(matches!(outcome, capture::CaptureOutcome::NoNewCopy));
            }
        }
    }

    #[test]
    fn public_build_rejects_fallback_even_when_requested() {
        for uia in [false, true] {
            let outcome = selection_with_policy(
                uia,
                true,
                || None,
                |_| panic!("No UIA text"),
                || panic!("Public build must never invoke automatic clipboard capture"),
            );
            assert_eq!(outcome, capture::CaptureOutcome::NoNewCopy);
        }
    }

    #[test]
    fn clipboard_fallback_policy_keeps_uia_priority_and_raw_copy_content() {
        for allow in [false, true] {
            let raw = "第一行\n  第二行 😀";
            let mut saved = None;
            let outcome = selection_with_policy(
                true,
                allow,
                || Some(raw.into()),
                |text| saved = Some(text),
                || panic!("UIA already succeeded"),
            );
            assert!(
                matches!(outcome, capture::CaptureOutcome::Ok(text) if text == capture::normalize_text(raw))
            );
            assert_eq!(saved.as_deref(), Some(raw));
        }
    }

    #[test]
    fn clipboard_fallback_policy_never_runs_requested_fallback_in_public_build() {
        for use_uia in [false, true] {
            let calls = std::cell::Cell::new(0);
            let outcome = selection_with_policy(
                use_uia,
                true,
                || None,
                |_| panic!("missing UIA must not save raw text"),
                || {
                    calls.set(calls.get() + 1);
                    capture::CaptureOutcome::Ok("synthetic fallback".into())
                },
            );
            assert_eq!(outcome, capture::CaptureOutcome::NoNewCopy);
            assert_eq!(calls.get(), 0);
        }
    }

    /// 工作区 1920×1080 @ (0,0)。
    const WORK: (i32, i32, i32, i32) = (0, 0, 1920, 1080);

    #[test]
    fn place_near_keeps_prefer_when_room_available() {
        // 空间充裕 → 原样落在首选位置（工具条：光标右下）。
        let (x, y) = place_near((408, 308), (400, 300, 0, 0), (320, 42), WORK, 8);
        assert_eq!((x, y), (408, 308));
    }

    #[test]
    fn place_near_flips_left_at_right_edge() {
        // 锚点贴右边界 → 翻到锚点左侧。
        let (x, y) = place_near((1888, 100), (1880, 100, 0, 0), (320, 42), WORK, 8);
        assert_eq!(x, 1880 - 320 - 8);
        assert_eq!(y, 100);
    }

    #[test]
    fn place_near_flips_up_at_bottom_edge() {
        // 锚点贴下边界 → 翻到锚点上方。
        let (x, y) = place_near((100, 1078), (100, 1070, 0, 0), (320, 42), WORK, 8);
        assert_eq!(x, 100);
        assert_eq!(y, 1070 - 42 - 8);
    }

    #[test]
    fn place_near_rect_anchor_sits_below() {
        // 结果卡：锚点矩形**正下方**、左边缘对齐。
        let (x, y) = place_near((400, 408), (400, 300, 200, 100), (380, 260), WORK, 8);
        assert_eq!((x, y), (400, 408));
    }

    #[test]
    fn place_near_rect_anchor_flips_up_at_bottom() {
        // 选区贴近底部 → 卡片翻到选区上方。
        let (x, y) = place_near((400, 1088), (400, 1000, 200, 80), (380, 260), WORK, 8);
        assert_eq!(x, 400);
        assert_eq!(y, 1000 - 260 - 8);
    }

    #[test]
    fn place_near_clamps_when_work_area_narrower_than_window() {
        // 极端：窗口比工作区还大 → 夹到工作区左上角，不得为负。
        let work = (10, 20, 200, 150);
        let (x, y) = place_near((0, 0), (0, 0, 0, 0), (380, 260), work, 8);
        assert_eq!((x, y), (10, 20));
    }

    #[test]
    fn toolbar_width_estimation_is_clamped() {
        // 工具条宽度估算仍在 [MIN_W, MAX_W] 区间（回归：抽函数不应影响该口径）。
        assert_eq!(toolbar_width(std::iter::empty()), MIN_W);
        let many = (0..40).map(|i| format!("提示词{i}")).collect::<Vec<_>>();
        assert_eq!(toolbar_width(many.into_iter()), MAX_W);
    }
}
