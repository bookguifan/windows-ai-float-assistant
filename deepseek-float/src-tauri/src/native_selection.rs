//! Exact standard Unicode Edit selection; no clipboard or synthetic input APIs.
use crate::{native_copy_proof, selection::Request};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND},
    System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, IsWindow,
        GA_ROOT, GUITHREADINFO,
    },
};
struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn focus(r: &Request) -> Option<HWND> {
    unsafe {
        let root = HWND(r.target.hwnd as *mut _);
        let mut pid = 0;
        if !IsWindow(Some(root)).as_bool()
            || GetForegroundWindow() != root
            || GetWindowThreadProcessId(root, Some(&mut pid)) != r.target.thread
            || pid != r.target.pid
        {
            return None;
        }
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(r.target.thread, &mut info).ok()?;
        let h = info.hwndFocus;
        if !IsWindow(Some(h)).as_bool()
            || GetAncestor(h, GA_ROOT) != root
            || GetWindowThreadProcessId(h, Some(&mut pid)) != r.target.thread
            || pid != r.target.pid
        {
            return None;
        }
        Some(h)
    }
}
/// `valid` includes current request generation/deadline and UIA-enhancement permission.
pub(crate) fn capture(r: &Request, valid: impl Fn() -> bool) -> Result<String, &'static str> {
    if !valid() {
        return Err("EDIT_CONTEXT_CHANGED");
    }
    let h = focus(r).ok_or("EDIT_TARGET_UNVERIFIED")?;
    let process = Process(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, r.target.pid) }
            .map_err(|_| "EDIT_TARGET_UNVERIFIED")?,
    );
    let guard = || {
        let mut code = 0;
        valid()
            && focus(r) == Some(h)
            && unsafe { GetExitCodeProcess(process.0, &mut code).is_ok() }
            && code == 259
    };
    native_copy_proof::read_stable(guard, || {
        native_copy_proof::capture_observed(h, guard, |start, end, total, text| {
            crate::selection_trace::range(r, h.0 as isize, start, end, total, text);
            crate::selection_trace::text("native-read", r, r.session_id, text);
        })
    })
}
