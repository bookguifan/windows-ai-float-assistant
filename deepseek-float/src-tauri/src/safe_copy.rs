//! Session-consented, one-shot clipboard fallback. The legacy capture_selection
//! entry point stays locked off. Compatibility mode is explicitly consented.
use crate::selection::{
    formats_allowed, restore_allowed, Consent, Request, Stamp, Target, POLICY_VERSION,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, WebviewWindow};
use windows::core::w;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, GlobalFree, SetLastError, ERROR_SUCCESS, HANDLE, HGLOBAL, HWND,
};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, GetThreadDesktop, GetUserObjectInformationW, OpenInputDesktop,
    DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS, UOI_NAME,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, GetCurrentThreadId, GetExitCodeProcess, OpenProcess,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongPtrW,
    GetWindowThreadProcessId, IsWindow, IsWindowUnicode, MessageBoxW, GA_ROOT, GA_ROOTOWNER,
    GUITHREADINFO, GWL_STYLE, IDOK, MB_DEFBUTTON2, MB_ICONWARNING, MB_OKCANCEL,
};
static CONSENT: Mutex<Consent> = Mutex::new(Consent::empty());
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Off,
    #[default]
    Safe,
    Compatibility,
}
static MODE: Mutex<Mode> = Mutex::new(Mode::Safe);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CopyShortcut {
    #[default]
    CtrlInsert,
    CtrlC,
}
impl CopyShortcut {
    fn key(self) -> VIRTUAL_KEY {
        match self {
            Self::CtrlInsert => VK_INSERT,
            Self::CtrlC => VK_C,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::CtrlInsert => "Ctrl+Insert",
            Self::CtrlC => "Ctrl+C",
        }
    }
}
static SHORTCUT: Mutex<CopyShortcut> = Mutex::new(CopyShortcut::CtrlInsert);
static LAST_ERROR: Mutex<Option<&'static str>> = Mutex::new(None);
static TRANSACTION: Mutex<()> = Mutex::new(());
static PROMPTING: AtomicBool = AtomicBool::new(false);
#[derive(serde::Serialize)]
pub struct State {
    pub smart_enabled: bool,
    pub shortcut: CopyShortcut,
    pub mode: Mode,
    pub last_error: Option<&'static str>,
    pub enabled: bool,
    pub available: bool,
    pub policy_version: u32,
    pub session_only: bool,
}
pub fn state() -> State {
    let smart_enabled = crate::config::load().privileged_capture;
    let consent = CONSENT.lock().unwrap_or_else(|p| p.into_inner());
    let enabled = copy_authorized(smart_enabled, &consent, consent.epoch());
    let mode = *MODE.lock().unwrap_or_else(|p| p.into_inner());
    let shortcut = *SHORTCUT.lock().unwrap_or_else(|p| p.into_inner());
    drop(consent);
    let last_error = *LAST_ERROR.lock().unwrap_or_else(|p| p.into_inner());
    State {
        smart_enabled,
        shortcut,
        mode,
        last_error,
        enabled,
        available: crate::release_policy::safe_copy_available(),
        policy_version: POLICY_VERSION,
        session_only: true,
    }
}
fn revoke_epoch() -> u64 {
    let epoch = {
        let mut consent = CONSENT.lock().unwrap_or_else(|p| p.into_inner());
        consent.revoke();
        consent.epoch()
    };
    *LAST_ERROR.lock().unwrap_or_else(|p| p.into_inner()) = None;
    crate::selection_runtime::invalidate();
    epoch
}
pub fn revoke() {
    revoke_epoch();
}
pub fn set_passive_mode(mode: Mode) -> Result<State, String> {
    if mode == Mode::Compatibility {
        return Err("兼容模式必须先确认原生授权".into());
    }
    revoke();
    *MODE.lock().unwrap_or_else(|p| p.into_inner()) = mode;
    Ok(state())
}
fn epoch() -> Option<u64> {
    if !crate::release_policy::safe_copy_available() {
        return None;
    }
    let smart_enabled = crate::config::load().privileged_capture;
    let c = CONSENT.lock().ok()?;
    copy_authorized(smart_enabled, &c, c.epoch()).then_some(c.epoch())
}
fn copy_authorized(smart_enabled: bool, consent: &Consent, epoch: u64) -> bool {
    crate::release_policy::safe_copy_available() && smart_enabled && consent.matches(epoch)
}
fn granted(e: u64) -> bool {
    CONSENT.lock().map(|c| c.matches(e)).unwrap_or(false)
}
fn local_settings(label: &str, url: &tauri::Url, visible: bool, focused: bool) -> bool {
    if label != "settings" || !visible || !focused {
        return false;
    }
    let app = url.scheme() == "tauri" && url.host_str() == Some("localhost")
        || matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost");
    let dev = cfg!(debug_assertions)
        && url.scheme() == "http"
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
        && url.port() == Some(5173);
    (app || dev) && url.path() == "/poc/settings.html"
}
pub(crate) fn trusted(window: &WebviewWindow) -> bool {
    window
        .url()
        .map(|u| {
            local_settings(
                window.label(),
                &u,
                window.is_visible().unwrap_or(false),
                window.is_focused().unwrap_or(false),
            )
        })
        .unwrap_or(false)
}
/// The IPC is only a request: only the real native modal's OK can grant this process.
pub fn request_authorization(
    window: WebviewWindow,
    mode: Mode,
    shortcut: CopyShortcut,
) -> Result<State, String> {
    if !crate::release_policy::safe_copy_available() {
        return Err("此构建未提供自动剪贴板回退".into());
    }
    if !trusted(&window) {
        return Err("只能在聚焦的本地设置页请求授权".into());
    }
    if mode != Mode::Compatibility {
        return set_passive_mode(mode);
    }
    if !crate::config::load().privileged_capture {
        return Err("请先开启智能划词增强".into());
    }
    if PROMPTING.swap(true, Ordering::AcqRel) {
        return Err("授权确认正在进行".into());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            PROMPTING.store(false, Ordering::Release);
        }
    }
    let _reset = Reset;
    // Suspend the old grant while choosing a new shortcut or showing a new modal.
    let start_epoch = revoke_epoch();
    let hwnd = window.hwnd().map_err(|_| "设置窗口不可用")?;
    let message: Vec<u16> = format!("自动复制回退会在智能划词失败时模拟复制快捷键，并尝试恢复原剪贴板。操作可能短暂修改 Windows 剪贴板；特殊格式、异步复制或其他程序并发修改时，可能无法完整恢复。请避免用于敏感内容。是否仅在本次运行中开启？\n\n本次仅发送一次 {}，不会自动尝试另一种快捷键。原剪贴板仅支持空内容及 Unicode/ANSI/OEM 文本和区域格式；HTML、RTF、图片等会在发送前拒绝。", shortcut.label()).encode_utf16().chain(Some(0)).collect();
    let answer = unsafe {
        MessageBoxW(
            Some(HWND(hwnd.0)),
            windows::core::PCWSTR(message.as_ptr()),
            w!("授权自动剪贴板划词回退"),
            MB_OKCANCEL | MB_ICONWARNING | MB_DEFBUTTON2,
        )
    };
    let still_trusted = trusted(&window);
    // Same lock order as config writes: data -> config -> consent. A master-off
    // change cannot race between the final check and granting the new epoch.
    let data = crate::data_store::read_guard();
    let smart_enabled = crate::config::load_under_data_lock().privileged_capture;
    let mut consent = CONSENT.lock().unwrap_or_else(|p| p.into_inner());
    if answer == IDOK && consent.epoch() == start_epoch && still_trusted && smart_enabled {
        *SHORTCUT.lock().unwrap_or_else(|p| p.into_inner()) = shortcut;
        consent.grant(POLICY_VERSION);
        *MODE.lock().unwrap_or_else(|p| p.into_inner()) = Mode::Compatibility;
    } else {
        consent.revoke();
        *MODE.lock().unwrap_or_else(|p| p.into_inner()) = Mode::Safe;
    }
    drop(consent);
    drop(data);
    Ok(state())
}
struct Clipboard;
impl Clipboard {
    unsafe fn open_until(
        owner: Option<HWND>,
        deadline: Instant,
        busy_code: &'static str,
    ) -> Result<Self, &'static str> {
        loop {
            if OpenClipboard(owner).is_ok() {
                return Ok(Self);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(busy_code);
            }
            std::thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}
struct Block(HGLOBAL);
impl Drop for Block {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = GlobalFree(Some(self.0));
            }
        }
    }
}
struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
unsafe fn stamp() -> Option<Stamp> {
    let hwnd = GetClipboardOwner().ok()?;
    let mut pid = 0;
    let thread = GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if hwnd.is_invalid() || pid == 0 || thread == 0 {
        return None;
    }
    let mut format_count = 0;
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
        format_count += 1;
        if format_count > 64 {
            return None;
        }
        previous = format;
    }
    let sequence = GetClipboardSequenceNumber();
    // Preserve an identifiable writer's empty publication phase for wait(). The
    // observer and final source gate still forbid reading/restoring empty data.
    if sequence == 0 {
        return None;
    }
    Some(Stamp {
        root_hwnd: GetAncestor(hwnd, GA_ROOT).0 as isize,
        owner: Target {
            hwnd: hwnd.0 as isize,
            pid,
            thread,
        },
        sequence,
        format_count,
    })
}
struct Snapshot {
    sequence: u32,
    blocks: Vec<(u32, Block)>,
}
unsafe fn snapshot() -> Result<Snapshot, &'static str> {
    if !crate::release_policy::safe_copy_available() {
        return Err("COPY_BUILD_DISABLED");
    }
    let start = Instant::now();
    let _lock = Clipboard::open_until(
        None,
        start + Duration::from_millis(100),
        "COPY_CLIPBOARD_BUSY",
    )?;
    let sequence = GetClipboardSequenceNumber();
    let owner = GetClipboardOwner().ok();
    let mut formats = Vec::new();
    let mut previous = 0;
    loop {
        SetLastError(ERROR_SUCCESS);
        let format = EnumClipboardFormats(previous);
        if format == 0 {
            if GetLastError() != ERROR_SUCCESS {
                return Err("COPY_SNAPSHOT_FAILED");
            }
            break;
        }
        formats.push(format);
        if formats.len() > 4 {
            return Err("COPY_FORMAT_UNSAFE");
        }
        previous = format;
    }
    if sequence == 0 || !formats_allowed(&formats) {
        return Err("COPY_FORMAT_UNSAFE");
    }
    let mut total = 0;
    let mut blocks = Vec::new();
    for format in formats {
        let data = GetClipboardData(format).map_err(|_| "COPY_SNAPSHOT_FAILED")?;
        let h = HGLOBAL(data.0);
        let size = GlobalSize(h);
        total += size;
        if size == 0
            || total > 1_048_576
            || (format == 13 && size % 2 != 0)
            || (format == 16 && size != 4)
        {
            return Err("COPY_FORMAT_UNSAFE");
        }
        let ptr = GlobalLock(h);
        if ptr.is_null() {
            return Err("COPY_SNAPSHOT_FAILED");
        }
        let bytes = std::slice::from_raw_parts(ptr as *const u8, size).to_vec();
        let _ = GlobalUnlock(h);
        // Validate termination; never promise a partial or lossy snapshot of text variants.
        if (format == 13 && !bytes.chunks_exact(2).any(|u| u == [0, 0]))
            || (matches!(format, 1 | 7) && !bytes.contains(&0))
        {
            return Err("COPY_SNAPSHOT_FAILED");
        }
        let copy = Block(GlobalAlloc(GMEM_MOVEABLE, size).map_err(|_| "COPY_SNAPSHOT_FAILED")?);
        let p = GlobalLock(copy.0);
        if p.is_null() {
            return Err("COPY_SNAPSHOT_FAILED");
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p as *mut u8, size);
        let _ = GlobalUnlock(copy.0);
        blocks.push((format, copy));
    }
    // Rendering/synthesis changing the sequence or owner invalidates the entire snapshot.
    if GetClipboardSequenceNumber() != sequence
        || GetClipboardOwner().ok() != owner
        || start.elapsed() > Duration::from_millis(100)
    {
        return Err("COPY_SNAPSHOT_UNSTABLE");
    }
    Ok(Snapshot { sequence, blocks })
}
fn current(r: &Request, e: u64, p: &Process) -> bool {
    let mut code = 0;
    crate::release_policy::safe_copy_allowed(
        granted(e),
        crate::selection_runtime::is_native_target(r.target),
        crate::selection_runtime::valid(r),
    ) && unsafe { GetExitCodeProcess(p.0, &mut code).is_ok() }
        && code == 259
}
fn key_input(v: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: v,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
fn copy_inputs(
    shortcut: CopyShortcut,
    authorized: bool,
    mut pressed: impl FnMut(VIRTUAL_KEY) -> bool,
) -> Option<[INPUT; 4]> {
    if !authorized
        || [
            VK_CONTROL,
            VK_MENU,
            VK_SHIFT,
            VK_LWIN,
            VK_RWIN,
            shortcut.key(),
        ]
        .into_iter()
        .any(&mut pressed)
    {
        return None;
    }
    Some([
        key_input(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
        key_input(shortcut.key(), KEYBD_EVENT_FLAGS(0)),
        key_input(shortcut.key(), KEYEVENTF_KEYUP),
        key_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ])
}
fn release_inputs(shortcut: CopyShortcut, sent: u32) -> Vec<INPUT> {
    match sent {
        1 | 3 => vec![key_input(VK_CONTROL, KEYEVENTF_KEYUP)],
        2 => vec![
            key_input(shortcut.key(), KEYEVENTF_KEYUP),
            key_input(VK_CONTROL, KEYEVENTF_KEYUP),
        ],
        _ => Vec::new(),
    }
}
unsafe fn copy_chord(shortcut: CopyShortcut, authorized: bool) -> crate::copy_engine::SendStatus {
    if !crate::release_policy::safe_copy_available() {
        return crate::copy_engine::SendStatus::Rejected;
    }
    let Some(inputs) = copy_inputs(shortcut, authorized, |v| {
        GetAsyncKeyState(i32::from(v.0)) < 0
    }) else {
        return crate::copy_engine::SendStatus::Rejected;
    };
    // Exactly one chosen chord; cleanup can only release keys, never try another copy.
    let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    if sent != 4 && sent > 0 {
        let releases = release_inputs(shortcut, sent);
        let _ = SendInput(&releases, std::mem::size_of::<INPUT>() as i32);
    }
    match sent {
        0 | 1 => crate::copy_engine::SendStatus::Rejected,
        4 => crate::copy_engine::SendStatus::Sent,
        _ => crate::copy_engine::SendStatus::Partial,
    }
}
pub fn capture(r: &Request, app: &AppHandle) -> Result<String, &'static str> {
    let outcome = capture_inner(r, app);
    let reason = outcome.as_ref().err().map(|code| copy_failure_reason(code));
    *LAST_ERROR.lock().unwrap_or_else(|p| p.into_inner()) = reason;
    if let (Some(settings), Ok(status)) = (
        app.get_webview_window("settings"),
        serde_json::to_string(&state()),
    ) {
        let _ = settings.eval(format!("window.__dsfSafeCopyStatus?.({status});"));
    }
    crate::diag::log(&format!(
        "selection status={} reason={} elapsed_ms={}",
        outcome
            .as_ref()
            .map(|_| "COPY_SUCCESS")
            .unwrap_or_else(|e| e),
        reason.unwrap_or(""),
        crate::selection_runtime::now_ms().saturating_sub(r.created_ms)
    ));
    outcome
}
fn copy_failure_reason(code: &str) -> &'static str {
    match code {
        "COPY_UNAUTHORIZED" => "尚未在设置页确认本次运行的剪贴板回退",
        "COPY_FORMAT_UNSAFE" => "原剪贴板含 HTML、RTF、图片等不支持格式，或无法完整保存，未发送快捷键。确认原内容无需保留后，可在 Windows 记事本中重新复制纯文本再试；直接复制网页或 PDF 可能再次带入复杂格式",
        "COPY_CLIPBOARD_BUSY" => "复制前无法取得剪贴板快照，未发送快捷键",
        "COPY_CAPTURE_BUSY" => "目标已触发剪贴板变化，但剪贴板持续被占用，无法确认来源",
        "COPY_READ_BUSY" => "复制结果读取时剪贴板持续被占用，正在尝试安全恢复",
        "COPY_RESTORE_BUSY" => "剪贴板持续被占用，无法完成原内容恢复",
        "COPY_TARGET_PROTECTED" => "目标属于受保护的控制台窗口",
        "COPY_PROTECTED_OR_UNVERIFIED_CONTROL" => "无法确认目标焦点或控件受保护，未发送快捷键",
        "COPY_TIMEOUT" => "未能及时验证目标应用的复制结果",
        "COPY_SNAPSHOT_UNSTABLE" => "剪贴板快照超过 100 毫秒检查时限或读取期间内容变化，未发送快捷键；该检查不能中断仍未返回的延迟渲染调用",
        "COPY_SNAPSHOT_FAILED" => "无法完整读取原剪贴板，未发送快捷键",
        "COPY_READ_FAILED" => "无法读取目标应用的复制结果，正在尝试安全恢复",
        "COPY_SELECTION_MISMATCH" => "复制结果与当前选区不一致，未采纳该文本",
        "COPY_SELECTION_CHANGED" => "读取期间选区发生变化，未采纳该文本",
        "COPY_RESTORE_SKIPPED" => "复制结果来源不明、未稳定或事务期间发生变化；为避免覆盖新内容已跳过恢复，剪贴板中可能仍有本次复制内容",
        "COPY_RESTORE_FAILED" => "原剪贴板恢复未完整完成，剪贴板中可能仍有本次复制内容",
        "COPY_INPUT_FAILED" => "Windows 未完整接收所选复制快捷键输入",
        "COPY_SOURCE_UNVERIFIED" => "剪贴板写入者与目标无可信关联或复制结果不稳定，未采纳且未覆盖该内容",
        "COPY_DESKTOP_PROTECTED" => "当前不是可验证的普通交互桌面，未发送快捷键",
        "COPY_SENSITIVE_CONTROL" => "识别到密码框或敏感输入目标，未发送快捷键",
        _ => "安全检查未通过或目标状态已变化",
    }
}
struct NativeIo<'a> {
    request: &'a Request,
    epoch: u64,
    shortcut: CopyShortcut,
    process: Process,
    app: &'a AppHandle,
    focus: Target,
    compatibility: bool,
    prepared_sequence: u32,
}
impl crate::copy_engine::ClipboardIo for NativeIo<'_> {
    type Snapshot = Snapshot;
    fn compatibility(&self) -> bool {
        self.compatibility
    }
    fn related_writer(&self, stamp: Stamp) -> bool {
        writer_related(self.request.target, self.focus, stamp)
    }
    fn valid(&self) -> bool {
        current(self.request, self.epoch, &self.process)
            && if self.compatibility {
                compatibility_focus(self.request.target).ok() == Some(self.focus)
            } else {
                native_focus(self.request.target) == Some(self.focus.hwnd)
            }
    }
    fn selection_proof(&mut self) -> Result<crate::copy_engine::SelectionProof, &'static str> {
        crate::native_copy_proof::capture(HWND(self.focus.hwnd as *mut _), || self.valid())
    }
    fn prepare(&mut self) -> Result<(Snapshot, u32), &'static str> {
        unsafe {
            let s = snapshot()?;
            let sequence = s.sequence;
            self.prepared_sequence = sequence;
            Ok((s, sequence))
        }
    }
    fn sequence(&self) -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }
    fn send_once(&mut self) -> crate::copy_engine::SendStatus {
        if !self.valid()
            || (self.compatibility && !crate::uia::compatibility_target_safe(self.request))
            || !self.valid()
            || self.sequence() != self.prepared_sequence
        {
            return crate::copy_engine::SendStatus::Rejected;
        }
        // Serialize the final permission check + SendInput with config writes and
        // revocation. A queued request cannot slip through after either switch is off.
        let _data = crate::data_store::read_guard();
        let smart_enabled = crate::config::load_under_data_lock().privileged_capture;
        let consent = CONSENT.lock().unwrap_or_else(|p| p.into_inner());
        let target_current = self.request.valid(
            crate::selection_runtime::target(),
            crate::selection_runtime::generation(),
            crate::selection_runtime::now_ms(),
        ) && crate::selection_runtime::is_native_target(self.request.target)
            && if self.compatibility {
                compatibility_focus(self.request.target).ok() == Some(self.focus)
            } else {
                native_focus(self.request.target) == Some(self.focus.hwnd)
            };
        let status = unsafe {
            copy_chord(
                self.shortcut,
                target_current
                    && copy_authorized(smart_enabled, &consent, self.epoch)
                    && self.sequence() == self.prepared_sequence,
            )
        };
        crate::diag::log(&format!(
            "selection copy_shortcut={} input_status={status:?} request={}",
            self.shortcut.label(),
            self.request.generation
        ));
        status
    }
    fn wait(&mut self, before: u32) -> Result<Stamp, crate::copy_engine::WaitFailure> {
        let deadline = Instant::now() + Duration::from_millis(400);
        let started = Instant::now();
        let mut observation = crate::copy_engine::CopyObservation::default();
        let mut changed = false;
        unsafe {
            loop {
                if GetClipboardSequenceNumber() != before {
                    changed = true;
                    let _lock = Clipboard::open_until(None, deadline, "COPY_CAPTURE_BUSY")
                        .map_err(crate::copy_engine::WaitFailure::changed)?;
                    let observed = stamp();
                    if self.compatibility {
                        let settled = observation
                            .observe(
                                before,
                                observed,
                                observed.is_some_and(|s| self.related_writer(s)),
                                started.elapsed().as_millis() as u64,
                            )
                            .map_err(crate::copy_engine::WaitFailure::changed)?;
                        if IsClipboardFormatAvailable(13).is_ok() {
                            if let Some(settled) = settled {
                                return Ok(settled);
                            }
                        }
                    } else if let Some(observed) = observed.filter(|stamp| stamp.format_count != 0)
                    {
                        return Ok(observed);
                    }
                }
                if !self.valid() {
                    if let Some(settled) = observation.settled() {
                        return Ok(settled);
                    }
                    return Err(if changed {
                        crate::copy_engine::WaitFailure::changed("COPY_CONTEXT_CHANGED")
                    } else {
                        crate::copy_engine::WaitFailure::unchanged("COPY_CONTEXT_CHANGED")
                    });
                }
                if Instant::now() >= deadline {
                    // A related stable non-text result still needs cleanup; read()
                    // will fail and the engine restores the exact matching stamp.
                    if let Some(settled) = observation.settled() {
                        return Ok(settled);
                    }
                    return Err(crate::copy_engine::WaitFailure {
                        code: "COPY_TIMEOUT",
                        clipboard_changed: changed,
                    });
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    fn recover_after_wait_failure(
        &mut self,
        snapshot: Snapshot,
        before: u32,
        target: Target,
        focused_control: isize,
    ) -> Result<(), &'static str> {
        unsafe {
            let lock = Clipboard::open_until(
                None,
                Instant::now() + Duration::from_millis(150),
                "COPY_RESTORE_BUSY",
            )?;
            let observed = stamp().ok_or("COPY_RESTORE_SKIPPED")?;
            // An observed foreign/unstable writer must never be cleaned up as ours.
            // Generic mode only recovers the exact stabilized candidate recorded by wait.
            if self.compatibility
                || !crate::selection::source_allowed(target, before, focused_control, observed)
            {
                return Err("COPY_RESTORE_SKIPPED");
            }
            drop(lock);
            self.restore(snapshot, observed)
        }
    }
    fn read(&mut self, expected: Stamp) -> Result<String, &'static str> {
        unsafe {
            let _lock = Clipboard::open_until(
                None,
                Instant::now() + Duration::from_millis(150),
                "COPY_READ_BUSY",
            )?;
            if !self.valid() || stamp() != Some(expected) {
                return Err("COPY_SOURCE_UNVERIFIED");
            }
            let text = crate::capture::read_open_clipboard_text().ok_or("COPY_READ_FAILED")?;
            if !self.valid() || stamp() != Some(expected) {
                return Err("COPY_SOURCE_UNVERIFIED");
            }
            Ok(text)
        }
    }
    fn restore(&mut self, mut saved: Snapshot, expected: Stamp) -> Result<(), &'static str> {
        if !crate::release_policy::safe_copy_available() {
            return Err("COPY_BUILD_DISABLED");
        }
        unsafe {
            let owner = self
                .app
                .get_webview_window("ball")
                .and_then(|w| w.hwnd().ok())
                .ok_or("COPY_RESTORE_SKIPPED")?;
            let _lock = Clipboard::open_until(
                Some(HWND(owner.0)),
                Instant::now() + Duration::from_millis(150),
                "COPY_RESTORE_BUSY",
            )?;
            let observed = stamp().ok_or("COPY_RESTORE_SKIPPED")?;
            if !restore_allowed(expected, observed) {
                return Err("COPY_RESTORE_SKIPPED");
            }
            // Cleanup may outlive a focus/consent change only while the exact transaction
            // stamp is current. The clipboard lock prevents a race before EmptyClipboard;
            // publishing several formats is still non-atomic and may fail part-way.
            EmptyClipboard().map_err(|_| "COPY_RESTORE_FAILED")?;
            let mut failed = false;
            for (format, block) in &mut saved.blocks {
                if SetClipboardData(*format, Some(HANDLE(block.0 .0))).is_ok() {
                    block.0 = HGLOBAL::default();
                } else {
                    failed = true;
                }
            }
            if failed {
                Err("COPY_RESTORE_FAILED")
            } else {
                Ok(())
            }
        }
    }
}
/// Compatibility keeps a stable native focus identity, including a cross-process
/// child/owned window, without assuming Edit messages or a shared PID/TID.
fn compatibility_focus(target: Target) -> Result<Target, &'static str> {
    unsafe {
        if !ordinary_input_desktop() {
            return Err("COPY_DESKTOP_PROTECTED");
        }
        let root = HWND(target.hwnd as *mut _);
        if GetForegroundWindow() != root || !IsWindow(Some(root)).as_bool() {
            return Err("COPY_CONTEXT_CHANGED");
        }
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(target.thread, &mut info)
            .map_err(|_| "COPY_PROTECTED_OR_UNVERIFIED_CONTROL")?;
        let focus = info.hwndFocus;
        let mut pid = 0;
        let thread = GetWindowThreadProcessId(focus, Some(&mut pid));
        let related =
            GetAncestor(focus, GA_ROOT) == root || GetAncestor(focus, GA_ROOTOWNER) == root;
        if !crate::selection::compatible_focus_allowed(
            IsWindow(Some(focus)).as_bool(),
            related,
            pid,
            thread,
            GetCurrentProcessId(),
            info.flags.0 & 0x1e != 0,
        ) {
            return Err("COPY_PROTECTED_OR_UNVERIFIED_CONTROL");
        }
        let mut name = [0u16; 256];
        let n = GetClassNameW(focus, &mut name);
        if n == 0 {
            return Err("COPY_PROTECTED_OR_UNVERIFIED_CONTROL");
        }
        let class = String::from_utf16_lossy(&name[..n as usize]).to_ascii_lowercase();
        if !crate::selection::compatibility_control_allowed(
            &class,
            GetWindowLongPtrW(focus, GWL_STYLE) as u32 & 0x20 != 0,
        ) {
            return Err("COPY_SENSITIVE_CONTROL");
        }
        Ok(Target {
            hwnd: focus.0 as isize,
            pid,
            thread,
        })
    }
}
unsafe fn ordinary_input_desktop() -> bool {
    unsafe fn name(handle: HANDLE) -> Option<String> {
        let mut buf = [0u16; 256];
        GetUserObjectInformationW(
            handle,
            UOI_NAME,
            Some(buf.as_mut_ptr().cast()),
            std::mem::size_of_val(&buf) as u32,
            None,
        )
        .ok()?;
        let n = buf.iter().position(|c| *c == 0)?;
        Some(String::from_utf16_lossy(&buf[..n]))
    }
    let Ok(input) = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) else {
        return false;
    };
    let input_name = name(HANDLE(input.0));
    let thread_name = GetThreadDesktop(GetCurrentThreadId())
        .ok()
        .and_then(|d| name(HANDLE(d.0)));
    let _ = CloseDesktop(input);
    input_name.as_deref() == Some("Default") && thread_name == input_name
}
fn writer_related(target: Target, focus: Target, observed: Stamp) -> bool {
    // A hidden clipboard helper in either verified process is permitted. A new
    // arbitrary process is not trusted merely because it wrote after SendInput.
    unsafe {
        let writer = HWND(observed.owner.hwnd as *mut _);
        let mut pid = 0;
        IsWindow(Some(writer)).as_bool()
            && GetWindowThreadProcessId(writer, Some(&mut pid)) == observed.owner.thread
            && pid == observed.owner.pid
            && crate::selection::compatibility_writer_related(
                target,
                focus,
                observed,
                GetAncestor(writer, GA_ROOTOWNER).0 as isize,
                GetCurrentProcessId(),
            )
    }
}
/// Generic/custom virtual focus cannot authorize strict copy. Only
/// the verified Unicode Edit control in the current foreground target is supported.
fn native_focus(target: Target) -> Option<isize> {
    unsafe {
        let root = HWND(target.hwnd as *mut _);
        if GetForegroundWindow() != root {
            return None;
        }
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(target.thread, &mut info).ok()?;
        let focus = info.hwndFocus;
        if focus.is_invalid() || GetAncestor(focus, GA_ROOT) != root {
            return None;
        }
        let mut pid = 0;
        if GetWindowThreadProcessId(focus, Some(&mut pid)) != target.thread || pid != target.pid {
            return None;
        }
        let mut name = [0u16; 128];
        let size = GetClassNameW(focus, &mut name);
        if size <= 0 {
            return None;
        }
        let class = String::from_utf16_lossy(&name[..size as usize]).to_ascii_lowercase();
        let style = GetWindowLongPtrW(focus, GWL_STYLE) as u32;
        if crate::native_copy_proof::control_metadata_allowed(
            &class,
            IsWindowUnicode(focus).as_bool(),
            style & 0x20 != 0,
        ) {
            Some(focus.0 as isize)
        } else {
            None
        }
    }
}
fn capture_inner(r: &Request, app: &AppHandle) -> Result<String, &'static str> {
    if !crate::release_policy::safe_copy_available() {
        return Err("COPY_BUILD_DISABLED");
    }
    let _serial = TRANSACTION.try_lock().map_err(|_| "COPY_BUSY")?;
    let e = epoch().ok_or("COPY_UNAUTHORIZED")?;
    let shortcut = {
        let consent = CONSENT.lock().unwrap_or_else(|p| p.into_inner());
        if !consent.matches(e) {
            return Err("COPY_UNAUTHORIZED");
        }
        *SHORTCUT.lock().unwrap_or_else(|p| p.into_inner())
    };
    unsafe {
        let process = Process(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, r.target.pid)
                .map_err(|_| "COPY_TARGET_CHANGED")?,
        );
        if !current(r, e, &process) {
            return Err("COPY_TARGET_CHANGED");
        }
        let mut class = [0u16; 256];
        let n = GetClassNameW(HWND(r.target.hwnd as *mut _), &mut class);
        if n == 0 {
            return Err("COPY_TARGET_UNVERIFIED");
        }
        let name = String::from_utf16_lossy(&class[..n as usize]).to_ascii_lowercase();
        if name.contains("console") || name.contains("cascadia") {
            return Err("COPY_TARGET_PROTECTED");
        }
        let compatibility = state().mode == Mode::Compatibility;
        let focus = if compatibility {
            if !crate::uia::compatibility_target_safe(r) {
                return Err("COPY_SENSITIVE_CONTROL");
            }
            compatibility_focus(r.target)?
        } else {
            let hwnd = native_focus(r.target).ok_or("COPY_PROTECTED_OR_UNVERIFIED_CONTROL")?;
            Target { hwnd, ..r.target }
        };
        crate::copy_engine::run_copy(
            &mut NativeIo {
                request: r,
                epoch: e,
                shortcut,
                process,
                app,
                focus,
                compatibility,
                prepared_sequence: 0,
            },
            r.target,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn keys(inputs: &[INPUT]) -> Vec<(u16, u32)> {
        inputs
            .iter()
            .map(|input| {
                assert_eq!(input.r#type, INPUT_KEYBOARD);
                let key = unsafe { input.Anonymous.ki };
                (key.wVk.0, key.dwFlags.0)
            })
            .collect()
    }
    #[test]
    fn selected_shortcut_emits_exactly_one_complete_chord() {
        for (shortcut, expected) in [
            (
                CopyShortcut::CtrlInsert,
                vec![(17, 0), (45, 0), (45, 2), (17, 2)],
            ),
            (
                CopyShortcut::CtrlC,
                vec![(17, 0), (67, 0), (67, 2), (17, 2)],
            ),
        ] {
            assert_eq!(
                keys(&copy_inputs(shortcut, true, |_| false).unwrap()),
                expected
            );
        }
    }
    #[test]
    fn revoked_queued_copy_or_held_modifier_never_produces_input() {
        for shortcut in [CopyShortcut::CtrlInsert, CopyShortcut::CtrlC] {
            let mut consent = Consent::empty();
            consent.grant(POLICY_VERSION);
            let queued_epoch = consent.epoch();
            consent.revoke();
            assert!(copy_inputs(shortcut, consent.matches(queued_epoch), |_| false).is_none());
            for held in [17, 18, 16, 91, 92, shortcut.key().0] {
                assert!(copy_inputs(shortcut, true, |key| key.0 == held).is_none());
            }
        }
    }
    #[test]
    fn partial_input_cleanup_releases_only_the_selected_keys() {
        for (shortcut, key) in [(CopyShortcut::CtrlInsert, 45), (CopyShortcut::CtrlC, 67)] {
            assert!(release_inputs(shortcut, 0).is_empty());
            assert_eq!(keys(&release_inputs(shortcut, 1)), vec![(17, 2)]);
            assert_eq!(keys(&release_inputs(shortcut, 2)), vec![(key, 2), (17, 2)]);
            assert_eq!(keys(&release_inputs(shortcut, 3)), vec![(17, 2)]);
            assert!(release_inputs(shortcut, 4).is_empty());
        }
    }
    #[test]
    fn snapshot_timeout_reason_does_not_claim_it_can_interrupt_delayed_rendering() {
        let reason = copy_failure_reason("COPY_SNAPSHOT_UNSTABLE");
        assert!(reason.contains("未发送快捷键"));
        assert!(reason.contains("不能中断"));
    }

    #[test]
    fn only_focused_visible_local_settings_can_request_native_consent() {
        let local = tauri::Url::parse("https://tauri.localhost/poc/settings.html").unwrap();
        assert!(local_settings("settings", &local, true, true));
        for label in ["panel", "popup", "result"] {
            assert!(!local_settings(label, &local, true, true));
        }
        assert!(!local_settings("settings", &local, false, true));
        assert!(!local_settings("settings", &local, true, false));
        for url in [
            "https://example.com/poc/settings.html",
            "https://tauri.localhost.evil/poc/settings.html",
            "https://tauri.localhost/poc/result.html",
        ] {
            assert!(!local_settings(
                "settings",
                &tauri::Url::parse(url).unwrap(),
                true,
                true
            ));
        }
    }
    #[test]
    fn state_and_epoch_follow_the_build_gate_but_old_config_never_grants_consent() {
        let mut consent = Consent::empty();
        let available = crate::release_policy::safe_copy_available();
        assert!(!copy_authorized(true, &consent, consent.epoch()));
        consent.grant(POLICY_VERSION);
        let queued = consent.epoch();
        assert_eq!(copy_authorized(true, &consent, queued), available);
        assert!(!copy_authorized(false, &consent, queued));
        consent.revoke();
        assert!(!copy_authorized(true, &consent, queued));
        consent.grant(POLICY_VERSION);
        assert!(!copy_authorized(true, &consent, queued));
        assert!(!copy_authorized(true, &Consent::empty(), 0));
    }
    #[test]
    fn centralized_policy_requires_all_gates() {
        for a in [false, true] {
            for b in [false, true] {
                for c in [false, true] {
                    assert_eq!(
                        crate::release_policy::safe_copy_allowed(a, b, c),
                        crate::release_policy::safe_copy_available() && a && b && c
                    );
                }
            }
        }
    }
}
