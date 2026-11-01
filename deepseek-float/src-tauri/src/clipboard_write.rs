//! Immediate plain-text publication. The allocation is ready before destructive writes.
//! No retries and no promise of rollback after EmptyClipboard succeeds.
use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
};

const CF_UNICODETEXT_FORMAT: u32 = 13;

struct Owner(HWND);
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

/// Keep the owner alive until after CloseClipboard. No delayed rendering is used.
pub(super) unsafe fn write_text(text: &str, permitted: impl FnOnce() -> bool) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * std::mem::size_of::<u16>()) else {
        return false;
    };
    let dst = GlobalLock(hmem) as *mut u16;
    if dst.is_null() {
        let _ = GlobalFree(Some(hmem));
        return false;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
    let _ = GlobalUnlock(hmem);

    // OpenClipboard(NULL) followed by EmptyClipboard leaves a NULL owner and makes
    // SetClipboardData fail. A private message-only window also works on worker threads.
    let Ok(hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        w!(""),
        WINDOW_STYLE(0),
        0,
        0,
        0,
        0,
        Some(HWND_MESSAGE),
        None,
        None,
        None,
    ) else {
        let _ = GlobalFree(Some(hmem));
        return false;
    };
    let owner = Owner(hwnd);
    if OpenClipboard(Some(owner.0)).is_err() {
        let _ = GlobalFree(Some(hmem));
        return false;
    }
    let ok = (|| -> bool {
        // Restore authorization is checked under the same clipboard lock as the write.
        if !permitted() || EmptyClipboard().is_err() {
            return false;
        }
        // A failure here can leave the clipboard empty; arbitrary formats are not backed up.
        SetClipboardData(CF_UNICODETEXT_FORMAT, Some(HANDLE(hmem.0))).is_ok()
    })();
    let _ = CloseClipboard();
    if !ok {
        let _ = GlobalFree(Some(hmem));
    }
    // On success the system owns hmem. Destroying the owner does not free immediate data.
    ok
}
