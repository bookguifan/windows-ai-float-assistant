//! 悬浮球和托盘共用自绘菜单：setup 创建隐藏 WebView，动作按打开代次消费一次。

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

const LABEL: &str = "context-menu";
const WIDTH: f64 = 200.0;
const HEIGHT: f64 = 200.0;
static READY: AtomicBool = AtomicBool::new(false);
static VISIBLE: AtomicBool = AtomicBool::new(false);
static STORE: Mutex<MenuStore> = Mutex::new(MenuStore::new());

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MenuView {
    panel_url: String,
    sites: Vec<crate::panel_site::Site>,
    generation: u64,
    ball_visible: bool,
    panel_visible: bool,
    hotkey: Option<String>,
    shot_busy: bool,
}

#[derive(Clone)]
struct MenuRequest {
    view: MenuView,
    anchor: (i32, i32),
    bounds: Option<(i32, i32, i32, i32)>,
}

impl MenuRequest {
    fn outside(&self, x: i32, y: i32) -> bool {
        self.bounds.is_some_and(|(left, top, width, height)| {
            x < left
                || y < top
                || i64::from(x) >= i64::from(left) + i64::from(width)
                || i64::from(y) >= i64::from(top) + i64::from(height)
        })
    }
}

struct MenuStore {
    next: u64,
    pending: Option<MenuRequest>,
}

impl MenuStore {
    const fn new() -> Self {
        Self {
            next: 0,
            pending: None,
        }
    }

    fn open(&mut self, mut request: MenuRequest) -> Option<MenuRequest> {
        if self.next >= 9_007_199_254_740_991 {
            return None;
        }
        self.next += 1;
        request.view.generation = self.next;
        self.pending = Some(request.clone());
        Some(request)
    }

    fn take(&mut self, generation: u64) -> Option<MenuRequest> {
        if self
            .pending
            .as_ref()
            .is_some_and(|r| r.view.generation == generation)
        {
            self.pending.take()
        } else {
            None
        }
    }
}

fn store() -> std::sync::MutexGuard<'static, MenuStore> {
    STORE.lock().unwrap_or_else(|p| p.into_inner())
}

/// 仅在 setup 中创建；Windows 事件处理期间同步 build WebView2 会死锁。
pub(crate) fn ensure(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(LABEL) {
        return Some(w);
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("poc/menu.html".into()))
        .title("悬浮助手快捷操作")
        .inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focusable(true)
        .visible(false)
        .build();
    match built {
        Ok(win) => {
            crate::diag::log("context-menu: 窗口创建成功，等待页面握手");
            let app = app.clone();
            win.on_window_event(move |event| match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    dismiss(&app);
                }
                tauri::WindowEvent::Focused(false) if VISIBLE.load(Ordering::SeqCst) => {
                    dismiss(&app)
                }
                _ => {}
            });
            Some(win)
        }
        Err(e) => {
            crate::diag::warn(&format!("context-menu: 创建失败: {e}"));
            None
        }
    }
}

pub fn open(app: &AppHandle) {
    crate::diag::log("context-menu: 收到打开请求");
    let app_c = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        crate::popup::hide(&app_c);
        let panel_visible = app_c
            .get_webview_window("panel")
            .is_some_and(|p| p.is_visible().unwrap_or(false) && !p.is_minimized().unwrap_or(false));
        let request = MenuRequest {
            view: MenuView {
                panel_url: crate::config::panel_url(),
                sites: crate::panel_site::choices(&crate::config::panel_url()),
                generation: 0,
                ball_visible: !crate::panels::ball_is_hidden(),
                panel_visible,
                hotkey: crate::hotkey::current(),
                shot_busy: crate::screenshot::capture_active(),
            },
            anchor: crate::popup::cursor_pos(),
            bounds: None,
        };
        let request = store().open(request);
        let Some(request) = request else {
            return;
        };
        if app_c.get_webview_window(LABEL).is_none() {
            crate::diag::warn("context-menu: 启动时未创建菜单窗口");
            close(&app_c, request.view.generation);
            return;
        }
        if READY.load(Ordering::SeqCst) {
            present(&app_c);
        }
    }) {
        crate::diag::warn(&format!("context-menu: 无法打开: {e}"));
    }
}

/// 页面建好全局渲染钩子后握手，首开不会展示空白帧。
pub fn ready(app: &AppHandle) {
    crate::diag::log("context-menu: 收到页面就绪握手");
    let app_c = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        READY.store(true, Ordering::SeqCst);
        present(&app_c);
    }) {
        crate::diag::warn(&format!("context-menu: 就绪回调失败: {e}"));
    }
}

fn present(app: &AppHandle) {
    let request = store().pending.clone();
    let Some(request) = request else {
        return;
    };
    let Some(win) = app.get_webview_window(LABEL) else {
        return;
    };
    let Ok(payload) = serde_json::to_string(&request.view) else {
        return;
    };
    let app_c = app.clone();
    let generation = request.view.generation;
    if let Err(e) = win.eval_with_callback(
        format!("!!(window.__dsfMenuSet && window.__dsfMenuSet({payload}))"),
        move |raw| {
            crate::diag::log(&format!("context-menu: 页面渲染结果={raw}"));
            if raw.trim() == "true" {
                display(&app_c, generation);
            } else {
                close(&app_c, generation);
            }
        },
    ) {
        crate::diag::warn(&format!("context-menu: 渲染失败: {e}"));
        close(app, generation);
    }
}

fn display(app: &AppHandle, generation: u64) {
    let request = store()
        .pending
        .clone()
        .filter(|r| r.view.generation == generation);
    let Some(request) = request else {
        return;
    };
    let Some(win) = app.get_webview_window(LABEL) else {
        return;
    };
    let (cx, cy) = request.anchor;
    let monitor = app
        .monitor_from_point(cx as f64, cy as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        close(app, generation);
        return;
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let size = (
        (WIDTH * scale).round() as i32,
        (HEIGHT * scale).round() as i32,
    );
    let gap = (6.0 * scale).round() as i32;
    let (x, y) = crate::popup::place_near(
        (cx + gap, cy + gap),
        (cx, cy, 0, 0),
        size,
        (
            area.position.x,
            area.position.y,
            area.size.width as i32,
            area.size.height as i32,
        ),
        gap,
    );
    let result = win
        .set_position(PhysicalPosition::new(x, y))
        .and_then(|_| win.set_size(LogicalSize::new(WIDTH, HEIGHT)))
        .and_then(|_| win.set_position(PhysicalPosition::new(x, y)));
    if let Err(e) = result {
        crate::diag::warn(&format!("context-menu: 定位失败: {e}"));
        close(app, generation);
        return;
    }
    if let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) {
        if let Some(r) = store()
            .pending
            .as_mut()
            .filter(|r| r.view.generation == generation)
        {
            r.bounds = Some((pos.x, pos.y, size.width as i32, size.height as i32));
        }
    }
    VISIBLE.store(true, Ordering::SeqCst);
    crate::diag::log("context-menu: 正在显示窗口");
    if let Err(e) = win.show() {
        crate::diag::warn(&format!("context-menu: 显示失败: {e}"));
        close(app, generation);
    }
    // 不在 show 后强制 set_focus，沿用项目的 WebView2 稳定性约束。
}

pub fn outside_generation(x: i32, y: i32) -> Option<u64> {
    if !VISIBLE.load(Ordering::SeqCst) {
        return None;
    }
    // 鼠标钩子绝不等待窗口线程持有的锁。
    let guard = STORE.try_lock().ok()?;
    guard
        .pending
        .as_ref()
        .filter(|r| r.outside(x, y))
        .map(|r| r.view.generation)
}

fn hide_window(app: &AppHandle) {
    VISIBLE.store(false, Ordering::SeqCst);
    if let Some(win) = app.get_webview_window(LABEL) {
        let _ = win.hide();
    }
    crate::flush_ball(app);
}

pub fn close(app: &AppHandle, generation: u64) {
    let app_c = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        // 点外关闭来自鼠标 worker；检查和窗口隐藏必须与新 open/display 串行。
        let consumed = store().take(generation).is_some();
        if consumed {
            hide_window(&app_c);
        }
    }) {
        crate::diag::warn(&format!("context-menu: 关闭回调失败: {e}"));
    }
}

fn dismiss(app: &AppHandle) {
    let generation = store().pending.as_ref().map(|r| r.view.generation);
    if let Some(generation) = generation {
        close(app, generation);
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuAction {
    ShowBall,
    HideBall,
    ShowPanel,
    HidePanel,
    Screenshot,
    Ocr,
    Favorites,
    Settings,
    Relogin,
    Quit,
}

pub fn on_action(app: &AppHandle, generation: u64, action: MenuAction) {
    let app_c = app.clone();
    if let Err(e) = app.run_on_main_thread(move || apply_action(&app_c, generation, action)) {
        crate::diag::warn(&format!("context-menu: 动作投递失败: {e}"));
    }
}

pub fn switch_site(app: &AppHandle, generation: u64, url: String) -> Result<String, String> {
    if !crate::config::is_valid_panel_url(&url) {
        return Err("面板网址无效".into());
    }
    if store().pending.as_ref().map(|r| r.view.generation) != Some(generation) {
        return Err("菜单已过期，请重新打开".into());
    }
    let applied = crate::panel_site::switch(app, url)?;
    if store().take(generation).is_some() {
        hide_window(app);
    }
    Ok(applied)
}

fn apply_action(app: &AppHandle, generation: u64, action: MenuAction) {
    let accepted = store().take(generation).is_some();
    if !accepted {
        return;
    }
    hide_window(app);
    match action {
        MenuAction::ShowBall => {
            crate::panels::reveal_ball(app);
            crate::fullscreen::note_manual_ball_visibility_change();
        }
        MenuAction::HideBall => {
            crate::panels::ball_hide(app);
            crate::fullscreen::note_manual_ball_visibility_change();
        }
        MenuAction::ShowPanel => {
            crate::show_panel(app);
        }
        MenuAction::HidePanel => crate::hide_panel(app),
        MenuAction::Screenshot => crate::screenshot::begin(app),
        MenuAction::Ocr => {
            crate::screenshot::begin_with_mode(app, crate::screenshot::ShotMode::Ocr)
        }
        MenuAction::Settings => crate::settings::open(app),
        MenuAction::Favorites => crate::result::open_favorites(app),
        MenuAction::Relogin => {
            crate::show_panel(app);
            if let Some(panel) = app.get_webview_window("panel") {
                if let Ok(url) = serde_json::to_string(&crate::config::panel_url()) {
                    let _ = panel.eval(format!("location.href={url};"));
                }
            }
        }
        MenuAction::Quit => {
            crate::lifecycle::prepare_exit();
            std::process::exit(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> MenuRequest {
        MenuRequest {
            view: MenuView {
                panel_url: crate::config::DEFAULT_PANEL_URL.into(),
                sites: crate::panel_site::choices(crate::config::DEFAULT_PANEL_URL),
                generation: 0,
                ball_visible: true,
                panel_visible: false,
                hotkey: None,
                shot_busy: false,
            },
            anchor: (100, 100),
            bounds: None,
        }
    }

    #[test]
    fn actions_consume_the_current_menu_only_once() {
        let mut store = MenuStore::new();
        let menu = store.open(request()).unwrap();
        assert!(store.take(menu.view.generation).is_some());
        assert!(store.take(menu.view.generation).is_none());
    }

    #[test]
    fn old_close_or_action_cannot_consume_a_reopened_menu() {
        let mut store = MenuStore::new();
        let old = store.open(request()).unwrap();
        let new = store.open(request()).unwrap();
        assert!(store.take(old.view.generation).is_none());
        assert_eq!(
            store.pending.as_ref().unwrap().view.generation,
            new.view.generation
        );
        assert!(store.take(new.view.generation).is_some());
    }

    #[test]
    fn outside_hit_test_uses_physical_window_bounds() {
        let mut r = request();
        r.bounds = Some((-1920, 50, 340, 435));
        assert!(!r.outside(-1900, 80));
        assert!(r.outside(-1580, 80));
        assert!(r.outside(-1900, 485));
        assert!(r.outside(100, 100));
    }
}
