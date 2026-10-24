//! 单次提交：只在新增用户消息可定位时确认发送；不确定状态禁止自动重发。
use std::time::{Duration, Instant};
use tauri::{Manager, WebviewWindow};
const ST_SENT: &str = "ok";
const ST_NO_INPUT: &str = "no-input";
const ST_EMPTY: &str = "empty-after-fill";
const ST_MISMATCH: &str = "content-mismatch";
const ST_NO_SEND_BUTTON: &str = "no-send-button";
#[cfg(test)]
const ST_SOFT_MISMATCH: &str = "ok-soft-mismatch";
#[cfg(test)]
const ST_SOFT_MISMATCH_NO_SEND: &str = "soft-mismatch-no-send-button";
const ST_NO_API: &str = "no-api";
const FILL_RETRY_MAX: u32 = 8;
const FILL_RETRY_DELAY_MS: u64 = 400;
const FILL_RETRY_DELAY_CAP_MS: u64 = 3_000;
const SEND_DEADLINE_MS: u64 = 25_000;
fn log(msg: &str) {
    crate::diag::log(&format!("inject: {msg}"));
}
#[cfg(debug_assertions)]
fn warn(msg: &str) {
    crate::diag::warn(&format!("inject: {msg}"));
}
fn js_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}
fn parse_js_string(raw: &str) -> String {
    serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.trim_matches('"').to_string())
}
#[cfg(test)]
fn parse_js_bool(raw: &str) -> bool {
    serde_json::from_str::<bool>(raw).unwrap_or(true)
}
fn fill_retry_delay(attempt: u32) -> u64 {
    (FILL_RETRY_DELAY_MS << attempt.saturating_sub(1).min(10)).min(FILL_RETRY_DELAY_CAP_MS)
}
fn uncertain(panel: &WebviewWindow, token: crate::workflow::Token) {
    crate::workflow::update(
        panel.app_handle(),
        token,
        crate::workflow::SendStatus::Uncertain(
            "发送结果不确定，请打开面板核对；不会自动重复提交".into(),
        ),
    );
}
/// 包括 eval 回调丢失的情况：截止后仅观察，禁止继续提交。
pub fn inject_and_send_async(panel: WebviewWindow, text: String) {
    let token = crate::workflow::begin(panel.app_handle());
    crate::readback::remember_send(text.clone(), token);
    let deadline = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
        + SEND_DEADLINE_MS;
    let watchdog = panel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(SEND_DEADLINE_MS));
        if crate::workflow::status(token) == crate::workflow::SendStatus::Sending {
            uncertain(&watchdog, token);
        }
    });
    fill_with_retry(panel, text, 0, token, deadline);
}
fn send_guard(token: crate::workflow::Token, deadline: u64, js: &str) -> String {
    crate::workflow::guarded(
        token,
        &format!("(function(){{if(Date.now()>={deadline})return 'expired';return ({js});}})()"),
    )
}
fn fill_with_retry(
    panel: WebviewWindow,
    text: String,
    attempt: u32,
    token: crate::workflow::Token,
    deadline: u64,
) {
    if crate::workflow::status(token) != crate::workflow::SendStatus::Sending {
        return;
    }
    let arg = js_string(&text);
    let js = format!("(function(){{try{{const a=window.__DSFLOAT__;if(!a||!a.prepareSend)return 'no-api';a.prepareSend({arg});return a.fill({arg})?'filled':'fill-failed';}}catch(e){{return 'exception:'+e.message;}}}})()");
    let next = panel.clone();
    let called = panel.eval_with_callback(send_guard(token, deadline, &js), move |raw| {
        if crate::workflow::status(token) != crate::workflow::SendStatus::Sending {
            return;
        }
        let state = parse_js_string(&raw);
        if state == "filled" {
            submit_step(next.clone(), text.clone(), token, deadline);
            return;
        }
        if state == "expired" {
            uncertain(&next, token);
            return;
        }
        if attempt < FILL_RETRY_MAX
            && (state == ST_NO_API || state == "fill-failed" || state.starts_with("exception:"))
        {
            let next = next.clone();
            let text = text.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(fill_retry_delay(attempt + 1)));
                fill_with_retry(next, text, attempt + 1, token, deadline);
            });
        } else {
            crate::workflow::failed(
                &next,
                token,
                "无法填入，尚未提交。请检查面板登录及输入框后重试",
            );
        }
    });
    if called.is_err() {
        crate::workflow::failed(&panel, token, "填入脚本无法执行，尚未提交");
    }
}
fn submit_step(panel: WebviewWindow, text: String, token: crate::workflow::Token, deadline: u64) {
    if crate::workflow::status(token) != crate::workflow::SendStatus::Sending {
        return;
    }
    let js=format!("(function(){{try{{return window.__DSFLOAT__?window.__DSFLOAT__.submit({},0):'no-api';}}catch(e){{return 'exception:'+e.message;}}}})()",js_string(&text));
    let next = panel.clone();
    let called = panel.eval_with_callback(send_guard(token, deadline, &js), move |raw| {
        if crate::workflow::status(token) != crate::workflow::SendStatus::Sending {
            return;
        }
        let state = parse_js_string(&raw);
        match state.as_str() {
            ST_SENT | "already-submitted" => {
                log("已单次点击发送，等待新增用户消息证据");
                verify_sent(next.clone(), text.clone(), token);
            }
            ST_NO_INPUT | ST_EMPTY | ST_MISMATCH | ST_NO_SEND_BUTTON | ST_NO_API => {
                crate::workflow::failed(
                    &next,
                    token,
                    &format!("尚未提交（{state}），请检查面板后重试"),
                )
            }
            _ => uncertain(&next, token),
        }
    });
    if called.is_err() {
        uncertain(&panel, token);
    }
}
fn verify_sent(panel: WebviewWindow, text: String, token: crate::workflow::Token) {
    std::thread::spawn(move || {
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(SEND_DEADLINE_MS)
            && crate::workflow::status(token) == crate::workflow::SendStatus::Sending
        {
            let js=format!("(function(){{try{{return window.__DSFLOAT__?window.__DSFLOAT__.sendEvidence({}):'unknown';}}catch(e){{return 'unknown';}}}})()",js_string(&text));
            let (tx, rx) = std::sync::mpsc::channel();
            if panel
                .eval_with_callback(crate::workflow::guarded(token, &js), move |raw| {
                    let _ = tx.send(parse_js_string(&raw));
                })
                .is_err()
            {
                break;
            }
            if rx.recv_timeout(Duration::from_millis(1500)).ok().as_deref() == Some("sent") {
                crate::workflow::update(
                    panel.app_handle(),
                    token,
                    crate::workflow::SendStatus::Reading,
                );
                return;
            }
            std::thread::sleep(Duration::from_millis(450));
        }
        if crate::workflow::status(token) == crate::workflow::SendStatus::Sending {
            uncertain(&panel, token);
        }
    });
}

// ★ 分发版（release）**不编译**下列诊断探针：它们的唯一调用方是 `lib.rs::setup_selftest`，
//   而后者本身带 `#[cfg(debug_assertions)]` ⇒ 在 release 里这些函数必然不可达
//   （实测 `cargo build --release` 会报 7 处 dead_code 警告）。
//   按本工程纪律**不用 `#[allow(dead_code)]` 掩盖**，而是用**与调用方完全相同的 cfg 条件**
//   把它们一并排除 ⇒ release 既零警告，二进制也不再携带只服务排障的代码（体积也更小）。
//   需要探针时用 debug 构建：`DSFLOAT_SELFTEST=<秒> DSFLOAT_SELFTEST_PROBE=upload|conversation`。
//
//   注意：**诊断日志（`diag`）不受影响** —— 它才是发给用户排障的主力手段，release 同样开启。

/// 「截图提问」S1 诊断探针：让 panel 页枚举候选「图片上传入口」并把结果落盘。
///
/// 这决定 S1 的验证项 (b)：若存在可注入的 `input[type=file]`，首选投递路线（页内
/// `DataTransfer` 注入）成立；若完全没有，则路线须改为剪贴板 / CDP（设计文档 §5.6）。
///
/// 与 `fill` / `submit` 同构：Rust 侧主动 `eval_with_callback`（不受 ACL 限制），
/// 回调运行在**主线程** → 绝不在回调里 sleep（硬约束 §6）。
#[cfg(debug_assertions)]
pub fn inspect_upload(app: &tauri::AppHandle) {
    let Some(panel) = app.get_webview_window("panel") else {
        warn("inspect-upload: panel 窗口不存在（应用尚未创建面板）");
        return;
    };
    log("inspect-upload: 请求 panel 枚举图片上传入口…");
    // ★ 传的是**表达式**：WebView2 的 ExecuteScript 会把表达式结果 JSON 序列化成回调入参，
    //   所以这里**不能**自己 JSON.stringify（否则拿到的是被双重转义的字符串）。
    let js = "(function(){try{if(!window.__DSFLOAT__||!window.__DSFLOAT__.inspectUpload)return {error:'未注入'};return window.__DSFLOAT__.inspectUpload();}catch(e){return {error:String((e&&e.message)||e)};}})()";
    let called = panel.eval_with_callback(js, move |raw| {
        let text = if raw.trim().is_empty() {
            "<空：脚本未执行或页面未加载完成>".to_string()
        } else {
            raw
        };
        log(&format!(
            "inspect-upload 结果: {}",
            crate::diag::content(&text)
        ));
    });
    if called.is_err() {
        warn("inspect-upload: 无法在面板中执行脚本");
    }
}

/// R0 探针的跨次指纹（`url` + 判定量 + 「渲染窗口是否移动」的辅助证据）。
#[cfg(debug_assertions)]
struct ConvFp {
    url: String,
    child_count: Option<i64>,
    text_len: Option<i64>,
    scroll_top: Option<i64>,
    /// ★ **结构性虚拟化信号**（`pick.virtStruct.hit`）—— **主判据**：容器内存在
    /// 「高 ≥ 0.9×`scrollHeight`、直接子节点 ≤8、子高之和 < 0.5×自身高」的 spacer / 撑高层
    /// （react-window / virtua 同款结构）。它是**结构事实**，不受块高分布影响，故可单独定论。
    virt_struct: bool,
    /// `pick.virtRatio` —— **辅助启发式**（见 `VIRT_RATIO_HINT`）：该比值**系统性偏低**，
    /// 故**不得单独定论**，只用于与结构信号 / Δ 交叉。
    virt_ratio: Option<f64>,
    /// 首块文本（`firstTexts[0]`）—— 计数不变时用它判断渲染窗口是否移动过。
    first_text: String,
    /// 真·末块文本（`lastTexts` 的**最后一个元素**；QA P3-3：原来取 `[0]` 实为倒数第 3 块）。
    last_text: String,
}

/// 上一次 `pick` 的指纹。
///
/// 目的：把「滚动会话后 `children` / 文本长度是否变化」这一判据，从**一次 `Δ` 日志**直接读出，
/// 无需人工比对两行长 JSON。纯 Rust 侧内存状态，不触碰页面（探针整体仍属纯观测）。
///
/// ★ **必须带 `url`**（P2-3）：本静态是**进程级**的，而 panel 页会 reload（`__DSFLOAT__`
///   随之重建）、用户也会切换会话 —— 那时两次触发的指纹来自**不同页面/会话**，Δ 与滚动无关。
///   url 变化即重置基线、不输出 Δ。
#[cfg(debug_assertions)]
static LAST_CONV: std::sync::Mutex<Option<ConvFp>> = std::sync::Mutex::new(None);

/// 「结果回流」R0 诊断探针：让 panel 页枚举候选「会话容器 / 消息块列表」并把结果落盘。
///
/// 这是设计文档 §十 路线图的 **go/no-go 闸门**，要回答 §三 U1 的前置问题：
///   ① 能否定位到承载整段会话的容器（不依赖 class/id）；
///   ② 该容器是否**虚拟化**（滚动卸载旧节点 → R1 的「前后快照差集」会失真，见 §七 R3）。
///
/// 与 `inspect_upload` 同构：Rust 侧主动 `eval_with_callback`（不受 ACL 限制），
/// 回调运行在**主线程** → 绝不在回调里 sleep（硬约束 §6）。
/// **额外**做一件 `inspect_upload` 没有的事：缓存上次 `pick` 的指纹，于是「两次触发之间
/// 滚动面板」的差异会直接落成一行 `Δ` 日志（见 `log_conversation_delta`）。
#[cfg(debug_assertions)]
pub fn inspect_conversation(app: &tauri::AppHandle) {
    let Some(panel) = app.get_webview_window("panel") else {
        warn("inspect-conversation: panel 窗口不存在（应用尚未创建面板）");
        return;
    };
    log("inspect-conversation: 请求 panel 枚举候选会话容器…");
    // ★ 与 inspect_upload 同因：传的是**表达式**。WebView2 的 ExecuteScript 会把表达式结果
    //   JSON 序列化成回调入参，所以这里**不能**自己 JSON.stringify（否则双重转义）。
    let js = "(function(){try{if(!window.__DSFLOAT__||!window.__DSFLOAT__.inspectConversation)return {error:'未注入'};return window.__DSFLOAT__.inspectConversation();}catch(e){return {error:String((e&&e.message)||e)};}})()";
    let called = panel.eval_with_callback(js, move |raw| {
        if raw.trim().is_empty() {
            warn("inspect-conversation 结果: <空：脚本未执行或页面未加载完成>");
            return;
        }
        log(&format!(
            "inspect-conversation 结果: {}",
            crate::diag::content(&raw)
        ));
        log_conversation_delta(&raw);
    });
    if called.is_err() {
        warn("inspect-conversation: 无法在面板中执行脚本");
    }
}

/// `virtRatio` 的**参考**阈值 —— ⚠️ **仅用于「值得复核」的提示，绝不能单独定论**。
///
/// ★ 为什么只能当提示（独立 QA P1-1，2026-09-14）：`virtRatio = renderedItems / expectedItems` 的
///   分子分母**不同源** —— `expectedItems = round(scrollHeight / medianItemHeight)`，而 `scrollHeight`
///   计入了 ① 被当 sizer 排除的长块、② 块间 margin/gap，这两者都**不计入** `itemH`；再加
///   ③ 块高右偏时 `sumHeights / median > count`。三重效应使其**系统性 ≤ 1 且偏低**：
///   反例 —— 10 块（8×40px + 2×300px，均 < 0.6×`clientHeight` 故不被排除）时
///   `scrollHeight = 920`、`median = 40` → `expectedItems = 23`，而 `renderedItems = 10`
///   → **比值 0.435 < 0.5**，可这是一个**完全非虚拟化**的容器。
///   ⇒ 该比值单独使用会把真·非虚拟化判成虚拟化（**反向误判**）。主判据是结构信号 `virtStruct`。
#[cfg(debug_assertions)]
const VIRT_RATIO_HINT: f64 = 0.5;

/// 把本次 `pick` 的指纹与上次对比，落一行 `Δ` 日志（判据 W-V2 的直接证据）。
///
/// 判定口径（**结构信号为主、Δ 与比值辅助**；主判据不依赖任何滚动操作）：
/// 1. `pick.virtStruct.hit == true` → **虚拟化特征（结构性证据，可单独定论）** —— 容器内存在
///    「高 ≈ `scrollHeight`、子节点少、子高之和 ≪ 自身高」的 spacer / 撑高层。与是否滚动无关。
/// 2. `scrollTop` **未变** → 两次触发之间没有滚动，本次 Δ **无判定力**（提示先滚动）。
/// 3. `virtRatio < VIRT_RATIO_HINT` → **倾向虚拟化（需复核）**，**不单独定论** —— 该比值系统性偏低，
///    非虚拟化容器也可能低于阈值（见常量注释里的反例，QA P1-1）。
/// 4. Δ：`childCount` / `textLen` 变化，或首/末块文本变化（窗口移动但条目数恰好相同）
///    → 滚动卸载/装载了节点 → **虚拟化特征**；但 `textLen` **增长而 `childCount` 未变**时要先排除
///    「流式回答正在增长」这一同源干扰（QA P2-1，日志会带提示）。
/// 5. 只有「已滚动 + 计数未变 + 首末块文本未变 + 比值未触提示阈值 + 结构信号为假」→ **非虚拟化特征**；
///    若 `virtRatio` 缺失（`n/a`）则**不下结论**（明示本次 Δ 无判定力）。
///
/// 缺失字段一律渲染 `n/a`，**不**退化成 `-1`（否则会出现 `1234 → -1 (-1235)` 这类误导读数）。
#[cfg(debug_assertions)]
fn log_conversation_delta(raw: &str) {
    /// 可选数值 → 字符串：缺失一律 `n/a`。
    fn show(o: Option<i64>) -> String {
        match o {
            Some(n) => n.to_string(),
            None => "n/a".to_string(),
        }
    }
    /// 两个可选数值的增量：任一侧缺失即 `n/a`。
    fn delta(cur: Option<i64>, prev: Option<i64>) -> String {
        match (cur, prev) {
            (Some(c), Some(p)) => format!("{:+}", c - p),
            _ => "n/a".to_string(),
        }
    }
    /// 文本预览：空串渲染 `∅`，否则取前 **36** 个字符 —— **与 `moved` 的比较长度一致**
    /// （QA P3-2：日志若比比较截得更短，会出现「日志显示相同却判 moved」的误读）。
    ///
    /// ★ R17-P1 正文脱敏：这 36 字来自**页面消息正文**，是全项目隐私面最大的一处诊断输出。
    ///   `Meta` 档（release 开启诊断时的默认档）换成 `«正文已隐去 len=36 h=…»` ——
    ///   哈希仍能支撑「首/末块在两帧之间变没变」的判读，而那正是本行的用途。
    fn preview(s: &str) -> String {
        if s.is_empty() {
            "∅".to_string()
        } else {
            // JS 侧已 `slice(0,36)`，这里再截一次是防御：配置若被改，也不至于把整段正文灌进日志。
            let t: String = s.chars().take(36).collect();
            if crate::diag::verbose() {
                format!("«{t}»")
            } else {
                crate::diag::content(&t)
            }
        }
    }

    let v = match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(v) => v,
        Err(e) => {
            warn(&format!(
                "inspect-conversation Δ: 结果非合法 JSON（{e}），跳过跨次对比"
            ));
            return;
        }
    };
    let Some(pick) = v.get("pick").filter(|p| !p.is_null()) else {
        // ★ QA P3-1：本次 `pick` 缺失（页面已清空 / 尚未渲染）时**必须清空基线** —— 否则下次的 Δ
        //   会与「上一次有效快照」**跨一次 reload** 比对（url 未变故不会触发重置基线）。
        *LAST_CONV.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return;
    };
    let num = |k: &str| pick.get(k).and_then(|x| x.as_i64());
    // 取 `firstTexts` / `lastTexts` 的首项（JS 侧已 `slice(0,36)`+`norm`，长度有界）。
    let arrhead = |k: &str| -> String {
        pick.get(k)
            .and_then(|x| x.as_array())
            .and_then(|a| a.first())
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let arrtail = |k: &str| -> String {
        pick.get(k)
            .and_then(|x| x.as_array())
            .and_then(|a| a.last())
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let cur = ConvFp {
        url: v
            .get("url")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        child_count: num("childCount"),
        text_len: num("textLen"),
        scroll_top: num("scrollTop"),
        virt_struct: pick
            .get("virtStruct")
            .and_then(|x| x.get("hit"))
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
        virt_ratio: pick.get("virtRatio").and_then(|x| x.as_f64()),
        first_text: arrhead("firstTexts"),
        last_text: arrtail("lastTexts"),
    };
    let virt = match cur.virt_ratio {
        Some(f) => format!("{f}"),
        None => "n/a".to_string(),
    };

    let mut slot = LAST_CONV.lock().unwrap_or_else(|e| e.into_inner());
    match slot.as_ref() {
        Some(prev) if prev.url == cur.url => {
            let moved = cur.first_text != prev.first_text || cur.last_text != prev.last_text;
            log(&format!(
                "inspect-conversation Δ: childCount {}→{} ({}) · textLen {}→{} ({}) · scrollTop {}→{} ({}) · virtRatio={} · virtStruct={} · 首块 {}→{} · 末块 {}→{}",
                show(prev.child_count),
                show(cur.child_count),
                delta(cur.child_count, prev.child_count),
                show(prev.text_len),
                show(cur.text_len),
                delta(cur.text_len, prev.text_len),
                show(prev.scroll_top),
                show(cur.scroll_top),
                delta(cur.scroll_top, prev.scroll_top),
                virt,
                cur.virt_struct,
                preview(&prev.first_text),
                preview(&cur.first_text),
                preview(&prev.last_text),
                preview(&cur.last_text),
            ));
            let hint = matches!(cur.virt_ratio, Some(r) if r < VIRT_RATIO_HINT);
            let counted = cur.child_count != prev.child_count || cur.text_len != prev.text_len;
            let grew = matches!((cur.text_len, prev.text_len), (Some(c), Some(p)) if c > p);
            // ① 主判据：结构性 spacer 信号 —— 与是否滚动无关，故**先于**滚动前置条件判定。
            if cur.virt_struct {
                log("inspect-conversation Δ: **virtStruct 命中** → 容器内存在「高 ≈ scrollHeight、子节点少、子高之和 ≪ 自身高」的撑高层 → **虚拟化特征**（结构性证据，与是否滚动无关）");
            } else if cur.scroll_top.is_some() && cur.scroll_top == prev.scroll_top {
                // ★ 判定必须要求「确实滚动过」：否则两次**未滚动**的点击也会命中「未卸载」，
                //   把「什么都没做」误读成「非虚拟化的证据」。
                log("inspect-conversation Δ: scrollTop 未变 → 两次触发之间**未观察到滚动**，本次 Δ 无判定力（请先滚动面板再触发）");
            } else if hint {
                // ★ 不再单凭比值定论（QA P1-1）：该比值系统性偏低，非虚拟化容器也可能低于阈值。
                log(&format!(
                    "inspect-conversation Δ: virtRatio={} < {} → **倾向虚拟化（需复核）**；该比值系统性偏低，不单独定论 —— 请核对 virtStruct / childHT / 容器类名",
                    virt, VIRT_RATIO_HINT
                ));
            } else if counted || moved {
                if grew && cur.child_count == prev.child_count {
                    // ★ QA P2-1：Δ 无法区分「节点回收」与「内容增长」。
                    log("inspect-conversation Δ: 计数/文本变化 → **虚拟化特征**（⚠️ textLen 增长而 childCount 未变，也可能是**流式回答正在增长** —— 测试前请确认无流式生成）");
                } else {
                    log("inspect-conversation Δ: 已滚动且计数/首末块文本变化 → 滚动卸载/装载了节点（虚拟化特征）");
                }
            } else if cur.virt_ratio.is_none() {
                log("inspect-conversation Δ: 已滚动、计数与首末块文本均未变，但 virtRatio 缺失 → 本次 Δ 无判定力（无法排除「窗口未动」）");
            } else {
                log("inspect-conversation Δ: 已滚动、计数与首末块文本均未变、比值未触提示阈值、且未检出撑高层 → 非虚拟化特征");
            }
        }
        Some(prev) => log(&format!(
            "inspect-conversation: 页面/会话已变化（{} → {}），重置基线指纹（不输出 Δ）",
            prev.url, cur.url
        )),
        None => {
            log(&format!(
                "inspect-conversation: 已记录基线指纹（childCount={}, textLen={}, scrollTop={}, virtRatio={}, virtStruct={}）—— 滚动面板后再触发一次即可读 Δ",
                show(cur.child_count),
                show(cur.text_len),
                show(cur.scroll_top),
                virt,
                cur.virt_struct
            ));
            // ★ 结构信号是**单次快照**即可定论的（不需要滚动、也不需要基线）→ 首次触发就报结论。
            if cur.virt_struct {
                log("inspect-conversation: **virtStruct 命中** → **虚拟化特征**（结构性证据，单次快照即可定论，无需滚动）");
            }
        }
    }
    *slot = Some(cur);
}

#[cfg(test)]
mod tests {
    use super::{fill_retry_delay, js_string, parse_js_bool, parse_js_string};

    #[test]
    fn js_string_quotes_and_escapes() {
        assert_eq!(js_string("ok"), "\"ok\"");
        // 反斜杠与引号要转义，注入进 JS 才能原样还原。
        assert_eq!(js_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn parse_js_string_handles_wrapped_and_plain() {
        assert_eq!(parse_js_string("\"ok\""), "ok");
        // 未带引号的裸文本走 trim 兜底。
        assert_eq!(parse_js_string("ok"), "ok");
        // 非法 JSON 时不 panic，退化为去掉首尾引号。
        assert_eq!(parse_js_string("\"broken\""), "broken");
    }

    #[test]
    fn parse_js_bool_conservative_on_failure() {
        assert!(parse_js_bool("true"));
        assert!(!parse_js_bool("false"));
        // 解析失败保守返回 true（=「仍是未发送状态」），避免谎报成功跳过重试。
        assert!(parse_js_bool("not-a-bool"));
        assert!(parse_js_bool(""));
    }

    #[test]
    fn fill_retry_delay_exponential_backoff_with_cap() {
        // FILL_RETRY_DELAY_MS=400，FILL_RETRY_DELAY_CAP_MS=3000
        assert_eq!(fill_retry_delay(1), 400);
        assert_eq!(fill_retry_delay(2), 800);
        assert_eq!(fill_retry_delay(3), 1600);
        // 第 4 次起封顶 3000（400<<3=3200 > 3000 被截断）
        assert_eq!(fill_retry_delay(4), 3000);
        assert_eq!(fill_retry_delay(5), 3000);
        // 超大 attempt 仍封顶（位移上限 10 位，但被 CAP 截断），不溢出
        assert_eq!(fill_retry_delay(100), 3000);
    }

    /// ★ 批次1（2026-10-03）：状态串必须与 `dsfloat_init.js::submit()` 的返回值**逐字一致**。
    ///
    /// 这类「跨语言字符串契约」一旦拼错，故障表现是**静默的**——Rust 侧落到 `other =>` 分支，
    /// 只打一条「提交异常」，发送行为可能整条丢失。故用单测把契约钉死。
    #[test]
    fn submit_status_strings_match_js_contract() {
        use super::{
            ST_EMPTY, ST_MISMATCH, ST_NO_API, ST_NO_INPUT, ST_NO_SEND_BUTTON, ST_SENT,
            ST_SOFT_MISMATCH, ST_SOFT_MISMATCH_NO_SEND,
        };
        // 原有状态（不得改动 —— 旧版注入脚本仍可能返回它们）
        assert_eq!(ST_SENT, "ok");
        assert_eq!(ST_NO_INPUT, "no-input");
        assert_eq!(ST_EMPTY, "empty-after-fill");
        assert_eq!(ST_MISMATCH, "content-mismatch");
        assert_eq!(ST_NO_SEND_BUTTON, "no-send-button");
        assert_eq!(ST_NO_API, "no-api");
        // ★ 批次1 新增：软校验两态
        assert_eq!(ST_SOFT_MISMATCH, "ok-soft-mismatch");
        assert_eq!(ST_SOFT_MISMATCH_NO_SEND, "soft-mismatch-no-send-button");
        // 两者必须**不同**：前者已点发送、后者需转页内回车兜底 —— 若相等会走错分支。
        assert_ne!(ST_SOFT_MISMATCH, ST_SOFT_MISMATCH_NO_SEND);
        // 新软校验态不得与旧 `ok` 相等（否则「额外告警」会被吞掉）。
        assert_ne!(ST_SOFT_MISMATCH, ST_SENT);
    }
}
