// Native IPC fixture only: hidden synthetic Edit, no hooks or clipboard operations.
#[path = "../src/copy_engine.rs"]
#[allow(dead_code)]
mod copy_engine;
#[path = "../src/native_copy_proof.rs"]
mod native_copy_proof;
#[path = "../src/selection.rs"]
#[allow(dead_code)]
mod selection;
use std::{
    path::PathBuf,
    process::{Child, Command},
    time::{Duration, Instant},
};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
static OLD_PROC: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
unsafe extern "system" fn slow_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if m == WM_GETTEXT {
        std::thread::sleep(Duration::from_millis(200));
    }
    let old = OLD_PROC.load(std::sync::atomic::Ordering::Acquire);
    let old: WNDPROC = Some(unsafe {
        std::mem::transmute::<isize, unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>(
            old,
        )
    });
    unsafe { CallWindowProcW(old, h, m, w, l) }
}
#[test]
#[ignore = "subprocess fixture; run only via the parent test"]
fn synthetic_edit_child() {
    let dir = PathBuf::from(std::env::var("COPY_PROOF_IPC").unwrap());
    let text: Vec<u16> = "prefix 😀中文\r\n  code <&> tail"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            PCWSTR(text.as_ptr()),
            WINDOW_STYLE(4),
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
    };
    let start = "prefix ".encode_utf16().count();
    let end = start + "😀中文\r\n  code <&>".encode_utf16().count();
    unsafe {
        SendMessageW(hwnd, 0xb1, Some(WPARAM(start)), Some(LPARAM(end as isize)));
    }
    if std::env::var_os("COPY_PROOF_SLOW").is_some() {
        let old = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, slow_proc as *const () as isize) };
        assert_ne!(old, 0);
        OLD_PROC.store(old, std::sync::atomic::Ordering::Release);
    }
    std::fs::write(dir.join("ready"), format!("{}", hwnd.0 as isize)).unwrap();
    let start = Instant::now();
    while !dir.join("release").exists() && start.elapsed() < Duration::from_secs(8) {
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() } {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    unsafe {
        DestroyWindow(hwnd).unwrap();
    }
}
struct Fixture {
    dir: PathBuf,
    child: Child,
    hwnd: HWND,
}
impl Fixture {
    fn new(slow: bool) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "copy-proof-{}-{}",
            std::process::id(),
            if slow { "slow" } else { "exact" }
        ));
        std::fs::create_dir(&dir).expect("fresh IPC directory required");
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "synthetic_edit_child",
            "--ignored",
            "--nocapture",
        ])
        .env("COPY_PROOF_IPC", &dir);
        if slow {
            cmd.env("COPY_PROOF_SLOW", "1");
        }
        let mut child = cmd.spawn().unwrap();
        let start = Instant::now();
        let ready = loop {
            if let Ok(s) = std::fs::read_to_string(dir.join("ready")) {
                break s;
            };
            if start.elapsed() > Duration::from_secs(5) {
                let _ = std::fs::write(dir.join("release"), b"done");
                let _ = child.wait();
                panic!("fixture startup timeout")
            };
            std::thread::sleep(Duration::from_millis(5));
        };
        Self {
            dir,
            child,
            hwnd: HWND(ready.parse::<isize>().unwrap() as *mut _),
        }
    }
    fn valid(&self) -> bool {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(self.hwnd, Some(&mut pid));
        }
        pid == self.child.id()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::write(self.dir.join("release"), b"done");
        let _ = self.child.wait();
    }
}
#[test]
fn cross_process_edit_selection_is_exact_without_clipboard() {
    let f = Fixture::new(false);
    let proof = native_copy_proof::capture(f.hwnd, || f.valid()).unwrap();
    assert_eq!(proof.text, "😀中文\r\n  code <&>");
}
#[test]
fn cross_process_hung_text_read_is_bounded_and_rejected() {
    let f = Fixture::new(true);
    let start = Instant::now();
    assert!(native_copy_proof::capture(f.hwnd, || f.valid()).is_err());
    assert!(start.elapsed() < Duration::from_millis(500));
}
