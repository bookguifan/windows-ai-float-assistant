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
            w.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Some(win) = h.get_webview_window(SETTINGS_LABEL) {
                        let _ = win.hide();
                    }
                }
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
    let Some(w) = ensure(app) else {
        return;
    };
    let text = crate::popup::peek_pending_text().unwrap_or_default();
    let preview: String = text.chars().take(800).collect();
    let prompts = crate::prompts::load(app);
    let payload = serde_json::json!({
      "text": preview,
      "prompts": prompts,
    });
    let js = format!(
        "if(window.__dsfSettingsInit)window.__dsfSettingsInit({});",
        payload
    );
    let _ = w.show();
    if let Err(e) = w.eval(js) {
        crate::diag::warn(&format!("[M2] settings init eval 失败: {e}"));
    }
}

/// Refresh the already-open settings page after a backup restore without reopening the window.
pub fn refresh_after_backup(app: &AppHandle) {
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
