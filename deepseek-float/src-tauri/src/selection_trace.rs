//! Opt-in synthetic-only diagnostics. Never logs text or accesses the clipboard.
//! The separate flag is required even when normal diagnostics are on/full.
use crate::selection::{Request, Session};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{sync_channel, SyncSender},
    OnceLock,
};
use windows::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
static ENABLED: OnceLock<bool> = OnceLock::new();
static CLICKS: OnceLock<SyncSender<Click>> = OnceLock::new();
static TRACE_TICKET: AtomicU64 = AtomicU64::new(0);
static TRACE_SETTINGS_TICKET: AtomicU64 = AtomicU64::new(0);
static TRACE_GENERATION: AtomicU64 = AtomicU64::new(0);
static FOREGROUNDS: OnceLock<SyncSender<Foreground>> = OnceLock::new();
static STOP: AtomicBool = AtomicBool::new(false);
pub(crate) fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        allowed(
            &std::env::var("DSFLOAT_SYNTHETIC_SELECTION_TRACE").unwrap_or_default(),
            crate::diag::enabled(),
        )
    })
}
fn allowed(value: &str, diag: bool) -> bool {
    diag && value == "synthetic-only"
}
// Even a globally installed hook in a synthetic run must not fingerprint unrelated data.
const FIXTURES: &[&str] = &[
    "  N01 Unicode Edit synthetic 中文 😀.\r\n  code <&> preserved.  ",
    "  N01 latest selection B 中文 😀.\r\n    indent four <&> preserved.  ",
    "N04 WPF UIA synthetic selection. Only controlled data.",
    "  中文 😀\r\n  code <&>  ",
];
fn fingerprint(active: bool, text: &str) -> Option<(usize, String)> {
    if !active || text.chars().count() < 2 || !FIXTURES.iter().any(|s| s.contains(text)) {
        return None;
    }
    let mut input: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let units = input.len() / 2;
    let mut hash = [0u8; 32];
    let ok = unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, &input, &mut hash) }.is_ok();
    for byte in &mut input {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    ok.then(|| (units, hash.iter().map(|b| format!("{b:02X}")).collect()))
}
fn fixture_title(title: &str) -> bool {
    matches!(
        title,
        "Beta2 Postmerge Unicode Edit Diagnostic - Synthetic Only"
            | "Beta2 Postmerge WPF UIA Diagnostic - Synthetic Only"
            | "Beta2 N01 N05 Retest - Synthetic Unicode Edit"
    )
}
fn fixture_target(r: &Request) -> bool {
    if !enabled() || !crate::selection_runtime::is_native_target(r.target) {
        return false;
    }
    let mut title = [0u16; 160];
    let n = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(
            windows::Win32::Foundation::HWND(r.target.hwnd as *mut _),
            &mut title,
        )
    };
    n > 0 && fixture_title(&String::from_utf16_lossy(&title[..n as usize]))
}
fn scoped(ticket: Option<u64>) -> bool {
    enabled() && ticket.is_some_and(|t| t != 0 && t == TRACE_TICKET.load(Ordering::Acquire))
}
pub(crate) fn text(stage: &'static str, r: &Request, ticket: u64, raw: &str) {
    if !fixture_target(r) {
        TRACE_TICKET.store(0, Ordering::Release);
        TRACE_GENERATION.store(0, Ordering::Release);
        return;
    }
    if let Some((units, hash)) = fingerprint(true, raw) {
        TRACE_GENERATION.store(r.generation, Ordering::Release);
        TRACE_TICKET.store(r.session_id, Ordering::Release);
        if stage == "settings-preview" {
            TRACE_SETTINGS_TICKET.store(ticket, Ordering::Release);
        }
        crate::diag::log(&format!("selection-trace stage={stage} request={} session={} generation={} ticket={ticket} source_hwnd={} units={units} sha256_utf16le={hash}",r.generation,r.session_id,crate::selection_runtime::generation(),r.target.hwnd));
    } else {
        TRACE_TICKET.store(0, Ordering::Release);
        TRACE_GENERATION.store(0, Ordering::Release);
    }
}
pub(crate) fn session(stage: &'static str, s: &Session) {
    text(stage, &s.request, s.request.session_id, &s.raw);
}
pub(crate) fn range(r: &Request, control: isize, start: u32, end: u32, total: usize, raw: &str) {
    if fixture_target(r) && fingerprint(true, raw).is_some() {
        crate::diag::log(&format!("selection-trace stage=native-range request={} session={} generation={} control={control} start={start} end={end} selected_units={} control_units={total}",r.generation,r.session_id,crate::selection_runtime::generation(),end.saturating_sub(start)));
    }
}
pub(crate) fn event(stage: &'static str, ticket: Option<u64>, outcome: &'static str) {
    if scoped(ticket) {
        crate::diag::log(&format!("selection-trace stage={stage} ticket={ticket:?} generation={} now_ms={} outcome={outcome}",crate::selection_runtime::generation(),crate::selection_runtime::now_ms()));
    }
}
pub(crate) fn settings(stage: &'static str, ticket: Option<u64>, revision: u64) {
    let source_ticket = TRACE_TICKET.load(Ordering::Acquire);
    if ticket.is_some_and(|t| t != 0 && t == TRACE_SETTINGS_TICKET.load(Ordering::Acquire))
        && scoped(Some(source_ticket))
    {
        crate::diag::log(&format!("selection-trace stage={stage} source_ticket={source_ticket} settings_ticket={ticket:?} revision={revision} now_ms={}", crate::selection_runtime::now_ms()));
    }
}
// Numeric, bounded telemetry only. The hook never formats text, hashes, writes files,
// blocks on a queue, or waits for a UI/COM operation.
#[derive(Debug)]
pub(crate) struct Click {
    pub root: isize,
    pub x: i32,
    pub y: i32,
    pub inside: bool,
    pub own: bool,
    pub armed: bool,
    pub ticket: Option<u64>,
    pub generation: u64,
    pub now_ms: u64,
}
pub(crate) fn start() {
    if !enabled() || CLICKS.get().is_some() {
        return;
    }
    start_foregrounds();
    let (tx, rx) = sync_channel::<Click>(32);
    if CLICKS.set(tx).is_err() {
        return;
    }
    std::thread::spawn(move || {
        while !STOP.load(Ordering::Acquire) {
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(c) => crate::diag::log(&format!("selection-trace stage=native-down root={} x={} y={} inside={} own={} armed={} ticket={:?} generation={} captured_ms={}",c.root,c.x,c.y,c.inside,c.own,c.armed,c.ticket,c.generation,c.now_ms)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {},
                Err(_) => break,
            }
        }
    });
}
pub(crate) fn click(c: Click) {
    if !scoped(c.ticket) {
        return;
    }
    if let Some(tx) = CLICKS.get() {
        let _ = tx.try_send(c);
    }
}
pub(crate) fn stop() {
    STOP.store(true, Ordering::Release);
}
pub(crate) fn frontend(stage: &str, action: &str, ticket: Option<u64>) {
    if scoped(ticket)
        && [
            "click",
            "busy",
            "no-ticket",
            "dispatch",
            "resolved",
            "rejected",
        ]
        .contains(&stage)
        && ["copy", "open-more", "open-settings", "prompt"].contains(&action)
    {
        crate::diag::log(&format!("selection-trace stage=frontend-{stage} action={action} ticket={ticket:?} generation={}",crate::selection_runtime::generation()));
    }
}
// Callback observations are queued without formatting, disk I/O, hashing or COM.
// event_ms is the WinEvent OS tick; captured_ms and written_ms use the process clock.
// Queue output order is not the event's chronological order.
struct Foreground {
    hwnd: isize,
    event_thread: u32,
    event_ms: u32,
    captured_generation: u64,
    handled_generation: u64,
    captured_ms: u64,
    preserved: bool,
    ticket: Option<u64>,
    show: Option<u64>,
    source_alive: bool,
}
fn start_foregrounds() {
    let (tx, rx) = sync_channel::<Foreground>(32);
    if FOREGROUNDS.set(tx).is_err() {
        return;
    }
    std::thread::spawn(move || {
        while !STOP.load(Ordering::Acquire) {
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(e) => crate::diag::log(&format!("selection-trace stage=foreground hwnd={} event_thread={} os_event_ms={} captured_generation={} handled_generation={} captured_ms={} written_ms={} preserved={} ticket={:?} show={:?} source_alive={}",
                    e.hwnd,e.event_thread,e.event_ms,e.captured_generation,e.handled_generation,e.captured_ms,crate::selection_runtime::now_ms(),e.preserved,e.ticket,e.show,e.source_alive)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {},
                Err(_) => break,
            }
        }
    });
}
#[allow(clippy::too_many_arguments)] // Fixed numeric observation copied from the callback.
pub(crate) fn foreground(
    hwnd: isize,
    event_thread: u32,
    event_ms: u32,
    captured_generation: u64,
    handled_generation: u64,
    captured_ms: u64,
    preserved: bool,
    ticket: Option<u64>,
    show: Option<u64>,
    source_alive: bool,
) {
    if captured_generation != TRACE_GENERATION.load(Ordering::Acquire) || !scoped(ticket) {
        return;
    }
    if let Some(tx) = FOREGROUNDS.get() {
        let _ = tx.try_send(Foreground {
            hwnd,
            event_thread,
            event_ms,
            captured_generation,
            handled_generation,
            captured_ms,
            preserved,
            ticket,
            show,
            source_alive,
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_diagnostics_never_hash_even_in_full_mode() {
        assert!(!allowed("", true));
        assert!(!allowed("full", true));
        assert!(!allowed("synthetic-only", false));
        assert!(allowed("synthetic-only", true));
        assert_eq!(fingerprint(false, "SYNTHETIC ONLY"), None);
        assert_eq!(fingerprint(true, "unrelated nonfixture content"), None);
    }
    #[test]
    fn unrelated_window_titles_are_not_synthetic_even_if_the_text_matches() {
        assert!(!fixture_title("ordinary document"));
        assert!(!fixture_title(
            "Beta2 Postmerge Unicode Edit Diagnostic - Synthetic Only - other"
        ));
        assert!(fixture_title(
            "Beta2 Postmerge Unicode Edit Diagnostic - Synthetic Only"
        ));
    }
    #[test]
    fn sha256_is_utf16le_not_utf8_and_keeps_spaces_crlf() {
        // Independent Python hashlib SHA256 over the literal UTF-16LE fixture.
        let (units, h) = fingerprint(true, "  中文 😀\r\n  code <&>  ").unwrap();
        assert_eq!(units, 21);
        assert_eq!(
            h,
            "4BD34106289028AD177B4462D6523AF57323E6555331EF1C7FE2AA0E703796A0"
        );
        assert_ne!(
            fingerprint(true, FIXTURES[3]),
            fingerprint(true, FIXTURES[3].trim())
        );
        assert_eq!(fingerprint(true, "code\n"), None);
        assert_ne!(
            fingerprint(true, "  code <&>"),
            fingerprint(true, "code <&>")
        );
    }
}
