//! Opt-in, one-shot clipboard capture. Old capture_selection remains locked off.
//! Same-process writers and brokered/delayed renderers cannot be fully attributed.
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
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetGUIThreadInfo, GetWindowLongPtrW, GetWindowThreadProcessId,
    MessageBoxW, GA_ROOT, GUITHREADINFO, GWL_STYLE, IDOK, MB_DEFBUTTON2, MB_ICONWARNING,
    MB_OKCANCEL,
};
static CONSENT: Mutex<Consent> = Mutex::new(Consent::empty());
static TRANSACTION: Mutex<()> = Mutex::new(());
static PROMPTING: AtomicBool = AtomicBool::new(false);
#[derive(serde::Serialize)]
pub struct State {
    pub enabled: bool,
    pub policy_version: u32,
    pub session_only: bool,
}
pub fn state() -> State {
    State {
        enabled: CONSENT.lock().unwrap_or_else(|p| p.into_inner()).allows(),
        policy_version: POLICY_VERSION,
        session_only: true,
    }
}
pub fn revoke() {
    CONSENT.lock().unwrap_or_else(|p| p.into_inner()).revoke();
}
fn epoch() -> Option<u64> {
    let c = CONSENT.lock().ok()?;
    c.allows().then_some(c.epoch())
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
pub fn request_authorization(window: WebviewWindow) -> Result<State, String> {
    if !trusted(&window) {
        return Err("只能在聚焦的本地设置页请求授权".into());
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
    let start_epoch = CONSENT.lock().unwrap_or_else(|p| p.into_inner()).epoch();
    let hwnd = window.hwnd().map_err(|_| "设置窗口不可用")?;
    let answer = unsafe {
        MessageBoxW(Some(HWND(hwnd.0)),w!("当 UIA 无法读取选中文本时，程序可能模拟 Ctrl+C 获取选区，并尝试恢复原剪贴板。此操作可能短暂改变剪贴板内容，无法完全避免其他程序并发操作造成的风险。\n\n仅本次运行有效，重启后需重新授权。旧配置和备份不能授权。请勿在敏感资料中开启。"),w!("授权自动剪贴板划词回退"),MB_OKCANCEL|MB_ICONWARNING|MB_DEFBUTTON2)
    };
    let still_trusted = trusted(&window);
    let mut consent = CONSENT.lock().unwrap_or_else(|p| p.into_inner());
    if answer == IDOK && consent.epoch() == start_epoch && still_trusted {
        consent.grant(POLICY_VERSION);
    } else {
        consent.revoke();
    }
    drop(consent);
    Ok(state())
}
struct Clipboard;
impl Clipboard {
    unsafe fn open() -> Result<Self, &'static str> {
        OpenClipboard(None).map_err(|_| "COPY_CLIPBOARD_BUSY")?;
        Ok(Self)
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
    Some(Stamp {
        root_hwnd: GetAncestor(hwnd, GA_ROOT).0 as isize,
        owner: Target {
            hwnd: hwnd.0 as isize,
            pid,
            thread,
        },
        sequence: GetClipboardSequenceNumber(),
    })
}
struct Snapshot {
    sequence: u32,
    blocks: Vec<(u32, Block)>,
}
unsafe fn snapshot() -> Result<Snapshot, &'static str> {
    let _lock = Clipboard::open()?;
    let sequence = GetClipboardSequenceNumber();
    let start = Instant::now();
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
unsafe fn copy_chord() -> bool {
    if [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .iter()
        .any(|v| GetAsyncKeyState(i32::from(v.0)) < 0)
    {
        return false;
    }
    let key = |v: VIRTUAL_KEY, flags| INPUT {
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
    };
    let sent = SendInput(
        &[
            key(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
            key(VIRTUAL_KEY(0x43), KEYBD_EVENT_FLAGS(0)),
            key(VIRTUAL_KEY(0x43), KEYEVENTF_KEYUP),
            key(VK_CONTROL, KEYEVENTF_KEYUP),
        ],
        std::mem::size_of::<INPUT>() as i32,
    );
    if sent != 4 {
        let _ = SendInput(
            &[
                key(VIRTUAL_KEY(0x43), KEYEVENTF_KEYUP),
                key(VK_CONTROL, KEYEVENTF_KEYUP),
            ],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
    sent == 4
}
pub fn capture(r: &Request, app: &AppHandle) -> Result<String, &'static str> {
    let outcome = capture_inner(r, app);
    crate::diag::log(&format!(
        "selection status={} elapsed_ms={}",
        outcome
            .as_ref()
            .map(|_| "COPY_SUCCESS")
            .unwrap_or_else(|e| e),
        crate::selection_runtime::now_ms().saturating_sub(r.created_ms)
    ));
    outcome
}
struct NativeIo<'a> {
    request: &'a Request,
    epoch: u64,
    process: Process,
    app: &'a AppHandle,
    focus: isize,
}
impl crate::copy_engine::ClipboardIo for NativeIo<'_> {
    type Snapshot = Snapshot;
    fn valid(&self) -> bool {
        current(self.request, self.epoch, &self.process)
            && native_focus(self.request.target) == Some(self.focus)
    }
    fn selection_proof(&mut self) -> Result<crate::copy_engine::SelectionProof, &'static str> {
        crate::native_copy_proof::capture(HWND(self.focus as *mut _), || self.valid())
    }
    fn prepare(&mut self) -> Result<(Snapshot, u32), &'static str> {
        unsafe {
            let s = snapshot()?;
            let sequence = s.sequence;
            Ok((s, sequence))
        }
    }
    fn sequence(&self) -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }
    fn send_once(&mut self) -> bool {
        unsafe { copy_chord() }
    }
    fn wait(&mut self, before: u32) -> Result<Stamp, &'static str> {
        let deadline = Instant::now() + Duration::from_millis(350);
        unsafe {
            while GetClipboardSequenceNumber() == before {
                if !self.valid() {
                    return Err("COPY_TARGET_CHANGED");
                }
                if Instant::now() >= deadline {
                    return Err("COPY_TIMEOUT");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let _lock = Clipboard::open()?;
            stamp().ok_or("COPY_SOURCE_UNVERIFIED")
        }
    }
    fn read(&mut self, expected: Stamp) -> Result<String, &'static str> {
        unsafe {
            let _lock = Clipboard::open()?;
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
        unsafe {
            let owner = self
                .app
                .get_webview_window("ball")
                .and_then(|w| w.hwnd().ok())
                .ok_or("COPY_RESTORE_SKIPPED")?;
            OpenClipboard(Some(HWND(owner.0))).map_err(|_| "COPY_CLIPBOARD_BUSY")?;
            let _lock = Clipboard;
            let observed = stamp().ok_or("COPY_RESTORE_SKIPPED")?;
            if !self.valid() || !restore_allowed(expected, observed) {
                return Err("COPY_RESTORE_SKIPPED");
            }
            // Other processes cannot open/write between this comparison and EmptyClipboard.
            // Restoring several formats can still partially fail; do not claim atomic recovery.
            EmptyClipboard().map_err(|_| "COPY_RESTORE_FAILED")?;
            for (format, block) in &mut saved.blocks {
                SetClipboardData(*format, Some(HANDLE(block.0 .0)))
                    .map_err(|_| "COPY_RESTORE_FAILED")?;
                block.0 = HGLOBAL::default();
            }
            Ok(())
        }
    }
}
/// Generic/custom virtual focus cannot safely authorize synthetic copy. Only
/// verified standard Edit/RichEdit focus is currently supported by this path.
fn native_focus(target: Target) -> Option<isize> {
    unsafe {
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(target.thread, &mut info).ok()?;
        let focus = info.hwndFocus;
        if focus.is_invalid() || GetAncestor(focus, GA_ROOT) != HWND(target.hwnd as *mut _) {
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
        if crate::selection::copy_control_allowed(&class, style) {
            Some(focus.0 as isize)
        } else {
            None
        }
    }
}
fn capture_inner(r: &Request, app: &AppHandle) -> Result<String, &'static str> {
    let _serial = TRANSACTION.try_lock().map_err(|_| "COPY_BUSY")?;
    let e = epoch().ok_or("COPY_UNAUTHORIZED")?;
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
        let focus = native_focus(r.target).ok_or("COPY_PROTECTED_OR_UNVERIFIED_CONTROL")?;
        crate::copy_engine::run_copy(
            &mut NativeIo {
                request: r,
                epoch: e,
                process,
                app,
                focus,
            },
            r.target,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn centralized_policy_requires_all_gates() {
        for a in [false, true] {
            for b in [false, true] {
                for c in [false, true] {
                    assert_eq!(
                        crate::release_policy::safe_copy_allowed(a, b, c),
                        a && b && c
                    );
                }
            }
        }
    }
}
