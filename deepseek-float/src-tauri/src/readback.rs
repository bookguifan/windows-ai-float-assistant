//! §5.1：回答回读桥（R1）。
//!
//! 职责：我方把提示词注入 panel 并发送后，**从 panel 页读回「本轮回答」**，并在回答写完（终态）
//! 后交给结果卡展示（R2）。全过程在 Rust 侧编排；页内只新增一个**纯观测**函数
//! `__DSFLOAT__.snapshotConversation()`（见 `dsfloat_init.js`），不发起任何主动行为（硬约束 G5）。
//!
//! 通道：沿用 `inject.rs` 的结论 —— 远端页注入脚本**不能**回调 Tauri（ACL 拒绝），
//! 但 **Rust 侧 `eval_with_callback` 不受 ACL 限制**，所以回读只能由 Rust 主动轮询。
//!
//! 子模块拆分（职责单一，便于维护）：
//! - `analyze`：纯文本 / DOM 启发式（定位「本轮回答」、判装饰/占位/回显/登录）；
//! - `watch`：轮询引擎（代次/退出标志、告警原子位、终态判定、轮询主循环、单次采样）；
//! - `deliver`：单点交付、登录/无进展早退提示、真机诊断入口；
//! - `snapshot`：页内快照解析（`Snapshot` / `parse_snapshot`）。
//!
//! 对外公开项经本根模块再导出，故既有 `crate::readback::*` 调用点无需改动。

mod analyze;
mod deliver;
mod snapshot;
mod watch;
// 对外再导出**确有 `crate::readback::*` 调用点**的公开 API（resume / begin_watch / WatchOpts /
// 各 mark_* / begin_watch_diagnostic），使其调用点无需改动。纯文本判据、快照解析、终态判定等
// 经各自子模块路径（`crate::readback::analyze::*` / `snapshot::*` / `watch::*`）复用与单测，
// 不在此再导出，避免 cdylib 下的 unused-import 告警（clippy -D warnings）。
#[cfg(debug_assertions)]
pub use deliver::begin_watch_diagnostic;
pub use watch::{
    begin_watch, cancel, mark_exiting, remember_send, resume, retry_prompt, retry_send, WatchOpts,
};
// 仅 crate 内部使用：被 `watch` 的无进展早退分支引用。
pub(crate) use deliver::NO_LOCATE_NOTICE_TEXT;

#[cfg(test)]
mod tests {
    use crate::readback::analyze::*;
    use crate::readback::snapshot::{parse_snapshot, Snapshot};
    use crate::readback::watch::*;
    use std::sync::atomic::Ordering;

    /// 单测里绝大多数用例只关心「回答」这一路 —— 用同名 helper 屏蔽掉思考，
    /// 使全部既有断言**逐字保留**（它们本就是在验证定位口径）。
    fn locate_answer(snap: &Snapshot, prompt: &str) -> String {
        locate_answer_and_thinking(snap, prompt).0
    }

    fn snap(texts: &[&str]) -> Snapshot {
        Snapshot {
            request_bound: false,
            url: "https://chat.deepseek.com/a/chat/s/x".to_string(),
            texts: texts.iter().map(|s| s.to_string()).collect(),
            blocks: texts.len(),
            total: texts.iter().map(|s| s.len()).sum(),
            hops: 0,
            streaming: false,
            pending: false,
            done: false,
            reasoning: false,
            container: String::new(),
            dump: String::new(),
            split_v: 0,
            thinking: Vec::new(),
            roles: Vec::new(),
        }
    }

    /// ★ #004：带「块角色」的快照 —— `roles` 与 `texts` 逐项对齐（`assistant` / `user` / `unknown`）。
    fn snap_roles(texts: &[&str], roles: &[&str]) -> Snapshot {
        let mut s = snap(texts);
        s.roles = roles.iter().map(|r| r.to_string()).collect();
        s
    }

    // ---- fmt_blocks ----

    #[test]
    fn fmt_blocks_empty_and_indexed() {
        assert_eq!(fmt_blocks(&[]), "(空)");
        let s = fmt_blocks(&["a".to_string(), "回读测试".to_string()]);
        assert!(s.contains("#0(1)「a」"));
        assert!(s.contains("#1(4)「回读测试」"));
    }

    #[test]
    fn fmt_blocks_truncates_long_block() {
        let long = "甲".repeat(200);
        let s = fmt_blocks(&[long]);
        // 长度标注仍为真实长度（200），但正文只留 60 字符。
        assert!(s.contains("#0(200)"));
        assert_eq!(s.matches('甲').count(), 60);
    }

    // ---- strip_own_prompt ----

    #[test]
    fn rich_answer_preserves_code_whitespace_through_location() {
        let html = "<div><pre><code class=\"language-python\">def answer():\n    return 17 + 25\n\nprint(answer())</code></pre><p>RICH-END</p></div>";
        assert_eq!(strip_own_prompt(html, "输出代码"), html);
        assert_eq!(strip_own_prompt(html, ""), html);
        let snapshot = snap_roles(&["<p>输出代码</p>", html], &["user", "assistant"]);
        assert_eq!(locate_answer(&snapshot, "输出代码"), html);
    }

    #[test]
    fn strip_exact_echo_yields_empty() {
        assert_eq!(strip_own_prompt("请翻译：你好", "请翻译：你好"), "");
    }

    #[test]
    fn strip_prefix_echo_keeps_answer() {
        // 回显与回答同处一块：剥掉前导提示词后剩下的才是回答。
        let t = "请翻译：你好 Hello, world!";
        assert_eq!(strip_own_prompt(t, "请翻译：你好"), "Hello, world!");
    }

    #[test]
    fn strip_tolerates_whitespace_normalization() {
        // 提示词原文含换行/多空格，页内文本已折叠空白 —— 两种形态都要能剥（设计文档 §9.1）。
        let prompt = "指令A\n\n正文B";
        assert_eq!(strip_own_prompt("指令A 正文B 回答C", prompt), "回答C");
    }

    #[test]
    fn strip_keeps_middle_occurrence() {
        // 提示词出现在**中间**（模型引用了原文）时**不得**剥离，否则会把回答削掉一截。
        let t = "回答开头 请翻译：你好 回答结尾";
        assert_eq!(strip_own_prompt(t, "请翻译：你好"), t);
    }

    #[test]
    fn strip_empty_prompt_is_noop() {
        assert_eq!(strip_own_prompt("  任意 文本 ", ""), "任意 文本");
    }

    #[test]
    fn strip_empty_text_yields_empty() {
        assert_eq!(strip_own_prompt("", "任意提示词"), "");
    }

    // ---- locate_answer ----

    #[test]
    fn locate_answer_prefers_block_after_echo() {
        // 尾窗：… 上一轮回答 · 回显块 · 本轮回答
        let s = snap(&["上一轮的回答内容", "请翻译：你好", "Hello, world!"]);
        assert_eq!(locate_answer(&s, "请翻译：你好"), "Hello, world!");
    }

    /// ★ QA P0-1 回归：上一轮**同款提示词**的回显也在尾窗里 → 必须取**最后一处**命中的后一块。
    #[test]
    fn locate_answer_uses_last_echo_not_first() {
        let s = snap(&["请翻译：你好", "上一轮译文", "请翻译：你好", "本轮译文"]);
        assert_eq!(locate_answer(&s, "请翻译：你好"), "本轮译文");
    }

    /// ★ QA P0-1 回归：上一轮回答里**引用了提示词原文** → 也不能被它抢先命中。
    #[test]
    fn locate_answer_skips_prompt_quoted_in_previous_answer() {
        let s = snap(&["旧回答：请翻译：你好", "请翻译：你好", "本轮译文"]);
        assert_eq!(locate_answer(&s, "请翻译：你好"), "本轮译文");
    }

    #[test]
    fn locate_answer_waits_when_echo_is_last() {
        // 回答尚未开始：回显是末块 → 结果为空（由 decide_done 继续等待）。
        let s = snap(&["上一轮的回答内容", "请翻译：你好"]);
        assert_eq!(locate_answer(&s, "请翻译：你好"), "");
    }

    #[test]
    fn locate_answer_falls_back_to_last_block() {
        // 提示词为空（退化模式）→ 直接取末块。
        let s = snap(&["A", "B", "最新一条回答"]);
        assert_eq!(locate_answer(&s, ""), "最新一条回答");
    }

    #[test]
    fn locate_answer_strips_combined_block() {
        // 无独立回显块、回显与回答同块 → 兜底取末块并剥前导提示词。
        let s = snap(&["旧回答", "请翻译：你好 Hello"]);
        assert_eq!(locate_answer(&s, "请翻译：你好"), "Hello");
    }

    #[test]
    fn locate_answer_empty_snapshot() {
        assert_eq!(locate_answer(&snap(&[]), "任意提示词"), "");
    }

    /// ★ 思考过程必须与回答**同块取回**（用户要求「把思考过程也回转回来」）。
    #[test]
    fn locate_answer_returns_thinking_of_the_same_block() {
        let mut s = snap(&["上一轮", "请翻译：你好", "Hello"]);
        // 页内保证 thinking 与 texts 逐项对齐：本轮回答块（下标 2）带一段思考。
        s.thinking = vec![String::new(), String::new(), "让我想想".to_string()];
        let (ans, think) = locate_answer_and_thinking(&s, "请翻译：你好");
        assert_eq!(ans, "Hello");
        assert_eq!(think, "让我想想");
    }

    /// 页内未回传 thinking（旧版页面 / 字段缺失）时兜底为空串，不得 panic。
    #[test]
    fn locate_answer_tolerates_missing_thinking() {
        let s = snap(&["请翻译：你好", "Hello"]);
        let (ans, think) = locate_answer_and_thinking(&s, "请翻译：你好");
        assert_eq!(ans, "Hello");
        assert_eq!(think, "");
    }

    // ---- is_decoration（装饰文案否决，2026-09-15 真机缺陷）----

    #[test]
    fn decoration_footer_is_rejected() {
        // 真机页脚：「内容由 AI 生成，请仔细甄别。」（含全角标点与空格两种形态）
        assert!(is_decoration("内容由 AI 生成，请仔细甄别。"));
        assert!(is_decoration("内容由AI生成，请仔细甄别"));
        assert!(is_decoration(
            "深度思考智能搜索\n内容由 AI 生成，请仔细甄别"
        ));
    }

    #[test]
    fn decoration_chatgpt_footer_is_rejected() {
        // ★ 2026-10-03 六次真机（方案 B）：ChatGPT 的免责页脚
        //   「ChatGPT 也可能会犯错。请核查重要信息。」（真机 `#61` 曾当回答交付，188 字符）。
        assert!(is_decoration("ChatGPT 也可能会犯错。请核查重要信息。"));
        assert!(is_decoration("ChatGPT也可能会犯错。请核查重要信息。"));
        assert!(is_decoration(
            "<div><div><div>ChatGPT 也可能会犯错。请核查重要信息。</div></div></div>"
        ));
        // 只含后一条文案也应命中（拆成两条特征串）。
        assert!(is_decoration("请核查重要信息"));
        assert!(is_decoration("内容仅供参考"));
    }

    #[test]
    fn decoration_chatgpt_footer_does_not_reject_real_answers() {
        // 真回答里出现「请核查重要信息」但整体超长 → 不算装饰（长度超过 DECOR_MAX_CHARS）。
        let long = format!("{}请核查重要信息", "答".repeat(80));
        assert!(!is_decoration(&long));
        assert!(!is_decoration("ChatGPT 的回答内容在这里"));
    }

    #[test]
    fn decoration_check_ignores_real_answers() {
        assert!(!is_decoration("这是模型给的回答"));
        assert!(!is_decoration(""));
        // 长回答里引用了这句话 → 不算装饰（长度超过 DECOR_MAX_CHARS）。
        let long = format!("{}内容由 AI 生成，请仔细甄别", "答".repeat(80));
        assert!(!is_decoration(&long));
    }

    #[test]
    fn locate_answer_rejects_decoration_block_after_echo() {
        // 尾窗 = [回显, 免责页脚]（回答尚未开始）→ 不得把页脚当回答交付。
        let s = snap(&["请翻译：你好", "内容由 AI 生成，请仔细甄别"]);
        let (ans, think) = locate_answer_and_thinking(&s, "请翻译：你好");
        assert_eq!(ans, "");
        assert_eq!(think, "");
    }

    #[test]
    fn locate_answer_rejects_decoration_as_last_block() {
        // 兜底路径（末块即页脚）同样否决。
        let s = snap(&["旧回答", "内容由 AI 生成，请仔细甄别"]);
        assert_eq!(locate_answer(&s, ""), "");
    }

    #[test]
    fn locate_answer_rejects_chatgpt_footer_after_echo() {
        // ★ 六次真机 `#60`：交付前尾窗 = [回显, ChatGPT 免责页脚] → 不得把页脚当回答交付。
        let s = snap(&["请翻译：你好", "ChatGPT 也可能会犯错。请核查重要信息。"]);
        let (ans, think) = locate_answer_and_thinking(&s, "请翻译：你好");
        assert_eq!(ans, "");
        assert_eq!(think, "");
    }

    #[test]
    fn locate_answer_returns_answer_before_chatgpt_footer() {
        // ★ A14 修复后的目标形态：尾窗 = [回显, 回答, 页脚] → 取「回显的后一块」= 回答。
        let s = snap(&[
            "请翻译：你好",
            "你好，请问有什么可以帮您的吗？",
            "ChatGPT 也可能会犯错。请核查重要信息。",
        ]);
        let (ans, _) = locate_answer_and_thinking(&s, "请翻译：你好");
        assert_eq!(ans, "你好，请问有什么可以帮您的吗？");
    }

    // ---- is_markup_only（R10：净化 HTML 空壳否决，2026-09-15 真机缺陷）----

    #[test]
    fn markup_only_rejects_empty_html_shell() {
        // 真机证据：净化 HTML 下空块回传 `<div></div>`（11 字符标记），骗过最短长度闸门。
        assert!(is_markup_only("<div></div>"));
        assert!(is_markup_only("<div><div></div></div>"));
        assert!(is_markup_only("<p>   </p>"));
        assert!(is_markup_only(""));
        assert!(is_markup_only("   \n  "));
    }

    #[test]
    fn markup_only_keeps_real_content() {
        assert!(!is_markup_only("<p>你好</p>"));
        assert!(!is_markup_only("纯文本回答"));
        // 图片无文本但可见 → 不算空壳。
        assert!(!is_markup_only(
            r#"<p><img src="https://x/y.png" alt="图"></p>"#
        ));
    }

    // ---- is_status_text（R12：页面瞬时状态占位否决，2026-09-15 截图识字真机缺陷）----

    #[test]
    fn status_text_rejects_analyzing_placeholder() {
        // 真机原文（净化 HTML，97 字符）：DeepSeek 收到图片后先渲染的瞬时状态。
        let placeholder = "<div><div><div><span>正在分析图片<span><span></span>\
                           <span>正在分析图片</span></span></span></div></div></div>";
        assert!(is_status_text(placeholder));
        assert!(is_status_text("<div><span>正在思考</span></div>"));
        assert!(is_status_text("正在识别图片中的文字"));
    }

    #[test]
    fn status_text_keeps_real_answers() {
        assert!(!is_status_text(""));
        assert!(!is_status_text("这是模型给的回答"));
        // 长正文里出现「正在思考」不算占位（超过 STATUS_MAX_CHARS）。
        let long = format!("{}正在思考", "答".repeat(80));
        assert!(!is_status_text(&long));
    }

    #[test]
    fn locate_answer_rejects_status_placeholder_after_echo() {
        // 尾窗 = [回显, 「正在分析图片」占位块] → 不得交付（否则结果卡显示状态文案而非转录）。
        let s = snap(&[
            "请把这张图片里的文字完整转录出来",
            "<div><span>正在分析图片</span></div>",
        ]);
        let (ans, think) = locate_answer_and_thinking(&s, "请把这张图片里的文字完整转录出来");
        assert_eq!(ans, "");
        assert_eq!(think, "");
    }

    #[test]
    fn locate_answer_rejects_empty_html_block_after_echo() {
        // 尾窗 = [回显, "<div></div>"（回答尚未开始生成的空占位块）] → 不得交付（卡片空白）。
        let s = snap(&["请翻译：你好", "<div></div>"]);
        let (ans, think) = locate_answer_and_thinking(&s, "请翻译：你好");
        assert_eq!(ans, "");
        assert_eq!(think, "");
    }

    // ---- is_echo_only（R14：把「我方提示词回显」当回答交付，2026-09-15 截图识字真机缺陷）----

    #[test]
    fn echo_only_detects_user_bubble_with_markup_prefix() {
        // ★ 真机原文形态：一串空 `<div>` 占位 + 我方提示词。块首是**标记** ⇒
        //   `strip_own_prompt` 的 `strip_prefix` 剥不掉 ⇒ 整块（真机 513 字符）曾被当回答交付。
        let prompt = "请把这张图片里的文字完整、准确地转录出来，保持原有的段落与换行；\
                      不要翻译、不要解释、不要总结，也不要添加原文里没有的内容。";
        let raw = "<div><div></div><div><div></div></div><div><div><p><span>\
                   请把这张图片里的文字完整、准确地转录出来，保持原有的段落与换行；\
                   不要翻译、不要解释、不要总结，也不要添加原文里没有的内容。\
                   </span></p></div></div>";
        assert!(is_echo_only(raw, prompt));
        // 同时证明它**确实**骗得过 strip_own_prompt（剥离后仍是原样）—— 这是必须提前否决的原因。
        assert!(!strip_own_prompt(raw, prompt).is_empty());
    }

    #[test]
    fn echo_only_keeps_real_answers() {
        let prompt = "请把这张图片里的文字完整转录出来";
        assert!(!is_echo_only("", prompt));
        // 真转录结果（与提示词毫无关系）→ 不得误判。
        assert!(!is_echo_only("<p>今朝有酒今朝醉</p>", prompt));
        // 长正文里引用提示词 → 超出 ECHO_SLACK_CHARS ⇒ 不判回显。
        let long = format!("{prompt}——这是模型在长回答里引用原文的一个例子，后面还有很多内容。");
        assert!(!is_echo_only(&long, prompt));
        // 空提示词（诊断退化路径）→ 恒不否决。
        assert!(!is_echo_only("任意内容", ""));
    }

    #[test]
    fn echo_only_allows_short_ui_chrome() {
        // 允许少量 UI 文案（「复制 / 编辑」按钮、「内容由 AI 生成，请仔细甄别」页脚）。
        assert!(is_echo_only(
            "<div>复制编辑<p>请翻译：你好</p></div>",
            "请翻译：你好"
        ));
        assert!(is_echo_only(
            "<div><p>请翻译：你好</p><span>内容由 AI 生成，请仔细甄别</span></div>",
            "请翻译：你好"
        ));
    }

    #[test]
    fn locate_answer_rejects_echo_when_answer_not_started() {
        // ★ R14 真机场景：尾窗 = [图片块, 用户消息气泡（回显）]，回答尚未开始
        //   ⇒ 旧实现走兜底路径把**回显**交付（513 字符），结果卡显示用户自己的指令。
        let prompt = "请把这张图片里的文字完整、准确地转录出来";
        let echo =
            "<div><div></div><p><span>请把这张图片里的文字完整、准确地转录出来</span></p></div>";
        let s = snap(&["<div><img src=\"https://x/y.png\"></div>", echo]);
        let (ans, think) = locate_answer_and_thinking(&s, prompt);
        assert_eq!(ans, "");
        assert_eq!(think, "");
    }

    #[test]
    fn locate_answer_still_returns_answer_after_echo_bubble() {
        // 回显之后的真回答必须照常取回（R14 修复不得把正常路径一起拦掉）。
        let prompt = "请翻译：你好";
        let bubble = format!("<div>{prompt}</div>");
        let s = snap(&[bubble.as_str(), "<p>Hello</p>"]);
        // 载荷是**净化 HTML**（标签随行透传，由卡片渲染），故此处保留 `<p>`。
        assert_eq!(locate_answer(&s, prompt), "<p>Hello</p>");
    }

    #[test]
    fn echo_only_does_not_reject_combined_echo_plus_answer() {
        // ★ 分水岭用例：提示词之后**还有正文** ⇒ 不得判成回显（否则会把真回答一起拦掉）。
        //   与既有语义 `locate_answer_strips_combined_block` 一致。
        assert!(!is_echo_only(
            "<div>请翻译：你好Hello</div>",
            "请翻译：你好"
        ));
    }

    #[test]
    fn echo_only_normalizes_whitespace_entities() {
        // ★ #004 兜底加固：块侧空白被序列化成**实体**（`&nbsp;` / 数字实体）时，仍应与提示词（真实空格）对齐。
        assert!(is_echo_only("<p>请&nbsp;翻译：你好</p>", "请 翻译：你好"));
        assert!(is_echo_only("<p>请&#160;翻译：你好</p>", "请 翻译：你好"));
        assert!(is_echo_only("<p>请&#xa0;翻译：你好</p>", "请 翻译：你好"));
        // 反例：非回显内容仍不得误判。
        assert!(!is_echo_only("<p>今朝有酒今朝醉</p>", "请 翻译：你好"));
    }

    // ---- #004 结构化「块角色」判据（回显交付修复；真机 2026-09-26 第 4 轮）----

    #[test]
    fn locate_answer_rejects_user_role_last_block_round4() {
        // ★ 核心回归（真机第 4 轮形态）：尾窗只剩 1 块（#0 = 我方回显），roles = ["user"]。
        //   旧实现仅靠 `is_echo_only` → 因回显「多带包裹」而漏判 → 把回显当回答交付
        //   （真机交付 4315 字符 = 解释预设 + 划选原文）。
        let s = snap_roles(
            &["解释下面的内容：这是用户划选的一段长文本内容……"],
            &["user"],
        );
        let (ans, think) = locate_answer_and_thinking(&s, "这是用户划选的一段长文本内容……");
        assert_eq!(ans, "");
        assert_eq!(think, "");
    }

    #[test]
    fn role_judge_catches_echo_that_text_judge_misses() {
        // ★ 关键：块文本**不是**纯回显（前置 UI 文案超长 ⇒ 文本判据 `is_echo_only` 确实会漏），
        //   但页内已**正向识别为 user** ⇒ 仍必须拒绝。这证明结构判据补上了文本判据的缺口。
        let echo = "<div><div class=\"avatar\">头像占位文字很长很长很长很长很长</div>\
                    解释下面的内容：这是划选原文</div>";
        assert!(
            !is_echo_only(echo, "这是划选原文"),
            "前置 UI 文案超出 ECHO_SLACK_CHARS ⇒ 文本判据确实会漏"
        );
        let s = snap_roles(&[echo], &["user"]);
        let (ans, _) = locate_answer_and_thinking(&s, "这是划选原文");
        assert_eq!(ans, "");
    }

    #[test]
    fn locate_answer_rejects_user_role_after_echo() {
        // 「回显块**之后一块**」被正向识别为 user ⇒ 走 `texts[i+1]` 分支拒绝。
        let s = snap_roles(
            &["请翻译：你好", "<p>嗯，这是一段与提示词无关的文本</p>"],
            &["user", "user"],
        );
        let (ans, _) = locate_answer_and_thinking(&s, "请翻译：你好");
        assert_eq!(ans, "");
    }

    #[test]
    fn locate_answer_unknown_role_still_delivers() {
        // ★ 防退化：roles 全为 unknown（站点改名 ds-markdown 等）⇒ 照常交付，绝不「永不交付 / 恒超时」。
        let s = snap_roles(&["请翻译：你好", "Hello"], &["unknown", "unknown"]);
        assert_eq!(locate_answer(&s, "请翻译：你好"), "Hello");
    }

    #[test]
    fn locate_answer_assistant_role_delivers() {
        // roles 含 assistant ⇒ 正常交付答案（角色判据只否决 user，不误伤 assistant）。
        let s = snap_roles(&["请翻译：你好", "Hello"], &["user", "assistant"]);
        assert_eq!(locate_answer(&s, "请翻译：你好"), "Hello");
    }

    #[test]
    fn locate_answer_empty_roles_matches_legacy_behavior() {
        // ★ roles 为空（旧脚本 / 站点未回传）⇒ 行为与加固前**逐字一致**（退回 is_echo_only）。
        // 形态 A：回显是末块 → 仍否决。
        let s1 = snap_roles(&["上一轮的回答内容", "请翻译：你好"], &[]);
        assert_eq!(locate_answer(&s1, "请翻译：你好"), "");
        // 形态 B：回显后是真回答 → 仍交付。
        let s2 = snap_roles(&["请翻译：你好", "Hello"], &[]);
        assert_eq!(locate_answer(&s2, "请翻译：你好"), "Hello");
    }

    // ---- url_needs_login（R15-C1：未登录不再静默等 60s）----

    #[test]
    fn login_url_detection_matches_panel_state() {
        // 与 `lib.rs::panel_state` 同口径（含 `/sign_in` 即未登录）。
        assert!(url_needs_login("https://chat.deepseek.com/sign_in"));
        assert!(url_needs_login(
            "https://chat.deepseek.com/sign_in?redirect=%2Fa%2Fchat%2Fs%2Fx"
        ));
        // 已登录的会话页 / 首页 / 懒跳前的 about:blank → 都**不得**误判成未登录
        // （误判会把一次正常提问直接掐掉，并给出错误的行动建议）。
        assert!(!url_needs_login(
            "https://chat.deepseek.com/a/chat/s/a91f67c6-23e9-4771-9e42-269c2f2fb807"
        ));
        assert!(!url_needs_login("https://chat.deepseek.com/"));
        assert!(!url_needs_login("about:blank"));
        assert!(!url_needs_login(""));
    }

    // ---- R3：注入特征检测（极保守，正文始终照常净化展示，仅额外提示）----

    #[test]
    fn injection_detects_executable_script_forms() {
        assert!(detect_injection("<script>alert(1)</script>"));
        assert!(detect_injection("点击 javascript:alert(1)"));
        assert!(detect_injection("<img src=x onerror=alert(1)>"));
        assert!(detect_injection("<img src=x onload=alert(1)>"));
        // 大小写变体不能绕过。
        assert!(detect_injection("<SCRIPT>alert(1)</SCRIPT>"));
        assert!(detect_injection("JaVaScRiPt:alert(1)"));
    }

    #[test]
    fn injection_does_not_flag_normal_text() {
        // 普通回答（谈代码时提到 script、以及不含可执行签名的正文）绝不误报。
        assert!(!detect_injection(
            "这段代码里的 script 标签作用是把 CSS 内联。"
        ));
        assert!(!detect_injection("请解释 javascript 的闭包。"));
        assert!(!detect_injection(""));
        assert!(!detect_injection("normal answer without flags"));
    }

    // ---- R7：发送复核失败标志（原子置位/读即清）----

    #[test]
    fn send_failed_flag_set_and_cleared_by_take() {
        assert!(!SEND_CONFIRM_FAILED.load(Ordering::SeqCst));
        mark_send_confirmed_failed();
        assert!(SEND_CONFIRM_FAILED.load(Ordering::SeqCst));
        // take_flags 读出即清 —— 避免下一次交付把上一次的失败带进新卡片。
        let (send, _inj, _sess) = take_flags();
        assert!(send);
        assert!(!SEND_CONFIRM_FAILED.load(Ordering::SeqCst));
        let (send2, _inj2, _sess2) = take_flags();
        assert!(!send2);
    }

    // ---- R11：回显判据必须**去标签**比较（净化 HTML 载荷，2026-09-15 真机缺陷）----

    #[test]
    fn compact_html_strips_tags_and_whitespace() {
        assert_eq!(compact_html("<p>a</p><p>b</p>"), "ab");
        assert_eq!(compact_html("a b\tc"), "abc");
        assert_eq!(compact_html("纯文本"), "纯文本");
        assert_eq!(compact_html("<div><span>你好</span></div>"), "你好");
    }

    #[test]
    fn echo_detection_ignores_html_tags_between_nodes() {
        // 真机形态：提示词被渲染成**多个节点** → 标签插在提示词中间。
        let p = "请解释下面的内容：这是一段很长的选中文本";
        let block = "<div><p><span>请解释下面的内容：</span></p>\
                     <p><span>这是一段很长的选中文本</span></p></div>";
        let p_norm = crate::capture::normalize_text(p);
        // 裸 `normalize_text` 比较会失败（这正是 echo=0 的成因）……
        assert!(!crate::capture::normalize_text(block).contains(&p_norm));
        // ……去标签比较则成立。
        assert!(block_has(block, &p_norm));
        let s = snap(&[block]);
        assert!(last_is_echo(&s, &p_norm));
        assert_eq!(echo_count(&s, &p_norm), 1);
    }

    #[test]
    fn echo_detection_still_finds_single_node_echo() {
        // 单节点形态（短选 / 单段）在两者下都应成立 —— 修复不得引入回归。
        let p = "请翻译：你好";
        let block = "<div><span>请翻译：你好</span></div>";
        let p_norm = crate::capture::normalize_text(p);
        assert!(block_has(block, &p_norm));
        assert!(crate::capture::normalize_text(block).contains(&p_norm));
    }

    #[test]
    fn echo_detection_rejects_unrelated_block() {
        let p_norm = crate::capture::normalize_text("请翻译：你好");
        assert!(!block_has("<div><span>侧栏历史标题</span></div>", &p_norm));
        assert!(!block_has("", &p_norm));
        assert!(!block_has("<p>任意</p>", ""));
    }

    // ---- echo_count / last_is_echo（启动信号） ----

    #[test]
    fn echo_signals_detect_new_echo() {
        let p = "请只回复这四个字：回读测试";
        // 上一轮已答完（旧回声 + 旧回答）。
        let before = snap(&[p, "上一轮回答"]);
        // 本轮提问刚**落盘**、回答尚未开始：新增一个回声块，且它正好是末块。
        let after_echo = snap(&[p, "上一轮回答", p]);
        // 回答开始流式：末块变成回答（回声块仍在，故计数不变）。
        let streaming = snap(&[p, "上一轮回答", p, "回读测试"]);

        assert_eq!(echo_count(&before, p), 1);
        assert_eq!(echo_count(&after_echo, p), 2);
        // 正向证据 ①：含提示词的块数**增加** ⇒ 新回声落盘。
        assert!(echo_count(&after_echo, p) > echo_count(&before, p));
        // 正向证据 ②：**末块即回声块** ⇔ 我方提问刚落盘、回答未开始。
        assert!(!last_is_echo(&before, p));
        assert!(last_is_echo(&after_echo, p));
        assert!(!last_is_echo(&streaming, p));
        assert_eq!(echo_count(&streaming, p), 2);
    }

    // ---- decide_done ----

    #[test]
    fn decide_done_false_on_first_sample() {
        assert!(!decide_done(None, "回答", 9, 99_999, false, true, 1200, 1));
    }

    #[test]
    fn decide_done_true_when_stable_and_quiet() {
        // 用常量（`MIN_STABLE_SAMPLES` / `QUIET_MS`）而非写死，避免与判定红线脱钩。
        assert!(decide_done(
            Some("回答内容"),
            "回答内容",
            MIN_STABLE_SAMPLES,
            QUIET_MS,
            false,
            true,
            QUIET_MS,
            1
        ));
    }

    /// ★★ 2026-09-20 判终重构回归：即便静默 + 稳定全满足，只要「已写完」正向信号 `done=false`
    ///   就**不得**判终（否则 DeepSeek 落尾段前的瞬间停顿会把最后一段静默丢掉）。
    #[test]
    fn decide_done_false_when_done_signal_missing() {
        assert!(!decide_done(
            Some("回答内容"),
            "回答内容",
            9,
            99_999,
            false,
            false,
            1200,
            1
        ));
    }

    #[test]
    fn decide_done_false_when_quiet_not_reached() {
        assert!(!decide_done(
            Some("回答内容"),
            "回答内容",
            9,
            1199,
            false,
            true,
            1200,
            1
        ));
    }

    /// ★ QA P1-1 回归：慢采样下「静默够久但只稳定 1 次」不得判终态。
    #[test]
    fn decide_done_false_when_stable_samples_insufficient() {
        assert!(!decide_done(
            Some("回答内容"),
            "回答内容",
            1,
            1500,
            false,
            true,
            1200,
            1
        ));
        assert!(!decide_done(
            Some("回答内容"),
            "回答内容",
            2,
            1500,
            false,
            true,
            1200,
            1
        ));
    }

    /// ★ QA P1-1 回归：页内报告仍在生成时，一律否决终态（结构事实优先于静默启发式）。
    #[test]
    fn decide_done_vetoed_while_streaming() {
        assert!(!decide_done(
            Some("回答内容"),
            "回答内容",
            9,
            99_999,
            true,
            true,
            1200,
            1
        ));
    }

    /// 2026-09-15：末块仍只有思考过程（`pending`）时同样否决 —— 「思考完毕 → 正文开始」
    /// 之间的停顿真机可超过静默阈值，且此时页面上未必有可识别的「停止生成」控件。
    #[test]
    fn decide_done_vetoed_while_only_thinking() {
        assert!(!decide_done(
            Some("已思考（用时 1 秒）我们需要回答用户"),
            "已思考（用时 1 秒）我们需要回答用户",
            9,
            99_999,
            true,
            true,
            1200,
            1
        ));
    }

    #[test]
    fn decide_done_false_while_growing() {
        // 流式增长中：上一采样与当前不同 → 不得判终态（W-V4）。
        assert!(!decide_done(
            Some("回答"),
            "回答内容",
            9,
            5_000,
            false,
            true,
            1200,
            1
        ));
    }

    #[test]
    fn decide_done_false_when_too_short() {
        assert!(!decide_done(Some(""), "", 9, 5_000, false, true, 1200, 1));
    }

    // ---- parse_snapshot ----

    #[test]
    fn parse_snapshot_ok() {
        let raw = r#"{"url":"https://chat.deepseek.com/a/chat/s/x","ok":true,"blocks":3,"childCount":4,"total":123,"hops":1,"streaming":true,"done":true,"reasoning":false,"container":"cls-x pass=1 hops=0","texts":["a","b","c"]}"#;
        let s = parse_snapshot(raw).expect("应能解析");
        assert_eq!(s.texts, vec!["a", "b", "c"]);
        assert_eq!(s.blocks, 3);
        assert_eq!(s.total, 123);
        assert_eq!(s.hops, 1);
        assert!(s.streaming);
        assert!(s.done);
        assert!(!s.reasoning);
        assert!(s.url.contains("chat.deepseek.com"));
        assert!(s.container.contains("pass=1"));
    }

    /// ★★ 2026-09-20：`done` 缺失 → 解析默认 `false`（旧脚本 / 字段缺失时安全默认「未完成」）。
    #[test]
    fn parse_snapshot_done_defaults_false() {
        let raw = r#"{"url":"x","ok":true,"texts":["a"]}"#;
        let s = parse_snapshot(raw).expect("应能解析");
        assert!(!s.done);
    }

    /// ★★ 2026-09-20（A+B 判终修复回归）：思考占位块 → 页内把 `done` 压成 `false`、`pending` 拉成 `true`
    /// 并自报 `splitV=4`；Rust 侧必须原样解析，使 `decide_done` 能被 `veto`/`!done` 拦截、继续等真答案，
    /// 绝不把「长思考占位正文」当回答交付（真机 `#236/#238`）。
    #[test]
    fn parse_snapshot_thinking_placeholder_vetoes_done() {
        let raw = r#"{
            "url":"x","ok":true,"splitV":4,
            "texts":["我们"],"thinking":["正在思考正在思考我们需要回答用户"],
            "pending":true,"done":false
        }"#;
        let s = parse_snapshot(raw).expect("应能解析");
        assert!(s.pending);
        assert!(!s.done);
        assert_eq!(s.split_v, 4);
        // pending 必须能否决 decide_done，无论静默/稳定/`streaming` 如何。
        assert!(!decide_done(
            Some("我们"),
            "我们",
            MIN_STABLE_SAMPLES,
            QUIET_MS,
            true, // veto = streaming(潜在) | pending
            true,
            QUIET_MS,
            1
        ));
    }

    /// ★★ 2026-09-20（A+B 判终修复回归）：真答案已渲染 → `done=true` 且非 `pending` 时可判终。
    #[test]
    fn parse_snapshot_real_answer_permits_done() {
        let raw = r#"{
            "url":"x","ok":true,"splitV":4,
            "texts":["真回答正文"],"thinking":["已思考（用时 5 秒）需要思考"],
            "pending":false,"done":true
        }"#;
        let s = parse_snapshot(raw).expect("应能解析");
        assert!(!s.pending);
        assert!(s.done);
        assert_eq!(s.split_v, 4);
    }

    #[test]
    fn parse_snapshot_error_payload() {
        let err = parse_snapshot(r#"{"ok":false,"error":"未定位到会话容器"}"#).unwrap_err();
        assert_eq!(err, "未定位到会话容器");
    }

    #[test]
    fn parse_snapshot_invalid_json() {
        assert!(parse_snapshot("").is_err());
        assert!(parse_snapshot("<空：脚本未执行>").is_err());
    }

    // ---- E1（2026-09-26）：回读启动门竞态 —— 第四路「尾形」证据 + 二次收敛 ----

    /// ★ 核心回归：真机现场形态 `[上一轮答案, 回显, 助手块]` —— 三路既有信号全 false 时，
    ///   第四路必须能让本轮**开始**，且因二次收敛**须第二拍才放行**（第一拍只累计到 1）。
    #[test]
    fn startup_gate_site_shape_starts_after_second_consecutive_sample() {
        // 第 1 拍：located=true、cur_echo=1（回显在尾窗）、末块不是回显（其后已是助手块）。
        let (started1, streak1) = startup_gate(false, false, false, true, 1, 0);
        assert!(!started1, "第一拍不应开始（二次收敛未达成）");
        assert_eq!(streak1, 1);
        // 第 2 拍：形态**持续** ⇒ 收敛达成，开始。
        let (started2, streak2) = startup_gate(false, false, false, true, 1, streak1);
        assert!(started2, "连续第二拍形态持续 ⇒ 应开始");
        assert_eq!(streak2, 2);
    }

    /// 三路既有正向证据任一命中 → **立即**开始（既有语义不得回归）。
    #[test]
    fn startup_gate_legacy_paths_start_immediately() {
        assert!(
            startup_gate(true, false, false, false, 0, 0).0,
            "新回显落盘 → 立即开始"
        );
        assert!(
            startup_gate(false, true, false, false, 0, 0).0,
            "末块即回显 → 立即开始"
        );
        assert!(
            startup_gate(false, false, true, false, 0, 0).0,
            "正在生成 → 立即开始"
        );
    }

    /// 第四路的三个必要条件：任一不满足即不成立，且**连续计数清零**。
    #[test]
    fn startup_gate_fourth_path_requires_all_conditions() {
        // 未定位容器 → 不成立（即便有回显、非末块）。
        let (s1, k1) = startup_gate(false, false, false, false, 1, 5);
        assert!(!s1, "未定位容器 → 第四路不成立");
        assert_eq!(k1, 0, "形态不成立 → 连续计数必须清零");
        // cur_echo == 0（尾窗无回显）→ 不成立。
        let (s2, k2) = startup_gate(false, false, false, true, 0, 5);
        assert!(!s2, "尾窗无回显 → 第四路不成立");
        assert_eq!(k2, 0);
        // 末块即回显（`last_echo=true`）→ 第四路**不成立**（其形态要求 `!last_echo`）⇒ 连续计数清零。
        // ★ 注意：该拍 `started=true` 是**第二路 Legacy 证据**（末块即回显）触发的，与第四路无关，
        //   故此处只断言「第四路的连续计数被清零」，不断言 `started`（那会张冠李戴）。
        let (s3, k3) = startup_gate(false, true, false, true, 1, 5);
        assert!(s3, "末块即回显 → 由 Legacy 第二路触发开始");
        assert_eq!(k3, 0, "末块即回显 → 第四路形态不成立、连续计数清零");
    }

    /// 二次收敛必须是「**连续**」：中途形态断裂清零后，只累计到 1 拍仍**不得**开始。
    #[test]
    fn startup_gate_streak_requires_consecutive_samples() {
        let (_s1, k1) = startup_gate(false, false, false, true, 1, 0);
        assert_eq!(k1, 1);
        let (s2, k2) = startup_gate(false, false, false, true, 1, k1);
        assert!(s2, "连续第二拍 → 应开始");
        assert_eq!(k2, 2);
        // 形态断裂（cur_echo=0）→ 连续计数清零。
        let (_s3, k3) = startup_gate(false, false, false, true, 0, k2);
        assert_eq!(k3, 0, "形态断裂 → 连续计数必须清零");
        // 清零后仅累计到 1 拍 → 仍不得开始（须重新连续两拍）。
        let (s4, k4) = startup_gate(false, false, false, true, 1, k3);
        assert_eq!(k4, 1);
        assert!(!s4, "清零后仅 1 拍不得开始");
    }

    /// 常量契约：二次收敛要求次数 = 2（即「连续两次采样」）。
    #[test]
    fn startup_tail_confirm_constant_is_two() {
        assert_eq!(STARTUP_TAIL_CONFIRM, 2);
    }

    // ---- E2（2026-09-26）：容器增长纳入「有进展」（流式信号失灵时的兜底依据）----

    #[test]
    fn container_grew_detects_total_or_blocks_increase() {
        assert!(container_grew(120, 100, 3, 3), "文本总长增长 → 有进展");
        assert!(container_grew(100, 100, 4, 3), "块数增长 → 有进展");
        assert!(container_grew(200, 100, 5, 3), "两者都增长 → 有进展");
        assert!(!container_grew(100, 100, 3, 3), "完全不变 → 无进展");
        // 缩短（页面重排 / 容器切换）**不算**进展 —— 避免用「变小」伪装增长。
        assert!(!container_grew(90, 100, 2, 3), "变小不算进展");
        assert!(!container_grew(0, 0, 0, 0), "全零 → 无进展");
    }
}
