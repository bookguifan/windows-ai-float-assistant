//! D1 划词管线：抓取当前选区文本 → 归一化 → 组装 prompt。
//!
//! ⚠️ 历史说明：早期与前端 `src/lib/promptTemplate.ts` 同源（preset 定义与 normalize 规则须两侧一致）。
//!    该前端已随「去 React」提交删除，现由本文件末尾的 `#[cfg(test)]` 独占锁定行为。
//!
//! 抓取策略（蓝图 §2）：不依赖 UI 自动化 / Accessibility，走
//! 「合成 Ctrl+Insert → 剪贴板序列号确认 → 读 CF_UNICODETEXT → 恢复旧剪贴板」，
//! 对支持复制的应用使用；原剪贴板含非文本/富文本或不可读时跳过回退，避免格式丢失。
//! UIA 优先策略由 popup.rs 控制。全程不改动前台焦点。

use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::w;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, GlobalFree, SetLastError, ERROR_SUCCESS, HANDLE, HGLOBAL, HWND,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_INSERT, VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
};

/// `CF_UNICODETEXT` == 13。windows crate 把它定义在 `Win32::System::Ole` 里，
/// 为了不给体积很大的 `Ole` 模块单独开 feature，这里直接用等值字面量。
const CF_UNICODETEXT_FORMAT: u32 = 13;

/// 轮询参数（team-lead 规定 `capture_selection(400, 20000)`）。
pub const POLL_TIMEOUT_MS: u64 = 400;
pub const POLL_INTERVAL_MS: u64 = 25;
/// 长度闸门（按 Unicode 字符数，不按字节）。
pub const MIN_CHARS: usize = 2;
pub const MAX_CHARS: usize = 20_000;

/// UTF-16 扫描的工作量上限；实际指针读取还必须受 GlobalSize 分配边界限制。
const MAX_WIDE_SCAN: usize = 1 << 20;

fn decode_clipboard_units(units: &[u16]) -> Option<String> {
    let end = units.iter().position(|&unit| unit == 0)?;
    Some(String::from_utf16_lossy(&units[..end]))
}

// Text/ANSI/OEM/locale are Windows' text conversion family, not rich-format snapshots.
// Never clone arbitrary clipboard handles or invoke delayed renderers for unknown formats.
fn clipboard_capture_allowed(formats: &[u32], text: Option<&str>, complete: bool) -> bool {
    complete
        && (formats.is_empty()
            || (text.is_some() && formats.iter().all(|f| matches!(f, 1 | 7 | 13 | 16))))
}

fn clipboard_restore_allowed(expected_sequence: Option<u32>, current_sequence: u32) -> bool {
    match expected_sequence {
        None => true,
        Some(expected) => expected != 0 && expected == current_sequence,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowIdentity {
    hwnd: isize,
    pid: u32,
    thread_id: u32,
}

impl WindowIdentity {
    fn valid(self) -> bool {
        self.hwnd != 0 && self.pid != 0 && self.thread_id != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ClipboardSource {
    owner: WindowIdentity,
    sequence: u32,
}

struct CopyGuard {
    target: WindowIdentity,
    sequence_before: u32,
}

impl CopyGuard {
    fn new(target: WindowIdentity, sequence_before: u32, sent: u32) -> Option<Self> {
        (target.valid() && sequence_before != 0 && sent == 4).then_some(Self {
            target,
            sequence_before,
        })
    }

    fn accepts_capture(
        &self,
        current_target: Option<WindowIdentity>,
        process_alive: bool,
        source: Option<ClipboardSource>,
    ) -> bool {
        process_alive
            && current_target == Some(self.target)
            && source.is_some_and(|source| {
                source.owner.valid()
                    && source.owner.pid == self.target.pid
                    && source.sequence != 0
                    && source.sequence != self.sequence_before
            })
    }

    fn accepts_restore(
        &self,
        expected: ClipboardSource,
        current_target: Option<WindowIdentity>,
        process_alive: bool,
        source: Option<ClipboardSource>,
    ) -> bool {
        source == Some(expected)
            && self.accepts_capture(current_target, process_alive, source)
            && clipboard_restore_allowed(Some(expected.sequence), source.unwrap().sequence)
    }
}

unsafe fn window_identity(hwnd: HWND) -> Option<WindowIdentity> {
    if !IsWindow(Some(hwnd)).as_bool() {
        return None;
    }
    let mut pid = 0;
    let thread_id = GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let identity = WindowIdentity {
        hwnd: hwnd.0 as isize,
        pid,
        thread_id,
    };
    identity.valid().then_some(identity)
}

// Retaining the process handle prevents PID reuse from authorizing an old operation.
// Owner PID alone is not proof: foreground identity, liveness, owner and generation must
// all remain consistent. Brokered/null owners deliberately fail closed.
struct CaptureTarget {
    identity: WindowIdentity,
    process: HANDLE,
}

impl CaptureTarget {
    unsafe fn bind() -> Option<Self> {
        let identity = window_identity(GetForegroundWindow())?;
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, identity.pid).ok()?;
        let target = Self { identity, process };
        target.is_current().then_some(target)
    }

    fn process_alive(&self) -> bool {
        let mut code = 0;
        unsafe { GetExitCodeProcess(self.process, &mut code).is_ok() && code == 259 }
    }

    fn current_identity(&self) -> Option<WindowIdentity> {
        unsafe { window_identity(GetForegroundWindow()) }
    }

    fn is_current(&self) -> bool {
        self.process_alive() && self.current_identity() == Some(self.identity)
    }
}

impl Drop for CaptureTarget {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.process);
        }
    }
}

/// Caller holds OpenClipboard; never consume an unverified writer's payload.
unsafe fn clipboard_source() -> Option<ClipboardSource> {
    let owner = window_identity(GetClipboardOwner().ok()?)?;
    let sequence = GetClipboardSequenceNumber();
    (sequence != 0).then_some(ClipboardSource { owner, sequence })
}

struct RestoreGuard<'a> {
    target: &'a CaptureTarget,
    copy: &'a CopyGuard,
    source: ClipboardSource,
}

impl RestoreGuard<'_> {
    /// Called only while the clipboard is open, immediately before destructive writes.
    unsafe fn permits_current(&self) -> bool {
        self.copy.accepts_restore(
            self.source,
            self.target.current_identity(),
            self.target.process_alive(),
            clipboard_source(),
        )
    }
}

/// Only inspect format IDs. Return before reading any payload if a non-text format exists.
/// The caller owns the clipboard lock throughout enumeration and snapshot creation.
unsafe fn restorable_clipboard_snapshot() -> Option<(Option<String>, u32)> {
    if OpenClipboard(None).is_err() {
        return None;
    }
    let snapshot = (|| {
        let mut formats = Vec::new();
        let mut previous = 0;
        loop {
            SetLastError(ERROR_SUCCESS);
            let format = EnumClipboardFormats(previous);
            if format == 0 {
                if GetLastError() != ERROR_SUCCESS {
                    return None;
                }
                break;
            }
            if formats.len() >= 64 || !matches!(format, 1 | 7 | 13 | 16) {
                return None;
            }
            formats.push(format);
            previous = format;
        }
        let text = if formats.is_empty() {
            None
        } else {
            read_open_clipboard_text()
        };
        let sequence = GetClipboardSequenceNumber();
        if sequence == 0 || !clipboard_capture_allowed(&formats, text.as_deref(), true) {
            return None;
        }
        Some((text, sequence))
    })();
    let _ = CloseClipboard();
    snapshot
}

/// 上次成功抓取的文本，用于「与上次相同」闸门。`Mutex::new` 是 const fn，可作 static 初始化。
static LAST_CAPTURED: Mutex<Option<String>> = Mutex::new(None);

/// 上次抓取的**原始**选区文本（未经 `normalize_text` 折叠）。
///
/// 用途：工具条「复制」动作应保留选区原有排版（换行 / 缩进 / 制表符），
/// 而注入面板的文本必须是归一化版（DeepSeek 输入框对多行敏感）。
/// 归一化会不可逆地折叠所有空白，故原文只能在来源处旁路留存一份。
///
/// 两条抓词路径的写入方式：
/// - **剪贴板法**（`capture_selection`）：闸门通过后写入 `read`，失败则清空；
/// - **提权划词 / UIA 直读**（`popup::trigger_selection`）：成功后写入
///   `uia::uia_selection_text()` 的返回值（其 range 内部换行是保留的）。
///
/// ⚠️ 与 `LAST_CAPTURED` 语义**互相独立**。复制处另有 `raw_matches_pending`
/// 自校验兜底：万一两份缓存错配，也会安全退化为归一化文本，绝不复制出错误内容。
static LAST_RAW: Mutex<Option<String>> = Mutex::new(None);

/// 取出并消费上次抓取的原文（配合工具条「复制」使用）。
///
/// 返回 `None` 表示「本次抓词没有留下原文」（如 UIA 直读路径），
/// 调用方应退化为使用归一化文本，**绝不失败**。
pub fn take_last_raw() -> Option<String> {
    LAST_RAW.lock().ok().and_then(|mut g| g.take())
}

/// 写入旁路原文缓存。
///
/// 供**不走剪贴板法**的抓词路径使用（提权划词 / UIA 直读）：它的原文来自
/// `uia::uia_selection_text()`，与剪贴板路径的 `read` 语义相同，都应留存给「复制」用。
///
/// ⚠️ 调用方须保证 `raw` 与随后写入 `PENDING_TEXT` 的归一化文本**同源**，
/// 否则复制处的 `raw_matches_pending` 自校验会判否并退化为归一化文本（安全但无收益）。
pub fn set_last_raw(raw: String) {
    if let Ok(mut g) = LAST_RAW.lock() {
        *g = Some(raw);
    }
}

/// 判断旁路原文 `raw` 是否**确属**当前待发文本 `pending` 的那一次划词。
///
/// 判据：`normalize_text(raw)` 以 `pending` 为前缀。两种合法形态都满足——
/// 1. 正常：`normalize_text(raw) == pending`，前缀恒成立；
/// 2. 超长：`pending` 是 `normalize_text(raw)` 截断到 `MAX_CHARS` 的前缀（原文不截断）。
///
/// 若 `raw` 是**上一次**划词残留的原文，其归一化结果不会以本次 `pending` 为前缀 ⇒ 判否。
/// 这使「缓存错配」从"理论上不会"变成"物理上不可能生效"。
pub fn raw_matches_pending(raw: &str, pending: &str) -> bool {
    if pending.is_empty() {
        return false;
    }
    normalize_text(raw).starts_with(pending)
}

// ---------------------------------------------------------------------------
// normalize / compose
// ---------------------------------------------------------------------------

/// 与 TS `normalizeText`（`raw.replace(/\s+/g, ' ').trim()`）对齐：
/// 连续空白折叠为单个半角空格并去首尾。
///
/// 注意一处跨语言差异：JS 的 `\s` 包含 `\u{FEFF}`，而 Rust 的 `char::is_whitespace`
/// 不含它，这里显式把 `\u{FEFF}` 也当空白处理，保证两侧对边界输入结果一致。
pub fn normalize_text(raw: &str) -> String {
    raw.split(|c: char| c.is_whitespace() || c == '\u{FEFF}')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// 统一提示词拼接（内置与自定义共用，数据驱动自 prompts.rs 的 `Prompt.instruction`）：
/// - instruction 里含 `{text}` 占位符 → 原样替换（用户完全掌控排版）；
/// - 否则：`指令\n\n正文`；
/// - 空正文 → 仅指令；空指令 → 仅正文（调用方须在此之前用长度闸门挡住「双空」）。
pub fn compose_custom(instruction: &str, body: &str) -> String {
    let text = normalize_text(body);
    let instr = instruction.trim();
    if instr.contains("{text}") {
        return instr.replace("{text}", &text);
    }
    if text.is_empty() {
        return instr.to_string();
    }
    if instr.is_empty() {
        return text;
    }
    format!("{instr}\n\n{text}")
}

// ---------------------------------------------------------------------------
// 抓取结果与四道闸门
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// 成功，携带**已归一化**的文本。
    Ok(String),
    /// 剪贴板序列号没变 → 说明 Ctrl+Insert 没产生新复制（多半是没选中东西）。
    NoNewCopy,
    /// 剪贴板里没有 CF_UNICODETEXT（选中的是图片等非文本）。
    ClipboardNotText,
    /// A safe snapshot is unavailable, or another writer changed it before copy/restore.
    ClipboardProtected,
    /// 归一化后长度 < MIN_CHARS。
    TooShort,
}

impl CaptureOutcome {
    /// 给用户看的提示文案；成功时为 `None`。
    pub fn user_message(&self) -> Option<String> {
        match self {
            CaptureOutcome::Ok(_) => None,
            CaptureOutcome::NoNewCopy => Some("没有检测到选中的文本".into()),
            CaptureOutcome::ClipboardNotText => Some("选中的不是文本内容".into()),
            CaptureOutcome::ClipboardProtected => {
                Some("为保留剪贴板原格式，已跳过剪贴板划词；可尝试设置中的 UIA 划词".into())
            }
            CaptureOutcome::TooShort => Some(format!("选中的文本太短（至少 {MIN_CHARS} 个字）")),
        }
    }
}

/// 四道闸门 + 长度闸门，**纯函数**（`last` 由调用方传入，便于单测确定性）。
///
/// 判定顺序刻意如此：先看「有没有新复制」，再看「是不是文本」，
/// 否则在用户什么都没选中的情况下会误报「剪贴板里没有文本」。
pub fn evaluate(
    raw: Option<&str>,
    seq_changed: bool,
    max_len: usize,
    last: Option<&str>,
) -> CaptureOutcome {
    if !seq_changed {
        return CaptureOutcome::NoNewCopy;
    }
    let Some(raw) = raw else {
        return CaptureOutcome::ClipboardNotText;
    };
    let text = normalize_text(raw);
    let len = text.chars().count();
    if len < MIN_CHARS {
        return CaptureOutcome::TooShort;
    }
    if len > max_len {
        // ★ 边界增强（2026-09-16）：超长不再「整段废弃」。拖动大段文本（长文/整段复制）是常见
        //   场景，一超 20k 就直接废弃会让划词成功率大幅下降。改为**截断到上限成功返回**，
        //   把前 max_len 个字符送出去（DeepSeek 输入框同样有 20k 上限，截断是贴合平台的一致行为）。
        //   完整正文仍保留在诊断日志里供排查。提权划词（UIA）路径在 popup.rs 中镜像此截断逻辑，
        //   保证两条捕获链路行为一致（均截断成功，而非返回失败变体）。
        let truncated: String = text.chars().take(max_len).collect();
        crate::diag::warn(&format!(
            "capture: 选区超长（{len} 字符 > 上限 {max_len}），已截断为前 {max_len} 字符"
        ));
        return CaptureOutcome::Ok(truncated);
    }
    // ★ SameAsLast 不再阻断（2026-09-16）：若上次注入/回流失败，本次相同内容**必须能重试**；
    //   且 DeepSeek 侧无幂等去重，重复拖选同段后重发无害。故与内容变化时一样返回 Ok，只是记日志。
    if last == Some(text.as_str()) {
        crate::diag::log("capture: 与上次内容相同，仍允许重发");
    }
    CaptureOutcome::Ok(text)
}

// ---------------------------------------------------------------------------
// Win32：合成 Ctrl+Insert / 读剪贴板 / 恢复剪贴板
// ---------------------------------------------------------------------------

fn key_input(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// 一次 `SendInput` 原子投递 Ctrl↓ Insert↓ Insert↑ Ctrl↑。
/// 分四次调用容易在快速应用里被吞键，合并成一次更稳。
/// 返回实际投递的事件数（用于诊断）。
///
/// ★★ 为什么是 **Ctrl+Insert** 而不是 Ctrl+C（2026-09-14 真机事故的根治）：
///   `SendInput` 合成的 **Ctrl+C 会被控制台转成 `CTRL_C_EVENT`**。dev 下二进制是 console
///   子系统（`main.rs` 只在 release 设 `windows_subsystem="windows"`），所以当前台是终端 /
///   conhost / Windows Terminal / mintty 时，这一次合成按键会**中断整个控制台进程组** ——
///   `pnpm` / `node`(vite) / `tauri` CLI 一起被打断，`pnpm tauri dev` 连同应用**无声消失**
///   （无 `panic.log`、无 WER、无退出日志，表现为「启动后没多久自动关闭」）。
///   `lib.rs` 里的 `SetConsoleCtrlHandler(None, true)` **只保护本进程**，救不了同控制台里的
///   dev server —— 这就是本函数换键的原因。
///   `Ctrl+Insert` 是 Windows **控制台的「复制」标准键**（conhost / Windows Terminal / mintty
///   均内置），语义等价于复制但**不产生 SIGINT / CTRL_C_EVENT**；Chromium 系（浏览器 / Electron）
///   与传统 Win32 编辑控件同样支持 → 一次改动同时消除危险并保留「在终端里划词」场景。
///   ★ 刻意**不做 Ctrl+C 回落**：回落会把上面这条事故路径原样引回来。
fn send_copy_chord() -> u32 {
    let inputs = [
        key_input(VK_CONTROL, KEYBD_EVENT_FLAGS::default()),
        key_input(VK_INSERT, KEYBD_EVENT_FLAGS::default()),
        key_input(VK_INSERT, KEYEVENTF_KEYUP),
        key_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    unsafe {
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent != inputs.len() as u32 {
            crate::diag::warn(&format!("SendInput(Ctrl+Insert) 只投递了 {sent}/4 个事件"));
        }
        sent
    }
}

/// 轮询剪贴板序列号直至变化（或超时）。返回是否检测到新复制。
fn poll_seq_changed(target: &CaptureTarget, seq_before: u32, timeout_ms: u64) -> Option<bool> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        if !target.is_current() {
            return None;
        }
        let sequence = unsafe { GetClipboardSequenceNumber() };
        if sequence == 0 {
            return None;
        }
        if sequence != seq_before {
            return Some(true);
        }
    }
    Some(false)
}

/// 等用户松开 Ctrl / Alt（最长 `timeout_ms`）。
///
/// ★ NoNewCopy 的历史根因（当时由全局热键 Ctrl+Alt+D 触发）：热键触发时用户往往**仍按着** Ctrl+Alt，
///   此刻立即 SendInput(Ctrl+Insert)，目标应用实际收到的是 **Ctrl+Alt+Insert**（多按了 Alt），
///   绝大多数应用不把 Ctrl+Alt+C 当「复制」，于是剪贴板序列号不变 → NoNewCopy。
///   热键虽已移除，但用户拖选时仍可能按着修饰键，故合成复制前先轮询 GetAsyncKeyState，
///   直到 Ctrl / Alt 都松开，再投递干净的 Ctrl+Insert。
fn wait_modifiers_released(timeout_ms: u64) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        let ctrl_down = unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) } as u16 & 0x8000 != 0;
        let alt_down = unsafe { GetAsyncKeyState(VK_MENU.0 as i32) } as u16 & 0x8000 != 0;
        if !ctrl_down && !alt_down {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    crate::diag::log("wait_modifiers_released 超时（Ctrl/Alt 仍按下，仍继续复制）");
}

/// 读取剪贴板文本；无文本/被占用时返回 `None`。
unsafe fn read_clipboard_text(
    target: &CaptureTarget,
    guard: &CopyGuard,
) -> Option<(String, ClipboardSource)> {
    if OpenClipboard(None).is_err() {
        crate::diag::log("clip: OpenClipboard 失败");
        return None;
    }
    let text = (|| {
        let before = clipboard_source()?;
        if !guard.accepts_capture(
            target.current_identity(),
            target.process_alive(),
            Some(before),
        ) {
            return None;
        }
        let text = read_open_clipboard_text()?;
        // Delayed rendering may change the owner or sequence during GetClipboardData.
        if !guard.accepts_restore(
            before,
            target.current_identity(),
            target.process_alive(),
            clipboard_source(),
        ) {
            return None;
        }
        Some((text, before))
    })();
    let _ = CloseClipboard();
    text
}

/// Caller must hold OpenClipboard until this bounded read is complete.
unsafe fn read_open_clipboard_text() -> Option<String> {
    (|| -> Option<String> {
        if IsClipboardFormatAvailable(CF_UNICODETEXT_FORMAT).is_err() {
            // ★ 诊断（纯观测）：区分「格式缺失」与「读到空文本」。此前两者日志同形。
            //   动态探测 HTML Format / Rich Text Format 是否可用（格式 ID 运行时注册，
            //   绝不能硬编码，故用 RegisterClipboardFormatW 取动态 ID）。
            //   RegisterClipboardFormatW 失败返回 0 → 标注 reg_fail，与「已注册但剪贴板无此格式」
            //   相区分（后者才是真正的「格式不存在」）。纯观测，不参与任何判断。
            let html_id = RegisterClipboardFormatW(w!("HTML Format"));
            let rtf_id = RegisterClipboardFormatW(w!("Rich Text Format"));
            let html_ok = html_id != 0 && IsClipboardFormatAvailable(html_id).is_ok();
            let rtf_ok = rtf_id != 0 && IsClipboardFormatAvailable(rtf_id).is_ok();
            let html = if html_ok {
                "1".to_string()
            } else if html_id == 0 {
                "0(reg_fail)".to_string()
            } else {
                "0".to_string()
            };
            let rtf = if rtf_ok {
                "1".to_string()
            } else if rtf_id == 0 {
                "0(reg_fail)".to_string()
            } else {
                "0".to_string()
            };
            crate::diag::log(&format!(
                "clip: CF_UNICODETEXT(13) 不可用 html={html} rtf={rtf}"
            ));
            return None;
        }
        let handle = match GetClipboardData(CF_UNICODETEXT_FORMAT) {
            Ok(h) => h,
            Err(e) => {
                crate::diag::log(&format!("clip: GetClipboardData(CF_UNICODETEXT) 失败: {e}"));
                return None;
            }
        };
        if handle.is_invalid() {
            crate::diag::log("clip: GetClipboardData 返回 invalid handle");
            return None;
        }
        let hglobal = HGLOBAL(handle.0);
        let units = (GlobalSize(hglobal) / std::mem::size_of::<u16>()).min(MAX_WIDE_SCAN);
        if units == 0 {
            return None;
        }
        let ptr = GlobalLock(hglobal) as *const u16;
        if ptr.is_null() {
            crate::diag::log("clip: GlobalLock 返回 null");
            return None;
        }
        // Clipboard remains open and the HGLOBAL locked while reading at most its allocation.
        // An absent terminator is invalid data, never permission to scan past that allocation.
        let s = decode_clipboard_units(std::slice::from_raw_parts(ptr, units));
        let _ = GlobalUnlock(hglobal);
        s
    })()
}

/// 把文本写回剪贴板（用于恢复旧内容）。失败静默返回 false。
unsafe fn write_clipboard_text(text: &str, restore: Option<&RestoreGuard<'_>>) -> bool {
    // SetClipboardData 需要以 NUL 结尾的 UTF-16，且内存必须是 GMEM_MOVEABLE 的 HGLOBAL。
    // ★ 数据一致性加固（2026-09-19）：先把 HGLOBAL 分配并填好，**成功后再** EmptyClipboard +
    //   SetClipboardData —— 避免「先清空、后分配失败」把用户剪贴板留在空态（数据丢失）。
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) else {
        return false;
    };
    let dst = GlobalLock(hmem) as *mut u16;
    if dst.is_null() {
        let _ = GlobalFree(Some(hmem));
        return false;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
    let _ = GlobalUnlock(hmem);
    if OpenClipboard(None).is_err() {
        let _ = GlobalFree(Some(hmem));
        return false;
    }
    let ok = (|| -> bool {
        // Check while holding OpenClipboard: an intervening writer, even copying identical
        // text, must not be overwritten by restoration of an earlier capture snapshot.
        if let Some(restore) = restore {
            if !restore.permits_current() {
                return false;
            }
        }
        if EmptyClipboard().is_err() {
            return false;
        }
        if SetClipboardData(CF_UNICODETEXT_FORMAT, Some(HANDLE(hmem.0))).is_err() {
            // SetClipboardData 失败，所有权未移交，需要自己释放。
            return false;
        }
        // 成功则所有权移交系统（EmptyClipboard 已清空，HMEM 归系统），**不能**再 free。
        true
    })();
    let _ = CloseClipboard();
    if !ok {
        // 失败路径（OpenClipboard/Empty/Set 任一失败但内存仍在）：自行归还。
        // 注意：EmptyClipboard 成功后 Set 失败时剪贴板已空，此双重失败场景不再回填，
        // 但这是极罕见的系统级失败，而「分配失败即清空」的常见路径已由前置分配消除。
        let _ = GlobalFree(Some(hmem));
    }
    ok
}

/// 把文本写入系统剪贴板（供划词工具条的「复制」动作使用）。
/// 调用方拿到的是**已归一化**的选区文本；成功返回 true。
pub fn set_clipboard_text(text: &str) -> bool {
    unsafe { write_clipboard_text(text, None) }
}

// ---------------------------------------------------------------------------
// 对外主入口
// ---------------------------------------------------------------------------

/// 抓取当前选区文本。
///
/// `poll_timeout_ms` / `max_len` 由调用方给出（team-lead 规定 400 / 20000）。
/// 无法安全恢复原格式时不投递复制。仅恢复仍对应本次读取代次的旧文本。
pub fn capture_selection(poll_timeout_ms: u64, max_len: usize) -> CaptureOutcome {
    // 最底层入口也硬拒绝：未来新增调用方不能绕过配置/UI策略。
    // 必须在绑定目标、读取剪贴板或投递按键之前返回；主动set_clipboard_text不受影响。
    if !crate::release_policy::automatic_copy_allowed(true) {
        return CaptureOutcome::ClipboardProtected;
    }
    let protected = || {
        let _ = take_last_raw();
        CaptureOutcome::ClipboardProtected
    };
    let Some(target) = (unsafe { CaptureTarget::bind() }) else {
        return protected();
    };
    // Wait before taking the snapshot, so changes during modifier release are not stale.
    wait_modifiers_released(300);
    let Some((saved, seq_before)) = (unsafe { restorable_clipboard_snapshot() }) else {
        let _ = take_last_raw();
        return CaptureOutcome::ClipboardProtected;
    };
    // SendInput cannot be atomic with another application's clipboard operation. Narrow
    // that interval and fail closed when a change is already observable; never hold the
    // clipboard open while asking another process to copy (that would block its writer).
    if !target.is_current() || unsafe { GetClipboardSequenceNumber() } != seq_before {
        return protected();
    }
    let sent = send_copy_chord();
    crate::diag::log(&format!("capture: 已投递 Ctrl+Insert（{sent}/4 事件）"));
    let Some(guard) = CopyGuard::new(target.identity, seq_before, sent) else {
        return protected();
    };

    // 3) 轮询序列号，25ms 一次，最多 poll_timeout_ms
    //    ★ 边界增强（2026-09-16）：首轮若超时未变（慢应用 / 剪辑板内容更新滞后），**再合成一次
    //    Ctrl+Insert 并补等半个 timeout**，而非直接判 NoNewCopy。一次为限，避免无限重投。
    let Some(mut seq_changed) = poll_seq_changed(&target, seq_before, poll_timeout_ms) else {
        return protected();
    };
    if !seq_changed {
        if !target.is_current() || unsafe { GetClipboardSequenceNumber() } != seq_before {
            return protected();
        }
        crate::diag::log("capture: 首轮轮询超时未变，补投一次 Ctrl+Insert 并重等");
        if send_copy_chord() != 4 {
            return protected();
        }
        let Some(changed) = poll_seq_changed(&target, seq_before, poll_timeout_ms / 2) else {
            return protected();
        };
        seq_changed = changed;
    }
    if !seq_changed {
        let _ = take_last_raw();
        return CaptureOutcome::NoNewCopy;
    }

    // 4) 读新内容
    let Some((text, source)) = (unsafe { read_clipboard_text(&target, &guard) }) else {
        return protected();
    };
    let read = Some(text);

    crate::diag::log(&format!(
        "capture: seq_changed={} read_len={}",
        seq_changed,
        read.as_deref().map(|s| s.chars().count()).unwrap_or(0)
    ));

    // Restore text only while the clipboard still has the captured sequence, under the
    // same clipboard lock as EmptyClipboard. Rich/non-text snapshots never reach copy.
    let restore = RestoreGuard {
        target: &target,
        copy: &guard,
        source,
    };
    if let Some(prev) = saved.as_deref() {
        if !unsafe { write_clipboard_text(prev, Some(&restore)) } {
            crate::diag::warn("[M2] 剪贴板来源已变化、被占用或恢复失败；拒绝本次捕获");
            return protected();
        }
    } else {
        // An initially empty clipboard has no text snapshot to restore. Still reject a
        // writer/focus change after reading, without clearing that writer's new data.
        let still_current = unsafe {
            if OpenClipboard(None).is_err() {
                false
            } else {
                let valid = restore.permits_current();
                let _ = CloseClipboard();
                valid
            }
        };
        if !still_current {
            return protected();
        }
    }

    // 6) 闸门
    let last = LAST_CAPTURED.lock().ok().and_then(|g| g.clone());
    let outcome = evaluate(read.as_deref(), seq_changed, max_len, last.as_deref());
    let succeeded = matches!(outcome, CaptureOutcome::Ok(_));
    if let CaptureOutcome::Ok(text) = &outcome {
        if let Ok(mut g) = LAST_CAPTURED.lock() {
            *g = Some(text.clone());
        }
    }
    if let Ok(mut g) = LAST_RAW.lock() {
        // ★ 旁路留存原文：仅供工具条「复制」保留排版用。
        //   仅在成功时写入，与 `PENDING_TEXT` 的赋值点（popup.rs 抓词成功分支）严格配对；
        //   失败分支（NoNewCopy / ClipboardNotText / TooShort）清空，避免残留下一次误取。
        *g = if succeeded { read } else { None };
        if let Some(r) = g.as_deref() {
            crate::diag::log(&format!(
                "capture: 留存原文供复制用（{} 字符 / {} 字节）",
                r.chars().count(),
                r.len()
            ));
        }
    }
    outcome
}

// ---------------------------------------------------------------------------
// 单测：只覆盖纯函数部分（normalize / compose / 闸门），不碰 Win32 与剪贴板
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn public_build_direct_capture_is_rejected_before_any_native_capture_work() {
        for (timeout, limit) in [(0, 0), (400, 20_000)] {
            assert_eq!(
                super::capture_selection(timeout, limit),
                super::CaptureOutcome::ClipboardProtected
            );
        }
    }

    fn source_test_target() -> WindowIdentity {
        WindowIdentity {
            hwnd: 1,
            pid: 10,
            thread_id: 100,
        }
    }
    fn source_test_stamp(pid: u32, sequence: u32) -> ClipboardSource {
        ClipboardSource {
            owner: WindowIdentity {
                hwnd: 2,
                pid,
                thread_id: 200,
            },
            sequence,
        }
    }
    fn source_test_guard() -> CopyGuard {
        CopyGuard::new(source_test_target(), 1137, 4).unwrap()
    }
    #[test]
    fn e01_foreign_owner_is_never_adopted_or_restored() {
        let guard = source_test_guard();
        let target = Some(source_test_target());
        let foreign = source_test_stamp(20, 1147);
        assert!(!guard.accepts_capture(target, true, Some(foreign)));
        assert!(!guard.accepts_restore(foreign, target, true, Some(foreign)));
    }
    #[test]
    fn source_requires_stable_target_window_thread_and_live_process() {
        let guard = source_test_guard();
        let stamp = Some(source_test_stamp(10, 1142));
        for changed in [
            WindowIdentity {
                hwnd: 3,
                ..source_test_target()
            },
            WindowIdentity {
                pid: 20,
                ..source_test_target()
            },
            WindowIdentity {
                thread_id: 300,
                ..source_test_target()
            },
        ] {
            assert!(!guard.accepts_capture(Some(changed), true, stamp));
        }
        assert!(!guard.accepts_capture(None, true, stamp));
        assert!(!guard.accepts_capture(Some(source_test_target()), false, stamp));
    }
    #[test]
    fn source_rejects_unknown_owner_invalid_identity_and_unchanged_sequence() {
        let guard = source_test_guard();
        let target = Some(source_test_target());
        assert!(!guard.accepts_capture(target, true, None));
        for source in [
            source_test_stamp(10, 0),
            source_test_stamp(10, 1137),
            source_test_stamp(0, 1142),
            ClipboardSource {
                owner: WindowIdentity {
                    hwnd: 0,
                    ..source_test_stamp(10, 1142).owner
                },
                sequence: 1142,
            },
            ClipboardSource {
                owner: WindowIdentity {
                    thread_id: 0,
                    ..source_test_stamp(10, 1142).owner
                },
                sequence: 1142,
            },
        ] {
            assert!(!guard.accepts_capture(target, true, Some(source)));
        }
    }
    #[test]
    fn source_accepts_target_owned_hidden_window_without_fixed_sequence_increment() {
        let target = Some(source_test_target());
        assert!(source_test_guard().accepts_capture(
            target,
            true,
            Some(source_test_stamp(10, 1142))
        ));
        let wrapping = CopyGuard::new(source_test_target(), u32::MAX, 4).unwrap();
        assert!(wrapping.accepts_capture(target, true, Some(source_test_stamp(10, 1))));
    }
    #[test]
    fn restore_requires_exact_owner_sequence_and_target_snapshot() {
        let guard = source_test_guard();
        let target = Some(source_test_target());
        let old = source_test_stamp(10, 1142);
        assert!(guard.accepts_restore(old, target, true, Some(old)));
        assert!(!guard.accepts_restore(old, target, true, Some(source_test_stamp(10, 1147))));
        let other_owner = ClipboardSource {
            owner: WindowIdentity {
                hwnd: 3,
                ..old.owner
            },
            ..old
        };
        assert!(!guard.accepts_restore(old, target, true, Some(other_owner)));
        assert!(!guard.accepts_restore(old, target, true, None));
        assert!(!guard.accepts_restore(old, target, false, Some(old)));
    }
    #[test]
    fn partial_input_or_invalid_start_context_cannot_authorize_capture() {
        for sent in 0..4 {
            assert!(CopyGuard::new(source_test_target(), 1137, sent).is_none());
        }
        assert!(CopyGuard::new(source_test_target(), 0, 4).is_none());
        assert!(CopyGuard::new(
            WindowIdentity {
                pid: 0,
                ..source_test_target()
            },
            1137,
            4
        )
        .is_none());
    }
    #[test]
    fn foreign_processes_cannot_gain_restore_permission_from_current_sequence() {
        let guard = source_test_guard();
        let target = Some(source_test_target());
        for pid in 1..1024 {
            if pid == 10 {
                continue;
            }
            let stamp = source_test_stamp(pid, 1147);
            assert!(!guard.accepts_restore(stamp, target, true, Some(stamp)));
        }
    }
    #[test]
    fn restoration_requires_the_same_sequence_even_for_identical_text() {
        assert!(clipboard_restore_allowed(Some(10), 10));
        assert!(!clipboard_restore_allowed(Some(10), 11));
        assert!(!clipboard_restore_allowed(Some(0), 0));
        assert!(!clipboard_restore_allowed(Some(10), 0));
        // An explicit Copy action intentionally replaces clipboard data.
        assert!(clipboard_restore_allowed(None, 11));
    }

    #[test]
    fn text_family_keeps_unicode_payload() {
        for formats in [vec![13], vec![13, 1, 7, 16]] {
            assert!(clipboard_capture_allowed(
                &formats,
                Some("中文🙂\n  code"),
                true
            ));
        }
        assert!(clipboard_capture_allowed(&[], None, true));
    }
    #[test]
    fn images_html_and_private_formats_must_not_trigger_copy() {
        for formats in [
            vec![2],
            vec![8],
            vec![17],
            vec![13, 49152],
            vec![13, 8],
            vec![13, 15],
            vec![0x201],
        ] {
            assert!(
                !clipboard_capture_allowed(&formats, Some("text"), true),
                "{formats:?}"
            );
        }
    }
    #[test]
    fn busy_incomplete_or_unreadable_snapshot_is_not_restorable() {
        assert!(!clipboard_capture_allowed(&[13], None, true));
        assert!(!clipboard_capture_allowed(&[], None, false));
        assert!(!clipboard_capture_allowed(&[13], Some("x"), false));
        assert!(!clipboard_capture_allowed(&[16], None, true));
    }

    #[test]
    fn clipboard_requires_a_terminator_within_the_bounded_allocation() {
        assert_eq!(super::decode_clipboard_units(&[]), None);
        assert_eq!(super::decode_clipboard_units(&[65]), None);
        assert_eq!(super::decode_clipboard_units(&[65, 66]), None);
    }

    #[test]
    fn clipboard_preserves_unicode_and_newlines_before_the_terminator() {
        let original = "中文🙂\n  code\t";
        let mut units: Vec<u16> = original.encode_utf16().collect();
        units.extend([0, 88]);
        assert_eq!(
            super::decode_clipboard_units(&units).as_deref(),
            Some(original)
        );
        assert_eq!(super::decode_clipboard_units(&[0]), Some(String::new()));
    }

    use super::*;

    #[test]
    fn normalize_collapses_all_whitespace() {
        assert_eq!(normalize_text("  a \t\n b\r\nc  "), "a b c");
        assert_eq!(normalize_text(""), "");
        assert_eq!(normalize_text("   "), "");
    }

    #[test]
    fn normalize_treats_nbsp_and_bom_as_whitespace() {
        // \u{00a0} 与 \u{3000} 两边都算空白；\u{feff} 是 JS \s 有、Rust is_whitespace 没有的差异点
        assert_eq!(normalize_text("a\u{00a0}b"), "a b");
        assert_eq!(normalize_text("a\u{3000}b"), "a b");
        assert_eq!(normalize_text("a\u{feff}b"), "a b");
    }

    #[test]
    fn normalize_keeps_inner_punctuation_untouched() {
        assert_eq!(normalize_text("  你好，  世界！ "), "你好， 世界！");
    }

    #[test]
    fn compose_custom_prepends_instruction() {
        assert_eq!(
            compose_custom("  帮我提取关键词： ", "  你好   世界 "),
            "帮我提取关键词：\n\n你好 世界"
        );
    }

    #[test]
    fn compose_custom_honours_placeholder() {
        // 含 {text} 时原样替换，其余排版由用户掌控
        assert_eq!(
            compose_custom("把下面这段改写成文言文：\n{text}\n只输出结果", " hello "),
            "把下面这段改写成文言文：\nhello\n只输出结果"
        );
    }

    #[test]
    fn compose_custom_empty_edges() {
        // 空指令 + 有正文 → 仅正文
        assert_eq!(compose_custom("   ", " abc "), "abc");
        // 有指令 + 空正文 → 仅指令
        assert_eq!(compose_custom(" 总结一下 ", "   "), "总结一下");
        // 双空 → 空串（调用方须先用闸门挡住）
        assert_eq!(compose_custom("  ", "  "), "");
    }

    #[test]
    fn gate_sequence_unchanged_wins_over_not_text() {
        // 什么都没选中时，不该误报「剪贴板没有文本」
        assert_eq!(
            evaluate(None, false, MAX_CHARS, None),
            CaptureOutcome::NoNewCopy
        );
        assert_eq!(
            evaluate(Some("hi"), false, MAX_CHARS, None),
            CaptureOutcome::NoNewCopy
        );
    }

    #[test]
    fn gate_rejects_non_text() {
        assert_eq!(
            evaluate(None, true, MAX_CHARS, None),
            CaptureOutcome::ClipboardNotText
        );
    }

    #[test]
    fn gate_rejects_too_short() {
        assert_eq!(
            evaluate(Some(" a "), true, MAX_CHARS, None),
            CaptureOutcome::TooShort
        );
        // 折叠空白后只剩 1 个字
        assert_eq!(
            evaluate(Some("  x  "), true, MAX_CHARS, None),
            CaptureOutcome::TooShort
        );
        // 恰好 2 个字通过
        assert_eq!(
            evaluate(Some(" ab "), true, MAX_CHARS, None),
            CaptureOutcome::Ok("ab".into())
        );
        // 长度按字符数而非字节数：2 个汉字通过
        assert_eq!(
            evaluate(Some("你好"), true, MAX_CHARS, None),
            CaptureOutcome::Ok("你好".into())
        );
    }

    #[test]
    fn gate_adjusts_too_long() {
        // 超长：截断到上限成功返回，而非整段废弃（拖动大段文本不应划词失败）。
        let text = "字".repeat(20_001);
        let want = "字".repeat(MAX_CHARS);
        assert_eq!(
            evaluate(Some(&text), true, MAX_CHARS, None),
            CaptureOutcome::Ok(want)
        );
        let ok = "字".repeat(20_000);
        assert_eq!(
            evaluate(Some(&ok), true, MAX_CHARS, None),
            CaptureOutcome::Ok(ok)
        );
    }

    #[test]
    fn gate_allows_same_as_last() {
        // SameAsLast 不再阻断：注入/回流失败后，相同内容必须能重发（DeepSeek 侧无幂等去重）。
        assert_eq!(
            evaluate(Some("hello"), true, MAX_CHARS, Some("hello")),
            CaptureOutcome::Ok("hello".into())
        );
        // 归一化后再比较：空白差异不算变化，同样照常返回 Ok
        assert_eq!(
            evaluate(Some("  hello\n"), true, MAX_CHARS, Some("hello")),
            CaptureOutcome::Ok("hello".into())
        );
    }

    #[test]
    fn gate_returns_normalized_text() {
        assert_eq!(
            evaluate(Some("  你好  世界  "), true, MAX_CHARS, None),
            CaptureOutcome::Ok("你好 世界".into())
        );
    }

    #[test]
    fn user_message_only_for_failures() {
        assert!(CaptureOutcome::Ok("x".into()).user_message().is_none());
        for bad in [
            CaptureOutcome::NoNewCopy,
            CaptureOutcome::ClipboardNotText,
            CaptureOutcome::ClipboardProtected,
            CaptureOutcome::TooShort,
        ] {
            assert!(bad.user_message().is_some(), "应该给出提示: {bad:?}");
        }
    }

    // --- 旁路原文缓存（工具条「复制」保留排版用） ---------------------------
    //
    // ⚠️ `LAST_RAW` 是进程级 static，多个测试并行跑会互相干扰。
    //    故 set/get 语义与「保留空白」两组合并为**同一个** `#[test]` 顺序执行。

    #[test]
    fn last_raw_semantics_and_whitespace_preservation() {
        // 局部辅助：清空缓存（生产代码不再需要该操作，故测试内联，避免引入 dead code）。
        fn reset() {
            if let Ok(mut g) = LAST_RAW.lock() {
                *g = None;
            }
        }

        // ---------- (1) take 是消费语义 ----------
        reset();
        assert_eq!(take_last_raw(), None, "清空后应取不到");

        set_last_raw("a\nb\n".into());
        assert_eq!(take_last_raw().as_deref(), Some("a\nb\n"), "首次应取到");
        assert_eq!(take_last_raw(), None, "take 是消费语义，第二次应为空");

        // set 覆盖写：后写覆盖先写（保证缓存始终对应最近一次抓词）
        set_last_raw("old".into());
        set_last_raw("new".into());
        assert_eq!(take_last_raw().as_deref(), Some("new"), "应取最新写入值");

        // ---------- (2) 原文的空白必须原样留存（本机制存在的唯一理由） ----------
        let raw = "第一行\n第二行\n\n- 列表项\n\t缩进";
        let normalized = normalize_text(raw);
        assert!(!normalized.contains('\n'), "归一化必然吃掉换行");
        assert_eq!(normalized, "第一行 第二行 - 列表项 缩进");

        set_last_raw(raw.to_string());
        let got = take_last_raw().expect("应取到原文");
        assert_eq!(got, raw, "原文须逐字节保留");
        // 4 个 `\n` ⇒ `lines()` 切出 5 段（含空行造成的空段），证明空行与换行未被折叠。
        assert_eq!(got.lines().count(), 5, "换行结构完整");
        assert_eq!(got.matches('\n').count(), 4, "换行符一个不少");
        assert!(got.contains('\t'), "制表符须保留");

        // 测试收尾：不留残余，避免影响其他用例
        reset();
    }

    #[test]
    fn raw_matches_pending_guards_against_stale_cache() {
        // ① 正常：原文归一化后 == pending
        assert!(raw_matches_pending(
            "第一行\n第二行\n\n- 列表项",
            "第一行 第二行 - 列表项"
        ));

        // ② 超长：pending 是 normalize(raw) 截断到 MAX_CHARS 的前缀（必须判真！）
        let long_raw = "字".repeat(MAX_CHARS * 2);
        let pending: String = long_raw.chars().take(MAX_CHARS).collect();
        assert!(
            raw_matches_pending(&long_raw, &pending),
            "超长截断场景必须判真，否则用户「不截断」的诉求会被自校验破坏"
        );

        // ③ 错配：这是**上一次**划词的原文，不得判真
        assert!(
            !raw_matches_pending("上一条的内容\n第二行", "本次完全不同的一段话"),
            "陈旧缓存必须被挡住，否则会复制出上一条文本"
        );
        // 更刁的错配：内容同前缀但后续不同 → 前缀判据会放行，属可接受的宽松
        assert!(raw_matches_pending("ABC\n后续不同", "ABC"));
        // 而完全无关则必然挡住
        assert!(!raw_matches_pending("XYZ", "ABC"));

        // ④ 空 pending：不得判真（调用方本不该走到，防御性返回 false）
        assert!(!raw_matches_pending("任意", ""));
    }
}
