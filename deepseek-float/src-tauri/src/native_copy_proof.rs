//! Independent, bounded selection evidence for standard Edit controls only.
//! This reduces wrong-text adoption; it is not proof of clipboard writer causality.
use crate::copy_engine::SelectionProof;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetWindowLongPtrW, IsWindowUnicode, SendMessageTimeoutW, GWL_STYLE,
    SMTO_ABORTIFHUNG, SMTO_BLOCK, SMTO_ERRORONEXIT, WM_GETTEXT, WM_GETTEXTLENGTH,
};
const EM_GETSEL: u32 = 0x00b0;
const MAX_CONTROL_UNITS: usize = 32_768;
struct TextBuffer(Vec<u16>);
impl Drop for TextBuffer {
    fn drop(&mut self) {
        // The whole control is only a bounded transient buffer; return just the selection.
        for unit in &mut self.0 {
            unsafe { std::ptr::write_volatile(unit, 0) };
        }
    }
}
fn message(
    hwnd: HWND,
    id: u32,
    w: usize,
    l: isize,
    deadline: Instant,
    valid: &impl Fn() -> bool,
) -> Result<usize, &'static str> {
    if !valid() {
        return Err("COPY_CONTEXT_CHANGED");
    }
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .as_millis();
    if remaining == 0 {
        return Err("COPY_SELECTION_TIMEOUT");
    }
    let mut result = 0usize;
    let ok = unsafe {
        SendMessageTimeoutW(
            hwnd,
            id,
            WPARAM(w),
            LPARAM(l),
            SMTO_BLOCK | SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT,
            remaining.min(20) as u32,
            Some(&mut result),
        )
    };
    if ok.0 == 0 {
        return Err("COPY_SELECTION_UNVERIFIED");
    }
    if !valid() || Instant::now() >= deadline {
        return Err("COPY_CONTEXT_CHANGED");
    }
    Ok(result)
}
fn range(
    hwnd: HWND,
    deadline: Instant,
    valid: &impl Fn() -> bool,
) -> Result<(u32, u32), &'static str> {
    let (mut start, mut end) = (u32::MAX, u32::MAX);
    message(
        hwnd,
        EM_GETSEL,
        &mut start as *mut _ as usize,
        &mut end as *mut _ as isize,
        deadline,
        valid,
    )?;
    if start == u32::MAX || end == u32::MAX || start >= end {
        return Err("COPY_SELECTION_UNVERIFIED");
    }
    Ok((start, end))
}
fn control_allowed(hwnd: HWND) -> bool {
    unsafe {
        let mut name = [0u16; 16];
        let size = GetClassNameW(hwnd, &mut name);
        // RichEdit has different text/range conventions; do not assume equivalence.
        size == 4
            && String::from_utf16_lossy(&name[..4]).eq_ignore_ascii_case("edit")
            && IsWindowUnicode(hwnd).as_bool()
            && GetWindowLongPtrW(hwnd, GWL_STYLE) & 0x20 == 0
    }
}
fn selected(units: &[u16], start: u32, end: u32) -> Result<String, &'static str> {
    let slice = units
        .get(start as usize..end as usize)
        .ok_or("COPY_SELECTION_UNVERIFIED")?;
    let text = String::from_utf16(slice).map_err(|_| "COPY_SELECTION_UNVERIFIED")?;
    if text.trim().chars().count() < 2 || text.chars().count() > 20_000 {
        return Err("COPY_SELECTION_UNVERIFIED");
    }
    Ok(text)
}
pub(crate) fn capture(
    hwnd: HWND,
    valid: impl Fn() -> bool,
) -> Result<SelectionProof, &'static str> {
    if !valid() || !control_allowed(hwnd) {
        return Err("COPY_SELECTION_UNVERIFIED");
    }
    let deadline = Instant::now() + Duration::from_millis(80);
    let (start, end) = range(hwnd, deadline, &valid)?;
    let len = message(hwnd, WM_GETTEXTLENGTH, 0, 0, deadline, &valid)?;
    if len == 0 || len > MAX_CONTROL_UNITS || end as usize > len {
        return Err("COPY_SELECTION_UNVERIFIED");
    }
    let mut buffer = TextBuffer(vec![0u16; len + 1]);
    let copied = message(
        hwnd,
        WM_GETTEXT,
        buffer.0.len(),
        buffer.0.as_mut_ptr() as isize,
        deadline,
        &valid,
    )?;
    if copied != len
        || buffer.0[len] != 0
        || range(hwnd, deadline, &valid)? != (start, end)
        || message(hwnd, WM_GETTEXTLENGTH, 0, 0, deadline, &valid)? != len
        || !control_allowed(hwnd)
        || !valid()
    {
        return Err("COPY_SELECTION_CHANGED");
    }
    let text = selected(&buffer.0[..len], start, end)?;
    Ok(SelectionProof {
        control: hwnd.0 as isize,
        start,
        end,
        text,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SendMessageW, WINDOW_EX_STYLE, WINDOW_STYLE,
    };
    struct Window(HWND);
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
    fn edit(text: &str, password: bool) -> Window {
        let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        Window(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                PCWSTR(text.as_ptr()),
                WINDOW_STYLE(if password { 0x20 } else { 4 }),
                0,
                0,
                100,
                100,
                None,
                None,
                None,
                None,
            )
            .unwrap()
        })
    }
    fn select(w: &Window, start: usize, end: usize) {
        unsafe {
            SendMessageW(w.0, 0xb1, Some(WPARAM(start)), Some(LPARAM(end as isize)));
        }
    }
    #[test]
    fn utf16_ranges_preserve_unicode_crlf_and_indentation() {
        let w = edit("prefix 😀中文\r\n  code <&> tail", false);
        let expected = "😀中文\r\n  code <&>";
        let start = "prefix ".encode_utf16().count();
        select(&w, start, start + expected.encode_utf16().count());
        let proof = capture(w.0, || true).unwrap();
        assert_eq!(proof.text, expected);
        assert_eq!(proof.start, start as u32);
    }
    #[test]
    fn password_empty_and_stale_context_never_produce_proof() {
        let p = edit("SYNTHETIC PASSWORD", true);
        select(&p, 0, 9);
        assert!(capture(p.0, || true).is_err());
        let e = edit("SYNTHETIC", false);
        select(&e, 2, 2);
        assert!(capture(e.0, || true).is_err());
        select(&e, 0, 4);
        assert!(capture(e.0, || false).is_err());
    }
    #[test]
    fn split_surrogate_out_of_bounds_short_and_oversized_are_rejected() {
        let units: Vec<u16> = "😀x".encode_utf16().collect();
        assert!(selected(&units, 1, 3).is_err());
        assert!(selected(&units, 0, 4).is_err());
        assert!(selected(&units, 2, 3).is_err());
        assert!(selected(&vec![65; 20_001], 0, 20_001).is_err());
    }
    #[test]
    fn oversized_control_refused_before_reading_body() {
        let w = edit(&"A".repeat(MAX_CONTROL_UNITS + 1), false);
        select(&w, 0, 2);
        assert!(capture(w.0, || true).is_err());
    }
}
