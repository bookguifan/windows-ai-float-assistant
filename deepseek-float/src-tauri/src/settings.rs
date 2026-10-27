//! §5：划词工具条的「更多设置」窗口。
//!
//! 用途：管理自定义提示词 + 对当前抓到的选区文本运行某个自定义提示词。
//!
//! 生命周期：与 popup 同样的常驻策略——`setup()` 内一次性常驻创建（visible=false），
//! 后续只 show/hide，**不**反复建/毁 webview（Windows 上反复建/毁是 WebView2 类
//! 反注册 abort 即 Error 1411/1412 的主要来源）。
//!
//! 与 popup 的差异：这是**装饰性、可缩放**的常规窗口（类比 panel），
//! 关闭按钮走 prevent_close + hide 模式，保留窗口与已加载的提示词列表。

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
static SELECTION: Mutex<crate::selection::SettingsSelection> =
    Mutex::new(crate::selection::SettingsSelection::empty());
static WAS_FOCUSED: AtomicBool = AtomicBool::new(false);

pub const SETTINGS_LABEL: &str = "settings";
const LOGICAL_W: f64 = 460.0;
const LOGICAL_H: f64 = 560.0;
const MIN_W: f64 = 380.0;
const MIN_H: f64 = 420.0;

/// setup 内调用一次：常驻创建 + 注册 close→hide 钩子。已存在则直接返回。
pub fn ensure(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(SETTINGS_LABEL) {
        return Some(w);
    }
    let build = WebviewWindowBuilder::new(
        app,
        SETTINGS_LABEL,
        WebviewUrl::App("poc/settings.html".into()),
    )
    .title("DeepSeek 划词设置")
    .inner_size(LOGICAL_W, LOGICAL_H)
    .min_inner_size(MIN_W, MIN_H)
    .resizable(true)
    .always_on_top(true)
    .decorations(true) // 普通窗口：用户可用标题栏拖动与原生关闭
    .visible(false)
    .build();
    match build {
        Ok(w) => {
            crate::diag::log("settings 窗口创建成功（常驻，visible=false）");
            // 关闭 → 隐藏（不销毁）：保留加载过的提示词状态
            let h = app.clone();
            w.on_window_event(move |event| match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    clear_selection(&h);
                    WAS_FOCUSED.store(false, Ordering::Release);
                    if let Some(win) = h.get_webview_window(SETTINGS_LABEL) {
                        let _ = win.hide();
                    }
                }
                tauri::WindowEvent::Focused(focused) => {
                    let was = WAS_FOCUSED.swap(*focused, Ordering::AcqRel);
                    if was && !focused {
                        clear_selection(&h);
                    }
                }
                tauri::WindowEvent::Destroyed => {
                    revoke_selection();
                    WAS_FOCUSED.store(false, Ordering::Release);
                }
                _ => {}
            });
            Some(w)
        }
        Err(e) => {
            crate::diag::error(&format!("settings 窗口创建失败: {e}"));
            None
        }
    }
}

/// 打开设置窗口，并把「当前抓到的选区文本 + 已持久化的提示词列表」一次性注入。
///
/// `text` 长度被粗略截到 800 字，避免对 settings 的 webview eval 输入巨大 JSON
/// （JSON 走 serde_json 转义本身就是安全的，但浏览器一次性解析 100k 字符无意义）。
pub fn open(app: &AppHandle) {
    clear_selection(app);
    let Some(w) = ensure(app) else {
        return;
    };
    let revision = SELECTION
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .revision();
    show(app, &w, String::new(), None, revision);
}
/// Only an explicit toolbar action may delegate a currently verified selection.
pub(crate) fn open_from_selection(app: &AppHandle, action: crate::selection::ToolbarAction) {
    crate::selection_trace::session("settings-handoff-input", action.session());
    let source_request = action.session().request.clone();
    clear_selection(app);
    let window = ensure(app);
    let now = crate::selection_runtime::now_ms();
    let (preview, ticket, revision) = {
        let mut state = SELECTION.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(w) = &window {
            if let Ok(hwnd) = w.hwnd() {
                state.prepare_action(
                    action,
                    crate::selection_runtime::generation(),
                    now,
                    hwnd.0 as isize,
                );
            }
        }
        let preview: String = window
            .as_ref()
            .and_then(|w| w.hwnd().ok())
            .and_then(|h| state.preview(h.0 as isize, now))
            .unwrap_or_default()
            .chars()
            .take(800)
            .collect();
        (preview, state.ticket(), state.revision())
    };
    crate::selection_trace::text(
        "settings-preview",
        &source_request,
        ticket.unwrap_or(0),
        &preview,
    );
    crate::selection_trace::event(
        "settings-handoff",
        ticket,
        if ticket.is_some() {
            "prepared"
        } else {
            "rejected"
        },
    );
    crate::popup::hide(app);
    if let Some(w) = window {
        show(app, &w, preview, ticket, revision);
    }
}
fn show(app: &AppHandle, w: &WebviewWindow, preview: String, ticket: Option<u64>, revision: u64) {
    let prompts = crate::prompts::load(app);
    let payload = serde_json::json!({"text":preview,"prompts":prompts,"selection_ticket":ticket.map(|t|t.to_string()),"selection_revision":revision.to_string()});
    crate::diag::log(&format!(
        "selection stage=settings-show revision={revision} ticket={ticket:?} now_ms={}",
        crate::selection_runtime::now_ms()
    ));
    let js = format!(
        "if(window.__dsfSettingsInit)window.__dsfSettingsInit({});",
        payload
    );
    if w.show().is_err() {
        clear_selection(app);
        return;
    }
    if let Err(e) = w.eval(js) {
        clear_selection(app);
        crate::diag::warn(&format!("settings init eval failed: {e}"));
    }
}
pub(crate) fn revoke_selection() {
    SELECTION.lock().unwrap_or_else(|p| p.into_inner()).clear();
}
fn clear_preview(app: &AppHandle, revision: u64) {
    if let Some(w) = app.get_webview_window(SETTINGS_LABEL) {
        let js = format!(
            "if(window.__dsfSettingsSelectionExpired)window.__dsfSettingsSelectionExpired({});",
            serde_json::json!(revision.to_string())
        );
        let _ = w.eval(js);
    }
}
pub(crate) fn clear_selection(app: &AppHandle) {
    let (had, revision) = {
        let mut s = SELECTION.lock().unwrap_or_else(|p| p.into_inner());
        (s.clear(), s.revision())
    };
    if had {
        clear_preview(app, revision);
    }
}
pub(crate) fn expire_selection(app: &AppHandle) {
    let (expired, revision) = {
        let mut s = SELECTION.lock().unwrap_or_else(|p| p.into_inner());
        (s.expire(crate::selection_runtime::now_ms()), s.revision())
    };
    if expired {
        clear_preview(app, revision);
    }
}
pub(crate) fn take_selection(
    app: &AppHandle,
    window: &WebviewWindow,
    ticket: &str,
) -> Result<String, String> {
    if !crate::safe_copy::trusted(window) {
        return Err("只能在聚焦的本地设置页运行提示词".into());
    }
    let ticket = ticket.parse::<u64>().map_err(|_| "选区凭据无效")?;
    let hwnd = window.hwnd().map_err(|_| "设置窗口不可用")?;
    let now = crate::selection_runtime::now_ms();
    let (text, revision, has_current) = {
        let mut s = SELECTION.lock().unwrap_or_else(|p| p.into_inner());
        let text = s.take(hwnd.0 as isize, ticket, now);
        (
            text,
            s.revision(),
            s.preview(hwnd.0 as isize, now).is_some(),
        )
    };
    // An old request must neither consume nor visually revoke a newer grant.
    if text.is_some() || !has_current {
        clear_preview(app, revision);
    }
    text.ok_or_else(|| "选区已更新、过期或已使用，请重新划词后从工具条进入设置".into())
}

/// Refresh the already-open settings page after a backup restore without reopening the window.
pub fn refresh_after_backup(app: &AppHandle) {
    clear_selection(app);
    let Some(window) = app.get_webview_window(SETTINGS_LABEL) else {
        return;
    };
    let prompts = crate::prompts::load(app);
    let payload = serde_json::json!({"prompts": prompts});
    let js = format!(
        "if(window.__dsfSettingsDataRestored)window.__dsfSettingsDataRestored({});",
        payload
    );
    if let Err(error) = window.eval(js) {
        crate::diag::warn(&format!("settings: backup restore 后刷新界面失败: {error}"));
    }
}

pub(crate) fn ready_snapshot(app: &AppHandle, window: &WebviewWindow) -> Option<serde_json::Value> {
    if !window.is_visible().unwrap_or(false) {
        return None;
    }
    let hwnd = window.hwnd().ok()?.0 as isize;
    let now = crate::selection_runtime::now_ms();
    let (text, ticket, revision) = {
        let mut s = SELECTION.lock().unwrap_or_else(|p| p.into_inner());
        s.expire(now);
        (
            s.preview(hwnd, now)
                .unwrap_or_default()
                .chars()
                .take(800)
                .collect::<String>(),
            s.ticket(),
            s.revision(),
        )
    };
    crate::diag::log(&format!(
        "selection stage=settings-ready revision={revision} ticket={ticket:?} now_ms={now}"
    ));
    Some(
        serde_json::json!({"text":text,"prompts":crate::prompts::load(app),"selection_ticket":ticket.map(|t|t.to_string()),"selection_revision":revision.to_string()}),
    )
}
