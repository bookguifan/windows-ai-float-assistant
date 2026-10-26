//! 全局低级别鼠标钩子（WH_MOUSE_LL）—— 类豆包，是划词**唯一**的触发源：
//!   1) 光标选中文字（拖动后松开左键）→ 触发划词工具条；
//!   2) 工具条可见时，在工具条外点击（左键按下）→ 自动隐藏工具条。
//!
//! 早期版本的全局热键（Ctrl+Alt+D）已移除，无需按键即可触发。
//!
//! 关键设计（★ 都是踩坑点）：
//!   - 钩子装在独立「泵线程」上——低级别钩子要求安装线程有消息循环，否则回调不会到达；
//!   - 钩子回调**只做轻量事**（记录按下位置 / 判定拖动 / 发信号），绝不阻塞：
//!     真正的抓词会阻塞约 400ms（剪贴板轮询），若放在泵线程里，低级别钩子会因
//!     LowLevelHooksTimeout 被系统自动摘除。故抓词放到独立 worker 线程；
//!   - 只在「拖动」松开左键时触发划词（按下/抬起位移 > 阈值），避免普通点击也去抢剪贴板；
//!   - 「点击工具条外 → 隐藏」在 WM_LBUTTONDOWN 立即发信号（不防抖），隐藏要即时。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetAncestor, GetMessageW, GetWindowThreadProcessId,
    PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WindowFromPoint,
    GA_ROOT, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT,
    WM_RBUTTONDOWN,
};

use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT};
unsafe extern "system" fn foreground_changed(
    _: HWINEVENTHOOK,
    _: u32,
    _: windows::Win32::Foundation::HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    crate::selection_runtime::invalidate();
}

/// 鼠标钩子产生的事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseEvent {
    /// 拖动选中了文字（松开左键且位移超阈值）→ 抓词弹工具条。
    Selection(crate::selection::Request),
    /// 工具条可见时在工具条外点击 → 隐藏工具条。
    Dismiss(u64),
    /// 自绘菜单外按下鼠标；携带代次，迟到事件不关闭新菜单。
    DismissMenu(u64),
}

/// 判定「拖动」的物理像素阈值。
const DRAG_THRESHOLD: i32 = 8;
/// 两次「划词触发」的最小间隔（防抖）。
const DEBOUNCE_MS: u64 = 300;

static STARTED: AtomicBool = AtomicBool::new(false);
static STOPPED: AtomicBool = AtomicBool::new(false);
static PUMP_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static SIGNAL: Mutex<Option<SyncSender<MouseEvent>>> = Mutex::new(None);
static DOWN_POS: Mutex<Option<(i32, i32)>> = Mutex::new(None);
static DOWN_TARGET: Mutex<Option<crate::selection::Target>> = Mutex::new(None);
static LAST_TRIGGER: AtomicU64 = AtomicU64::new(0);
/// 「截图遮罩显示中」→ 暂停划词判定。遮罩上的框选（按下→松开，位移必然超阈值）
/// 若不挂起会被当成划词触发，白等轮询并弹出工具条盖住遮罩（设计文档 §5.5）。
static SUSPENDED: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn debounce_ok() -> bool {
    let now = now_ms();
    let last = LAST_TRIGGER.load(Ordering::SeqCst);
    if now.saturating_sub(last) < DEBOUNCE_MS {
        return false;
    }
    LAST_TRIGGER.store(now, Ordering::SeqCst);
    true
}

fn emit(ev: MouseEvent) {
    let tx = SIGNAL.lock().ok().and_then(|g| g.as_ref().cloned());
    if let Some(tx) = tx {
        let _ = tx.try_send(ev);
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // ★ 最前置的挂起判定：截图中（遮罩上的框选）不得触发任何划词语义。
    if code >= 0 && !SUSPENDED.load(Ordering::SeqCst) {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        match wparam.0 as u32 {
            WM_LBUTTONDOWN => {
                if let Some(generation) =
                    crate::context_menu::outside_generation(info.pt.x, info.pt.y)
                {
                    emit(MouseEvent::DismissMenu(generation));
                }
                if let Ok(mut g) = DOWN_POS.lock() {
                    // 球窗 NOACTIVATE，前台仍可能是其他应用；不能靠前台 PID 排除球拖动。
                    // WebView 子 HWND 可能属于 renderer，按命中窗口的顶层 owner PID 判定。
                    let hit = WindowFromPoint(info.pt);
                    let root = GetAncestor(hit, GA_ROOT);
                    let mut owner = 0;
                    GetWindowThreadProcessId(root, Some(&mut owner));
                    if owner != GetCurrentProcessId()
                        || !crate::popup::point_in_popup(info.pt.x, info.pt.y)
                    {
                        crate::selection_runtime::invalidate();
                        if let Ok(mut target) = DOWN_TARGET.lock() {
                            *target = crate::selection_runtime::target();
                        }
                    }
                    *g = (owner != GetCurrentProcessId()).then_some((info.pt.x, info.pt.y));
                }
                // 工具条可见时，在它外面按下左键 → 立即请求隐藏
                if crate::popup::popup_visible()
                    && !crate::popup::point_in_popup(info.pt.x, info.pt.y)
                {
                    emit(MouseEvent::Dismiss(crate::selection_runtime::generation()));
                }
            }
            WM_RBUTTONDOWN => {
                crate::selection_runtime::invalidate();
                if let Some(generation) =
                    crate::context_menu::outside_generation(info.pt.x, info.pt.y)
                {
                    emit(MouseEvent::DismissMenu(generation));
                }
            }
            WM_LBUTTONUP => {
                let dragged = DOWN_POS
                    .lock()
                    .ok()
                    .and_then(|mut g| g.take())
                    .map(|(x, y)| {
                        (info.pt.x - x).abs() > DRAG_THRESHOLD
                            || (info.pt.y - y).abs() > DRAG_THRESHOLD
                    })
                    .unwrap_or(false);
                let original = DOWN_TARGET.lock().ok().and_then(|mut t| t.take());
                if dragged
                    && original.is_some()
                    && original == crate::selection_runtime::target()
                    && debounce_ok()
                {
                    if let Some(request) = crate::selection_runtime::request((info.pt.x, info.pt.y))
                    {
                        emit(MouseEvent::Selection(request));
                    }
                }
            }
            _ => {}
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// 启动鼠标钩子。`on_event` 会在 worker 线程上被调用（Selection 只投递有界请求，Dismiss 不等待 UIA）。
pub fn start<F>(on_event: F) -> bool
where
    F: Fn(MouseEvent) + Send + 'static,
{
    if STARTED.swap(true, Ordering::SeqCst) {
        return true;
    }
    crate::diag::log("mousehook::start 被调用");

    // 控制 worker：只转发到有界选区调度器，关闭事件不等待 UIA。
    let (tx, rx) = std::sync::mpsc::sync_channel::<MouseEvent>(16);
    // ★ 健壮性（P4）：锁一旦 poison（极少，仅在持锁 panic 时）就别再 unwrap 造成二次崩溃，
    //   改为降级——通道建不起来，钩子事件发不出去，但进程不崩。
    if let Ok(mut g) = SIGNAL.lock() {
        *g = Some(tx);
    } else {
        crate::diag::log(
            "mousehook::start: SIGNAL 锁已 poison，钩子信号通道无法建立（划词将不可用）",
        );
    }
    std::thread::spawn(move || {
        while let Ok(ev) = rx.recv() {
            match ev {
                MouseEvent::Selection(request) => {
                    on_event(MouseEvent::Selection(request));
                }
                MouseEvent::Dismiss(generation) => {
                    on_event(MouseEvent::Dismiss(generation));
                }
                MouseEvent::DismissMenu(generation) => {
                    on_event(MouseEvent::DismissMenu(generation));
                }
            }
        }
    });

    // pump：装钩子 + 消息循环（低级别钩子要求）
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<bool>();
    std::thread::spawn(move || unsafe {
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), None, 0);
        let ok = hook.is_ok();
        crate::diag::log(if ok {
            "SetWindowsHookExW(WH_MOUSE_LL) OK"
        } else {
            "SetWindowsHookExW(WH_MOUSE_LL) FAILED —— 划词功能不可用（无其他触发源可降级）"
        });
        match hook {
            Ok(h) => {
                let foreground = SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    None,
                    Some(foreground_changed),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                );
                if foreground.is_invalid() {
                    let _ = UnhookWindowsHookEx(h);
                    let _ = ready_tx.send(false);
                    crate::diag::log("selection status=FOREGROUND_MONITOR_UNAVAILABLE");
                    return;
                }
                PUMP_THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);
                let _ = ready_tx.send(true);
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = UnhookWinEvent(foreground);
                let _ = UnhookWindowsHookEx(h);
            }
            Err(_) => {
                let _ = ready_tx.send(false);
            }
        }
    });

    ready_rx
        .recv_timeout(Duration::from_secs(3))
        .unwrap_or(false)
}

/// 退出清理：给泵线程投递 WM_QUIT，循环退出时自动 UnhookWindowsHookEx。
pub fn stop() {
    crate::selection_runtime::stop();
    let id = PUMP_THREAD_ID.load(Ordering::SeqCst);
    if id != 0 && !STOPPED.swap(true, Ordering::SeqCst) {
        unsafe {
            let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

/// 挂起 / 恢复划词判定。截图遮罩显示期间置 `true`（调用方：`screenshot::begin` / `finish`）。
/// 只影响钩子回调的判定，**不卸载钩子** —— 恢复后立刻可用，无安装/卸载开销与超时风险。
pub fn set_suspend(v: bool) {
    if v {
        crate::selection_runtime::invalidate();
    }
    SUSPENDED.store(v, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::{debounce_ok, now_ms, LAST_TRIGGER};
    use std::sync::atomic::Ordering;
    static DEBOUNCE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn now_ms_returns_plausible_epoch_millis() {
        // 2023-01-01 ≈ 1_671_244_800_000 ms；任何正常运行环境都应远大于此。
        let t = now_ms();
        assert!(
            t > 1_700_000_000_000,
            "now_ms 应返回合理的纪元毫秒，实际 {t}"
        );
    }

    #[test]
    fn debounce_blocks_immediate_repeat() {
        let _guard = DEBOUNCE_TEST.lock().unwrap_or_else(|p| p.into_inner());
        // 复位：首次视为「久未触发」应被允许。
        LAST_TRIGGER.store(0, Ordering::SeqCst);
        assert!(debounce_ok(), "复位后首次划词应被允许");
        // 紧接着的第二次（间隔远小于 DEBOUNCE_MS）必须被拦截，防止一次拖动连发。
        // 该断言与初始全局态无关（间隔≈0ns < 300ms 恒成立），抗并行测试干扰。
        assert!(!debounce_ok(), "瞬间重复触发必须被防抖拦截");
    }

    #[test]
    fn debounce_blocks_when_recently_triggered() {
        let _guard = DEBOUNCE_TEST.lock().unwrap_or_else(|p| p.into_inner());
        // 把上次触发时间设为「刚刚」，下一发必被拦截（即便真实时钟仅前进微秒）。
        LAST_TRIGGER.store(now_ms(), Ordering::SeqCst);
        assert!(!debounce_ok(), "距上次触发过近时必须被防抖拦截");
    }
}
