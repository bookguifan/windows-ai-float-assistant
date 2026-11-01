//! Explicit clipboard writes with a fail-closed snapshot for rollback.
use windows::core::w;
use windows::Win32::Foundation::{
    GetLastError, GlobalFree, SetLastError, HANDLE, HGLOBAL, HWND, WIN32_ERROR,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
};

const MAX_SNAPSHOT_FORMATS: usize = 128;
const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WriteError {
    AllocateText,
    OwnerWindow,
    OpenClipboard,
    PermissionDenied,
    UnsupportedFormat(u32),
    SnapshotFailed(u32),
    SnapshotTooLarge(u32),
    TooManyFormats,
    EnumerationFailed,
    EmptyClipboard,
    PublishFailedRestored,
    RestoreIncomplete(Vec<u32>),
    CloseClipboard,
}

struct Owner(HWND);
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

struct GlobalBlock(Option<HGLOBAL>);
impl GlobalBlock {
    fn handle(&self) -> HGLOBAL {
        self.0.expect("owned global block")
    }

    fn transfer(&mut self) {
        self.0 = None;
    }
}

impl Drop for GlobalBlock {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            unsafe {
                let _ = GlobalFree(Some(handle));
            }
        }
    }
}

struct ClipboardFormatSnapshot {
    format: u32,
    bytes: Vec<u8>,
}

unsafe fn allocate_global(bytes: &[u8], lock_error: WriteError) -> Result<GlobalBlock, WriteError> {
    let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|_| lock_error.clone())?;
    let block = GlobalBlock(Some(handle));
    let destination = GlobalLock(handle) as *mut u8;
    if destination.is_null() {
        return Err(lock_error);
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len());
    let _ = GlobalUnlock(handle);
    Ok(block)
}

fn uses_global_memory(format: u32) -> bool {
    matches!(format, 1 | 4..=8 | 10..=13 | 15..=17 | 0x0081)
        || (0x0300..=0x03ff).contains(&format)
        || (0xc000..=0xffff).contains(&format)
}

unsafe fn snapshot_clipboard() -> Result<Vec<ClipboardFormatSnapshot>, WriteError> {
    let mut snapshots = Vec::new();
    let mut formats = Vec::new();
    let mut previous = 0;
    let mut total_bytes = 0usize;

    loop {
        SetLastError(WIN32_ERROR(0));
        let format = EnumClipboardFormats(previous);
        if format == 0 {
            if GetLastError().0 != 0 {
                return Err(WriteError::EnumerationFailed);
            }
            break;
        }
        if formats.len() == MAX_SNAPSHOT_FORMATS {
            return Err(WriteError::TooManyFormats);
        }
        formats.push(format);
        previous = format;
    }

    let has_dib = formats.iter().any(|format| matches!(format, 8 | 17));
    for format in formats {
        // EnumClipboardFormats also returns synthesized formats. Keep the DIB payload;
        // Windows can synthesize the corresponding device-dependent bitmap/palette.
        if has_dib && matches!(format, 2 | 9) {
            continue;
        }
        if !uses_global_memory(format) {
            return Err(WriteError::UnsupportedFormat(format));
        }

        let handle = GetClipboardData(format).map_err(|_| WriteError::SnapshotFailed(format))?;
        let global = HGLOBAL(handle.0);
        let size = GlobalSize(global);
        let Some(next_total) = total_bytes.checked_add(size) else {
            return Err(WriteError::SnapshotTooLarge(format));
        };
        if size == 0 || next_total > MAX_SNAPSHOT_BYTES {
            return Err(WriteError::SnapshotTooLarge(format));
        }

        let source = GlobalLock(global) as *const u8;
        if source.is_null() {
            return Err(WriteError::SnapshotFailed(format));
        }
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(size).is_err() {
            let _ = GlobalUnlock(global);
            return Err(WriteError::SnapshotTooLarge(format));
        }
        bytes.extend_from_slice(std::slice::from_raw_parts(source, size));
        let _ = GlobalUnlock(global);

        snapshots.push(ClipboardFormatSnapshot { format, bytes });
        total_bytes = next_total;
    }
    Ok(snapshots)
}

unsafe fn restore_snapshot(snapshots: &[ClipboardFormatSnapshot]) -> Vec<u32> {
    let mut failed = Vec::new();
    for snapshot in snapshots {
        let Ok(mut block) =
            allocate_global(&snapshot.bytes, WriteError::SnapshotFailed(snapshot.format))
        else {
            failed.push(snapshot.format);
            continue;
        };
        if SetClipboardData(snapshot.format, Some(HANDLE(block.handle().0))).is_ok() {
            block.transfer();
        } else {
            failed.push(snapshot.format);
        }
    }
    failed
}

/// Keep the owner alive until after CloseClipboard. No delayed rendering is used.
pub(super) unsafe fn write_text(
    text: &str,
    permitted: impl FnOnce() -> bool,
) -> Result<(), WriteError> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let text_bytes = std::slice::from_raw_parts(
        wide.as_ptr().cast::<u8>(),
        wide.len() * std::mem::size_of::<u16>(),
    );
    let mut text_block = allocate_global(text_bytes, WriteError::AllocateText)?;

    let hwnd = CreateWindowExW(
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
    )
    .map_err(|_| WriteError::OwnerWindow)?;
    let owner = Owner(hwnd);
    OpenClipboard(Some(owner.0)).map_err(|_| WriteError::OpenClipboard)?;

    let result = (|| {
        if !permitted() {
            return Err(WriteError::PermissionDenied);
        }
        let snapshots = snapshot_clipboard()?;
        if EmptyClipboard().is_err() {
            return Err(WriteError::EmptyClipboard);
        }

        if SetClipboardData(13, Some(HANDLE(text_block.handle().0))).is_ok() {
            text_block.transfer();
            return Ok(());
        }

        let failed = restore_snapshot(&snapshots);
        if failed.is_empty() {
            Err(WriteError::PublishFailedRestored)
        } else {
            Err(WriteError::RestoreIncomplete(failed))
        }
    })();

    let close = CloseClipboard();
    drop(owner);
    if result.is_ok() && close.is_err() {
        Err(WriteError::CloseClipboard)
    } else {
        result
    }
}
