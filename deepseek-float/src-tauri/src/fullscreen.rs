//! §9：全屏隐藏 —— 轮询前台窗口，若其覆盖整个屏幕则隐藏悬浮球（离开全屏再恢复）。
//!
//! 判定口径（经典「覆盖整屏」法）：前台窗口矩形 ≈ 显示器 `rcMonitor` 全矩形（±1px），
//! 且可见、未最小化。足以覆盖视频全屏、游戏、F11 浏览器全屏等场景。
//!
//! 用 `FULLSCREEN_HIDING` 标记「本次是因全屏而隐藏」，离开全屏时只恢复被我们藏起来的球，
//! 恢复时会比较进入全屏时的用户显隐代次；若托盘/球菜单期间有过明确显隐操作，以用户操作为准。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tauri::Manager;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowRect, IsIconic, IsWindowVisible,
};

static FULLSCREEN_HIDING: AtomicBool = AtomicBool::new(false);
/// 进入全屏前球是否可见。离开全屏时只恢复「本就被我们藏起来」的球。
static BALL_WAS_VISIBLE: AtomicBool = AtomicBool::new(true);
/// 进入全屏前结果卡(result)是否可见 —— 进全屏时一并 hide，离开全屏按需恢复。
static RESULT_WAS_VISIBLE: AtomicBool = AtomicBool::new(true);
/// User-requested ball visibility changes. Fullscreen transitions snapshot this value so their
/// delayed hide/restore work cannot overwrite a newer explicit menu action.
static USER_BALL_VISIBILITY_GENERATION: AtomicU64 = AtomicU64::new(0);
static BALL_VISIBILITY_GENERATION_AT_ENTRY: AtomicU64 = AtomicU64::new(0);
/// 全屏隐藏开关的内存缓存（线程每轮只读原子量，不再每轮读 config 文件）。
/// 由设置命令 `config_set_fullscreen` 经 [`mark_enabled`] 同步；线程启动时用配置初始化一次。
static ENABLED: AtomicBool = AtomicBool::new(false);

/// 供设置命令同步全屏隐藏开关（避免常驻轮询线程每轮读盘）。
pub fn mark_enabled(v: bool) {
    ENABLED.store(v, Ordering::SeqCst);
}

/// Record an explicit Show/Hide action from the shared tray/ball menu.
pub fn note_manual_ball_visibility_change() {
    USER_BALL_VISIBILITY_GENERATION.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn user_ball_visibility_generation() -> u64 {
    USER_BALL_VISIBILITY_GENERATION.load(Ordering::SeqCst)
}

fn should_apply_fullscreen_hide(
    fullscreen_hiding: bool,
    entry_generation: u64,
    current_generation: u64,
) -> bool {
    fullscreen_hiding && entry_generation == current_generation
}

fn should_restore_ball(
    was_visible: bool,
    entry_generation: u64,
    current_generation: u64,
    fullscreen_hiding: bool,
) -> bool {
    was_visible && entry_generation == current_generation && !fullscreen_hiding
}

fn should_defer_capture_restore(enabled: bool, fullscreen: bool) -> bool {
    enabled && fullscreen
}

/// Complete the screenshot's temporary hide without briefly exposing the ball over a fullscreen app.
/// Called on Tauri's main thread alongside explicit menu visibility actions.
pub(crate) fn restore_ball_after_capture(app: &tauri::AppHandle, entry_generation: u64) {
    if USER_BALL_VISIBILITY_GENERATION.load(Ordering::SeqCst) != entry_generation {
        crate::diag::log("shot: 用户显隐选择已更新，保留当前悬浮球状态");
        return;
    }
    if should_defer_capture_restore(ENABLED.load(Ordering::SeqCst), foreground_is_fullscreen()) {
        FULLSCREEN_HIDING.store(true, Ordering::SeqCst);
        BALL_WAS_VISIBLE.store(true, Ordering::SeqCst);
        BALL_VISIBILITY_GENERATION_AT_ENTRY.store(entry_generation, Ordering::SeqCst);
        RESULT_WAS_VISIBLE.store(
            app.get_webview_window("result")
                .and_then(|window| window.is_visible().ok())
                .unwrap_or(false),
            Ordering::SeqCst,
        );
        crate::panels::ball_hide(app);
        crate::popup::hide(app);
        if let Some(window) = app.get_webview_window("result") {
            let _ = window.hide();
        }
        crate::panels::flush_ball(app);
        crate::diag::log("shot: 全屏仍活动，悬浮球恢复交由全屏退出流程处理");
    } else {
        show_ball(app);
    }
}

/// 全屏检测启用时的轮询间隔（ms）—— 前台窗口状态变化要够快感知。
/// 250ms ≈ 每秒 4 次探测，实测空闲 CPU 影响≈0，兼顾「游戏/视频切全屏立即隐藏」。
const POLL_ACTIVE_MS: u64 = 250;
/// 全屏检测关闭时的轮询间隔（ms）—— 只用于「每 1s 检查配置是否重新打开」，降频省 CPU。
const POLL_IDLE_MS: u64 = 1000;

/// 启动全屏监视后台线程。flag 关时线程空转（每 1s 检查一次配置是否重新打开）。
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        // 用配置初始化一次开关缓存；之后仅由设置命令 mark_enabled 同步。
        ENABLED.store(crate::config::load().hide_on_fullscreen, Ordering::SeqCst);
        // 高频轮询：250ms；关掉功能时降到 1s 省 CPU。
        loop {
            // ★ 截图中：遮罩是「覆盖整屏」的窗口，会被 foreground_is_fullscreen() 判成全屏，
            //   从而无谓地隐藏悬浮球 → 此时直接跳过本轮判定（不改状态，退出截图后自然恢复）。
            if crate::screenshot::capture_active() {
                std::thread::sleep(Duration::from_millis(POLL_ACTIVE_MS));
                continue;
            }
            if ENABLED.load(Ordering::SeqCst) {
                let fs = foreground_is_fullscreen();
                if fs && !FULLSCREEN_HIDING.swap(true, Ordering::SeqCst) {
                    // 记录隐藏前的可见性：用户若已手动隐藏，离开全屏时不应把球「复活」。
                    // ★ 逻辑显隐（2026-09-25）：判定用 ball_is_hidden（逻辑隐藏下 is_visible 恒 true）。
                    let visibility_generation =
                        USER_BALL_VISIBILITY_GENERATION.load(Ordering::SeqCst);
                    let was_visible = !crate::panels::ball_is_hidden();
                    BALL_WAS_VISIBLE.store(was_visible, Ordering::SeqCst);
                    BALL_VISIBILITY_GENERATION_AT_ENTRY
                        .store(visibility_generation, Ordering::SeqCst);
                    // 结果卡同样记录可见性：用户若已手动关掉，离开全屏时也不应弹回。
                    RESULT_WAS_VISIBLE.store(
                        app.get_webview_window("result")
                            .and_then(|w| w.is_visible().ok())
                            .unwrap_or(true),
                        Ordering::SeqCst,
                    );
                    crate::diag::log("fullscreen: 检测到全屏，隐藏悬浮球");
                    hide_ball(&app, visibility_generation);
                } else if !fs && FULLSCREEN_HIDING.swap(false, Ordering::SeqCst) {
                    if BALL_WAS_VISIBLE.load(Ordering::SeqCst) {
                        crate::diag::log("fullscreen: 离开全屏，恢复悬浮球");
                        restore_ball_if_unchanged(
                            &app,
                            BALL_VISIBILITY_GENERATION_AT_ENTRY.load(Ordering::SeqCst),
                        );
                    } else {
                        crate::diag::log("fullscreen: 离开全屏，但球此前已被用户隐藏，保持隐藏");
                    }
                    if RESULT_WAS_VISIBLE.load(Ordering::SeqCst) {
                        show_result(&app);
                    }
                }
                std::thread::sleep(Duration::from_millis(POLL_ACTIVE_MS));
            } else {
                // 功能关闭：若之前因全屏藏起了球（且它本就可见），才恢复。
                if FULLSCREEN_HIDING.swap(false, Ordering::SeqCst) {
                    if BALL_WAS_VISIBLE.load(Ordering::SeqCst) {
                        restore_ball_if_unchanged(
                            &app,
                            BALL_VISIBILITY_GENERATION_AT_ENTRY.load(Ordering::SeqCst),
                        );
                    }
                    if RESULT_WAS_VISIBLE.load(Ordering::SeqCst) {
                        show_result(&app);
                    }
                }
                std::thread::sleep(Duration::from_millis(POLL_IDLE_MS));
            }
        }
    });
}

fn hide_ball(app: &tauri::AppHandle, visibility_generation: u64) {
    // Serialize the automatic transition with menu actions on Tauri's main thread. If the user
    // acted after this transition was observed, their newer choice wins for the ball itself.
    let app_c = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        let fullscreen_hiding = FULLSCREEN_HIDING.load(Ordering::SeqCst);
        if !fullscreen_hiding {
            return;
        }
        let current_generation = USER_BALL_VISIBILITY_GENERATION.load(Ordering::SeqCst);
        if should_apply_fullscreen_hide(
            fullscreen_hiding,
            visibility_generation,
            current_generation,
        ) {
            // ★ 逻辑隐藏替代 hide()，保持球合成表面常驻。
            crate::panels::ball_hide(&app_c);
        } else {
            crate::diag::log("fullscreen: 检测到更新的用户显隐操作，跳过悬浮球自动隐藏");
        }
        // 工具条和结果卡仍按全屏策略收起。
        crate::popup::hide(&app_c);
        if let Some(w) = app_c.get_webview_window("result") {
            let _ = w.hide();
        }
        crate::flush_ball(&app_c);
        crate::diag::log("fullscreen: 已隐藏工具条/结果卡并冲刷球合成表面");
    }) {
        crate::diag::warn(&format!("fullscreen: 自动隐藏任务投递失败: {e}"));
    }
}

fn show_ball(app: &tauri::AppHandle) {
    // ★ 统一走 `reveal_ball`（逻辑显隐 + 三层冲刷，含无标题栏确认），替换此前裸 `show()`。
    crate::panels::reveal_ball(app);
}

fn restore_ball_if_unchanged(app: &tauri::AppHandle, entry_generation: u64) {
    let app_c = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        let current_generation = USER_BALL_VISIBILITY_GENERATION.load(Ordering::SeqCst);
        let fullscreen_hiding = FULLSCREEN_HIDING.load(Ordering::SeqCst);
        if !should_restore_ball(
            true,
            entry_generation,
            current_generation,
            fullscreen_hiding,
        ) {
            crate::diag::log("fullscreen: 用户显隐操作或新的全屏状态已更新，跳过悬浮球自动恢复");
            return;
        }
        show_ball(&app_c);
    }) {
        crate::diag::warn(&format!("fullscreen: 自动恢复任务投递失败: {e}"));
    }
}

fn show_result(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("result") {
        let _ = w.show();
    }
}

/// 前台窗口是否「覆盖整个屏幕」（全屏）。
fn foreground_is_fullscreen() -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return false;
        }
        // 我们的窗口（ball 64x64 / popup / panel）不可能「覆盖整屏」，直接放行判断即可；
        // 但若是 panel 被用户拉到全屏尺寸也跟随隐藏，属可接受行为。

        // 跳过最小化 / 不可见窗口
        if IsIconic(fg).as_bool() || !IsWindowVisible(fg).as_bool() {
            return false;
        }
        // 跳过被 DWM cloaked 的窗口（虚拟桌面切换等）
        let mut cloaked: i32 = 0;
        if DwmGetWindowAttribute(
            fg,
            DWMWA_CLOAKED,
            &mut cloaked as *mut i32 as *mut _,
            std::mem::size_of::<i32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return false;
        }

        let mut rect = windows::Win32::Foundation::RECT::default();
        if GetWindowRect(fg, &mut rect).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
        if mon.is_invalid() {
            return false;
        }
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        rect.left <= m.left + 1
            && rect.top <= m.top + 1
            && rect.right >= m.right - 1
            && rect.bottom >= m.bottom - 1
    }
}

#[cfg(test)]
mod tests {
    use super::{should_apply_fullscreen_hide, should_defer_capture_restore, should_restore_ball};

    #[test]
    fn normal_fullscreen_transition_hides_then_restores_a_previously_visible_ball() {
        assert!(should_apply_fullscreen_hide(true, 7, 7));
        assert!(should_restore_ball(true, 7, 7, false));
    }

    #[test]
    fn does_not_restore_a_ball_that_was_hidden_before_fullscreen() {
        assert!(!should_restore_ball(false, 7, 7, false));
    }

    #[test]
    fn manual_show_or_hide_keeps_delayed_automatic_visibility_work_from_overriding_it() {
        for current_generation in [8, 9] {
            assert!(!should_apply_fullscreen_hide(true, 7, current_generation));
            assert!(!should_restore_ball(true, 7, current_generation, false));
        }
    }

    #[test]
    fn repeated_user_changes_keep_an_older_fullscreen_snapshot_stale() {
        assert!(!should_apply_fullscreen_hide(true, 3, 10));
        assert!(!should_restore_ball(true, 3, 10, false));
    }

    #[test]
    fn delayed_automatic_callbacks_respect_the_current_fullscreen_state() {
        assert!(!should_apply_fullscreen_hide(false, 7, 7));
        assert!(!should_restore_ball(true, 7, 7, true));
    }

    #[test]
    fn screenshot_restore_waits_for_fullscreen_exit_when_fullscreen_hiding_is_enabled() {
        assert!(should_defer_capture_restore(true, true));
        assert!(!should_defer_capture_restore(false, true));
        assert!(!should_defer_capture_restore(true, false));
    }
}
