//! §5.1 回读桥 · 轮询引擎（从 `readback.rs` 抽出）。
//!
//! 本模块持有回读的全部**线程状态**与**轮询主循环**：代次 / 进程退出标志、告警原子位、
//! 终态判定 [`decide_done`]、`WatchOpts` / `WatchOutcome` / `ResumeCtx` 三种载体、
//! [`begin_watch`] 入口与 [`watch_loop`] 后台线程、单次采样 [`poll_once`]。
//!
//! 纯文本判据在 [`analyze`]，交付与诊断入口在 [`deliver`]，三者经 `readback` 根再导出后对外。
//!
//! [`analyze`]: crate::readback::analyze
//! [`deliver`]: crate::readback::deliver

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::readback::analyze::{echo_count, panel_needs_login};
use crate::readback::deliver::{deliver, deliver_login_notice};
use crate::readback::snapshot::{parse_snapshot, Snapshot};
use crate::result::{show, show_async, AnchorRect, CardView, TurnMode};

/// 轮询间隔（快速档）：活动回读初期 / 尚未定位容器时用。
pub const POLL_INTERVAL_MS: u64 = 250;
/// 轮询间隔（慢速档）：已定位容器且文本连续稳定（≥ `MIN_STABLE_SAMPLES` 次采样不变）后放宽，
/// 显著降低 eval 次数（长回答等完整期间不再每 250ms 打一次主线程）。
pub const POLL_SLOW_MS: u64 = 1000;
/// 静默阈值：回答文本连续这么久不再变化才可能判「已写完」。
///
/// ★ 由 800ms → 1200ms（QA P1-1）→ **2400ms**（Pydantic 划词尾段丢失，2026-09-17）：
///   DeepSeek 长回答分段生成，chunk 间的自然停顿可 >1.2s，且此时页面上「停止生成」控件未必常驻
///   （`snap.streaming` 被间歇误报 false ⇒ `veto=false`）→ 1200ms 静默会误判完成、把后续尾段丢掉。
///   2400ms + 5 次稳定采样，迫使「完整判终」必须跨过更长稳定期，压过分段停顿。真正的护栏仍是
///   `MIN_STABLE_SAMPLES`（采样次数义务），本阈值是第二道墙。
pub const QUIET_MS: u64 = 2400;
/// 终态所需的**连续稳定采样次数**。与 `QUIET_MS` 二者需**同时**满足。
pub const MIN_STABLE_SAMPLES: usize = 5;
/// 最小长度：短于此长度的文本不视为「回答」（避免把空串/瞬时残留当结果）。
pub const MIN_LEN: usize = 1;

/// ★ E1（2026-09-26 启动门竞态修复）：第四路「尾形」启动证据的**二次收敛**要求次数。
///
/// 第四路判据 `located && cur_echo > 0 && !last_echo` 比前三路宽：它只证明「容器里含我方提示词」
/// 且「末块不是回显（其后已有块，可能是本轮助手块）」，**不**证明那一定是**本轮**的新提问。
/// 故要求该形态**连续命中** `STARTUP_TAIL_CONFIRM` 次采样（默认 2 次 ≈ 一拍）才判「已开始」，
/// 以排除「回显尚未落盘 / 命中的是上一轮同文本提问」的瞬时误判。详见 [`startup_gate`]。
pub const STARTUP_TAIL_CONFIRM: u32 = 2;
/// 总超时：超过则降级交付「可能未完成」的部分结果。
///
/// ★ 语义自 2026-09-17 起被「自适应截止」接管（见 [`watch_loop`]）：本值仍是**已定位容器但尚未
///   见流式/增长**时的默认上限；在「有进展」时会被 [`PROGRESS_TIMEOUT_MS`] 放宽，在「无进展」时
///   会被 [`NO_PROGRESS_TIMEOUT_MS`] 提前掐断。
pub const TOTAL_TIMEOUT_MS: u64 = 60_000;
/// ★ 自适应截止-宽限：一旦「已定位容器 且（文本仍在变化 或 曾见流式生成）」，总超时由
///   `TOTAL_TIMEOUT_MS`（60s）放宽到本值（120s），让**长回答**等完整 —— 修复「回流不全」根因
///   （长回答生成可超过 60s，撞 60s 会被降级交付半截）。
pub const PROGRESS_TIMEOUT_MS: u64 = 120_000;
/// ★ 自适应截止-早退：当**从未定位到容器 / 无任何回答文本**（约定为「无进展」，如 0 次有效采样、
///   页面未注入、面板停在非会话页等）持续到本时长（25s）→ 提前放弃，给一句可执行提示，优于
///   静默干等 60s 后才看到模糊的「回答可能未完成」（反例 `#740`）。
pub const NO_PROGRESS_TIMEOUT_MS: u64 = 25_000;
/// ★ 流式推送的最小间隔（毫秒）：答案边生成边推给结果卡，但不必每个轮询周期（250ms）都推 ——
///   节流到这个间隔，既够「看着它长出来」，又不会把主线程灌满 `eval`。
pub const STREAM_PUSH_INTERVAL_MS: u64 = 400;
/// `streaming` 信号的**安全阀**：若它持续为真超过这时长而回答毫无变化，视为信号失灵、忽略之
/// （否则页内启发式一旦误报「正在生成」，回读会必然超时）。
const STREAMING_VETO_MAX_MS: u64 = 10_000;
/// 单次 `eval_with_callback` 的回调等待上限（页面忙 / 未加载时的兜底）。
const POLL_CALLBACK_TIMEOUT_MS: u64 = 1_500;

/// 内部诊断日志（统一前缀 `readback:`）。
pub(crate) fn log(msg: &str) {
    crate::diag::log(&format!("readback: {msg}"));
}

/// 内部告警日志（统一前缀 `readback:`）。
pub(crate) fn warn(msg: &str) {
    crate::diag::warn(&format!("readback: {msg}"));
}

/// 轮询代次：新的回读会作废在途的旧回读（避免两次采集并发写同一张卡片）。
pub(crate) static WATCH_GEN: AtomicU64 = AtomicU64::new(0);

/// 进程退出标志。`lib.rs` 在 `RunEvent::ExitRequested` 时置位，
/// `watch_loop` 据此提前结束 —— 避免应用已退出、panel Webview 已被析构后，
/// 残留的 worker 线程仍对其 `eval`（产生 `warn` 噪声与潜在竞态）。
/// 与 `WATCH_GEN` 正交：前者在「新的回读开始」时作废旧回读，后者在「进程退出」时统一中止。
static EXITING: AtomicBool = AtomicBool::new(false);

/// 由 `lib.rs::run` 在优雅退出路径调用，标记进程正在退出。
pub fn mark_exiting() {
    EXITING.store(true, Ordering::SeqCst);
}

// ---- R3/R5/R7 的跨线程告警位 ----
// 这些由「发送侧」或「回读判定」在各自线程置位，`deliver`（回读线程）在交付时读取并入卡片。
// 用原子/信号量，避免在新线程间引 Mutex。逐次交付读后即清（见 [`take_flags`]）。

/// R7：最近一次注入发送复核确认失败（输入框最终未清空，见 `inject::verify_sent` 的最终分支）。
pub(crate) static SEND_CONFIRM_FAILED: AtomicBool = AtomicBool::new(false);
/// R3：最近一次回读正文命中**极强**的注入特征（可执行脚本形态）。
static INJECTION_SUSPECTED: AtomicBool = AtomicBool::new(false);
/// R5：回读判定明确探测到登录失效（面板 URL 落入登录态、或快照持续「无发送按钮且无回答」）。
pub(crate) static SESSION_EXPIRED: AtomicBool = AtomicBool::new(false);

/// 交付时读出并**清位**的三个告警标志。（R3/R5/R7 共用。）
pub(crate) fn take_flags() -> (bool, bool, bool) {
    (
        SEND_CONFIRM_FAILED.swap(false, Ordering::SeqCst),
        INJECTION_SUSPECTED.swap(false, Ordering::SeqCst),
        SESSION_EXPIRED.swap(false, Ordering::SeqCst),
    )
}

/// R7：注入发送复核确认失败时，由 `inject::verify_sent` 调用，置位「发送可能未发出」。
/// 下一次 `deliver` 会把它并入结果卡，提示用户检查面板。
#[cfg(test)]
pub fn mark_send_confirmed_failed() {
    SEND_CONFIRM_FAILED.store(true, Ordering::SeqCst);
}

/// R3：纯函数 —— 探测回答正文是否命中**极强**的注入特征（可执行脚本形态）。
///
/// 刻意**极保守**：只匹配明确的可执行脚本/事件处理器语法（`<script`、`javascript:`、注入
/// 常用的事件监听 `onerror=`/`onload=`），普通文本（代码示例里本来就常见 `<script` 字样）会
/// 被误报，为此**正文照常净化展示**（result.html 的 HTML 白名单），这里只是额外给一个
/// 「疑似异常内容」的顶部提示，绝不拦截、绝不删除。判错最高代价 = 一条多余提示条，
/// 不会阻断任何正常回答。
pub fn detect_injection(text: &str) -> bool {
    // 全小写匹配，避免大小写变体绕过；命中任一即成立。
    let low = text.to_ascii_lowercase();
    low.contains("<script")
        || low.contains("javascript:")
        || low.contains(" onerror=")
        || low.contains(" onload=")
}

/// ★ 纯函数：终态判定 —— 「回答已写完」。
///
/// 条件（**全部**满足）：① **无任何否决信号**（页内仍在生成 `streaming`，或末块仍只有
/// 思考过程 `pending`）；② ★ 「「已写完」正向信号 `done` 必须为真** —— 末块已渲染出**真实答案
/// 元素**（非思考子树的 `ds-markdown`，见页内 `hasAnswerElement`），不再依赖复制/重新生成等
/// 操作行按钮（带联网搜索的长回答页底按钮 hover 才显示或语义不同，探不到会误卡死，真机 #108）；
/// ③ 当前文本长度 ≥ `min_len`；④ 与上一次采样**完全相同**；⑤ 已连续稳定 ≥ `MIN_STABLE_SAMPLES`
/// 次采样；⑥ 距上次变化已 ≥ `quiet_ms`。
///
/// ★★ 2026-09-20 判终重构：`done` 由「启发式」升级为「**必要条件**」。旧实现只靠「静默 × 稳定
/// 采样」，在 DeepSeek 分段生成、落尾段前瞬间停顿且无 streaming 标记时必然赛输，把最后一段
/// 静默丢给「已写完」。现在 —— `done=false` 即便静默再久、采样再稳，也**不得**判终；只有
/// 页内确凿出现「消息已完整」的操作行才放行。代价：`done` 探不到时走超时降级（「可能未完成」+
/// 继续读取按钮），但那正是「诚实降级」而不再是「静默丢段」。
///
/// `prev == None`（首次采样）时一律返回 `false` —— 单次采样**不足以**判定终态。
// 终态判定天然依赖多路信号（前后文 / 静默 / veto / done / 阈值），故参数偏多；
// 聚合成结构体收益有限且会扩散改动，这里显式允许。
#[allow(clippy::too_many_arguments)]
pub fn decide_done(
    prev: Option<&str>,
    cur: &str,
    stable_samples: usize,
    quiet_elapsed_ms: u64,
    veto: bool,
    done: bool,
    quiet_ms: u64,
    min_len: usize,
) -> bool {
    if veto {
        return false;
    }
    // ★ 「已写完」正向信号缺失 → 铁定未完成：即便静默/稳定满足也**永不**判终。
    if !done {
        return false;
    }
    if cur.chars().count() < min_len {
        return false;
    }
    match prev {
        Some(p) => p == cur && stable_samples >= MIN_STABLE_SAMPLES && quiet_elapsed_ms >= quiet_ms,
        None => false,
    }
}

/// ★ E1：**非空提示词**下「本轮是否已开始」的启动门判据（纯函数，便于单测）。
///
/// 返回 `(started, tail_streak)`：
/// - `started`：本轮是否判定为「已开始」；
/// - `tail_streak`：第四路（尾形）**连续命中**的次数（供下一拍二次收敛使用）。
///
/// ## 既有三路正向证据（QA P0-1 / P0-2，**逐位不变**）—— 任一命中即开始
/// ① `new_echo`：含提示词的块数较启动基线增长（新回显落盘）；
/// ② `last_echo`：末块即回显（提问刚落盘、回答未开始）；
/// ③ `streaming`：页内「正在生成」信号。
///
/// ## ★ 第四路（E1 新增）：覆盖真机现场形态 `[上一轮答案, 回显, 助手块]`
/// 判据 `located && cur_echo > 0 && !last_echo`：容器已定位、尾窗**含**我方提示词（回显已落盘）、
/// 但回显**不是末块**（其后已有块，即本轮助手回答块已开始渲染）。此时若 `new_echo=false`
/// （例如基线已把该回显计入）且 `streaming` 未探到（页内信号失灵），旧三路**全 false** →
/// 本轮永不开始 → 答案就地超时丢弃。这是「回读启动门竞态」的直接根因。
///
/// ## 二次收敛（防误判）
/// 第四路较宽，故要求形态**连续**命中 `STARTUP_TAIL_CONFIRM` 次采样（[`STARTUP_TAIL_CONFIRM`]）。
/// 每一次采样都重算形态：不满足即把连续计数**清零**，保证「连续」语义（非累计）。
pub fn startup_gate(
    new_echo: bool,
    last_echo: bool,
    streaming: bool,
    located: bool,
    cur_echo: usize,
    prev_tail_streak: u32,
) -> (bool, u32) {
    // 第四路形态（每拍重算；不满足即清零，保证「连续」而非「累计」）。
    let tail_shape = located && cur_echo > 0 && !last_echo;
    let tail_streak = if tail_shape {
        prev_tail_streak.saturating_add(1)
    } else {
        0
    };
    // 三路既有正向证据：任一命中 → 立即开始（既有语义，逐位不变）。
    if new_echo || last_echo || streaming {
        return (true, tail_streak);
    }
    // 第四路：二次收敛后放行。
    (tail_streak >= STARTUP_TAIL_CONFIRM, tail_streak)
}

/// ★ E2（2026-09-26 流式信号失灵修复）：容器是否「有进展」——文本总长或块数较上一次采样增长。
///
/// 页内 `streaming` 信号不可靠时（见 `dsfloat_init.js::pageStreaming` 的修复），仍可用
/// `snapshot.total`（容器内文本总长）/ `snapshot.blocks`（有文本的块数）的增长证明「仍在推进」，
/// 供 [`watch_loop`] 的自适应截止放宽（避免长回答被 `opts.timeout_ms`（60s）误掐）。
///
/// 刻意只认「增长」：文本变短（页面重排 / 容器切换）不算进展。
pub fn container_grew(total: usize, prev_total: usize, blocks: usize, prev_blocks: usize) -> bool {
    total > prev_total || blocks > prev_blocks
}

/// 回读参数。
#[derive(Debug, Clone)]
pub struct WatchOpts {
    pub request: crate::workflow::Token,
    pub quiet_ms: u64,
    pub min_len: usize,
    pub timeout_ms: u64,
    /// 诊断标签（出现在日志里，便于区分多个调用点）。
    pub tag: String,
    /// 结果卡的**来源标签**（卡片副标题：「划词结果」/「识文字」/「追问结果」…）。
    /// 与 `tag` 分开：`tag` 是日志口径，本字段是 UI 口径。
    pub source: String,
    /// 结果卡的**标题**（豆包同款的一句话标题，如「这里是我的解释」）。
    pub title: String,
    /// 被处理的**原文**（划词选中的句子 / 截图里的问题）→ 卡片顶部引用条。空则不显示。
    pub quote: String,
    /// ★ 本轮回读在卡片会话里的**轮次语义**（见 `result::TurnMode`）。
    ///
    /// 只有**本轮的第一推**用它；其后每一推一律 [`crate::result::TurnMode::Update`]
    /// （否则一轮回答会在卡片里被拆成好几轮）。
    pub turn: TurnMode,
}

impl Default for WatchOpts {
    fn default() -> Self {
        Self {
            request: crate::workflow::Token::default(),
            quiet_ms: QUIET_MS,
            min_len: MIN_LEN,
            timeout_ms: TOTAL_TIMEOUT_MS,
            tag: "watch".to_string(),
            source: "结果".to_string(),
            title: "DeepSeek 的回答".to_string(),
            quote: String::new(),
            // 绝大多数调用点是「新的划词 / 新的一次提问」⇒ 默认开一轮新会话。
            turn: TurnMode::default(),
        }
    }
}

/// 回读结果。
/// ★ 「继续读取」恢复点（方案 A，2026-09-19）：超时降级交付后，页面上「回答可能未完成」
///   有一条 `result_resume` 按钮。其本质是**重放一轮回读**（不重新注入）：
///   `begin_watch` 在每次开跑前把本轮参数（prompt / anchor / opts）存进这个恢复点，
///   前端点按钮时调用 [`resume`] 读取并重放 —— 用 `Update` turn 就地补齐**末轮**，
///   而非新开一轮。读面板的同一个 prompt 定位容器，故可安全补全「误判完整但漏读」的尾段。
#[derive(Clone)]
pub struct ResumeCtx {
    pub token: crate::workflow::Token,
    pub prompt: String,
    pub anchor: Option<AnchorRect>,
    pub opts: WatchOpts,
}

impl ResumeCtx {
    fn for_send(prompt: String, token: crate::workflow::Token) -> Self {
        Self {
            token,
            anchor: None,
            opts: WatchOpts {
                request: token,
                quote: prompt.clone(),
                ..Default::default()
            },
            prompt,
        }
    }
}
pub fn remember_send(prompt: String, token: crate::workflow::Token) {
    remember_resume(ResumeCtx::for_send(prompt, token));
}
pub fn retry_prompt(token: crate::workflow::Token) -> Option<String> {
    RESUME.lock().ok().and_then(|g| {
        g.as_ref()
            .filter(|c| c.token == token && crate::workflow::active(token))
            .map(|c| c.prompt.clone())
    })
}

static RESUME: Mutex<Option<ResumeCtx>> = Mutex::new(None);

/// 新会话或换址时作废旧轮询，并清除可恢复的旧问题。
pub fn cancel() {
    WATCH_GEN.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut ctx) = RESUME.lock() {
        *ctx = None;
    }
    let _ = take_flags();
}

/// 记录本轮可恢复参数（每次 `begin_watch` 开跑前调用）。
fn remember_resume(ctx: ResumeCtx) {
    if let Ok(mut g) = RESUME.lock() {
        *g = Some(ctx);
    }
}

/// 「继续读取」：重放最近一次被超时降级中断的回读。
///
/// 语义：与注入无关的**纯回读重放** —— 重放时 `opts.turn` 强制 `Update`（就地覆盖卡片末轮，
/// 不弹占位卡、不新开会话）；若期间用户已做新划词（Fresh 重置了卡片），`Update` 会退化到
/// 追加一轮并正常交付，仍是合理结果。没有可恢复参数时静默忽略（按钮只在降级轮出现，理论不触发）。
///
/// ★ 返回状态串供前端即时反馈（杜绝「点了没反应」的静默无输出）：
///   `"ok"` = 已启动续读；`"no-resume"` = 无可恢复的回读；`"panel-missing"` = 面板窗口不存在。
pub fn resume(app: &AppHandle) -> String {
    let ctx = RESUME
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .filter(|c| crate::workflow::active(c.token));
    if crate::workflow::status(crate::workflow::current()) != crate::workflow::SendStatus::TimedOut
    {
        return "no-resume".into();
    }
    match ctx {
        Some(mut ctx) => {
            // ★ 面板窗口不存在时，begin_watch 会静默 early-return（无回读、无反馈）→ 「点了没反应」。
            //   这里先判一次，给前端明确的失败原因。
            if app.get_webview_window("panel").is_none() {
                crate::diag::log(&format!(
                    "[{}] 继续读取：面板窗口不存在，无法续读",
                    ctx.opts.tag
                ));
                return "panel-missing".to_string();
            }
            if !crate::workflow::claim_resume(ctx.token) {
                return "busy".into();
            }
            ctx.opts.turn = TurnMode::Update;
            crate::diag::log(&format!(
                "[{}] 继续读取：重放上一轮回读（prompt {} 字符）",
                ctx.opts.tag,
                ctx.prompt.chars().count()
            ));
            begin_watch(app, ctx.prompt, ctx.anchor, ctx.opts);
            "ok".to_string()
        }
        None => {
            crate::diag::log("继续读取：无可恢复的回读（已被新会话覆盖或尚未发生超时降级）");
            "no-resume".to_string()
        }
    }
}

/// 回读结果。
#[derive(Debug, Clone)]
pub struct WatchOutcome {
    pub text: String,
    /// `true` = 判到终态；`false` = 超时降级（文本可能不完整，须标注「回答可能未完成」）。
    pub complete: bool,
    pub samples: u32,
    pub elapsed_ms: u64,
    pub tag: String,
    /// 结果卡的来源标签（随 `WatchOutcome` 一起交付给 `result::show`）。
    pub source: String,
    /// 思考过程（与本轮回答同源：同一消息块里被页内剥掉的那段前缀）。
    pub thinking: String,
    /// 结果卡标题（见 [`WatchOpts::title`]）。
    pub title: String,
    /// 被引用的原文（见 [`WatchOpts::quote`]）。
    pub quote: String,
}

/// 开始一次回读：取基线 → 短命线程轮询 → 判终态 → 交付。
///
/// 非阻塞：立即返回，实际轮询在后台线程。新的调用会使在途的旧回读作废。
pub fn begin_watch(
    app: &AppHandle,
    prompt: String,
    anchor: Option<AnchorRect>,
    mut opts: WatchOpts,
) {
    let token = crate::workflow::current();
    opts.request = token;
    let Some(panel) = app.get_webview_window("panel") else {
        crate::workflow::update(
            app,
            token,
            crate::workflow::SendStatus::Failed("面板窗口未创建".into()),
        );
        warn(&format!(
            "[{}] panel 窗口不存在（应用尚未创建面板）",
            opts.tag
        ));
        return;
    };
    // ★ R8-③：工具条点击即显卡片——若是开新会话（Fresh），立即弹一张「生成中」占位卡，
    //   随后回读到的数据以 `Update` 增量填入，不再等数据回来才显示（点完工具条立刻有反馈）。
    //   占位卡用 `Fresh` 弹出（清掉旧会话 + 清掉「已关闭」标记），其后回读统一改为 `Update`。
    if opts.turn == TurnMode::Fresh {
        show_async(
            app,
            CardView {
                request: Some(token),
                title: opts.title.clone(),
                source: opts.source.clone(),
                quote: opts.quote.clone(),
                text: String::new(),
                thinking: String::new(),
                think_label: String::new(),
                streaming: true, // 显示流式光标 + 隐藏动作行（「生成中」）
                complete: false,
                ..Default::default()
            }
            .with_turn(TurnMode::Fresh),
            anchor,
        );
        // 占位卡已用 Fresh 弹出 ⇒ 后续回读一律 Update，避免又开一轮。
        opts.turn = TurnMode::Update;
    }
    crate::result::bind_request(app, token);
    let gen = WATCH_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    // ★ 方案 A：本轮参数作为「继续读取」恢复点（prompt 定位容器必需；anchor 决定续读后卡片落点）。
    remember_resume(ResumeCtx {
        token,
        prompt: prompt.clone(),
        anchor,
        opts: opts.clone(),
    });
    log(&format!(
        "[{}] 开始回读（间隔 {POLL_INTERVAL_MS}ms · 静默 {quiet}ms × {stable} 次 · 最短 {min} · 超时 {timeout}ms）",
        opts.tag,
        quiet = opts.quiet_ms,
        stable = MIN_STABLE_SAMPLES,
        min = opts.min_len,
        timeout = opts.timeout_ms
    ));
    let app = app.clone();
    std::thread::spawn(move || watch_loop(app, panel, prompt, anchor, opts, gen, token));
}

/// 轮询主循环（worker 线程）。
fn watch_loop(
    app: AppHandle,
    panel: WebviewWindow,
    prompt: String,
    anchor: Option<AnchorRect>,
    opts: WatchOpts,
    gen: u64,
    token: crate::workflow::Token,
) {
    let mut start = Instant::now();
    let mut observing = false;
    let p_norm = crate::capture::normalize_text(&prompt);
    // 空提示词模式下的启动基线（仅该模式使用；非空提示词模式一律靠正向证据启动）。
    let mut empty_base: Option<String> = None;
    let mut echo_base: Option<usize> = None;
    let mut started = false;
    let mut prev_ans: Option<String> = None;
    let mut last_ans = String::new();
    let mut stable: usize = 0;
    let mut last_change = Instant::now();
    let mut streaming_since: Option<Instant> = None;
    let mut samples: u32 = 0;
    let mut last_err_log: Option<Instant> = None;
    // ★ 流式推送节流。用 `Option<Instant>` 而非「now - 常量」的减法：`Instant` 在 Windows 上
    //   自系统启动计时，开机不足阈值时做减法会**下溢 panic**（QA P3-1 同型坑）。
    let mut last_push: Option<Instant> = None;
    let mut pushed = String::new();
    // ★ 「思考过程」的流式推送也需节流去重：仍在思考时 `ans` 其实是思考正文（见下），
    //   与回答正文分开记，免得两者互相把对方判成「没变化」。
    let mut pushed_think = String::new();
    // 最近一次成功采样取到的思考正文 —— 超时分支（循环头，此处已无 `snap`）也要一并交付。
    let mut last_think = String::new();
    // 最近一次成功采样看到的「尾窗 + 骨架」—— 超时分支（循环头，此处已无 `snap`）也要能落盘。
    let mut last_blocks = String::new();
    let mut last_dump = String::new();
    // ★ 本轮回读是否已经推过卡片：**第一推**才用 `opts.turn` 的语义（Fresh / Follow），
    //   之后每一推一律 `Update` —— 否则一轮回答会在卡片里被拆成好几轮（见 `next_turn`）。
    let mut pushed_any = false;
    // ★ 自适应截止依据（见循环头的 `deadline_ms`）：
    //   · `located` = 是否已成功定位到容器（命中过回显 / 容器名非空）—— 区分「无进展早退」与「有进展宽限」；
    //   · `ever_grow` = 本轮回答是否仍在变化 —— 「有进展」信号（放宽容许的第二道依据）。
    //   · 「流式是否最近仍活跃」不再用变量记录，循环头直接从 `streaming_since` 的 10s 窗现判
    //     （见 `stream_active`）—— 若用历史性 `ever_streaming`，信号失灵时会永久放宽超时（审核 P1）。
    let mut located = false;
    let mut ever_grow = false;
    // ★ E1：第四路（尾形）启动证据的**连续**命中次数（二次收敛，见 [`startup_gate`]）。
    let mut tail_streak: u32 = 0;
    // ★ E2：容器「文本总长 / 有文本块数」的上一次读数 —— 用于把容器增长纳入「有进展」。
    //   `None` = 尚未取得首拍基线（避免把「容器里本就有上一轮内容」误当本轮增长）。
    let mut prev_total: Option<usize> = None;
    let mut prev_blocks: Option<usize> = None;

    loop {
        if WATCH_GEN.load(Ordering::SeqCst) != gen {
            log(&format!("[{}] 已被新的回读取代，中止", opts.tag));
            return;
        }
        if !crate::workflow::active(token) {
            return;
        }
        match crate::workflow::status(token) {
            crate::workflow::SendStatus::Failed(_) | crate::workflow::SendStatus::Cancelled => {
                return
            }
            crate::workflow::SendStatus::Sending => {
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                continue;
            }
            _ => {}
        }
        if EXITING.load(Ordering::SeqCst) {
            log(&format!("[{}] 进程退出，回读线程中止", opts.tag));
            return;
        }
        if !observing {
            start = Instant::now();
            observing = true;
        }
        let elapsed_ms = start.elapsed().as_millis() as u64;
        // ★ 自适应截止（2026-09-17）：
        //   ① 有进展 → 放宽：已定位容器 且（回答仍在变化 `ever_grow` 或 曾见流式 `ever_streaming`）
        //      ⇒ 上限抬到 [`PROGRESS_TIMEOUT_MS`]（120s），让**长回答**等完整（「回流不全」根因）。
        //   ② 无进展 → 早退：从未定位到容器 / 无任何回答文本 ⇒ [`NO_PROGRESS_TIMEOUT_MS`]（25s）
        //      就放弃并给一句可执行提示（反例 `#740` 白等 60s 的洞）。
        //   已定位但尚无进展（如思考中）⇒ 维持既有 `opts.timeout_ms`（60s），不收紧、不回溯。
        // ★ 审核 P1 修复：放宽容许以「**近端仍活跃**」为界，而非「曾发生过」。
        //   原实现用历史性 `ever_streaming` —— 一旦这次见过流式信号就**永不回退**，
        //   把超时永久锚在 120s；而 10s 安全阀只作用于 `veto`（终态判定）不作用于超时，
        //   于是页内 streaming 启发式中道失灵时，用户会白等 120s（P1）。
        //   改为复用 `STREAMING_VETO_MAX_MS` 这把尺：最近 10s 内**仍**有流式信号才放宽；
        //   信号停止刷新 10s 后，超时回落到既定 `opts.timeout_ms`（60s），不再静默拖死。
        //   `ever_grow`（文本仍变化）保留 —— 它与 `veto` 正交，长回答分段增长时保持放宽是正确行为。
        let stream_active = streaming_since
            .map(|t| t.elapsed() < Duration::from_millis(STREAMING_VETO_MAX_MS))
            .unwrap_or(false);
        let deadline_ms = if !located {
            NO_PROGRESS_TIMEOUT_MS
        } else if ever_grow || stream_active {
            PROGRESS_TIMEOUT_MS
        } else {
            opts.timeout_ms
        };
        if elapsed_ms > deadline_ms {
            // ★ 硬约束：超时/放弃**不得静默** —— 降级交付已读到的部分。
            // ★ 同时落盘「末窗口实际内容 + 容器骨架」：超时/早退也必须有可复盘的现场（B3 把错候选
            //   全部拦掉后，正常路径就是「一直等到超时」→ 没有这一行就无从判断拦对了没有）。
            if !last_blocks.is_empty() {
                log(&format!(
                    "[{}] 超时前尾窗：{}",
                    opts.tag,
                    crate::diag::content(&last_blocks)
                ));
            }
            if !last_dump.is_empty() {
                log(&format!(
                    "[{}] 超时前容器骨架：{}",
                    opts.tag,
                    crate::diag::content(&last_dump)
                ));
            }
            // ★ R15-C1 兜底：登录页上也可能出现「容器能定位、但永远等不到回答」的形态
            //   （此时走不到采样失败分支）⇒ 截止点再确认一次，避免把「没登录」
            //   说成「回答可能未完成」。（现场日志先落盘，取证不受影响。）
            if panel_needs_login(&panel) {
                deliver_login_notice(
                    &app,
                    &opts,
                    anchor,
                    &mut pushed_any,
                    samples,
                    elapsed_ms,
                    gen,
                );
                return;
            }
            if !located {
                // ② 早退：也没有任何已读内容可交付 → 给一句**可执行的提示**（而非模糊的「可能未完成」）。
                warn(&format!(
                    "[{}] **无进展早退**（{}ms 未定位到容器 · 共 {} 次采样）→ 提示用户检查面板",
                    opts.tag, elapsed_ms, samples
                ));
                // 保留远端现场：未确认发送或回读超时均不自动导航或重发。
                deliver(
                    &app,
                    WatchOutcome {
                        text: crate::readback::NO_LOCATE_NOTICE_TEXT.to_string(),
                        // 未读到回答属于超时，保留继续读取入口。
                        complete: false,
                        samples,
                        elapsed_ms,
                        tag: opts.tag.clone(),
                        source: "回读失败".to_string(),
                        thinking: last_think.clone(),
                        title: "未能读到回答".to_string(),
                        quote: opts.quote.clone(),
                    },
                    anchor,
                    gen,
                    token,
                    next_turn(&opts, &mut pushed_any),
                );
                return;
            }
            let text = prev_ans.clone().unwrap_or_else(|| last_ans.clone());
            deliver(
                &app,
                WatchOutcome {
                    text,
                    complete: false,
                    samples,
                    elapsed_ms,
                    tag: opts.tag.clone(),
                    source: opts.source.clone(),
                    thinking: last_think.clone(),
                    title: opts.title.clone(),
                    quote: opts.quote.clone(),
                },
                anchor,
                gen,
                token,
                next_turn(&opts, &mut pushed_any),
            );
            return;
        }

        // 诊断骨架**按需**索取：起始 2 次采样（看清容器形状）+ 文本已趋稳（即将交付）时。
        // 其余轮次不索取 —— 免得为诊断每 250ms 多跑一遍尾窗子树遍历。
        let want_dump = samples < 2 || stable >= 1;
        match poll_once_guarded(&app, &panel, &prompt, want_dump, token) {
            Ok(snap) => {
                if matches!(
                    crate::workflow::status(token),
                    crate::workflow::SendStatus::Uncertain(_)
                ) {
                    if !snap.request_bound {
                        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                        continue;
                    }
                    crate::workflow::update(&app, token, crate::workflow::SendStatus::Reading);
                }
                samples += 1;
                let cur_echo = echo_count(&snap, &p_norm);
                let (ans, think) =
                    crate::readback::analyze::locate_answer_and_thinking(&snap, &prompt);
                // ★ 仍在思考（`pending`）时 `ans` 恒为空（页内切不出回答，见 splitBlock 的
                //   `thinkingLike` 分支），**思考正文在 `think` 字段里**（= 整块文本 / 剥离后的
                //   思考子树）。`last_think` 统一取 `think`，正文记为空，免得思考正文被当成回答交付/留存。
                last_ans = if snap.pending {
                    String::new()
                } else {
                    ans.clone()
                };
                last_think = think.clone();
                last_blocks = fmt_blocks(&snap.texts);
                last_dump.clone_from(&snap.dump);
                // ★ 自适应截止依据：命中回显 / 容器名非空 → 已定位容器。
                located |= !snap.container.is_empty() || cur_echo > 0;

                // ★ E2：把「容器总文本 / 块数」的增长也纳入「有进展」—— 页内 `streaming` 信号
                //   失灵时，容器仍在长（`snap.total`/`snap.blocks`，见 `snapshot.rs:91-92`）可作为
                //   进展依据，避免长回答被 `opts.timeout_ms`（60s）误掐（配合循环头的超时放宽逻辑）。
                //   只认「增长」：文本变短（页面重排 / 容器切换）不算进展。
                if let (Some(pt), Some(pb)) = (prev_total, prev_blocks) {
                    if container_grew(snap.total, pt, snap.blocks, pb) {
                        ever_grow = true;
                    }
                }
                prev_total = Some(snap.total);
                prev_blocks = Some(snap.blocks);

                // ★ 流式输出：答案边生成边把增量推给结果卡。
                //   只在**已确认本轮开始之后**推送 —— 未开始时推的是上一轮的半截，
                //   正是 QA P0-2 那个「把上一轮当本轮」的洞；节流见 STREAM_PUSH_INTERVAL_MS。
                // ★ 变化判定：`pending` 时思考正文在 `think` 里、随生成增长，故用 `think` 比对；
                //   否则用 `ans`（回答）。原实现两处都用 `ans`，但 pending 时 `ans` 恒空 ⇒ 思考阶段
                //   永不推送 —— 即「结果卡片思考过程不显示」的直接根因（2026-09-20 改 splitBlock 后未同步）。
                let changed = if snap.pending {
                    think != pushed_think
                } else {
                    ans != pushed
                };
                if started
                    && (!ans.is_empty() || !think.is_empty())
                    && changed
                    && last_push
                        .map(|t| t.elapsed() >= Duration::from_millis(STREAM_PUSH_INTERVAL_MS))
                        .unwrap_or(true)
                {
                    // 仍在思考 → 卡片只长「思考过程」，**正文留空**：不把思考正文当回答推给用户。
                    // （`pending` 时思考正文在 `think` 里，故取 `think` 而非 `ans`。）
                    let (text, thinking) = if snap.pending {
                        pushed_think.clone_from(&think);
                        (String::new(), think.clone())
                    } else {
                        pushed.clone_from(&ans);
                        (ans.clone(), think.clone())
                    };
                    last_push = Some(Instant::now());
                    if !crate::workflow::active(token) {
                        return;
                    }
                    show(
                        &app,
                        &CardView {
                            request: Some(token),
                            title: opts.title.clone(),
                            source: opts.source.clone(),
                            quote: opts.quote.clone(),
                            text,
                            thinking,
                            think_label: String::new(),
                            streaming: true,
                            complete: false,
                            ..Default::default()
                        }
                        .with_turn(next_turn(&opts, &mut pushed_any)),
                        anchor,
                    );
                }

                // `streaming` 持续计时 + 安全阀（信号失灵时不至于把回读拖死）。
                if snap.streaming {
                    streaming_since.get_or_insert_with(Instant::now);
                } else {
                    streaming_since = None;
                }
                let streaming_veto = streaming_since
                    .map(|t| t.elapsed() < Duration::from_millis(STREAMING_VETO_MAX_MS))
                    .unwrap_or(false);
                // ★ 否决信号二：`pending` = 末块此刻**只有思考过程**、回答尚未开始（结构事实）。
                //   「思考完毕 → 正文开始」之间的停顿真机可超过静默阈值，且此时页面上未必有
                //   可识别的「停止生成」控件 ⇒ 只有这条信号能拦住「把思考正文当回答交付」。
                let veto = streaming_veto || snap.pending;

                if !started {
                    if echo_base.is_none() {
                        echo_base = Some(cur_echo);
                    }
                    if p_norm.is_empty() {
                        // 无提示词可依（诊断模式的退化路径）：只能以「答案文本变化」为启动信号。
                        match empty_base.as_ref() {
                            None => empty_base = Some(ans.clone()),
                            Some(b) if *b != ans => started = true,
                            _ => {}
                        }
                    } else {
                        // ★ 只认**正向证据**（QA P0-1 / P0-2）：新回显落盘 / 末块即回显 / 正在生成。
                        // ★ E1（2026-09-26）：追加**第四路「尾形」**证据，覆盖真机现场形态
                        //   `[上一轮答案, 回显, 助手块]`（回显已落盘但非末块、且基线未变 / streaming
                        //   未探到 ⇒ 旧三路全 false，本轮永不开始）；第四路需**二次收敛**防误判。
                        //   判据与收敛逻辑抽为纯函数 [`startup_gate`]（可单测）。
                        let new_echo = echo_base.is_some_and(|b| cur_echo > b);
                        let last_echo = crate::readback::analyze::last_is_echo(&snap, &p_norm);
                        let (start_ok, streak) = startup_gate(
                            new_echo,
                            last_echo,
                            snap.streaming,
                            located,
                            cur_echo,
                            tail_streak,
                        );
                        tail_streak = streak;
                        if start_ok {
                            started = true;
                            // ★ 起始即落盘「末窗口实际内容 + 容器骨架」：这是判定抽取策略的唯一事实来源。
                            log(&format!(
                                "[{}] 起始尾窗（container={}）：{}",
                                opts.tag,
                                snap.container,
                                crate::diag::content(&last_blocks)
                            ));
                            if !snap.dump.is_empty() {
                                log(&format!(
                                    "[{}] 起始容器骨架：{}",
                                    opts.tag,
                                    crate::diag::content(&snap.dump)
                                ));
                            }
                            log(&format!(
                                "[{}] 本轮已开始（第 {} 次采样 · 新回显={} 末块即回显={} streaming={} 尾形连续={} pending={} done={} reasoning={} · url={} · hops={} · container={}）",
                                opts.tag,
                                samples,
                                new_echo,
                                last_echo,
                                snap.streaming,
                                tail_streak,
                                snap.pending,
                                snap.done,
                                snap.reasoning,
                                snap.url,
                                snap.hops,
                                snap.container
                            ));
                        }
                    }
                    if started {
                        prev_ans = Some(ans);
                        stable = 0;
                        last_change = Instant::now();
                    }
                } else {
                    if prev_ans.as_deref() != Some(ans.as_str()) {
                        ever_grow = true; // ★ 回答仍在变化 → 「有进展」，触发超时放宽。
                        prev_ans = Some(ans.clone());
                        stable = 0;
                        last_change = Instant::now();
                    } else {
                        stable += 1;
                    }
                    let quiet = last_change.elapsed().as_millis() as u64;
                    if decide_done(
                        prev_ans.as_deref(),
                        &ans,
                        stable,
                        quiet,
                        veto,
                        snap.done,
                        opts.quiet_ms,
                        opts.min_len,
                    ) {
                        // ★ 交付前落盘「末窗口实际内容 + 容器骨架」—— 与最终文本逐条对照，
                        //   一眼看出「交付的到底是哪一块」（真机缺陷复盘靠它）。
                        log(&format!(
                            "[{}] 交付前尾窗（streaming={} pending={} done={} reasoning={}）：{}",
                            opts.tag,
                            snap.streaming,
                            snap.pending,
                            snap.done,
                            snap.reasoning,
                            crate::diag::content(&last_blocks)
                        ));
                        if !snap.dump.is_empty() {
                            log(&format!(
                                "[{}] 交付前容器骨架：{}",
                                opts.tag,
                                crate::diag::content(&snap.dump)
                            ));
                        }
                        deliver(
                            &app,
                            WatchOutcome {
                                text: ans,
                                complete: true,
                                samples,
                                elapsed_ms: start.elapsed().as_millis() as u64,
                                tag: opts.tag.clone(),
                                source: opts.source.clone(),
                                thinking: think,
                                title: opts.title.clone(),
                                quote: opts.quote.clone(),
                            },
                            anchor,
                            gen,
                            token,
                            next_turn(&opts, &mut pushed_any),
                        );
                        return;
                    }
                }
            }
            Err(e) => {
                // 页面尚未加载完成 / 未注入：降噪 —— 每 ~5s 最多记一次。
                // ★ 用 `Option<Instant>` 而非 `now - 60s`：`Instant` 在 Windows 上自系统启动计时，
                //   开机 60s 内做减法会**下溢 panic**（QA P3-1）。
                let due = last_err_log
                    .map(|t| t.elapsed() >= Duration::from_secs(5))
                    .unwrap_or(true);
                if due {
                    last_err_log = Some(Instant::now());
                    log(&format!("[{}] 本轮采样失败（{e}），继续等待", opts.tag));
                    // ★ R15-C1：采样失败时顺手确认一次「面板是不是停在登录页」——
                    //   未登录时提问根本没发出去，再怎么等也不会有答案，早退并给一句
                    //   可执行的提示，胜过让用户干等 60s 后看到「回答可能未完成」。
                    //   节流口径与日志一致（首次 + 每 ~5s 一次），不会变成轮询负担。
                    if panel_needs_login(&panel) {
                        deliver_login_notice(
                            &app,
                            &opts,
                            anchor,
                            &mut pushed_any,
                            samples,
                            elapsed_ms,
                            gen,
                        );
                        return;
                    }
                }
            }
        }
        // ★ 自适应轮询（Batch-2 性能优化）：已定位容器且文本连续稳定 ≥ MIN_STABLE_SAMPLES 次采样后
        //   放宽到 `POLL_SLOW_MS`（1s），否则保持 `POLL_INTERVAL_MS`（250ms）。稳态期间 eval 次数
        //   减少约 60%，且不改变终态判定语义（截止/判定逻辑完全不受轮询频率影响）。
        let poll_ms = if located && stable >= MIN_STABLE_SAMPLES {
            POLL_SLOW_MS
        } else {
            POLL_INTERVAL_MS
        };
        std::thread::sleep(Duration::from_millis(poll_ms));
    }
}

/// 本轮回读在卡片里的轮次：**第一推**用调用方声明的语义（`Fresh` / `Follow`），
/// 其后的每一推一律 `Update` —— 否则一轮回答会在卡片里被拆成好几轮。
pub(crate) fn next_turn(opts: &WatchOpts, pushed_any: &mut bool) -> TurnMode {
    let t = if *pushed_any {
        TurnMode::Update
    } else {
        opts.turn
    };
    *pushed_any = true;
    t
}

/// 把尾窗块压成**一整行**诊断（每块 ≤ 60 字符）—— 真机排障用来「看清到底读到了什么」。
///
/// 为什么值得单独落盘：R1 真机缺陷的两类错法（读到思考正文 67 字符 / 读到装饰页脚 23 字符）
/// **都不报错、都判 complete=true** —— 只有把「末窗口实际有哪些块」原样落盘，事后才能复盘。
pub(crate) fn fmt_blocks(texts: &[String]) -> String {
    if texts.is_empty() {
        return "(空)".to_string();
    }
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let head: String = t.chars().take(60).collect();
            format!("#{i}({})「{head}」", t.chars().count())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 单次采样：把 eval 投回主线程，worker 侧带超时等回调结果。
fn poll_once_guarded(
    app: &AppHandle,
    panel: &WebviewWindow,
    prompt: &str,
    want_dump: bool,
    token: crate::workflow::Token,
) -> Result<Snapshot, String> {
    if !crate::workflow::active(token) {
        return Err("已取消".into());
    }
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let panel = panel.clone();
    // 把提示词作为 **JS 字符串字面量**嵌进表达式：JSON 字符串字面量同时是合法 JS 字符串字面量。
    // 页内据此做「容器是否含我方提示词」的**结构判据**（见 `dsfloat_init.js` 的 locateConversation B3）。
    // ★ U+2028 / U+2029 在 ES2019 之前不允许出现在字符串字面量里 → 显式转义兜底。
    let lit = serde_json::to_string(prompt)
        .unwrap_or_else(|_| "\"\"".to_string())
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    // ★ 传的是**表达式**：WebView2 的 ExecuteScript 会把表达式结果 JSON 序列化成回调入参，
    //   所以这里**不能**自己 JSON.stringify（否则双重转义）。
    let js = format!(
        "(function(){{try{{if(!window.__DSFLOAT__||!window.__DSFLOAT__.snapshotConversation)return {{ok:false,error:'未注入'}};return window.__DSFLOAT__.snapshotConversation({lit},{want_dump});}}catch(e){{return {{ok:false,error:String((e&&e.message)||e)}};}}}})()"
    );
    let js = crate::workflow::guarded(token, &js);
    app.run_on_main_thread(move || {
        if !crate::workflow::active(token) {
            return;
        }
        if let Err(e) = panel.eval_with_callback(js, move |raw| {
            if !crate::workflow::active(token) {
                return;
            }
            let _ = tx.send(raw);
        }) {
            warn(&format!("eval_with_callback 失败: {e}"));
        }
    })
    .map_err(|e| format!("run_on_main_thread 失败: {e}"))?;

    let raw = rx
        .recv_timeout(Duration::from_millis(POLL_CALLBACK_TIMEOUT_MS))
        .map_err(|_| "等待页内回调超时".to_string())?;
    if raw.trim().is_empty() {
        return Err("脚本未执行或页面未加载完成".to_string());
    }
    let snap = parse_snapshot(&raw)?;
    // ★ 注入脚本版本探针（每次新代回读最多打一次）：确认新分块逻辑（splitV）已真正运行。页内没传
    //   splitV（=0）说明仍是旧脚本 —— 那正是「重启后修复看似无效」的常见原因（WebView 会话被复用）。
    let sv = snap.split_v;
    if sv != 0 {
        static LAST_SPLIT: AtomicU64 = AtomicU64::new(0);
        if LAST_SPLIT.swap(sv, Ordering::SeqCst) != sv {
            log(&format!("注入脚本在线 splitV={sv}（新分块逻辑已运行）"));
        }
    }
    Ok(snap)
}

pub fn retry_send(app: &AppHandle, request: u64) -> String {
    let token = crate::workflow::Token(request);
    let ctx = RESUME
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .filter(|c| c.token == token && crate::workflow::active(token));
    let Some(mut ctx) = ctx else {
        return "no-retry".into();
    };
    let Some(panel) = app.get_webview_window("panel") else {
        return "panel-missing".into();
    };
    if !crate::workflow::claim_retry(token) {
        return "busy".into();
    }
    crate::inject::inject_and_send_async(panel, ctx.prompt.clone());
    ctx.opts.turn = TurnMode::Update;
    begin_watch(app, ctx.prompt, ctx.anchor, ctx.opts);
    "ok".into()
}

#[cfg(test)]
mod retry_context_tests {
    use super::*;
    #[test]
    fn send_retry_retains_the_full_prompt_and_request_identity() {
        let prompt = "完整提示词\n".repeat(5000);
        let ctx = ResumeCtx::for_send(prompt.clone(), crate::workflow::Token(77));
        assert_eq!(ctx.prompt, prompt);
        assert_eq!(ctx.token, crate::workflow::Token(77));
        assert_eq!(ctx.opts.quote, prompt);
    }
}
