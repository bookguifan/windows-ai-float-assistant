//! §10：提权划词（UI Automation 直读选区）。
//!
//! 当 `config.privileged_capture` 打开时，优先用 UI Automation 的 `TextPattern` 读取
//! 焦点控件里**当前选区**的文本，不经过剪贴板。这能突破一部分「合成复制键取不到文本」的
//! 受保护 / UWP 控件（例如某些 UWP 文本控件、被标记为只读但可选中的控件）。
//!
//! 拿不到（控件没有 TextPattern / COM 未就绪 / 选区为空）时，调用方 `popup::trigger_selection`
//! 会回退到 `capture::capture_selection`（合成 Ctrl+Insert + 剪贴板轮询法）。
//!
//! ⚠️ UI Automation 的 COM 接口必须在 **STA** 线程上使用。调用方（鼠标钩子 worker 线程）
//! 一般是尚未初始化 COM 的普通线程，本模块在块内 `CoInitializeEx(STA)` 并成对 `CoUninitialize`。

use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationLegacyIAccessiblePattern,
    IUIAutomationTextPattern, IUIAutomationTextPattern2, IUIAutomationTextRangeArray,
    IUIAutomationValuePattern, UIA_LegacyIAccessiblePatternId, UIA_TextPattern2Id,
    UIA_TextPatternId, UIA_ValuePatternId,
};

/// 选区文本上限字符数（与 capture::MAX_CHARS 对齐）。
const MAX_CHARS: usize = 20_000;

/// 取焦点元素的选区文本；失败 / 无文本返回 `None`。
///
/// 仅读取，不改动剪贴板、不改动前台焦点。字符串已做 `trim` 与长度截断。
pub fn uia_selection_text() -> Option<String> {
    unsafe {
        // STA 必须用 CoInitializeEx 显式初始化；若本线程已以 MTA 初始化（返回 RPC_E_CHANGED_MODE），
        // UIA 无法工作，直接放弃回退到剪贴板法。
        let co_init =
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        if !co_init {
            // ★ 诊断（纯观测）：此前该失败静默 return，无法与其它原因区分。
            crate::diag::log("uia: CoInitializeEx(STA) 失败（线程可能已以 MTA 初始化）");
            return None;
        }

        let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL) {
            Ok(a) => a,
            Err(e) => {
                crate::diag::log(&format!("uia: CoCreateInstance 失败: {e}"));
                CoUninitialize();
                return None;
            }
        };

        let element: IUIAutomationElement = match automation.GetFocusedElement() {
            Ok(e) => e,
            Err(e) => {
                crate::diag::log(&format!("uia: GetFocusedElement 失败: {e}"));
                CoUninitialize();
                return None;
            }
        };

        // 取 TextPattern：先拿到 IUnknown，再 cast 到 TextPattern 接口。
        let pattern: IUIAutomationTextPattern = match element
            .GetCurrentPattern(UIA_TextPatternId)
            .and_then(|unk| unk.cast::<IUIAutomationTextPattern>())
        {
            Ok(p) => p,
            Err(e) => {
                // 没有文本模式（如纯按钮 / 图片控件）→ 回退剪贴板法。
                // ★ 诊断（纯观测，不改控制流）：此前该错误被完全丢弃，导致生产日志 89 次
                //   TextPattern 失败无痕。这里在返回前补齐：错误本身 + 焦点元素四属性 +
                //   三探针（TextPattern2 / Value / LegacyIAccessible 是否存在）。
                //   所有 getter 用 unwrap_or / map 兜底，任一失败都不 panic、不提前 return。
                let ct = element.CurrentControlType().map(|v| v.0).unwrap_or(-1);
                let cls = element
                    .CurrentClassName()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let fw = element
                    .CurrentFrameworkId()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                // ★ 隐私：`Name` 是焦点元素的 UIA Name 属性，**可能含用户可见文本**
                //   （如自定义输入框 / 列表项），故必须与其它正文一样经 `diag::content()` 包装：
                //   Meta 档下输出「正文已隐去 len=.. h=..」，Full 档才显示原文。切勿裸打。
                let name = crate::diag::content(
                    &element
                        .CurrentName()
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                        .chars()
                        .take(64)
                        .collect::<String>(),
                );
                let tp2 = element
                    .GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
                    .is_ok();
                let vp = element
                    .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                    .is_ok();
                let lp = element
                    .GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(
                        UIA_LegacyIAccessiblePatternId,
                    )
                    .is_ok();
                crate::diag::log(&format!(
                    "uia: GetCurrentPattern(TextPattern) 失败: {e}; CtrlType={ct} Class='{cls}' Fw='{fw}' Name={name} T2={tp2} VP={vp} LP={lp}"
                ));
                CoUninitialize();
                return None;
            }
        };

        let selection: IUIAutomationTextRangeArray = match pattern.GetSelection() {
            Ok(s) => s,
            Err(e) => {
                crate::diag::log(&format!("uia: GetSelection 失败: {e}"));
                CoUninitialize();
                return None;
            }
        };

        let mut text = String::new();
        if let Ok(count) = selection.Length() {
            // ★ 诊断（纯观测，零行为变更）：记录 range 数量与各段长度。
            //   用途 —— 拖动一段多行 Markdown 时，若 UIA 切成**多个 range**，
            //   则段间换行会被下面的 `push(' ')` 抹平（range 内部的 `\n` 仍保留）；
            //   若为**单个 range**，内部换行完整。这条日志是判断「UIA 复制为何仍丢换行」
            //   的一手证据，供后续决定是否改 range 拼接策略。
            let mut seg_lengths: Vec<usize> = Vec::new();
            for i in 0..count {
                if let Ok(range) = selection.GetElement(i) {
                    if let Ok(t) = range.GetText(-1) {
                        let s = t.to_string();
                        if !s.is_empty() {
                            if !text.is_empty() {
                                text.push(' ');
                            }
                            text.push_str(s.trim());
                            seg_lengths.push(s.chars().count());
                        }
                    } else {
                        // ★ 诊断（纯观测）：此前静默跳过。else 只记日志，不改控制流。
                        // ⚠️ 措辞区分：GetText 返回 Err 在 UIA 中多为「该 range 无文本/不可用」
                        //   （空 range 是常态，原逻辑跳过它继续拼下一个 range 是**有意设计**），
                        //   并非都是异常。故记「无文本/不可用」，避免排查时去找不存在的错误。
                        crate::diag::log(&format!("uia: range.GetText(-1) 无文本/不可用 (i={i})"));
                    }
                } else {
                    // ★ 诊断（纯观测）：GetElement 失败此前静默跳过。else 只记日志，不改控制流。
                    crate::diag::log(&format!("uia: selection.GetElement 失败 (i={i})"));
                }
            }
            let joined_newlines = text.matches('\n').count();
            crate::diag::log(&format!(
                "uia: 选区 range 数={count} 有效段={} 各段字符={seg_lengths:?} 拼接后换行数={joined_newlines}",
                seg_lengths.len()
            ));
        } else {
            // ★ 诊断（纯观测）：Length 失败此前静默跳过（整个选区被丢弃）。else 只记日志。
            crate::diag::log("uia: selection.Length() 失败");
        }

        CoUninitialize();

        let text = text.trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text.chars().take(MAX_CHARS).collect())
        }
    }
}
