//! One native worker, one replaceable pending request. No forced COM cancellation.
use crate::selection::{Latest, Method, Request, Session, Target};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Condvar, Mutex, OnceLock,
};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use windows::Win32::{
    Foundation::HWND,
    System::Threading::GetCurrentProcessId,
    UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, IsWindow},
};
static CLOCK: OnceLock<Instant> = OnceLock::new();
static GENERATION: AtomicU64 = AtomicU64::new(1);
static STOP: AtomicBool = AtomicBool::new(false);
static PENDING: Mutex<Latest> = Mutex::new(Latest::empty());
static READY: Condvar = Condvar::new();
static ACTIVE: Mutex<Option<Request>> = Mutex::new(None);
static START: std::sync::Once = std::sync::Once::new();
pub fn now_ms() -> u64 {
    CLOCK.get_or_init(Instant::now).elapsed().as_millis() as u64
}
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}
pub fn invalidate() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}
pub fn invalidate_if(generation: u64) {
    let _ = GENERATION.compare_exchange(
        generation,
        generation.wrapping_add(1),
        Ordering::AcqRel,
        Ordering::Acquire,
    );
}
pub fn target() -> Option<Target> {
    unsafe {
        let h = GetForegroundWindow();
        if !IsWindow(Some(h)).as_bool() {
            return None;
        }
        let mut pid = 0;
        let thread = GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == 0 || thread == 0 || pid == GetCurrentProcessId() {
            return None;
        }
        Some(Target {
            hwnd: h.0 as isize,
            pid,
            thread,
        })
    }
}
pub fn capture_enabled() -> bool {
    crate::config::load().privileged_capture || crate::safe_copy::state().enabled
}
pub fn valid(r: &Request) -> bool {
    !STOP.load(Ordering::Acquire) && capture_enabled() && r.valid(target(), generation(), now_ms())
}
pub fn request(point: (i32, i32)) -> Option<Request> {
    let target = target()?;
    let generation = generation();
    let now = now_ms();
    Some(Request {
        target,
        point,
        generation,
        session_id: generation,
        created_ms: now,
        expires_ms: now + 1500,
    })
}
pub fn submit(app: &AppHandle, r: Request) {
    start(app.clone());
    if valid(&r) {
        PENDING.lock().unwrap_or_else(|p| p.into_inner()).replace(r);
        READY.notify_one();
    }
}
fn start(app: AppHandle) {
    START.call_once(move || {
        let worker_app = app.clone();
        std::thread::spawn(move || loop {
            let r = {
                let mut q = PENDING.lock().unwrap_or_else(|p| p.into_inner());
                loop {
                    if STOP.load(Ordering::Acquire) {
                        return;
                    }
                    if let Some(r) = q.take() {
                        break r;
                    }
                    q = READY.wait(q).unwrap_or_else(|p| p.into_inner());
                }
            };
            if !valid(&r) {
                continue;
            }
            let started_ms = now_ms();
            *ACTIVE.lock().unwrap_or_else(|p| p.into_inner()) = Some(r.clone());
            let cfg = crate::config::load();
            if cfg.privileged_capture || crate::safe_copy::state().enabled {
                let Some(read) =
                    crate::selection::read_current(|| valid(&r), || crate::uia::read(&r))
                else {
                    *ACTIVE.lock().unwrap_or_else(|p| p.into_inner()) = None;
                    continue;
                };
                crate::diag::log(&format!(
                    "selection status={} elapsed_ms={} request={} target_pid={} queue_ms={}",
                    read.status,
                    now_ms().saturating_sub(r.created_ms),
                    r.generation,
                    r.target.pid,
                    started_ms.saturating_sub(r.created_ms)
                ));
                if let Some(text) = read.text {
                    if valid(&r) {
                        crate::selection_trace::text("uia-read", &r, r.session_id, &text);
                        crate::popup::accept_session(
                            &worker_app,
                            Session::new(r.clone(), text, Method::Uia),
                        );
                    }
                } else if read.safe_target && valid(&r) {
                    let native = crate::native_selection::capture(&r, || {
                        valid(&r) && crate::config::load().privileged_capture
                    });
                    if let Ok(text) = native {
                        if valid(&r) && crate::config::load().privileged_capture {
                            crate::popup::accept_session(
                                &worker_app,
                                Session::new(r.clone(), text, Method::NativeEdit),
                            );
                        }
                    } else if crate::release_policy::experimental_copy_available() {
                        if let Ok(text) = crate::safe_copy::capture(&r, &worker_app) {
                            if valid(&r) {
                                crate::popup::accept_session(
                                    &worker_app,
                                    Session::new(r.clone(), text, Method::Clipboard),
                                );
                            }
                        }
                    }
                }
            }
            *ACTIVE.lock().unwrap_or_else(|p| p.into_inner()) = None;
        });
        // This supervisor remains responsive even when a provider's COM call never returns.
        std::thread::spawn(move || {
            let mut reported = 0;
            while !STOP.load(Ordering::Acquire) {
                crate::popup::expire_session();
                crate::settings::expire_selection(&app);
                let active = ACTIVE.lock().unwrap_or_else(|p| p.into_inner()).clone();
                if let Some(r) = active {
                    if now_ms() >= r.expires_ms && reported != r.generation {
                        reported = r.generation;
                        crate::diag::log(&format!(
                            "selection status=UIA_WORKER_UNAVAILABLE request={} target_pid={}",
                            r.generation, r.target.pid
                        ));
                    }
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
    });
}
pub fn stop() {
    STOP.store(true, Ordering::Release);
    invalidate();
    crate::safe_copy::revoke();
    crate::settings::revoke_selection();
    // Synchronize with Condvar::wait so exit cannot lose a wake-up.
    let _pending = PENDING.lock().unwrap_or_else(|p| p.into_inner());
    READY.notify_all();
}
pub fn is_native_target(t: Target) -> bool {
    unsafe {
        let h = HWND(t.hwnd as *mut _);
        let mut pid = 0;
        IsWindow(Some(h)).as_bool()
            && GetWindowThreadProcessId(h, Some(&mut pid)) == t.thread
            && pid == t.pid
    }
}
