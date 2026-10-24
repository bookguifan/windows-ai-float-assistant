//! §5.1 回读桥 · 交付与诊断入口（从 `readback.rs` 抽出）。
//!
//! 本模块是回读的**单点交付**（`deliver`）、登录态早退提示（`deliver_login_notice`）与
//! 真机诊断入口（`begin_watch_diagnostic`）。轮询引擎在 [`watch`]，纯文本判据在 [`analyze`]。
//!
//! [`analyze`]: crate::readback::analyze
//! [`watch`]: crate::readback::watch

use tauri::{AppHandle, Manager};

use crate::diag::{content, enabled, verbose};
// 以下两项仅被 `#[cfg(debug_assertions)]` 的 `begin_watch_diagnostic` 使用，随 debug 门控，
// 避免 release 下的 unused 告警。
#[cfg(debug_assertions)]
use crate::inject::inject_and_send_async;
#[cfg(debug_assertions)]
use crate::readback::watch::begin_watch;
use crate::readback::watch::{
    detect_injection, log, next_turn, take_flags, warn, WatchOpts, WatchOutcome, SESSION_EXPIRED,
};
use crate::result::{show, AnchorRect, CardView, TurnMode};

/// ★ R15-C1：面板未登录时的**可执行提示**（替代 60s 静默超时）。
///
/// 措辞刻意给出「下一步做什么」+「只需做一次」，而不是「获取失败」这类无行动信息。
pub(crate) const LOGIN_NOTICE_TEXT: &str = "面板还没有登录 DeepSeek，所以这次提问拿不到回答。\n\n\
     请先打开面板并登录（登录态会被记住，只需一次），然后再划词 / 截图。";

/// ★ 自适应早退（无进展 ≥ `NO_PROGRESS_TIMEOUT_MS`）时的**可执行提示**（替代 60s 静默超时）。
/// 措辞给出「下一步做什么」，而不是模糊的「回答可能未完成」。
pub(crate) const NO_LOCATE_NOTICE_TEXT: &str =
    "暂时没有从面板读到本轮回答（持续 25 秒未定位到会话容器 / 无回答）。\n\n\
     请确认：① 面板是否已登录当前站点并停留在会话页；② 面板是否卡死（可切过去看一眼）。\
     \n若复现，面板日志已记录现场，可直接反馈给开发者。";

/// ★ R15-C1：交付「请先登录」提示卡并结束本轮回读。
///
/// 背景（真机可复现）：面板未登录时，`fill()` 找不到输入框 → 提问根本没发出去，
/// 页面上也不会有会话容器 ⇒ 回读每 250ms 采样失败、直到 60s 超时降级，
/// 用户最终只看到「回答可能未完成」——**看不出真正原因是没登录**（而 `panel_state`
/// 早已算出 `NEEDS_LOGIN`，只是它此前**只落日志、没有任何消费者**）。
///
/// 用法上这是**结果卡**而不是浮层：不新增窗口、不违反「不做提示浮层」的既有约束。
pub(crate) fn deliver_login_notice(
    app: &AppHandle,
    opts: &WatchOpts,
    anchor: Option<AnchorRect>,
    pushed_any: &mut bool,
    samples: u32,
    elapsed_ms: u64,
    gen: u64,
) {
    let token = opts.request;
    warn(&format!(
        "[{}] 面板停在登录页（未登录 DeepSeek）⇒ 提前结束回读并提示用户（{} 次采样 / {}ms）",
        opts.tag, samples, elapsed_ms
    ));
    // R5：置位「登录失效」告警，`deliver` 会把 `session_expired` 并入结果卡（供前端可重启的提示）。
    SESSION_EXPIRED.store(true, std::sync::atomic::Ordering::SeqCst);
    deliver(
        app,
        WatchOutcome {
            text: LOGIN_NOTICE_TEXT.to_string(),
            // ★ `complete: true`：这不是「降级交付」，而是一个**确定的结论**。
            //   若给 `false`，卡片会额外标「回答可能未完成（回读超时降级）」，把提示说成故障。
            complete: true,
            samples,
            elapsed_ms,
            tag: opts.tag.clone(),
            source: "需要登录".to_string(),
            thinking: String::new(),
            title: "请先登录 DeepSeek".to_string(),
            quote: opts.quote.clone(),
        },
        anchor,
        gen,
        token,
        next_turn(opts, pushed_any),
    );
}

/// 交付（R1 阶段：只落诊断日志；R2 起在此接结果卡）。
///
/// ★ 交付前**复检代次**（QA P1-4）：本线程可能在通过循环头检查后、交付前被新回读取代
///   （`begin_watch` 与本次交付之间有 ≥1 个采样间隔的窗口）→ 会被新回读**覆盖新卡片**。
///   此处统一拦住。
///
/// ★ `turn` 由 [`next_turn`] 给出：本推若是本轮的**第一推**则沿用调用方声明的语义
///   （`Fresh` 开新会话 / `Follow` 追加一轮），否则是 `Update`（只填内容，不新增轮次）。
pub(crate) fn deliver(
    app: &AppHandle,
    out: WatchOutcome,
    anchor: Option<AnchorRect>,
    gen: u64,
    token: crate::workflow::Token,
    turn: TurnMode,
) {
    if !crate::workflow::active(token) {
        return;
    }
    if crate::readback::watch::WATCH_GEN.load(std::sync::atomic::Ordering::SeqCst) != gen {
        log(&format!(
            "[{}] 交付时发现已被新的回读取代，丢弃本次结果（{} 字符）",
            out.tag,
            out.text.chars().count()
        ));
        return;
    }
    // ★ 正文脱敏（R17-P1）：回答预览是全项目最私密的现场（含用户划词原文的回显）。三档：
    //   · `Full`（开发机 / 用户显式 `DSFLOAT_DIAG=full`）→ 前 200 字原文，加「」界定；
    //   · `Meta`（release 开启诊断时的默认档）→ **不落正文**，只出长度与哈希；
    //     哈希取自**全文**（而非截断后的 200 字），这样与别处的 `h=` 比对语义一致；
    //   · `Off` → 空串（连一次 `collect` 都不做，顺带去掉原本的无条件构造）。
    let preview = if !enabled() {
        String::new()
    } else if verbose() {
        format!("「{}」", out.text.chars().take(200).collect::<String>())
    } else {
        content(&out.text)
    };
    let panel_visible = app
        .get_webview_window("panel")
        .and_then(|p| p.is_visible().ok());
    if out.complete {
        log(&format!(
            "[{}] **回读完成**（{} 字符 / {} 次采样 / {}ms · panel_visible={:?}）：{}",
            out.tag,
            out.text.chars().count(),
            out.samples,
            out.elapsed_ms,
            panel_visible,
            preview
        ));
    } else {
        warn(&format!(
            "[{}] **超时未判终态**（{}ms / {} 次采样 · 风险 R2）→ 降级交付已读到的部分（{} 字符，须标注「回答可能未完成」· panel_visible={:?}）：{}。若面板内回答其实完整，请先看同段时间的 `inject:` 日志确认**发送**是否成功",
            out.tag,
            out.elapsed_ms,
            out.samples,
            out.text.chars().count(),
            panel_visible,
            preview
        ));
    }
    // ★ R2：**结果卡单点接入**（设计文档 §六：「R2 落地后在 `readback::deliver` 单点接入」）。
    //   代次复检已在上方完成 ⇒ 在途旧回读不会覆盖新卡片（QA P1-4）。
    //   超时降级（complete=false）如实传给卡片，由卡片标注「回答可能未完成」（W-V5 不静默）。
    // ★ R3/R5/R7：交付时并入「发送侧 / 回读判定」置位的告警标志（原子，读即清）。
    let (send_failed, injection_suspected, session_expired) = take_flags();
    // R3：另对本轮正文做**交付时**注入特征检测（与发送侧置位做 OR，任一命中即标记）。
    let injection_suspected = injection_suspected || detect_injection(&out.text);
    crate::workflow::update(
        app,
        token,
        if out.complete {
            crate::workflow::SendStatus::Complete
        } else {
            crate::workflow::SendStatus::TimedOut
        },
    );
    show(
        app,
        &CardView {
            request: Some(token),
            title: out.title.clone(),
            source: out.source.clone(),
            quote: out.quote.clone(),
            text: out.text.clone(),
            thinking: out.thinking.clone(),
            think_label: String::new(),
            streaming: false,
            complete: out.complete,
            send_failed,
            injection_suspected,
            session_expired,
            ..Default::default()
        }
        .with_turn(turn),
        anchor,
    );
}

/// R1 真机诊断入口（托盘菜单）：向 panel 发一条**固定探针**提示词，并回读本轮回答。
///
/// ★ **不主动 `show()` / `set_focus()`**（QA P1-3）：与 W3「不弹面板」一致（§5.7），
///   且只有这样才能用**同一入口**验证 W-V13 / R11 ——「面板隐藏时能否照常流式生成并读回」。
///   注：面板窗口在 setup 内常驻创建、webview 即便隐藏也已加载 DeepSeek，故隐藏态下注入链路可用；
///   首选注入路径（页内 `fill()` + 页内 `submit()`）**不依赖窗口焦点**（见 §5.7 的取舍说明）。
///
/// 覆盖设计文档 §9.2 的三项：
///   W-V3 回读准确性 —— 探针提示词含可辨识串，回读结果应与面板回答一致（且回显被剥离）；
///   W-V4 终态判定 —— 回答流式增长期不得判终态；
///   W-V5 超时降级 —— 在**非会话页**（如未登录首页）触发时无容器 → 必然走超时降级分支。
///
/// 同 `inject::inspect_upload`：唯一调用方是 debug-only 的 `lib.rs::setup_selftest`，
/// 故用相同 cfg 排除，避免 release 构建报 dead_code（**不用 `allow(dead_code)` 掩盖**）。
#[cfg(debug_assertions)]
pub fn begin_watch_diagnostic(app: &AppHandle) {
    let Some(panel) = app.get_webview_window("panel") else {
        warn("[diagnostic] panel 窗口不存在（应用尚未创建面板，见 poc2-state.log）");
        return;
    };
    let visible_before = panel.is_visible().unwrap_or(false);
    // ★★ 探针**每次唯一**（R1 真机缺陷之三，2026-09-14）—— 这是本批最重要的**判据**修复。
    //
    //   原用**常量**探针：「请只回复这四个字：回读测试」。后果：
    //     · 「上一轮的同款提示词回显」和「上一轮回答里引用了提示词原文」都会命中「含提示词的块」；
    //     · 而 W-V3 的判据是「回读 == 面板回答」—— **两轮答案相同时无法察觉读的是上一轮**
    //       （**自证失效**）。这正是 QA P0-1 指出、却在诊断入口被常量探针掩盖的那个洞。
    //   给提示词嵌一个每次不同的编号后，`block_has` 只可能命中**本轮**回显 → 判据重新自证成立；
    //   同时仍要求「只回四个字」，故期望回答不变（「回读测试」），人工核对方式完全不变。
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let probe = format!("诊断编号 {nonce}：请只回复这四个字：回读测试");
    log(&format!(
        "[diagnostic] 面板当前可见={visible_before}（**不主动 show / set_focus**，与 W3 一致；隐藏态亦可验证 W-V13/R11）"
    ));
    log(&format!(
        "[diagnostic] 本轮探针（唯一化，杜绝「读到上一轮」）：{probe}"
    ));
    inject_and_send_async(panel, probe.clone());
    begin_watch(
        app,
        probe,
        // 诊断入口没有真实选区锚点 → 卡片退回「当前光标位置」兜底（足以验证 W-V6/W-V7/W-V8）。
        None,
        WatchOpts {
            tag: "diagnostic".to_string(),
            source: "回读诊断".to_string(),
            title: "回读诊断".to_string(),
            ..Default::default()
        },
    );
}
