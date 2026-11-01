// The production writer is compiled against this in-memory Win32 facade.
// No Windows crate, windows, input, or system clipboard are used by this test.
#![allow(non_snake_case, non_camel_case_types)]
extern crate self as windows;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::c_void,
};
#[macro_export]
macro_rules! mock_w {
    ($text:literal) => {
        $text
    };
}
pub mod core {
    pub use crate::mock_w as w;
}
#[derive(Default)]
struct State {
    clipboard: Vec<(u32, Vec<u8>)>,
    blocks: HashMap<usize, Box<[u16]>>,
    transferred: HashSet<usize>,
    next: usize,
    opened: bool,
    opened_owner: usize,
    owner: usize,
    fail: &'static str,
    opens: usize,
    empties: usize,
    publishes: usize,
    closes: usize,
    frees: usize,
    creates: usize,
    destroys: usize,
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    STATE.with(|s| f(&mut s.borrow_mut()))
}
fn original() -> Vec<(u32, Vec<u8>)> {
    vec![
        (13, vec![65, 0, 0, 0]),
        (49152, b"<b>SYNTHETIC</b>".to_vec()),
        (49153, br"{\rtf1 SYNTHETIC}".to_vec()),
        (8, vec![1, 2, 3, 4]),
    ]
}
fn reset(fail: &'static str) {
    with(|s| {
        *s = State {
            clipboard: original(),
            fail,
            ..Default::default()
        }
    });
}
pub mod Win32 {
    pub mod Foundation {
        use std::ffi::c_void;
        #[derive(Clone, Copy, Default)]
        pub struct HANDLE(pub *mut c_void);
        #[derive(Clone, Copy, Default)]
        pub struct HGLOBAL(pub *mut c_void);
        #[derive(Clone, Copy, Default)]
        pub struct HWND(pub *mut c_void);
        pub(crate) unsafe fn GlobalFree(h: Option<HGLOBAL>) -> Result<HGLOBAL, ()> {
            crate::with(|s| {
                let id = h.unwrap().0 as usize;
                assert!(
                    !s.transferred.contains(&id),
                    "must not free transferred memory"
                );
                assert!(s.blocks.remove(&id).is_some(), "must free exactly once");
                s.frees += 1;
            });
            Ok(HGLOBAL::default())
        }
    }
    pub mod System {
        pub mod Memory {
            use crate::{c_void, with, Win32::Foundation::HGLOBAL};
            pub const GMEM_MOVEABLE: u32 = 2;
            pub(crate) unsafe fn GlobalAlloc(flags: u32, size: usize) -> Result<HGLOBAL, ()> {
                assert_eq!(flags, GMEM_MOVEABLE);
                with(|s| {
                    if s.fail == "allocate" {
                        return Err(());
                    }
                    assert_eq!(size % 2, 0);
                    s.next += 1;
                    let id = s.next;
                    s.blocks.insert(id, vec![0u16; size / 2].into_boxed_slice());
                    Ok(HGLOBAL(id as *mut c_void))
                })
            }
            pub(crate) unsafe fn GlobalLock(h: HGLOBAL) -> *mut c_void {
                with(|s| {
                    if s.fail == "lock" {
                        return std::ptr::null_mut();
                    }
                    s.blocks
                        .get_mut(&(h.0 as usize))
                        .unwrap()
                        .as_mut_ptr()
                        .cast()
                })
            }
            pub(crate) unsafe fn GlobalUnlock(_: HGLOBAL) -> Result<(), ()> {
                Ok(())
            }
        }
        pub mod DataExchange {
            use crate::{
                with,
                Win32::Foundation::{HANDLE, HWND},
            };
            pub(crate) unsafe fn OpenClipboard(owner: Option<HWND>) -> Result<(), ()> {
                with(|s| {
                    s.opens += 1;
                    if s.fail == "open" || s.opened {
                        return Err(());
                    }
                    s.opened = true;
                    s.opened_owner = owner.map_or(0, |h| h.0 as usize);
                    Ok(())
                })
            }
            pub(crate) unsafe fn EmptyClipboard() -> Result<(), ()> {
                with(|s| {
                    assert!(s.opened);
                    s.empties += 1;
                    if s.fail == "empty" {
                        return Err(());
                    }
                    s.clipboard.clear();
                    s.owner = s.opened_owner;
                    Ok(())
                })
            }
            pub(crate) unsafe fn SetClipboardData(
                format: u32,
                handle: Option<HANDLE>,
            ) -> Result<HANDLE, ()> {
                with(|s| {
                    assert!(s.opened);
                    s.publishes += 1;
                    if s.owner == 0 || s.fail == "publish" {
                        return Err(());
                    }
                    let handle = handle.unwrap();
                    let id = handle.0 as usize;
                    let bytes = s
                        .blocks
                        .remove(&id)
                        .unwrap()
                        .iter()
                        .flat_map(|u| u.to_le_bytes())
                        .collect();
                    s.clipboard.push((format, bytes));
                    s.transferred.insert(id);
                    Ok(handle)
                })
            }
            pub(crate) unsafe fn CloseClipboard() -> Result<(), ()> {
                with(|s| {
                    assert!(s.opened);
                    s.opened = false;
                    s.closes += 1;
                });
                Ok(())
            }
        }
    }
    pub mod UI {
        pub mod WindowsAndMessaging {
            use crate::{c_void, with, Win32::Foundation::HWND};
            pub struct WINDOW_EX_STYLE(pub u32);
            pub struct WINDOW_STYLE(pub u32);
            pub const HWND_MESSAGE: HWND = HWND(-3isize as *mut c_void);
            #[allow(clippy::too_many_arguments)]
            pub(crate) unsafe fn CreateWindowExW(
                ex: WINDOW_EX_STYLE,
                class: &str,
                name: &str,
                style: WINDOW_STYLE,
                x: i32,
                y: i32,
                w: i32,
                h: i32,
                parent: Option<HWND>,
                _menu: Option<()>,
                _instance: Option<()>,
                _param: Option<*const c_void>,
            ) -> Result<HWND, ()> {
                assert_eq!(
                    (ex.0, class, name, style.0, x, y, w, h),
                    (0, "STATIC", "", 0, 0, 0, 0, 0)
                );
                assert_eq!(parent.unwrap().0, HWND_MESSAGE.0);
                with(|s| {
                    s.creates += 1;
                    if s.fail == "owner" {
                        Err(())
                    } else {
                        Ok(HWND(99usize as *mut c_void))
                    }
                })
            }
            pub(crate) unsafe fn DestroyWindow(h: HWND) -> Result<(), ()> {
                assert_eq!(h.0 as usize, 99);
                with(|s| {
                    assert!(!s.opened, "CloseClipboard must precede DestroyWindow");
                    s.destroys += 1;
                    s.owner = 0;
                });
                Ok(())
            }
        }
    }
}
#[path = "../src/clipboard_write.rs"]
mod clipboard_write;
fn write(text: &str) -> bool {
    unsafe { clipboard_write::write_text(text, || true) }
}
#[test]
fn explicit_copy_publishes_exact_chinese_english_multiline_and_unicode() {
    for text in [
        "English text",
        "中文复制",
        "  中文 😀\r\n\tEnglish e\u{301}\n𐐀  ",
        "",
        "a",
    ] {
        reset("");
        assert!(write(text), "copy failed for {text:?}");
        let expected: Vec<u8> = text
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        with(|s| {
            assert_eq!(s.clipboard, vec![(13, expected)]);
            assert_eq!((s.opens, s.empties, s.publishes, s.closes), (1, 1, 1, 1));
            assert_eq!((s.creates, s.destroys, s.frees), (1, 1, 0));
            assert!(s.blocks.is_empty());
        });
    }
}
#[test]
fn prepublication_failures_preserve_original_text_html_rtf_and_image_without_retry() {
    for fail in ["allocate", "lock", "owner", "open", "empty"] {
        reset(fail);
        assert!(!write("replacement"), "{fail}");
        with(|s| {
            assert_eq!(s.clipboard, original(), "{fail} destroyed original formats");
            assert_eq!(s.publishes, 0, "{fail}");
            assert!(s.opens <= 1 && s.empties <= 1);
            assert!(!s.opened && s.blocks.is_empty());
            assert_eq!(s.destroys, usize::from(s.creates == 1 && fail != "owner"));
        });
    }
}
#[test]
fn rejected_restore_guard_never_empties_or_publishes() {
    reset("");
    assert!(!unsafe { clipboard_write::write_text("replacement", || false) });
    with(|s| {
        assert_eq!(s.clipboard, original());
        assert_eq!((s.empties, s.publishes, s.closes, s.frees), (0, 0, 1, 1));
        assert!(!s.opened && s.blocks.is_empty());
    });
}
#[test]
fn publish_failure_returns_false_and_releases_resources_without_retry() {
    reset("publish");
    assert!(!write("replacement"));
    with(|s| {
        assert_eq!(
            (s.opens, s.empties, s.publishes, s.closes, s.frees),
            (1, 1, 1, 1, 1)
        );
        assert!(!s.opened && s.blocks.is_empty());
    });
}
#[test]
fn rapid_success_busy_success_cannot_publish_old_text_or_reuse_owned_memory() {
    reset("");
    assert!(write("first 中文"));
    let first = with(|s| s.clipboard.clone());
    with(|s| s.fail = "open");
    assert!(!write("must not publish"));
    with(|s| {
        assert_eq!(s.clipboard, first);
        s.fail = "";
    });
    assert!(write("last 😀\n  line"));
    let expected: Vec<u8> = [
        108u16, 97, 115, 116, 32, 0xD83D, 0xDE00, 10, 32, 32, 108, 105, 110, 101, 0,
    ]
    .into_iter()
    .flat_map(u16::to_le_bytes)
    .collect();
    with(|s| {
        assert_eq!(s.clipboard, vec![(13, expected)]);
        assert_eq!(
            (s.opens, s.empties, s.publishes, s.closes, s.frees),
            (3, 2, 2, 2, 1)
        );
        assert_eq!((s.creates, s.destroys), (3, 3));
        assert!(!s.opened && s.blocks.is_empty());
    });
}
