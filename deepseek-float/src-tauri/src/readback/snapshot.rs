//! 页内快照的结构与解析（R1 回读桥的数据层）。
//!
//! 抽离自 `readback.rs`：把 `Snapshot` 与 `parse_snapshot` 从巨型模块独立出来，
//! 便于单测与后续继续拆分（analyze / watch / deliver）。`readback.rs` 经 `pub use` 重新导出，
//! 既有调用点（`crate::readback::Snapshot`、`parse_snapshot`、模块内裸名）保持不变。

/// 页内快照（只保留 R1 需要的字段；`url` 供诊断，避免跨会话比对）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    /// 页面 URL（诊断用）。
    pub url: String,
    /// 尾窗各消息块的可读文本（末项 = 最新一条消息）。
    pub texts: Vec<String>,
    /// ★ 与 `texts` **逐项对齐**的「思考过程」：第 i 项即第 i 块的思考正文（无则空串）。
    ///
    /// DeepSeek 的助手块把「折叠头 + 思考正文 + 回答」首尾相连地塞进同一个块里，
    /// 页内 `answerOf()` 剥掉思考前缀之后才作为 `texts` 回传 —— 本字段把**被剥掉的那一段**
    /// 一并带回，于是结果卡能同时展示「思考过程」与「回答」（用户明确要求）。
    pub thinking: Vec<String>,
    /// ★ #004 **「块角色」**：与 `texts` / `thinking` **逐项对齐**，取值 `"assistant"` / `"user"` / `"unknown"`。
    ///
    /// 页内**结构标记优先**判定（见 `dsfloat_init.js::snapshotConversation`）：
    /// - `"assistant"`：该块带**助手标记**（思考子树之外的可见 `ds-markdown`，或思考折叠头形态）；
    /// - `"user"`：**不是**助手块，且块文本含我方注入提示词（= 我方那条消息的**回显**）；
    /// - `"unknown"`：其余。
    ///
    /// 用途（#004 回显交付修复）：Rust 侧**只在正向识别出 `user` 时**拒绝交付（见
    /// [`crate::readback::analyze::locate_answer_and_thinking`]），避免把「用户自己那条消息的回显」
    /// 当回答交付。★ 旧脚本 / 站点未回传该字段时为空 vec ⇒ 解析为空 ⇒ 行为**逐字退回**纯文本判据
    /// （绝不因缺字段而「永不交付 / 恒超时」）。
    pub roles: Vec<String>,
    pub request_bound: bool,
    /// 有文本的块数（诊断用）。
    pub blocks: usize,
    /// 容器内文本总长（诊断用；受页内长度闸门限制）。
    pub total: usize,
    /// 下钻包装层的深度（>0 时说明定位发生过下钻，人工复核用）。
    pub hops: u64,
    /// **结构信号**：页内是否仍在流式生成（存在可见的「停止生成」控件）。
    pub streaming: bool,
    /// ★ **结构信号**：尾窗末块此刻**只有思考过程**（命中思考折叠头却切不出回答正文）
    /// ⇒ 回答还没开始。与 `streaming` 互补 —— 真机里「思考中」时页面上未必有可识别的
    /// 「停止生成」控件（`streaming=false`），单靠它会把思考停顿误判成「回答已写完」。
    pub pending: bool,
    /// ★★ **「已写完」正向结构信号**（2026-09-20）：末块（最新一条助手消息）下方已渲染
    ///    复制/重新生成/点赞等**操作行** —— DeepSeek 只在一条消息**完整流式结束**后才渲染它。
    ///    用途：Rust 侧终态判定以此为「完成」的必要条件（见 [`decide_done`]），替换「静默 × 稳定
    ///    采样」这一启发式（后者在 DeepSeek 分段生成、落尾段前瞬间停顿且无 streaming 标记时
    ///    必然赛输，把最后一段静默丢给「已写完」）。
    pub done: bool,
    /// 诊断信号：是否存在「深度思考」折叠区（暂不参与判据）。
    pub reasoning: bool,
    /// 诊断信号：本轮页内实际选中的容器（`cls pass=N hops=N`）—— 换错容器一眼可见。
    pub container: String,
    /// 诊断信号：页内回传的**容器尾窗骨架**（短 JSON 字符串，逐字落盘，不参与判据）。
    ///
    /// 用途：判定「思考正文与回答是同块内联、还是同块的兄弟子块」—— 抽取策略的分水岭。
    pub dump: String,
    /// ★ 注入脚本版本探针：页内 `splitV`。Rust 侧只需**看一次**即可自证新分块逻辑已真正运行
    ///   （曾因重启 app 后 WebView 会话被复用、旧脚本仍挂在页上，导致修复看似无效——缺它就是旧脚本）。
    pub split_v: u64,
}

/// 解析页内 `snapshotConversation()` 的返回值。
///
/// 页内约定：`{ ok: true, url, texts, blocks, total, hops, streaming, reasoning }` 或 `{ ok: false, error }`。
pub fn parse_snapshot(raw: &str) -> Result<Snapshot, String> {
    let v: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("结果非合法 JSON: {e}"))?;
    if v.get("ok").and_then(|x| x.as_bool()) == Some(false) {
        return Err(v
            .get("error")
            .and_then(|x| x.as_str())
            .unwrap_or("页内返回未知错误")
            .to_string());
    }
    let texts = v
        .get("texts")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let thinking = v
        .get("thinking")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // ★ #004：块角色（与 texts/thinking 逐项对齐）。写法与上面一致；缺字段 ⇒ 空 vec（向后兼容）。
    let roles = v
        .get("roles")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(Snapshot {
        thinking,
        roles,
        request_bound: v
            .get("requestBound")
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
        // ★ P2-3：快照 URL（面板 location.href）仅用于诊断，落盘前脱敏（去 query/fragment），
        //   避免 Full 档把一次性 token/参数明文写进 d1-debug.log（审查 P2-3）。
        url: crate::redact_url(v.get("url").and_then(|x| x.as_str()).unwrap_or_default())
            .to_string(),
        texts,
        blocks: v.get("blocks").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
        total: v.get("total").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
        hops: v.get("hops").and_then(|x| x.as_u64()).unwrap_or(0),
        streaming: v
            .get("streaming")
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
        pending: v.get("pending").and_then(|x| x.as_bool()).unwrap_or(false),
        done: v.get("done").and_then(|x| x.as_bool()).unwrap_or(false),
        reasoning: v
            .get("reasoning")
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
        container: v
            .get("container")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        dump: v
            .get("dump")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        split_v: v.get("splitV").and_then(|x| x.as_u64()).unwrap_or(0),
    })
}
