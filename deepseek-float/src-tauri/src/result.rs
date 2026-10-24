//! §5.2：结果卡窗（R2）—— 第 6 个常驻窗（ball / panel / popup / settings / shot / result）。
//!
//! 职责：把 R1 回读到的「本轮回答」显示在**锚点附近**（截图选区正下方 / 划词光标右下方），
//! 支持**流式增量推送**（答案边生成边长出来），把「思考过程」与「超时降级」如实搬到 UI 上
//! （标注「回答可能未完成」——与 W-V5 的「不静默」同源），并在卡片内提供**继续提问输入框**：
//! 追问的回答回流成卡片的**下一轮**（R5：卡片从「单轮结果」变成「迷你会话」）。
//!
//! ## 会话模型（R5 起，本模块的核心）
//!
//! 卡片的内容 = 一份 [`Session`]（标题 + 来源 + `Vec<Turn>`），**会话是卡片内容的唯一真相**：
//! 页内不做任何业务状态的二次维护，只负责把会话渲染成 DOM。
//!
//! 每次更新都带一个 [`TurnMode`]，`show` 据此把视图「并入」会话：
//!   - [`TurnMode::Fresh`]  = 开新会话（划词 / 识文字 / 设置页运行）→ **清空**后插入首轮；
//!   - [`TurnMode::Follow`] = 追加一轮（卡片内追问的**骨架**，由 [`ask`] 立刻插入）；
//!   - [`TurnMode::Update`] = 更新**最后一轮**（流式增量与终态交付都走这条）。
//!
//! 页内只有两个渲染入口，与 `full` 一一对应：`__dsfResultSet`（整份会话）/
//! `__dsfResultStream`（只换末轮的正文·思考·状态）。**只有轮次集合变化时才整份重发**，
//! 流式增量只发末轮 —— 既省字节，又不会让长会话在每次推送时被整体重建。
//!
//! 硬约束（设计文档 §八 / §七）：
//!   1) 常驻创建一次（`visible=false`），之后只 show/hide —— 绝不销毁重建（1411/1412 铁律）；
//!   2) 不给本窗设 `additional_browser_args`（同源铁律 G2）；
//!   3) `show()` 之后**不调用** `set_focus()`（R4 / W-V7：不得打断用户当前正在输入的窗口）；
//!   4) 挂 `CloseRequested → prevent_close + hide` 守卫（同 `screenshot.rs::ensure`）；
//!   5) 挂页面存活看门狗：`poc/result.html` 未就绪（dist 未重建 → 404）时自动隐藏，
//!      不留下「永久挡住点击的透明死窗」（W-V8）；
//!   6) 载荷截断 + 页内禁用 `innerHTML`（远端回答是**不可信文本**，只走 `textContent`）；
//!   7) **卡片一旦打开就不再改位置**（用户可能已把它拖走）—— 后续推送只换内容/长高；
//!      唯一的例外是「长高到会顶出工作区底部」时上移，避免内容跑到屏幕外；
//!   8) ★ R5：**用户手动关闭过的卡片不得被后续推送自动弹回**（[`hide`] 置位
//!      [`RESULT_DISMISSED`]，只有 [`TurnMode::Fresh`] 才清位）—— 关掉又被弹回来是最恼人的
//!      一类「自作主张」。答案仍会照常并入会话，下次打开就能看到。
//!
//! 交付点在 `readback::deliver`（**单点接入**）：R1 阶段该处只落诊断日志，R2 起改调本模块。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

/// 结果卡窗 label（第 6 个常驻窗口）。
pub const RESULT_LABEL: &str = "result";

/// 卡片宽度（逻辑像素，固定）。与 `poc/result.html` 的排版口径一致。
const CARD_W: f64 = 400.0;
/// 卡片高度下限（逻辑像素）：至少放得下头部 + 一行正文 + 一行轮内动作 + 底部输入框。
const CARD_MIN_H: f64 = 176.0;
/// 卡片高度上限（逻辑像素）：超出部分走卡片内部滚动（`.thread{overflow-y:auto}`）。
///
/// ★ R5 起由 560 上调为 620：卡片成了「迷你会话」，一轮以上的问答需要更多空间；
///   真正的上限还会被工作区高度二次收窄（见 [`apply_size`]）。
const CARD_MAX_H: f64 = 620.0;
/// 与锚点之间的间隙（逻辑像素，与工具条的 `CURSOR_GAP` 同量级）。
const ANCHOR_GAP: f64 = 8.0;
/// 高度估算的排版常量（逻辑像素）。★ 改动需与 `result.html` 同步。
///
/// 窗口留白 16（body 上下各 8，给阴影留位置）+ 头部 42 + 底部输入框 56 + 轮内动作行 28；
/// 引用/追问气泡 ≈ 50；思考区折叠头 ≈ 30；降级提示条 ≈ 28；正文上下内边距 ≈ 10；
/// 正文行高 ≈ 13×1.62 ≈ 21；正文可用宽 400 − 24 − 16 = 360 → 每行约 27 个全角字。
///
/// ★ R5 起这套常量**只用于兜底**：正常路径由页内实测高度决定卡片尺寸
///   （见 [`measure`]），因为多轮会话的高度不是「字符数 × 行高」能算准的
///   （每轮都有气泡 / 动作行 / 思考区，且长回答会折行）。实测拿不到时才退回这里。
const H_CHROME: f64 = 16.0;
const H_HEAD: f64 = 42.0;
const H_COMPOSER: f64 = 56.0;
const H_TURN_ACTS: f64 = 28.0;
const H_QUOTE: f64 = 50.0;
const H_THINK: f64 = 30.0;
const H_NOTE: f64 = 28.0;
const H_BODY_PAD: f64 = 10.0;
const LINE_H: f64 = 21.0;
const CHARS_PER_LINE: f64 = 27.0;
/// 最小尺寸变化幅度（逻辑像素）：低于此值不重设尺寸，免得流式推送时窗口来回抖。
const GROW_STEP: f64 = 24.0;
/// 页内实测回调的等待上限（页面忙 / 未加载时的兜底 → 退回 [`estimate_height`]）。
const MEASURE_TIMEOUT_MS: u64 = 400;

/// 锚点矩形（**物理像素**）。
///
/// 能力 A（截图识文字）= 选区矩形；能力 B（划词）= 划词时的光标点（`w = h = 0`）。
/// 由调用方（`begin_watch`）传入 —— 诊断入口传 `None`，此时退回「当前光标位置」兜底。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// ★ 一次视图更新在会话里的**轮次语义**。
///
/// 由调用方声明（回读端口 / 卡片追问），`show` 直接照办 —— 不做任何「猜这是新一轮还是
/// 老一轮」的启发式：猜错就会把追问的回答追加成第 3 轮、或把流式增量覆盖掉上一轮。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnMode {
    /// 开一轮新会话：**清空卡片**，从第 1 轮开始（首轮带标题 / 来源 / 引用原文）。
    #[default]
    Fresh,
    /// 追加到卡片已有会话之后（卡片内「继续提问」的追问轮）。
    Follow,
    /// 更新**最后一轮**（流式增量 / 终态交付；追问骨架已由 [`ask`] 建好，避免重复追加）。
    Update,
}

/// 会话里的一轮问答：用户输入 + 回答 + 思考过程 + 状态。
#[derive(Debug, Clone, Default)]
pub struct Turn {
    pub cancelled: bool,
    pub request: crate::workflow::Token,
    /// 本轮的用户输入（首轮 = 划词原文 / 截图里的问题；后续 = 追问文本）。空则不显示气泡。
    pub quote: String,
    /// 回答正文。
    pub text: String,
    /// 思考过程（DeepSeek 的「已思考…」折叠区正文）。空则不显示思考区。
    pub thinking: String,
    /// 思考区标题（页内折叠头的文字，例如「已思考（用时 3 秒）」）。
    pub think_label: String,
    /// `true` = 仍在生成中（卡片显示流式光标 + 该轮动作行暂时隐藏）。
    pub streaming: bool,
    /// `false` 且**已停流** = 超时降级（该轮下方标注「回答可能未完成」，绝不静默）。
    pub complete: bool,
    /// R7：本轮发送复核失败（输入框最终未被清空）。页内渲染顶部提示条。
    pub send_failed: bool,
    /// R5：本轮明确探测到登录失效。页内渲染顶部提示条。
    pub session_expired: bool,
    /// R3：本轮回答正文命中极强注入特征。页内渲染顶部提示条（正文仍照常净化展示）。
    pub injection_suspected: bool,
}

impl Turn {
    /// 该轮的降级提示文案（空串 = 不显示）。
    ///
    /// ★ 判据是「**已停流 且 未判终态**」。流式过程中 `complete` 必然为 false，但那时
    ///   **不是**降级，标上「回答可能未完成」就是误导 —— 这条口径由本函数**单点**裁决，
    ///   整份渲染与增量渲染两条路径共用（旧实现只在整份渲染里写 note，而超时降级发生在
    ///   增量路径上 ⇒ 提示**从不出现**，静默降级，违反 W-V5）。
    fn note(&self) -> &'static str {
        if self.streaming || self.complete {
            ""
        } else {
            "回答可能未完成（回读超时降级）"
        }
    }

    fn json(&self) -> serde_json::Value {
        let status = if self.cancelled {
            crate::workflow::SendStatus::Cancelled
        } else if self.complete {
            crate::workflow::SendStatus::Complete
        } else {
            crate::workflow::status(self.request)
        };
        let status = if !self.cancelled && status == crate::workflow::SendStatus::Cancelled {
            if self.streaming {
                crate::workflow::SendStatus::Reading
            } else {
                crate::workflow::SendStatus::TimedOut
            }
        } else {
            status
        };
        let stopped = matches!(
            status,
            crate::workflow::SendStatus::Failed(_) | crate::workflow::SendStatus::Cancelled
        );
        let (state, message) = send_status_payload(status);
        serde_json::json!({
            "request": self.request.0, "sendState": state, "sendMessage": message,
            "quote": self.quote,
            "text": self.text,
            "thinking": self.thinking,
            "thinkLabel": self.think_label,
            "streaming": self.streaming && !stopped,
            "complete": self.complete,
            "note": if stopped { "" } else { self.note() },
            "sendFailed": self.send_failed,
            "sessionExpired": self.session_expired,
            "injectionSuspected": self.injection_suspected,
        })
    }
}

/// 卡片的一份会话（标题 / 来源 / 若干轮）。
#[derive(Debug, Default)]
pub struct Session {
    pub revision: u64,
    pub title: String,
    pub source: String,
    pub turns: Vec<Turn>,
}

impl Session {
    pub fn clear(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.title.clear();
        self.source.clear();
        self.turns.clear();
    }
    /// 整份会话的载荷（`__dsfResultSet` 用）。`reset=true` ⇒ 页内把滚动条与输入框也复位。
    fn payload(&self, reset: bool) -> String {
        let turns: Vec<serde_json::Value> = self.turns.iter().map(Turn::json).collect();
        escape_line_seps(
            serde_json::to_string(&serde_json::json!({
                "revision": self.revision,
                "title": self.title,
                "source": self.source,
                "reset": reset,
                "turns": turns,
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        )
    }

    /// 只含**末轮**的载荷（`__dsfResultStream` 用）—— 流式增量不必整份重发。
    fn last_payload(&self) -> String {
        let mut v = match self.turns.last() {
            Some(t) => t.json(),
            None => serde_json::json!({
                "text": "", "thinking": "", "streaming": false, "note": "",
            }),
        };
        v["revision"] = serde_json::json!(self.revision);
        escape_line_seps(serde_json::to_string(&v).unwrap_or_else(|_| "{}".to_string()))
    }
}

/// ★ 卡片的一次「视图」—— 打开、流式增量、终态交付、追问**共用同一结构体**。
#[derive(Debug, Clone, Default)]
pub struct CardView {
    pub request: Option<crate::workflow::Token>,
    /// 标题（豆包同款：「这里是我的解释」这类一句话标题）。
    pub title: String,
    /// 副标题 / 来源（「划词结果」/「追问结果」/「回读诊断」…）。
    pub source: String,
    /// 本轮的用户输入（见 [`Turn::quote`]）。
    pub quote: String,
    /// 回答正文。
    pub text: String,
    /// 思考过程。
    pub thinking: String,
    /// 思考区标题。
    pub think_label: String,
    /// `true` = 仍在生成中。
    pub streaming: bool,
    /// `false` = 超时降级（见 [`Turn::note`]）。
    pub complete: bool,
    /// R7：发送复核确认**失败**（输入框最终仍未被清空，未能确认发出）。卡片顶部提示，
    /// 不谎报「已发送」。默认 `false`（绝大多数调用点不关心，靠 `..Default::default()` 兜底）。
    pub send_failed: bool,
    /// R5：明确探测到会话登录失效（session cookie 过期 / 被登出）。卡片顶部提示重登。
    /// 默认 `false`；**只有回读明确判定**才置位，避免把「慢/暂时无回显」误报成「登录失效」。
    pub session_expired: bool,
    /// R3：回答正文命中**极强**的注入特征（如显式 `<script`/`alert(` 等可执行脚本形态）。
    /// 命中即提示「该回答疑似异常内容」并**仍照常展示**（净化层兜底），不强删、不误杀普通回答。
    pub injection_suspected: bool,
    /// 本轮在会话里的语义（见 [`TurnMode`]）。
    pub turn: TurnMode,
}

impl CardView {
    /// 标记本视图的轮次语义。
    ///
    /// ★ `Update` 一律**清空 `quote`**：那一轮的问题已经落进会话了，流式推送每 400ms 一次、
    ///   每次都把问题重写一遍既浪费字节，也让「Update 只填内容」这条语义变得含糊。
    pub fn with_turn(mut self, turn: TurnMode) -> Self {
        self.turn = turn;
        if turn == TurnMode::Update {
            self.quote.clear();
        }
        self
    }
}

/// ★ 纯函数：把一次视图更新**并入**会话（本模块唯一的写入口径，故可脱离窗口单测）。
///
/// - `Fresh` → 换标题/来源并**清空**后插入首轮；
/// - `Follow` → 追加一轮（标题/来源描述的是本轮请求，但卡片顶部保留首轮文案 ——
///   豆包同款：追问不换标题。仅当会话为空时兜底填上）；
/// - `Update` → 就地更新末轮；会话为空（理论上不该发生）时退化为追加一轮。
pub fn merge_into(s: &mut Session, view: &CardView) {
    fn turn_of(view: &CardView) -> Turn {
        Turn {
            cancelled: false,
            request: view.request.unwrap_or_else(crate::workflow::current),
            quote: view.quote.clone(),
            // 页内快照已有长度上限，此处保留完整载荷，使展示、收藏与导出一致。
            text: view.text.clone(),
            thinking: view.thinking.clone(),
            think_label: view.think_label.clone(),
            streaming: view.streaming,
            complete: view.complete,
            send_failed: view.send_failed,
            session_expired: view.session_expired,
            injection_suspected: view.injection_suspected,
        }
    }
    match view.turn {
        TurnMode::Fresh => {
            s.clear();
            s.title.clone_from(&view.title);
            s.source.clone_from(&view.source);
            s.turns.clear();
            s.turns.push(turn_of(view));
        }
        TurnMode::Follow => {
            if s.turns.is_empty() {
                s.title.clone_from(&view.title);
                s.source.clone_from(&view.source);
            }
            s.turns.push(turn_of(view));
        }
        TurnMode::Update => match s.turns.last_mut() {
            Some(last) => {
                // 空 `quote` 视为「不更新」：终态交付时调用方不重复携带本轮问题。
                if !view.quote.is_empty() {
                    last.quote.clone_from(&view.quote);
                }
                last.text.clone_from(&view.text);
                last.thinking.clone_from(&view.thinking);
                last.think_label.clone_from(&view.think_label);
                last.cancelled = false;
                last.request = view.request.unwrap_or_else(crate::workflow::current);
                last.streaming = view.streaming;
                last.complete = view.complete;
                last.send_failed = view.send_failed;
                last.session_expired = view.session_expired;
                last.injection_suspected = view.injection_suspected;
            }
            None => {
                s.title.clone_from(&view.title);
                s.source.clone_from(&view.source);
                s.turns.push(turn_of(view));
            }
        },
    }
}

/// 把整份载荷里的 U+2028 / U+2029 显式转义（ES2019 之前不允许出现在字符串字面量里，
/// 远端回答完全可能含这两个字符）。
fn escape_line_seps(s: String) -> String {
    s.replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// 卡片的会话（卡片内容的唯一真相）。
static SESSION: Mutex<Session> = Mutex::new(Session {
    revision: 0,
    title: String::new(),
    source: String::new(),
    turns: Vec::new(),
});
/// 卡片当前是否可见。
static RESULT_VISIBLE: AtomicBool = AtomicBool::new(false);
/// ★ 卡片是否被**用户主动关闭**过（见模块头硬约束 8）。
static RESULT_DISMISSED: AtomicBool = AtomicBool::new(false);
/// ★ 渲染串行锁：`show_async` 会为主线程避免自锁而把渲染拆到多条**短命 worker 线程**。
/// 若多路推送并发（例如流式推送与终态交付几乎同拍）且每路各 spawn 一个线程，会并发执行
/// `render` 里的「页内实测 → 定位 → set_size → show」—— 对同一 result 窗口并发 set_size/show
/// 是竞态来源（R4）。让每个渲染线程在真正 render 前先持这把锁，把并发渲染串行成队列。
static RENDER_LOCK: Mutex<()> = Mutex::new(());

fn log(msg: &str) {
    crate::diag::log(&format!("result: {msg}"));
}

fn warn(msg: &str) {
    crate::diag::warn(&format!("result: {msg}"));
}

/// ★ 纯函数：按**字符数**截断展示载荷（绝不按字节切，避免把 UTF-8 切断）。
///
/// 截断时在末尾追加省略标记 —— 用户必须知道「看到的不是全部」。
#[cfg(test)]
pub fn truncate_payload(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    format!("{head}\n\n…（正文过长，已截断显示前 {max} 字符）")
}

/// ★ 纯函数：按正文字数估算卡片高度（逻辑像素），clamp 到 `[CARD_MIN_H, CARD_MAX_H]`。
///
/// **兜底用**：正常路径由页内实测高度决定（见 [`measure`]）；只有页内脚本拿不到返回值
/// （页面未就绪 / 测量超时）时才退回本估算。
pub fn estimate_height(chars: usize, has_note: bool, has_quote: bool, has_think: bool) -> f64 {
    let lines = (chars as f64 / CHARS_PER_LINE).ceil().max(1.0);
    let h = H_CHROME
        + H_HEAD
        + H_BODY_PAD
        + lines * LINE_H
        + H_COMPOSER
        + H_TURN_ACTS
        + if has_note { H_NOTE } else { 0.0 }
        + if has_quote { H_QUOTE } else { 0.0 }
        + if has_think { H_THINK } else { 0.0 };
    h.clamp(CARD_MIN_H, CARD_MAX_H)
}

/// 本视图的高度估算（统一口径，避免多处调用点各写一遍）。
fn view_height(v: &CardView, text_chars: usize) -> f64 {
    estimate_height(
        text_chars,
        !v.streaming && !v.complete,
        !v.quote.is_empty(),
        !v.thinking.is_empty(),
    )
}

/// 光标所在显示器的**工作区** `(x, y, w, h)`（物理像素）。
fn work_area_at(app: &AppHandle, x: i32, y: i32) -> Option<(i32, i32, i32, i32)> {
    let m = app
        .monitor_from_point(x as f64, y as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let a = m.work_area();
    Some((
        a.position.x,
        a.position.y,
        a.size.width as i32,
        a.size.height as i32,
    ))
}

/// setup 内调用：常驻创建一次（`visible=false`），之后只 show/hide。
pub fn ensure(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(RESULT_LABEL) {
        return Some(w);
    }
    match WebviewWindowBuilder::new(app, RESULT_LABEL, WebviewUrl::App("poc/result.html".into()))
        .title("DeepSeek 结果卡")
        .inner_size(CARD_W, CARD_MIN_H)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        // 同 popup / shot：undecorated + resizable 是 TAURI_DRAG_RESIZE_*（1411/1412）的成因。
        .resizable(false)
        .shadow(false)
        // ★ 与 popup 相反：卡片**必须** focusable(true) —— 正文要能选中/复制、输入框要能打字、
        //   要收得到 Esc（W-V6）。「不抢焦点」由「show 后不 set_focus」保证（R4）。
        .focusable(true)
        .visible(false)
        .on_page_load(|webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                let token = crate::workflow::current();
                let _ = webview.eval(crate::workflow::initialize_script(token));
                let app = webview.app_handle().clone();
                std::thread::spawn(move || {
                    if SESSION.lock().map(|s| s.turns.is_empty()).unwrap_or(true) {
                        return;
                    }
                    render(
                        &app,
                        &CardView {
                            request: Some(token),
                            turn: TurnMode::Follow,
                            ..Default::default()
                        },
                        None,
                    );
                });
            }
        })
        .build()
    {
        Ok(w) => {
            log("卡片窗创建成功（常驻，visible=false，transparent + always_on_top + focusable）");
            // ★ 防销毁守卫（沿用 screenshot.rs::ensure 同款）：卡片是 focusable(true) 的功能窗，
            //   Alt+F4 / 外部 close 会真的销毁它 —— 既违反「常驻窗绝不销毁重建」（1411/1412），
            //   也会让 `readback::deliver` 之后的交付无处可去。改为「关 = 隐藏」。
            let app_c = app.clone();
            w.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    log("收到关闭请求，改为隐藏");
                    hide(&app_c);
                }
            });
            Some(w)
        }
        Err(e) => {
            crate::diag::error(&format!("result 卡片窗创建失败: {e}"));
            None
        }
    }
}

/// 隐藏卡片（**并记下「用户主动关闭」**，见模块头硬约束 8）。
///
/// 会话本身**不清空** —— 下次打开时能看到完整对话（这正是「继续提问」的价值）。
/// 只有 [`TurnMode::Fresh`]（新的划词）才会开一份新会话。
pub fn hide(app: &AppHandle) {
    RESULT_VISIBLE.store(false, Ordering::SeqCst);
    RESULT_DISMISSED.store(true, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(RESULT_LABEL) {
        let _ = w.hide();
    }
    // ★ 残影冲刷（与 `popup::hide` / `hide_panel` 同机制）：结果卡同为 `transparent + always_on_top`
    //   窗，`hide()` 只置 `WS_HIDE`，其合成表面回收有滞后 —— 透明球会读到卡片（含回答正文）的
    //   陈旧合成帧（Click-through 观感）。hide 后强制球重绘一次冲刷掉。
    crate::flush_ball(app);
    crate::diag::log("result 已 hide 并冲刷球合成表面");
}

/// 菜单入口复用常驻结果卡，仅打开收藏视图，不清空或重建当前会话。
pub fn open_favorites(app: &AppHandle) {
    let Some(win) = app.get_webview_window(RESULT_LABEL) else {
        warn("收藏夹窗口尚未创建");
        return;
    };
    let app_c = app.clone();
    if let Err(error) = win.eval_with_callback(
        "!!(window.__dsfFavoritesOpen && window.__dsfFavoritesOpen())",
        move |raw| {
            if raw.trim() != "true" {
                warn("收藏夹页面尚未就绪，请重新打开菜单后重试");
                return;
            }
            let Some(win) = app_c.get_webview_window(RESULT_LABEL) else {
                return;
            };
            if !RESULT_VISIBLE.load(Ordering::SeqCst) {
                let (x, y) = crate::popup::cursor_pos();
                let anchor = AnchorRect { x, y, w: 0, h: 0 };
                let height = win
                    .inner_size()
                    .ok()
                    .zip(win.scale_factor().ok())
                    .filter(|(_, scale)| *scale > 0.0)
                    .map(|(size, scale)| size.height as f64 / scale)
                    .unwrap_or(CARD_MIN_H);
                let (x, y, _, _) = placement(&app_c, &anchor, height);
                let _ = win.set_position(PhysicalPosition::new(x, y));
            }
            if let Err(error) = win.show() {
                warn(&format!("收藏夹显示失败: {error}"));
                return;
            }
            RESULT_DISMISSED.store(false, Ordering::SeqCst);
            RESULT_VISIBLE.store(true, Ordering::SeqCst);
            // 与结果卡其他入口一致，show 后不强制 set_focus。
        },
    ) {
        warn(&format!("打开收藏夹失败: {error}"));
    }
}

/// 会话末轮的回答正文（供 `copy` 的兜底取用）。
fn last_text() -> Option<String> {
    SESSION
        .lock()
        .ok()
        .and_then(|g| g.turns.last().map(|t| t.text.clone()))
}

/// 把视图并入会话（加锁 + 调用纯函数 [`merge_into`]）。
fn merge(view: &CardView) {
    match SESSION.lock() {
        Ok(mut g) => {
            if view.request.map_or(true, crate::workflow::active) {
                merge_into(&mut g, view);
            }
        }
        Err(_) => warn("会话锁已中毒，本次更新丢弃"),
    }
}

/// ★ 供**回读 worker 线程**调用的入口：并入会话 + 立即渲染。
///
/// 渲染会**阻塞等待页内实测回调**（[`MEASURE_TIMEOUT_MS`]），而该回调运行在**主线程**上 ——
/// 所以本函数**只能**从非主线程调用。主线程（命令处理）路径请用 [`show_async`]。
pub fn show(app: &AppHandle, view: &CardView, anchor: Option<AnchorRect>) {
    let mut view = view.clone();
    view.request.get_or_insert_with(crate::workflow::current);
    let view = &view;
    if !crate::workflow::active(view.request.unwrap()) {
        return;
    }
    merge(view);
    render(app, view, anchor);
}

/// ★ 供**主线程（Tauri 命令处理）**调用的入口：并入会话后把渲染交给短命线程。
///
/// 为什么不直接 `show`：`show` 会同步等主线程回调（见上），在主线程上同步做必然自锁
/// （只能等到超时再退回估算 —— 白白卡住 400ms 且尺寸不准）。
pub fn show_async(app: &AppHandle, mut view: CardView, anchor: Option<AnchorRect>) {
    view.request.get_or_insert_with(crate::workflow::current);
    if !crate::workflow::active(view.request.unwrap()) {
        return;
    }
    merge(&view);
    let app = app.clone();
    std::thread::spawn(move || {
        render(&app, &view, anchor);
    });
}

/// 渲染（worker 线程）：整份会话 / 只换末轮 → 页内实测高度 → 定位 / 改尺寸 / 显示。
fn render(app: &AppHandle, view: &CardView, anchor: Option<AnchorRect>) {
    // ★ MEDIUM-1 修正（2026-09-21）：渲染串行锁**搬进 render 入口**。原先锁只加在 `show_async`
    //   派生的短命线程上，而 `show`（回读 worker 线程直接调用）路径不持锁 ⇒ 两条渲染入口仍可能
    //   并发地对同一 result 窗口执行 set_size/show（占位卡与流式推送同拍 ⇒ 竞态，违反 R4）。
    //   统一在此持锁，无论哪个调用方都串行化。锁若被某次 render panic 毒化：`into_inner` 取回
    //   内部值恢复持有，而非 `.ok()` 丢弃 —— 否则后续所有推送都拿不到锁，退化为并发 set_size。
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let token = view.request.unwrap_or_else(crate::workflow::current);
    if !crate::workflow::active(token) {
        return;
    }
    let Some(win) = app.get_webview_window(RESULT_LABEL) else {
        warn("卡片窗不存在（setup 未创建成功），本次结果无处展示（内容已落日志）");
        return;
    };

    let opened = !RESULT_VISIBLE.load(Ordering::SeqCst);
    // ★ 用户手动关掉过的卡片**不得被后续推送弹回来**（硬约束 8）：结果照常并入会话，
    //   下次打开就能看到 —— 但绝不抢回屏幕。只有新的会话（Fresh）才重新弹。
    if opened && RESULT_DISMISSED.load(Ordering::SeqCst) && view.turn != TurnMode::Fresh {
        log(&format!(
            "卡片已被用户关闭 → 本轮结果只并入会话（{} 字符，共 {} 轮），不重新弹窗",
            view.text.chars().count(),
            SESSION.lock().map(|g| g.turns.len()).unwrap_or(0)
        ));
        return;
    }
    if view.turn == TurnMode::Fresh {
        RESULT_DISMISSED.store(false, Ordering::SeqCst);
    }

    // 整份还是增量：只有**轮次语义不是 Update** 时才整份重发。
    //
    // ★ 复位（清输入框 / 收起旧 DOM）与渲染**必须写在同一个脚本里**：分成两次 eval 就变成
    //   两次独立的脚本投递，先后顺序只能靠「WebView2 会按序执行」这个未经证实的假设 ——
    //   一旦乱序，复位会把刚渲染好的内容清掉（表现为「卡片空白」这种最难查的间歇性缺陷）。
    let full = view.turn != TurnMode::Update;
    let js = {
        let Ok(g) = SESSION.lock() else {
            warn("会话锁已中毒，本次渲染跳过");
            return;
        };
        let (payload, render_fn) = if full {
            (g.payload(view.turn == TurnMode::Fresh), "__dsfResultSet")
        } else {
            (g.last_payload(), "__dsfResultStream")
        };
        if full {
            format!(
                "(function(){{if(window.__dsfResultReset)window.__dsfResultReset();return window.{render_fn}?window.{render_fn}({payload}):0;}})()"
            )
        } else {
            format!("(window.{render_fn}?window.{render_fn}({payload}):0)")
        }
    };

    let measured = measure(&win, crate::workflow::guarded(token, &js));
    if !crate::workflow::active(token) {
        return;
    }
    let want_h = measured
        .map(|h| h.clamp(CARD_MIN_H, CARD_MAX_H))
        .unwrap_or_else(|| view_height(view, view.text.chars().count()));
    if measured.is_none() {
        log("页内未回报实测高度（页面未就绪 / 超时）→ 退回常量估算");
    }

    if opened {
        // ---- 首次显示：定位 + 定尺寸 + show ----
        let anchor = anchor.unwrap_or_else(|| {
            let (cx, cy) = crate::popup::cursor_pos();
            AnchorRect {
                x: cx,
                y: cy,
                w: 0,
                h: 0,
            }
        });
        let (x, y, w, mut h) = placement(app, &anchor, want_h);
        // 高度再被工作区**收窄**一次（矮屏上别让卡片比屏幕还高）。
        //
        // ★ R14 修复：这里原先是 `h = 工作区高` —— 一个**赋值**把 `placement` 算好的
        //   `want_h` 整个覆盖掉了，于是**首次显示恒为「近乎满屏」**（1080p@1.25 ⇒
        //   `(1040/1.25-24)*1.25 ≈ 1040` 物理 ⇒ 逻辑 832，已越过 `CARD_MAX_H = 620`）。
        //   叠加「流式期间只长大不收缩」（见下方 `apply_size` 的 `allow_shrink`），
        //   占位卡与整个流式过程都顶着这个超高值 ⇒ 用户报的
        //   「结果卡片的数据未回流时候高度太高」。
        //   正确语义是**cap（取小）**，与 `apply_size` 中的同款收窄口径一致。
        if let Ok(scale) = win.scale_factor() {
            if scale > 0.0 {
                if let Some((_, _, _, ah)) = work_area_at(app, x, y) {
                    let cap = ((ah as f64 / scale - 24.0).max(CARD_MIN_H) * scale).round() as i32;
                    h = h.min(cap);
                }
            }
        }
        let logical_h = h as f64 / win.scale_factor().unwrap_or(1.0);
        let _ = win.set_size(LogicalSize::new(CARD_W, logical_h));
        let _ = win.set_position(PhysicalPosition::new(x, y));
        if let Err(e) = win.show() {
            warn(&format!("show 失败: {e}"));
            return;
        }
        // ★ 刻意**不调用** `set_focus()`（R4 / W-V7）：卡片出现不得打断用户当前正在输入的窗口。
        //   本项目已记录「show() 后 set_focus() 触发 WebView2 abort（宿主 exit -1）」的坑。
        RESULT_VISIBLE.store(true, Ordering::SeqCst);
        let turns = SESSION.lock().map(|g| g.turns.len()).unwrap_or(0);
        log(&format!(
            "已 show（{} · {turns} 轮 · {} 字符{} · 高 {logical_h:.0} · pos=({x},{y}) size={w}x{h} · streaming={} · complete={}）",
            view.source,
            view.text.chars().count(),
            if view.thinking.is_empty() { "" } else { " + 思考" },
            view.streaming,
            view.complete
        ));
        arm_watchdog(app);
        return;
    }

    // ---- 已可见：只在需要时改尺寸（流式中只长大；终态一次到位，可缩）----
    //
    // ★ R14：`Fresh`（新会话）是**硬复位** —— 旧轮次已被清空，此时若仍沿用「流式中只长大」，
    //   卡片就会顶着上一问的高个子显示一张空卡（用户报「未回流时高度太高」的另一半成因）。
    //   故 `Fresh` 一律允许收缩；`Update`（同一轮的流式增量）仍保持「只长大」，避免抖动。
    let allow_shrink = !view.streaming || view.turn == TurnMode::Fresh;
    apply_size(app, &win, want_h, allow_shrink);
    log(&format!(
        "已更新（{} · 高 {want_h:.0} · 整份={full} · streaming={} · complete={}）",
        if full { "整份会话" } else { "末轮增量" },
        view.streaming,
        view.complete
    ));
}

/// 让页内渲染并**回报实测高度**（逻辑像素）。
///
/// 传的是**表达式**：`__dsfResultSet` / `__dsfResultStream` 渲染完（读 `offsetHeight` 会强制
/// 同步布局）后**返回**期望的卡片高度。WebView2 会把表达式结果 JSON 序列化后交给回调。
///
/// 返回 `None` 表示页内未就绪 / 超时（此时调用方退回 [`estimate_height`]）。
fn measure(win: &WebviewWindow, js: String) -> Option<f64> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if let Err(e) = win.eval_with_callback(js, move |raw| {
        let _ = tx.send(raw);
    }) {
        warn(&format!("渲染 eval 失败: {e}"));
        return None;
    }
    // ★ 回调在主线程执行 ⇒ 本函数绝不能从主线程调用（见 `show_async` 的说明）。
    let raw = rx
        .recv_timeout(Duration::from_millis(MEASURE_TIMEOUT_MS))
        .ok()?;
    let v: f64 = raw.trim().parse().ok()?;
    if v.is_finite() && v > 0.0 {
        Some(v)
    } else {
        None
    }
}

/// 首次显示时的定位（与工具条共用的 `place_near` 纯函数）。
///
/// 返回 `(x, y, w, h)` —— 全部**物理像素**。
fn placement(app: &AppHandle, anchor: &AnchorRect, logical_h: f64) -> (i32, i32, i32, i32) {
    let monitor = app
        .monitor_from_point(anchor.x as f64, anchor.y as f64)
        .ok()
        .flatten();
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let gap = (ANCHOR_GAP * scale).round() as i32;
    let w = (CARD_W * scale).round() as i32;
    let h = (logical_h * scale).round() as i32;
    // 点锚点（w=h=0）= 划词光标 / 无锚点兜底 → 卡片落在**右下方**（与工具条同款观感）；
    // 矩形锚点（截图选区）→ 卡片落在选区**正下方**、左边缘对齐。
    let prefer = if anchor.w > 0 || anchor.h > 0 {
        (anchor.x, anchor.y + anchor.h + gap)
    } else {
        (anchor.x + gap, anchor.y + gap)
    };
    let (x, y) = match monitor.as_ref() {
        Some(m) => {
            let area = m.work_area();
            crate::popup::place_near(
                prefer,
                (anchor.x, anchor.y, anchor.w, anchor.h),
                (w, h),
                (
                    area.position.x,
                    area.position.y,
                    area.size.width as i32,
                    area.size.height as i32,
                ),
                gap,
            )
        }
        // 取不到显示器信息：保持首选位置（不 clamp）。
        None => prefer,
    };
    (x, y, w, h)
}

/// 改卡片尺寸：`allow_shrink=false` 时只长大（流式推送的防抖口径）。
///
/// 高度先被**工作区**二次收窄（卡片不该高过屏幕）；若新高度会让底部溢出工作区，则整体上移
/// —— 水平位置始终尊重用户拖动。
fn apply_size(app: &AppHandle, win: &WebviewWindow, want_h: f64, allow_shrink: bool) {
    let (Ok(size), Ok(scale)) = (win.inner_size(), win.scale_factor()) else {
        return;
    };
    if scale <= 0.0 {
        return;
    }
    let cur = size.height as f64 / scale;
    let delta = want_h - cur;
    if allow_shrink {
        if delta.abs() < GROW_STEP {
            return;
        }
    } else if delta < GROW_STEP {
        return;
    }
    let Ok(pos) = win.outer_position() else {
        return;
    };
    let area = work_area_at(app, pos.x, pos.y);
    let want_h = match area {
        Some((_, _, _, ah)) => want_h.min(ah as f64 / scale - 24.0).max(CARD_MIN_H),
        None => want_h,
    };
    if (want_h - cur).abs() < 1.0 {
        return;
    }
    if let Err(e) = win.set_size(LogicalSize::new(CARD_W, want_h)) {
        warn(&format!("卡片改尺寸失败: {e}"));
        return;
    }
    // 只有「底部会溢出」时才上移：其它情况保持左上角，避免窗口在用户眼前跳动。
    if let Some((_, ay, _, ah)) = area {
        let h_px = (want_h * scale).round() as i32;
        if pos.y + h_px > ay + ah {
            let y = (ay + ah - h_px).max(ay);
            let _ = win.set_position(PhysicalPosition::new(pos.x, y));
        }
    }
}

/// 页内主动请求改尺寸（折叠 / 展开正文这类**纯页内**动作，Rust 这边看不到对应的渲染推送）。
///
/// 与 [`apply_size`] 共用一个出口 ⇒ 工作区收窄与溢出自上移的行为完全一致。
pub fn resize(app: &AppHandle, height: f64) {
    let Some(win) = app.get_webview_window(RESULT_LABEL) else {
        return;
    };
    if !height.is_finite() {
        return;
    }
    let h = height.clamp(CARD_MIN_H, CARD_MAX_H);
    apply_size(app, &win, h, true);
    log(&format!("页内请求改尺寸 → 高 {h:.0}"));
}

/// 页面存活看门狗：若 500ms 后页内 `__dsfResultSet` 仍未定义，说明 `poc/result.html`
/// 没被加载（最典型：`dist/` 未随 vite 新入口重建 → 404）。
/// 此时卡片是一扇**收不到任何载荷**的空窗 —— 不自动隐藏就可能长期挡在用户屏幕上（W-V8）。
fn arm_watchdog(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        let Some(win) = app.get_webview_window(RESULT_LABEL) else {
            return;
        };
        let app_c = app.clone();
        // ★ 传表达式：WebView2 会把表达式结果 JSON 序列化后交给回调（回调在主线程，不得 sleep）。
        let called = win.eval_with_callback("!!(window.__dsfResultSet)", move |raw| {
            if raw.trim() == "false" && RESULT_VISIBLE.load(Ordering::SeqCst) {
                warn("卡片页未就绪（__dsfResultSet 未定义），自动隐藏以避免死窗（请确认已 pnpm build）");
                hide(&app_c);
            }
        });
        if called.is_err() {
            warn("看门狗无法执行页面探针");
        }
    });
}

/// 头部拖动：`poc/result.html` 在位移超过阈值后调 `result_drag_start`。
///
/// ★ 刻意**不用** `data-tauri-drag-region`：它在 mousedown 阶段就吞掉事件，会让头部上的
///   按钮点不动（悬浮球上的同型教训）。手动 `start_dragging` 才能与按钮点击共存。
pub fn start_drag(app: &AppHandle) {
    let Some(win) = app.get_webview_window(RESULT_LABEL) else {
        warn("拖动失败：卡片窗不存在");
        return;
    };
    if let Err(e) = win.start_dragging() {
        warn(&format!("拖动失败: {e}"));
    } else {
        log("开始拖动卡片");
    }
}

/// 关闭卡片（右上角 × / Esc）。
pub fn close(app: &AppHandle) {
    log("用户关闭卡片");
    hide(app);
}

/// 复制某一轮的正文（页内把该轮的正文一起传上来；空则退回会话末轮）。
///
/// 不需要 `AppHandle`（只碰剪贴板与会话）—— 与其余 `result::*` 不同，这也是刻意的：
/// 接口上少一个「其实没用到的参数」，调用点就少一处可以传错的东西。
pub fn copy(text: Option<String>) {
    let t = text.filter(|s| !s.trim().is_empty()).or_else(last_text);
    match t {
        Some(t) if crate::capture::set_clipboard_text(&t) => {
            log(&format!("已复制 {} 字符到剪贴板", t.chars().count()));
        }
        Some(_) => warn("复制失败（剪贴板被占用）"),
        None => warn("复制失败（当前无卡片正文）"),
    }
}

/// ★ 卡片内「继续提问」：把**输入框里的文本**发往面板，回答回流成卡片的**下一轮**。
///
/// 与「划词 → 结果卡」的区别只有两点：① 走 [`TurnMode::Follow`]（追加一轮，不清空会话）；
/// ② 注入后回读的每一推都是 [`TurnMode::Update`]（骨架已经由本函数建好，不能重复追加）。
///
/// 保持「不弹面板」：追问的答案本来就该回到卡片里（用户点的是卡片上的输入框）。
pub fn ask(app: &AppHandle, text: String) {
    let text = text.trim().to_string();
    if text.is_empty() {
        warn("继续提问失败（输入为空）");
        return;
    }
    // ★ 长度闸门（与划词链路 capture::MAX_CHARS 对齐）：追问文本同样会被注入面板发送，
    //   超长输入既无意义也会加重注入/回读负担。生成长文时 CARD 端 maxlength 虽已限制，
    //   但命令层再兜一道（防御纵深，IPC 可被前端任意调用方携带任意长度入参）。
    if text.chars().count() > crate::capture::MAX_CHARS {
        warn(&format!(
            "继续提问失败（文本过长，上限 {} 字）",
            crate::capture::MAX_CHARS
        ));
        return;
    }
    // 先确认远端页在（面板窗已被删除 / 未创建时，连骨架都不该建，免得留下一轮永远「思考中」）。
    let Some(panel) = crate::ensure_panel_loaded(app) else {
        warn("继续提问失败（面板窗口未创建）");
        return;
    };
    // ★ 上一轮若还在流式，本轮提问会让旧回读**作废**（`WATCH_GEN` 递增 ⇒ 它的交付被丢弃）
    //   ⇒ 旧那一轮会永远停在「生成中…」。这里明确把它标成「未完成」，让卡片如实降级提示
    //   —— 被放弃的一轮必须说清是被放弃的，不能静默（W-V5 同源）。
    if let Ok(mut g) = SESSION.lock() {
        if let Some(last) = g.turns.iter_mut().rev().find(|t| t.streaming) {
            last.streaming = false;
            last.complete = false;
        }
    }
    crate::inject::inject_and_send_async(panel, text.clone());
    // 请求先取消旧任务；当前主线程同步合入骨架，注入回调在下一轮事件循环处理。
    show_async(
        app,
        CardView {
            title: String::new(),
            source: "追问结果".to_string(),
            quote: text.clone(),
            streaming: true,
            turn: TurnMode::Follow,
            ..Default::default()
        },
        None,
    );
    log(&format!(
        "继续提问（{} 字符 · 不弹面板 · 回答回到卡片下一轮）",
        text.chars().count()
    ));
    crate::readback::begin_watch(
        app,
        text,
        None,
        crate::readback::WatchOpts {
            tag: "result-ask".to_string(),
            source: "追问结果".to_string(),
            // ★ `Update`：轮次已由上方骨架建立，本轮回读只负责往里填内容。
            turn: TurnMode::Update,
            ..Default::default()
        },
    );
}

/// 开新会话：先取消旧任务，再清空本地会话并导航到面板首页。
/// 保留当前WebView及cookie；导航屏障解除前不允许向旧文档填入新问题。
/// 卡片保持显示空态，错误返回到按钮旁的状态文案。
pub fn new_session(app: &AppHandle) -> Result<(), String> {
    // 先作废旧任务；清空时禁止排队渲染旧取消卡。
    crate::workflow::cancel_for_new_session(app);
    let token = crate::workflow::current();
    {
        let mut session = SESSION.lock().unwrap_or_else(|p| p.into_inner());
        session.clear();
    }
    if let Some(window) = app.get_webview_window(RESULT_LABEL) {
        window
            .eval(format!("{}{}", crate::workflow::initialize_script(token), crate::workflow::guarded(
                token,
                "(function(){if(window.__dsfResultReset)window.__dsfResultReset();return 0;})()",
            )))
            .map_err(|e| format!("旧任务已取消，但卡片清空失败：{e}"))?;
        if RESULT_VISIBLE.load(Ordering::SeqCst) {
            let _ = window.set_size(LogicalSize::new(CARD_W, CARD_MIN_H));
        } else {
            RESULT_DISMISSED.store(false, Ordering::SeqCst);
        }
    }
    let home = crate::panel_site::new_session_url(&crate::config::panel_url())?;
    if !crate::config::is_valid_panel_url(&home) {
        return Err("旧任务已取消，请先设置有效面板网址".into());
    }
    let target = tauri::Url::parse(&home).map_err(|_| "面板网址无法解析")?;
    let panel = app
        .get_webview_window("panel")
        .ok_or("旧任务已取消，但面板尚未创建")?;
    panel
        .eval("window.__dsfloatNavigationPending=true;")
        .map_err(|e| format!("无法准备新会话：{e}"))?;
    // 原生导航建立新文档；Finished初始化代次前，fill一律等待，不会填入旧对话。
    if let Err(error) = panel.navigate(target) {
        let _ = panel.eval("window.__dsfloatNavigationPending=false;");
        return Err(format!("旧任务已取消，但新会话导航失败：{error}"));
    }
    log("已取消旧发送与回读，并请求打开新会话");
    Ok(())
}
fn send_status_payload(state: crate::workflow::SendStatus) -> (&'static str, String) {
    use crate::workflow::SendStatus::*;
    match state {
        Sending => ("sending", "发送中…".into()),
        Reading => ("reading", "读取中…".into()),
        Failed(reason) => ("failed", reason),
        Uncertain(reason) => ("uncertain", reason),
        Complete => ("complete", String::new()),
        TimedOut => ("timed-out", "读取超时，可继续读取".into()),
        Cancelled => ("cancelled", "已取消".into()),
    }
}
pub fn send_feedback(
    app: &AppHandle,
    token: crate::workflow::Token,
    state: crate::workflow::SendStatus,
) {
    if matches!(
        state,
        crate::workflow::SendStatus::Failed(_) | crate::workflow::SendStatus::Uncertain(_)
    ) {
        let has_turn = SESSION
            .lock()
            .map(|s| s.turns.last().is_some_and(|t| t.request == token))
            .unwrap_or(false);
        if !has_turn {
            if let Some(prompt) = crate::readback::retry_prompt(token) {
                show_async(
                    app,
                    CardView {
                        request: Some(token),
                        title: "发送未确认".into(),
                        source: "发送反馈".into(),
                        quote: prompt,
                        turn: TurnMode::Fresh,
                        ..Default::default()
                    },
                    None,
                );
                return;
            }
        }
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if !crate::workflow::active(token) {
            return;
        }
        let view = CardView {
            request: Some(token),
            turn: TurnMode::Update,
            ..Default::default()
        };
        render(&app, &view, None);
    });
}

fn cancel_turns(session: &mut Session, token: crate::workflow::Token) -> bool {
    let mut changed = false;
    for turn in &mut session.turns {
        if turn.request != token && !turn.complete && !turn.cancelled {
            turn.streaming = false;
            turn.cancelled = true;
            changed = true;
        }
    }
    changed
}

pub fn cancel_pending(app: &AppHandle, token: crate::workflow::Token) {
    let changed = if let Ok(mut session) = SESSION.lock() {
        cancel_turns(&mut session, token)
    } else {
        false
    };
    if changed {
        send_feedback(app, token, crate::workflow::SendStatus::Cancelled);
    }
}

pub fn bind_request(app: &AppHandle, token: crate::workflow::Token) {
    if !crate::workflow::active(token) {
        return;
    }
    if let Ok(mut session) = SESSION.lock() {
        if !crate::workflow::active(token) {
            return;
        }
        if let Some(turn) = session.turns.last_mut() {
            turn.request = token;
            turn.cancelled = false;
            turn.streaming = true;
            turn.complete = false;
        }
    }
    send_feedback(app, token, crate::workflow::status(token));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_cancels_timed_out_and_failed_turns_but_keeps_completed_answers() {
        let mut session = Session {
            turns: vec![
                Turn {
                    request: crate::workflow::Token(1),
                    streaming: false,
                    complete: false,
                    ..Default::default()
                },
                Turn {
                    request: crate::workflow::Token(1),
                    send_failed: true,
                    ..Default::default()
                },
                Turn {
                    request: crate::workflow::Token(1),
                    complete: true,
                    text: "已完成答案".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(cancel_turns(&mut session, crate::workflow::Token(2)));
        assert!(session.turns[0].cancelled);
        assert!(session.turns[1].cancelled);
        assert!(!session.turns[2].cancelled);
        assert_eq!(session.turns[2].text, "已完成答案");
    }

    fn view(turn: TurnMode, quote: &str, text: &str) -> CardView {
        CardView {
            title: "这里是我的解释".to_string(),
            source: "划词结果".to_string(),
            quote: quote.to_string(),
            text: text.to_string(),
            turn,
            ..Default::default()
        }
    }

    #[test]
    fn complete_long_answer_is_preserved_for_card_favorite_and_export() {
        let text = format!(
            "<pre><code>{}END-MARKER</code></pre>",
            "长回答".repeat(9000)
        );
        let mut session = Session::default();
        merge_into(&mut session, &view(TurnMode::Fresh, "问题", &text));
        assert_eq!(session.turns[0].text, text);
        assert_eq!(
            session.turns[0].json()["text"].as_str(),
            Some(text.as_str())
        );
        let updated = format!("{text}<p>完整尾段</p>");
        merge_into(&mut session, &view(TurnMode::Update, "问题", &updated));
        assert_eq!(session.turns[0].text, updated);
        assert!(session.turns[0].json()["text"]
            .as_str()
            .unwrap()
            .ends_with("<p>完整尾段</p>"));
    }
    // ---- truncate_payload ----

    #[test]
    fn truncate_keeps_short_text_intact() {
        assert_eq!(truncate_payload("回答内容", 100), "回答内容");
    }

    #[test]
    fn truncate_marks_long_text() {
        let long = "字".repeat(50);
        let out = truncate_payload(&long, 10);
        // 保留前 10 个字符（按**字符**而非字节切 → 不会出现乱码 / 半个字符），并追加截断标记。
        // ★ 断言只看**前缀**：截断标记本身含有「字」字（「…字符）」），不能用全量计数断言。
        assert_eq!(out.chars().take(10).collect::<String>(), "字".repeat(10));
        assert!(out.contains("已截断显示前 10 字符"));
    }

    #[test]
    fn truncate_boundary_is_inclusive() {
        let exact = "字".repeat(10);
        assert_eq!(truncate_payload(&exact, 10), exact);
    }

    // ---- estimate_height ----

    #[test]
    fn estimate_height_has_floor_and_ceiling() {
        assert_eq!(estimate_height(0, false, false, false), CARD_MIN_H);
        assert_eq!(estimate_height(1_000_000, false, false, false), CARD_MAX_H);
    }

    #[test]
    fn estimate_height_grows_with_text_and_extras() {
        let base = estimate_height(20, false, false, false);
        let long = estimate_height(600, false, false, false);
        assert!(long > base);
        // 同文本下，多一个「引用条 / 思考区 / 降级提示条」只会更高（或已触顶相等）。
        assert!(estimate_height(600, false, true, false) >= long);
        assert!(estimate_height(600, false, false, true) >= long);
        assert!(estimate_height(600, true, false, false) >= long);
    }

    #[test]
    fn estimate_height_stays_in_range() {
        for chars in [0usize, 1, 50, 300, 5_000, 20_000] {
            for note in [false, true] {
                for quote in [false, true] {
                    for think in [false, true] {
                        let h = estimate_height(chars, note, quote, think);
                        assert!(
                            (CARD_MIN_H..=CARD_MAX_H).contains(&h),
                            "chars={chars} note={note} quote={quote} think={think} → {h}"
                        );
                    }
                }
            }
        }
    }

    // ---- Turn::note（降级提示的**唯一**判据；R5 修「提示从不出现」）----

    #[test]
    fn note_only_when_stopped_and_incomplete() {
        // 流式中：不标（此刻 complete 必然为 false，标了就是误导）。
        let mut t = Turn {
            streaming: true,
            complete: false,
            ..Default::default()
        };
        assert_eq!(t.note(), "");
        // 停流 + 判到终态：不标。
        t.streaming = false;
        t.complete = true;
        assert_eq!(t.note(), "");
        // 停流 + 未判终态 = 超时降级：必须标（W-V5 不静默）。
        t.complete = false;
        assert!(t.note().contains("回答可能未完成"));
    }

    // ---- merge_into（会话是卡片内容的唯一真相）----

    #[test]
    fn merge_fresh_clears_and_inserts_first_turn() {
        let mut s = Session::default();
        merge_into(&mut s, &view(TurnMode::Fresh, "原文A", "答案A"));
        merge_into(&mut s, &view(TurnMode::Update, "", "答案A2"));
        assert_eq!(s.turns.len(), 1);
        assert_eq!(s.title, "这里是我的解释");
        assert_eq!(s.source, "划词结果");
        assert_eq!(s.turns[0].quote, "原文A");
        assert_eq!(s.turns[0].text, "答案A2");

        // 再来一份 Fresh（新的划词）→ 旧会话必须被清空，不能残留上一轮。
        let mut v = view(TurnMode::Fresh, "原文B", "答案B");
        v.source = "识文字".to_string();
        merge_into(&mut s, &v);
        assert_eq!(s.turns.len(), 1);
        assert_eq!(s.source, "识文字");
        assert_eq!(s.turns[0].quote, "原文B");
    }

    #[test]
    fn merge_follow_appends_and_keeps_header() {
        let mut s = Session::default();
        merge_into(&mut s, &view(TurnMode::Fresh, "原文", "答案1"));
        let mut v = view(TurnMode::Follow, "追问：能再展开吗", "");
        v.source = "追问结果".to_string();
        v.streaming = true;
        merge_into(&mut s, &v);

        assert_eq!(s.turns.len(), 2);
        // 追问**不换标题**（豆包同款）：顶部仍是首轮的文案。
        assert_eq!(s.title, "这里是我的解释");
        assert_eq!(s.source, "划词结果");
        assert_eq!(s.turns[1].quote, "追问：能再展开吗");
        assert_eq!(s.turns[1].text, "");
        assert!(s.turns[1].streaming);
    }

    // ---- R3/R5/R7：告警标志从 CardView 并入 Turn ----

    #[test]
    fn merge_fresh_propagates_warning_flags() {
        let mut s = Session::default();
        let mut v = view(TurnMode::Fresh, "原文", "答案");
        v.send_failed = true;
        v.session_expired = true;
        v.injection_suspected = true;
        merge_into(&mut s, &v);
        assert!(s.turns[0].send_failed);
        assert!(s.turns[0].session_expired);
        assert!(s.turns[0].injection_suspected);
        // 三个标志都进 payload（前端据此渲染告警横幅）。
        let payload = s.payload(false);
        assert!(payload.contains("sendFailed"));
        assert!(payload.contains("sessionExpired"));
        assert!(payload.contains("injectionSuspected"));
    }

    #[test]
    fn merge_update_propagates_warning_flags_to_last_turn() {
        let mut s = Session::default();
        merge_into(&mut s, &view(TurnMode::Fresh, "原文", "答1"));
        // 先追加第二轮，Update 才命中末轮（否则单轮会话时 last_mut 就是第 0 轮）。
        merge_into(&mut s, &view(TurnMode::Follow, "追问", ""));
        let mut v = view(TurnMode::Update, "", "答2");
        v.send_failed = true;
        merge_into(&mut s, &v);
        // 标志只落到**末轮**，首轮不受影响。
        assert!(!s.turns[0].send_failed);
        assert!(s.turns[1].send_failed);
    }

    #[test]
    fn default_view_has_no_warning_flags() {
        // 绝大多数调用点不设这些标志 —— 默认必须是 false，否则正常回答会被误标。
        let v = view(TurnMode::Fresh, "原文", "答案");
        assert!(!v.send_failed && !v.session_expired && !v.injection_suspected);
    }

    #[test]
    fn merge_update_replaces_last_only() {
        let mut s = Session::default();
        merge_into(&mut s, &view(TurnMode::Fresh, "原文", "答案1"));
        merge_into(&mut s, &view(TurnMode::Follow, "追问", ""));
        merge_into(&mut s, &view(TurnMode::Update, "", "追问答案"));
        assert_eq!(s.turns.len(), 2);
        assert_eq!(s.turns[0].text, "答案1");
        assert_eq!(s.turns[1].text, "追问答案");
        // Update 不带 quote 时**不得**把本轮问题抹掉。
        assert_eq!(s.turns[1].quote, "追问");
    }

    #[test]
    fn merge_update_without_turns_degrades_to_append() {
        // 理论上不该发生（Update 前必有骨架），但真发生时宁可多一轮也不能丢内容。
        let mut s = Session::default();
        merge_into(&mut s, &view(TurnMode::Update, "", "孤儿回答"));
        assert_eq!(s.turns.len(), 1);
        assert_eq!(s.turns[0].text, "孤儿回答");
    }

    // ---- 载荷 ----

    #[test]
    fn payload_escapes_line_separators() {
        let mut s = Session::default();
        let mut v = view(TurnMode::Fresh, "", "");
        v.text = "a\u{2028}b".to_string();
        v.thinking = "c\u{2029}d".to_string();
        merge_into(&mut s, &v);
        for p in [s.payload(true), s.last_payload()] {
            assert!(p.contains("\\u2028"));
            assert!(p.contains("\\u2029"));
            assert!(!p.contains('\u{2028}'));
            assert!(!p.contains('\u{2029}'));
        }
    }

    #[test]
    fn payload_carries_turns_thinking_and_note() {
        let mut s = Session::default();
        let mut v = view(TurnMode::Fresh, "原文", "答案");
        v.thinking = "让我想想".to_string();
        v.streaming = false;
        v.complete = false;
        merge_into(&mut s, &v);
        let p = s.payload(true);
        assert!(p.contains("这里是我的解释"));
        assert!(p.contains("让我想想"));
        assert!(p.contains("回答可能未完成"));
        assert!(p.contains("\"reset\":true"));
        // 末轮增量载荷只带末轮的字段，不含标题 / 来源（省字节 + 不覆盖头部）。
        let last = s.last_payload();
        assert!(!last.contains("这里是我的解释"));
        assert!(last.contains("答案"));
    }

    #[test]
    fn last_payload_on_empty_session_is_safe() {
        let s = Session::default();
        let p = s.last_payload();
        assert!(p.contains("\"text\""));
        assert!(serde_json::from_str::<serde_json::Value>(&p).is_ok());
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSnapshot {
    pub index: usize,
    pub answer_text: String,
    pub markdown: String,
}
fn checked_answers(
    s: &Session,
    revision: u64,
    turns: &[AnswerSnapshot],
    whole: bool,
) -> Result<Vec<(String, String)>, String> {
    if revision != s.revision
        || turns.is_empty()
        || (!whole && turns.len() != 1)
        || (whole && turns.len() != s.turns.len())
    {
        return Err("会话已变化，请重新操作".into());
    }
    let mut result = Vec::new();
    for (position, item) in turns.iter().enumerate() {
        let turn = s.turns.get(item.index).ok_or("回答已不存在")?;
        if (whole && item.index != position)
            || turn.text != item.answer_text
            || turn.streaming
            || (whole && !turn.complete)
        {
            return Err("回答仍在更新，请稍后重新操作".into());
        }
        if turn.text.trim().is_empty() || item.markdown.trim().is_empty() {
            return Err("答案为空，无法保存或导出".into());
        }
        result.push((turn.quote.clone(), item.markdown.clone()));
    }
    Ok(result)
}
pub fn favorite_save(
    revision: u64,
    turn: AnswerSnapshot,
) -> Result<crate::favorites::Favorite, String> {
    let s = SESSION.lock().unwrap_or_else(|p| p.into_inner());
    let mut answers = checked_answers(&s, revision, &[turn], false)?;
    let (question, answer_markdown) = answers.remove(0);
    let item = crate::favorites::Favorite {
        id: 0,
        title: s.title.clone(),
        source: s.source.clone(),
        question,
        answer_markdown,
        created_at: 0,
    };
    drop(s);
    crate::favorites::save(item)
}

/// Refresh an already-open favorites view after data restore; the list itself has no Rust cache.
pub fn refresh_favorites(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(RESULT_LABEL) {
        if let Err(error) =
            window.eval("if(window.__dsfFavoritesRefresh)window.__dsfFavoritesRefresh();")
        {
            crate::diag::warn(&format!(
                "result: backup restore 后刷新收藏列表失败: {error}"
            ));
        }
    }
}

pub fn export_answers(
    revision: u64,
    turns: Vec<AnswerSnapshot>,
    whole: bool,
) -> Result<String, String> {
    let s = SESSION.lock().unwrap_or_else(|p| p.into_inner());
    let answers = checked_answers(&s, revision, &turns, whole)?;
    let body = crate::favorites::markdown(&s.title, &s.source, &answers);
    let title = s.title.clone();
    drop(s);
    crate::favorites::export(&title, &body)
}
#[cfg(test)]
mod export_tests {
    use super::*;
    fn snapshot(index: usize) -> AnswerSnapshot {
        AnswerSnapshot {
            index,
            answer_text: "答案".into(),
            markdown: "答案".into(),
        }
    }
    #[test]
    fn rejects_stale_empty_streaming_and_reordered_answers() {
        let mut s = Session::default();
        merge_into(
            &mut s,
            &CardView {
                text: "答案".into(),
                quote: "问题".into(),
                complete: true,
                ..Default::default()
            },
        );
        let revision = s.revision;
        assert!(checked_answers(&s, revision, &[snapshot(0)], true).is_ok());
        assert!(checked_answers(&s, revision - 1, &[snapshot(0)], true).is_err());
        merge_into(
            &mut s,
            &CardView {
                text: "答案".into(),
                turn: TurnMode::Follow,
                ..Default::default()
            },
        );
        assert_eq!(s.revision, revision);
        assert!(checked_answers(&s, revision, &[snapshot(0)], false).is_ok());
        assert!(checked_answers(&s, revision, &[snapshot(0), snapshot(1)], true).is_err());
        s.turns[1].complete = true;
        assert!(checked_answers(&s, revision, &[snapshot(0), snapshot(1)], true).is_ok());
        let mut changed = snapshot(0);
        changed.answer_text = "旧正文".into();
        assert!(checked_answers(&s, revision, &[changed], false).is_err());
        let mut empty = snapshot(0);
        empty.markdown.clear();
        assert!(checked_answers(&s, revision, &[empty], false).is_err());
        assert!(checked_answers(&s, revision, &[snapshot(0)], true).is_err());
        assert!(checked_answers(&s, revision, &[snapshot(1), snapshot(0)], true).is_err());
        s.turns[0].streaming = true;
        assert!(checked_answers(&s, revision, &[snapshot(0)], false).is_err());
        s.clear();
        assert!(checked_answers(&s, revision, &[snapshot(0)], false).is_err());
    }
}
