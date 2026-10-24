//! §5.1 回读桥 · 纯文本 / DOM 启发式（从 `readback.rs` 抽出）。
//!
//! 本模块只做**无副作用**的纯函数：给定一份页内快照 [`Snapshot`] 与提示词，
//! 判定「本轮回答在哪一块」「这块是不是装饰 / 占位 / 回显」「回答是否已写完」。
//! 不持有任何线程状态、不触碰 Tauri 运行时、不引用 `watch` / `deliver` 引擎。
//!
//! 被 `watch`（轮询引擎）与 `#[cfg(test)]` 单测共同依赖；对外公开项经 `readback` 根再导出，
//! 故既有 `crate::readback::locate_answer_and_thinking` 等调用点无需改动。

use crate::capture;
use crate::config;
use crate::readback::snapshot::Snapshot;
use tauri::WebviewWindow;

/// 装饰文案（页脚免责声明等）的特征串 —— **只用于「否决」，绝不用于定位**。
///
/// ★ 2026-10-03 六次真机（方案 B）：补 ChatGPT 站点的免责页脚文案
///   「ChatGPT 也可能会犯错。请核查重要信息。」（真机 `#61` 曾把它当回答交付，188 字符）。
///   ★ 与页内 `dsfloat_init.js::CONV_DECOR_HINTS` **口径保持一致**（两处同步维护）。
///   ★ 本防线**不可单独生效**：页脚被否决后 `locate_answer_and_thinking` 返回空（回答在
///     `texts[0]` 里取不到）⟹ 走超时。**根治靠 JS 侧的 `tailIsDecor`（A14）**，本处是最后保险。
const DECOR_HINTS: [&str; 5] = [
    "内容由AI生成",
    "请仔细甄别",
    "可能会犯错",     // ChatGPT：「ChatGPT 也可能会犯错。请核查重要信息。」
    "请核查重要信息", // ChatGPT 同上（拆两条，任一命中即否）
    "内容仅供参考",
];
/// 装饰文案的长度上限（去空白后字符数）：长回答里即便引用了这句话也不算装饰。
const DECOR_MAX_CHARS: usize = 60;

/// 页面**瞬时状态提示**（视觉 / 推理进行中的占位文案）的特征串 —— 与 `is_decoration` 同口径，
/// **只用于否决，绝不用于定位**。
const STATUS_HINTS: [&str; 7] = [
    "正在分析图片",
    "正在分析图像",
    "正在识别",
    "正在思考",
    "正在生成",
    "正在搜索",
    "正在阅读",
];
/// 状态提示的长度上限（去标签去空白后字符数）：超长即认为是正文（正文里引用这些词不算占位）。
const STATUS_MAX_CHARS: usize = 40;

/// 回显块允许的**前置** UI 文案上限（去标签去空白后）—— 覆盖「复制 / 编辑」这类按钮文案。
///
/// ★ 只允许**前置**：提示词**之后**不得再有正文。这是「回显块」与「回显 + 同块内真回答」
///   的分水岭 —— 后者是既有语义（见 `locate_answer_strips_combined_block`），误否决会把
///   正常回答一起拦掉（本判据第一版正是这样被单测抓住的）。
const ECHO_SLACK_CHARS: usize = 16;

/// 「净化 HTML」→ **紧凑纯文本**（去标签 + 去一切空白）。
///
/// ★ R11：R8 起块载荷是**净化 HTML**，而「块是否含我方提示词」是**纯文本包含**判据 ——
///   标签会插在提示词中间打断连续性（`<p>a</p><p>b</p>` 不含 `ab`）。故比较前必须先
///   去标签、再去一切空白（与页内 `compactText(stripTags(...))` **完全同口径**）。
pub(crate) fn compact_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag && !c.is_whitespace() => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// HTML 实体只解码一遍，避免把字面量 &amp;lt; 再解释成标签。
fn decode_entities(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(end) = rest.find(';').filter(|e| *e <= 12) {
            let entity = &rest[1..end];
            let ch = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                    u32::from_str_radix(&entity[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                }
                _ if entity.starts_with('#') => {
                    entity[1..].parse::<u32>().ok().and_then(char::from_u32)
                }
                _ => None,
            };
            if let Some(ch) = ch {
                out.push(ch);
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

/// 归一化后的提示词含于某块文本中？
///
/// ★ R11：块载荷是净化 HTML，比较前**先去标签**（见 [`compact_html`]）—— 否则长文 / 多行划词
///   会因标签打断提示词的连续性而恒判「不含回显」。真机证据（2026-09-15）：
///   `未接受会话容器（cand=2 echo=0 pass=1 input=1）` 连续 60s → 超时 / 0 次采样 → 卡片空白。
pub(crate) fn block_has(t: &str, p_norm: &str) -> bool {
    !p_norm.is_empty()
        && compact_html(t).contains(
            &p_norm
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>(),
        )
}

/// 尾窗中含提示词的块数。
pub(crate) fn echo_count(snap: &Snapshot, p_norm: &str) -> usize {
    snap.texts.iter().filter(|t| block_has(t, p_norm)).count()
}

/// 末块是否就是回显块（= 我方提问刚落盘、回答尚未开始）。
pub(crate) fn last_is_echo(snap: &Snapshot, p_norm: &str) -> bool {
    snap.texts
        .last()
        .map(|t| block_has(t, p_norm))
        .unwrap_or(false)
}

/// ★ 纯函数：剥掉我方提示词。
///
/// 只剥**前导**一次出现 —— 因为「用户消息回显」的形态是「[可能的 UI 文案] + 我方提示词」，
/// 剥离后剩下的才是回答；而提示词**出现在回答中间**时**不得**剥离（那可能是模型在引用原文，
/// 剥了就把回答削掉一截）。两种归一化形态（原文 / 折叠空白）都会尝试。
///
/// 净化 HTML 原样保留排版；仅旧纯文本载荷按归一化后的前导提示词剥离。
pub fn strip_own_prompt(text: &str, prompt: &str) -> String {
    // HTML 回答和用户回显已由结构角色与 is_echo_only 分开判断。这里不能使用
    // 选区的 normalize_text，否则 pre/code 的换行、缩进会在交付前不可逆地丢失。
    if text.trim_start().starts_with('<') {
        return text.trim().to_string();
    }
    let t = capture::normalize_text(text);
    let p = capture::normalize_text(prompt);
    if p.is_empty() {
        return t;
    }
    if t == p {
        return String::new();
    }
    // 归一化后的 `t` 中分隔空白已折叠为单个半角空格 → 剥掉后 trim 掉前导空格。
    if let Some(rest) = t.strip_prefix(&p) {
        return rest.trim().to_string();
    }
    // 兜底：提示词原文可能带换行/多空格（`t` 已折叠），再试一次「原文形态」。
    let p_raw = prompt.trim();
    if !p_raw.is_empty() && p_raw != p {
        if let Some(rest) = t.strip_prefix(p_raw) {
            return rest.trim().to_string();
        }
    }
    t
}

/// ★ 纯函数：这段文本是**页面装饰**（免责页脚等）而不是模型回答吗？
///
/// 真机证据（2026-09-15，见 `docs/result-reflow-design.md` §13.10）：容器定位停在包装层时，
/// 尾窗块是「整段对话 + 免责页脚」，于是「回显块的后一块」取到**页脚**
/// 「内容由 AI 生成，请仔细甄别」（15 字符）—— 文本稳定 ⇒ 被判 `complete=true` ⇒ **假通过**。
///
/// 这类文案是页面装饰，与模型回答无关，命中即**否决**：宁可继续等到超时并如实告警
/// （W-V5 不静默），也绝不把装饰当回答（W-V3）。
///
/// 保守口径：先去掉**一切空白**再比对（页内 `readableText` 已归一化，但块内换行仍可能有差异），
/// 且要求整体足够短（≤ [`DECOR_MAX_CHARS`]）—— 长回答里引用这句话不算装饰。
pub fn is_decoration(text: &str) -> bool {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if t.is_empty() || t.chars().count() > DECOR_MAX_CHARS {
        return false;
    }
    DECOR_HINTS.iter().any(|h| t.contains(h))
}

/// ★ R10：净化 HTML 下「只有标记、没有可见文本」的答案（如 `<div></div>`）不算回答。
///
/// 真机证据（2026-09-15，用户报「数据有时候没有回流有时候回流正常」）：R8 把页内抽取从
/// **纯文本**改成**净化 HTML** 后，**空块**不再回传空串（0 字符），而是回传 `<div></div>`
/// （11 字符**标记**）—— 于是它**骗过**了终态判定的最短长度闸门（`min_len`），把「回答还没
/// 开始生成的空占位块」当回答交付，表现为**卡片空白**（读代码日志：
/// `回读完成（11 字符 / 7 次采样 / 1606ms）：「<div></div>」`）。
///
/// 判据：去掉所有 `<...>` 标记与空白后若无剩余字符，即为空壳（`<img>` 例外：无文本但可见）。
/// 与 [`is_decoration`] 同口径 —— **只用于否决，绝不用于定位**。
pub fn is_markup_only(text: &str) -> bool {
    if text.contains("<img") {
        return false;
    }
    let mut visible = 0usize;
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag && !c.is_whitespace() => visible += 1,
            _ => {}
        }
    }
    visible == 0
}

/// ★ R12：这段文本是**页面瞬时状态提示**（「正在分析图片」等）而不是模型回答吗？
///
/// 命中即让回读**继续等**真回答 —— 宁可等到超时并如实告警，也绝不把占位文案当答案。
pub fn is_status_text(text: &str) -> bool {
    let t = compact_html(text);
    if t.is_empty() || t.chars().count() > STATUS_MAX_CHARS {
        return false;
    }
    STATUS_HINTS.iter().any(|h| t.contains(h))
}

/// 去掉已知的**页面装饰**文案（页脚免责声明等）。
///
/// 用途：装饰可能被包含在回显块内（真机容器骨架里 `内容由 AI 生成，请仔细甄别` 就是用户
/// 气泡的兄弟节点）⇒ 比对前先剥掉，否则「提示词 + 页脚」会因尾部多出这 12 字而漏判。
///
/// ★ 刻意**不**顺手 trim 标点：提示词自身常以「。」「：」结尾，trim 会把提示词也削掉一截，
///   于是 `c == p` 不再成立（本判据第二版正是这样被单测抓住的）。标点残留交给调用方的
///   [`is_punct_only`] 单独容忍。
fn strip_decor(c: &str) -> String {
    let mut out = c.to_string();
    for h in DECOR_HINTS {
        out = out.replace(&compact_html(h), "");
    }
    out
}

/// 这段文本是否**只是标点 / 分隔符**（装饰被剥掉后残留的「，」之类的容忍判据）。
fn is_punct_only(s: &str) -> bool {
    s.chars().all(|ch| {
        ch.is_whitespace() || "，,。.、;；:：!！?？~～-—…·（）()【】[]《》<>".contains(ch)
    })
}

/// ★ #004 兜底加固：把 HTML **空白实体**还原为真空白，再交由 [`compact_html`] 统一去空白。
///
/// `compact_html` 只去**真实**空白字符；而**实体形式**的空白（`&nbsp;` / `&#160;` / `&ensp;` 等
/// 是 5~6 个 ASCII 字符，`char::is_whitespace()` 对它们恒 `false`）它去不掉 —— 而净化 HTML 常把
/// 空格序列化成 `&nbsp;` ⇒ 回显块（含 `&nbsp;`）与提示词（含真实空格）在「去空白」后**仍对不齐**
/// ⇒ [`is_echo_only`] 误判 `false` ⇒ 回显被当作回答放行。本函数让两侧口径一致。
///
/// 幂等：输入无空白实体时**逐字原样返回**（既有调用路径零行为变化）。大小写不敏感（`&NBSP;` 亦可）。
fn norm_ws_entities(s: &str) -> String {
    // 已知空白实体的**字面量**形式（十进制 / 十六进制数字实体一并覆盖）。
    const ENTS: [&str; 10] = [
        "&nbsp;", "&ensp;", "&emsp;", "&thinsp;", "&#160;", "&#xa0;", "&#32;", "&#9;", "&#10;",
        "&#13;",
    ];
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            let rest = &s[i..];
            let rb = rest.as_bytes();
            if let Some(ent) = ENTS
                .iter()
                .find(|e| rb.len() >= e.len() && rb[..e.len()].eq_ignore_ascii_case(e.as_bytes()))
            {
                out.push(' '); // 空白实体 → 单个空格（随后被 `compact_html` 抹掉）
                i += ent.len();
                continue;
            }
        }
        // 非实体：按 UTF-8 整字符原样拷贝（保住字符边界）。
        let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// ★ R14：这块是不是**我方提示词的纯回显**（= 用户消息气泡），而不是回答？
///
/// 真机证据（2026-09-15 截图识字，见 `docs/result-reflow-design.md` §13.18）：
/// 回答尚未开始生成时，尾窗末块就是**用户消息气泡** —— 一串空 `<div>` 占位 + 我方提示词。
/// [`strip_own_prompt`] 只剥**前导**提示词，而块首是标记（`<div><div>…`）⇒ 剥不掉 ⇒
/// 整块被当回答交付：
/// `[shot-ocr] 回读完成（513 字符）：「<div>…请把这张图片里的文字完整、准确地转录出来…」`
/// —— 用户看到的结果卡里**就是他刚发出去的那句指令**。
///
/// 判据（去标签去空白后）：
/// ① 可见文本**恰好就是**我方提示词（允许前置 ≤ [`ECHO_SLACK_CHARS`] 的 UI 文案，以及
///    被包含进本块的页面装饰页脚）；且
/// ② 提示词**之后再无正文**（否则就是「回显 + 同块内真回答」，那是合法回答，不得否决）。
///
/// 与 `is_decoration` / `is_markup_only` / `is_status_text` 同口径 —— **只用于否决，绝不用于
/// 定位**：最坏是继续等到超时并如实降级（W-V5 不静默），绝不会把回显当回答交付（W-V3）。
///
/// ★ 为什么不用 [`block_has`]（contains）代替：那是**包含**判据，长回答里引用我方提示词时
///   同样命中，会把真回答一起拦掉。
pub(crate) fn is_echo_only(text: &str, prompt: &str) -> bool {
    // ★ #004：两侧先做**同一套空白归一化**（先[`norm_ws_entities`]还原空白实体 → 再 `compact_html`
    //   去一切空白），消除「块侧 `&nbsp;` vs 提示词侧真实空格」这类口径差异 —— 第 4 轮很可能死在这里。
    let p: String = prompt.chars().filter(|c| !c.is_whitespace()).collect();
    if p.is_empty() {
        return false;
    }
    let c = compact_html(&norm_ws_entities(text));
    if c.is_empty() {
        return false;
    }
    let c = strip_decor(&c);
    if c.is_empty() {
        return false;
    }
    if c == p {
        return true;
    }
    // 通用形态：`[前置 UI 文案] + 我方提示词 + [装饰残留的标点]`。
    // ★ 判据的关键在 `is_punct_only(tail)`：提示词**之后还有正文**（tail 是文字）即
    //   「回显 + 同块内真回答」，那是合法回答，**不得**否决（见
    //   `echo_only_does_not_reject_combined_echo_plus_answer` 与既有语义
    //   `locate_answer_strips_combined_block`）。
    match c.find(&p) {
        Some(i) => {
            let head = &c[..i];
            let tail = &c[i + p.len()..];
            head.chars().count() <= ECHO_SLACK_CHARS && is_punct_only(tail)
        }
        None => false,
    }
}

/// ★ #004：块 `idx` 是否被页内**正向识别为「我方回显（user）」**。
///
/// **只在正向命中时**返回 `true`：`roles` 缺失（旧脚本 / 站点未回传）/ 越界 / 非 `"user"` 一律
/// `false` ⇒ 行为**逐字退回**纯文本判据（[`is_echo_only`]），绝不因缺字段而「永不交付 / 恒超时」。
/// 这正是本判据的安全性质：**只有确凿的结构证据才否决**，未知一律放行。
fn role_is_user(snap: &Snapshot, idx: usize) -> bool {
    snap.roles.get(idx).map(|r| r == "user").unwrap_or(false)
}

/// ★ 纯函数：与 [`locate_answer`] **同口径**，但连思考过程一起返回 `(回答, 思考)`。
///
/// 定位逻辑与 `locate_answer` 逐字一致（同样必须反向扫），只是在定位到目标块后，
/// 额外取 `thinking` 的**同一项**（对齐关系由页内保证；页内未回传该字段时为空串）。
pub fn locate_answer_and_thinking(snap: &Snapshot, prompt: &str) -> (String, String) {
    let p_norm = capture::normalize_text(prompt);
    if !p_norm.is_empty() {
        // ★ R11：用 `block_has`（去标签比较）而非裸 `normalize_text(t).contains` —— 块载荷是
        //   净化 HTML，标签会打断提示词连续性（同 `tailHasEcho` 的成因）。
        if let Some(i) = snap.texts.iter().rposition(|t| block_has(t, &p_norm)) {
            if let Some(next) = snap.texts.get(i + 1) {
                // ★ #004：**结构化角色**硬拒绝 —— 后一块若被页内正向识别为我方回显（user），直接否决。
                //   它比纯文本判据稳：回显块多带包裹 / 空白差异（真机第 4 轮）时文本判据会漏，结构判据不漏。
                if role_is_user(snap, i + 1) {
                    return (String::new(), String::new());
                }
                // ★ R14：回显块本身**不是回答**（见 `is_echo_only`）。判据必须取**原始块** ——
                //   `strip_own_prompt` 对「标记 + 提示词」的形态剥不掉，剥过之后再判就晚了。
                if is_echo_only(next, prompt) {
                    return (String::new(), String::new());
                }
                let ans = strip_own_prompt(next, prompt);
                // ★ 装饰块（免责页脚等）**不是回答** → 返回空串，让上层继续等真回答（W-V3）。
                // ★ R10：纯标记的空壳（`<div></div>`）同样不是回答 —— 净化 HTML 下空块的
                //   「长度」不为 0，会骗过最短长度闸门，必须在此否决。
                // ★ R12：页面瞬时状态（「正在分析图片」）也不是回答 —— 它同样会「稳定」。
                if is_decoration(&ans) || is_markup_only(&ans) || is_status_text(&ans) {
                    return (String::new(), String::new());
                }
                let think = snap.thinking.get(i + 1).cloned().unwrap_or_default();
                return (ans, think);
            }
        }
    }
    match snap.texts.last() {
        Some(t) => {
            // ★ #004：**末块被正向识别为我方回显（user）⇒ 直接否决**。核心场景 = 真机第 4 轮：
            //   尾窗只剩 1 块（#0 = 回显），文本判据（`is_echo_only`）因「多带包裹」漏判 → 曾把
            //   用户回显当回答交付（4315 字符 = 解释预设 + 划选原文）。
            if role_is_user(snap, snap.texts.len() - 1) {
                return (String::new(), String::new());
            }
            // ★ R14：**末块即回显** ⇒ 回答尚未开始 ⇒ 不是回答。这正是「截图识字回读完成
            //   （513 字符）：<div>…请把这张图片里的文字完整、准确地转录出来…」的根因 ——
            //   把用户自己刚发出去的指令当成了回答交付。
            if is_echo_only(t, prompt) {
                return (String::new(), String::new());
            }
            let ans = strip_own_prompt(t, prompt);
            // 兜底路径同样否决装饰（否则「末块即页脚」会把页脚当回答交付）、纯标记空壳（R10）
            // 与页面瞬时状态文案（R12）。
            if is_decoration(&ans) || is_markup_only(&ans) || is_status_text(&ans) {
                return (String::new(), String::new());
            }
            (ans, snap.thinking.last().cloned().unwrap_or_default())
        }
        None => (String::new(), String::new()),
    }
}

/// ★ 纯函数：这个 URL 是否表示**面板停在登录页**（= 尚未登录，任何提问都不可能被回答）。
///
/// 与 `lib.rs::panel_state` 同口径（`/sign_in`），单独抽出来是为了可单测 ——
/// 这条判据决定「要不要放弃回读并改说『请先登录』」，**判错比不判更糟**：
/// 误判会把一次正常提问直接掐掉并给出错误的行动建议。
pub fn url_needs_login(url: &str) -> bool {
    // 收敛到 config::url_is_sign_in（路径段匹配、去 query/fragment），与 lib::panel_state 同口径
    //（审核 P2 一致性项）：query 里恰好含 sign_in 不再误报「未登录」而掐断正常提问。
    config::url_is_sign_in(url)
}

/// 面板此刻是否在登录页（取不到 URL ⇒ 不判，宁可走正常回读）。
pub(crate) fn panel_needs_login(panel: &WebviewWindow) -> bool {
    panel
        .url()
        .map(|u| url_needs_login(u.as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod entity_regression_tests {
    use super::*;
    #[test]
    fn escaped_question_is_not_an_answer() {
        assert!(block_has("<p>A &amp; B &lt; C</p>", "A & B < C"));
        assert!(is_echo_only("<p>A &amp; B &lt; C</p>", "A & B < C"));
        assert!(!is_echo_only("<p>A &amp;amp; B</p>", "A & B"));
        assert!(block_has("<p>&#65; &amp; B</p>", "A & B"));
    }
}
