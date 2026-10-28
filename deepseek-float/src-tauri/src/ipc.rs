//! 类型化 IPC 命令（替代原先的字符串事件 + serde_json::Value 手工解析）。
//!
//! 本地三窗（ball/popup/settings）通过 `window.__TAURI__.core.invoke` 调用；
//! 参数由 Tauri 自动反序列化并校验，命令名/参数拼写错误在编译期即可发现。
//!
//! 命令函数一律 `fn`（私有于本模块）；由 `lifecycle::run` 的 `generate_handler!` 经
//! `crate::ipc::*` 路径引用。`#[tauri::command]` 宏生成的 `__cmd__*` 辅助函数为 `pub`，
//! 故无需把命令本体设为 `pub`（避免与 `generate_handler!` 产生宏展开冲突）。

use tauri::{AppHandle, Manager};

// ---- 类型化 IPC 命令（替代原先的字符串事件 + serde_json::Value 手工解析）----

/// 面板「是否呈现给用户」的判定（纯函数，便于单测 —— 这是「面板唤不回」真机缺陷的修复点）。
///
/// 口径：`is_visible()` 底层是 `IsWindowVisible`，**最小化的窗口同样返回 TRUE**，于是
/// 「面板被最小化、屏幕上根本没有它」会被判成「可见」→ 点球走进「收起」分支 → 用户永远
/// 唤不回面板。故必须叠加 `!minimized`：**已最小化一律视为不可见**。
/// 行为与抽取前的内联表达式 `visible && !minimized` **逐位等价**。
fn panel_shown(visible: bool, minimized: bool) -> bool {
    visible && !minimized
}

/// 球态左键：切换面板显示/隐藏（首次显示时懒跳 DeepSeek）。
///
/// 返回 `Result<String, String>`：成功后给用户可见文案；失败（面板窗不可用）走 Err，
/// 由前端统一 catch 判错 —— 与 `set_hotkey` 的命令层返回形态保持一致。
/// （错误不直接用 `AppError`：它内含不可序列化的 `io::Error`/`tauri::Error`，
///   让 IPC 命令 `Result<_, AppError>` 需额外编解码，且会破坏 error.rs 的 source 链设计。
///   UI 文案错误用 String 承载是命令层的既定口径，见 `error.rs` 模块头。）
#[tauri::command]
pub fn toggle_panel(app: AppHandle) -> Result<String, String> {
    let Some(p) = app.get_webview_window("panel") else {
        return Err("面板窗口未创建（见 poc2-state.log）".into());
    };
    // ★ 判定口径修正（真机缺陷「结果卡弹窗后面板无法显示」）：见 [`panel_shown`] ——
    //   最小化窗口的 `is_visible()` 仍为 true，必须叠加 `!is_minimized()` 才代表「用户看得见」。
    let visible = panel_shown(
        p.is_visible().unwrap_or(false),
        p.is_minimized().unwrap_or(false),
    );
    let rect = match (p.outer_position(), p.inner_size()) {
        (Ok(pos), Ok(sz)) => {
            format!(" pos=({},{}) size={}x{}", pos.x, pos.y, sz.width, sz.height)
        }
        _ => String::new(),
    };
    if visible {
        crate::hide_panel(&app);
        // ★ 带上几何读数：下次再报「面板出不来」，一眼可判是「最小化」还是「跑到屏幕外」。
        crate::diag::log(&format!("toggle-panel: 面板已收起（判定为可见{rect}）"));
        Ok("面板已收起".into())
    } else {
        match crate::show_panel(&app) {
            Some(_) => {
                crate::diag::log(&format!("toggle-panel: 面板已展开（判定为不可见{rect}）"));
                Ok("面板已展开（加载 DeepSeek…）".into())
            }
            None => Err("面板窗口未创建（见 poc2-state.log）".into()),
        }
    }
}

/// 球态右键：打开自绘圆角菜单。旧 x/y 参数保留兼容，定位使用实际桌面光标坐标。
#[tauri::command]
pub fn ball_contextmenu(app: AppHandle, x: f64, y: f64) {
    let _ = (x, y);
    crate::ballstyle::enforce_captionless(&app);
    crate::context_menu::open(&app);
}

#[tauri::command]
pub fn context_menu_ready(app: AppHandle) {
    crate::context_menu::ready(&app);
}

#[tauri::command]
pub fn context_menu_action(
    app: AppHandle,
    generation: u64,
    action: crate::context_menu::MenuAction,
) {
    crate::context_menu::on_action(&app, generation, action);
}

#[tauri::command]
pub fn context_menu_close(app: AppHandle, generation: u64) {
    crate::context_menu::close(&app, generation);
}
/// 悬浮球「首帧就绪」握手：由 `poc/ball.html` 在**首个渲染帧之后**调用。
///
/// 启动期 [`crate::panels::gate_ball_show`] 把球窗维持为**零面积空区域**（整窗不可见）并在
/// 后台等待本信号；收到后（或 ~3s 兜底超时）才套椭圆区域显形 —— 目的是让球「一露面即已绘制
/// 完成」，消除「启动瞬间球空白」。
///
/// **幂等**：重复调用安全（[`crate::ballready::BallReadyGate::signal`] 只置位一次）。
/// 无参数、无返回值：本命令只承担「置位就绪闸门」这一件事。
#[tauri::command]
pub fn ball_ready() {
    crate::ballready::signal_ball_ready();
    crate::diag::log("ball_ready: 收到悬浮球首帧就绪握手信号（Rust 侧随后套椭圆区域显形）");
}

/// Synthetic-only telemetry; rejects arbitrary stages/actions and external documents.
#[tauri::command]
pub fn popup_trace(
    window: tauri::WebviewWindow,
    stage: String,
    action: String,
    selection_ticket: Option<String>,
) -> Result<(), String> {
    if !crate::selection_trace::enabled() {
        return Ok(());
    }
    let url = window.url().map_err(|_| "工具条地址不可用")?;
    if !crate::selection::local_page(
        window.label(),
        &url,
        "popup",
        window.is_visible().unwrap_or(false),
    ) {
        return Err("只能由本地工具条记录合成诊断".into());
    }
    crate::selection_trace::frontend(
        &stage,
        &action,
        selection_ticket.and_then(|s| s.parse().ok()),
    );
    Ok(())
}

/// 工具条按钮点击：preset 为 copy / open-settings / 提示词 id。
#[tauri::command]
pub fn popup_action(
    app: AppHandle,
    window: tauri::WebviewWindow,
    preset: String,
    selection_ticket: String,
) -> Result<(), String> {
    let url = window.url().map_err(|_| "工具条地址不可用")?;
    if !crate::selection::local_page(
        window.label(),
        &url,
        "popup",
        window.is_visible().unwrap_or(false),
    ) {
        return Err("只能从本地工具条执行选区动作".into());
    }
    let ticket = selection_ticket
        .parse::<u64>()
        .map_err(|_| "选区凭据无效")?;
    crate::selection_trace::event("ipc-action", Some(ticket), "received");
    crate::popup::on_action(&app, &preset, ticket);
    Ok(())
}
#[tauri::command]
pub fn settings_ready(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Option<serde_json::Value>, String> {
    let url = window.url().map_err(|_| "设置页地址不可用")?;
    if !crate::selection::local_page(window.label(), &url, "settings", true) {
        return Err("只能由本地设置页读取当前快照".into());
    }
    Ok(crate::settings::ready_snapshot(&app, &window))
}

/// 设置页保存提示词列表（内置 + 自定义，覆盖式写入）。
#[tauri::command]
pub fn settings_save(
    app: AppHandle,
    prompts: Vec<crate::prompts::Prompt>,
) -> Result<Vec<crate::prompts::Prompt>, String> {
    match crate::prompts::save(&app, &prompts) {
        Ok(saved) => Ok(saved),
        Err(e) => Err(e.to_string()),
    }
}

/// 设置页运行某条提示词。
#[tauri::command]
pub fn settings_run(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    selection_ticket: String,
) -> Result<(), String> {
    let text = crate::settings::take_selection(&app, &window, &selection_ticket)?;
    crate::popup::run_prompt(&app, &id, text);
    Ok(())
}

/// 读取应用级偏好配置（位置记忆 / 开机自启 / 全屏隐藏 / 提权划词开关）。
///
/// `autostart` 以**注册表真实状态**为准（覆盖落盘值），避免用户已在任务管理器里
/// 禁用启动项后，设置页开关仍显示「已开启」。
///
/// ★ 2026-09-24 真机缺陷修正：再叠加「**能否真正生效**」这一层 ——
///   开发构建（无法独立运行）本就不支持开机自启，即便注册表里恰有一条也不该显示为「已开启」，
///   否则会出现「开关显示已开、开机后球却是坏的」这种误导（见 `autostart.rs` 模块头）。
#[tauri::command]
pub fn config_get() -> crate::config::AppConfig {
    let mut cfg = crate::config::load();
    cfg.autostart = crate::autostart::registration_supported() && crate::autostart::is_autostart();
    cfg
}

/// 当前构建**是否支持**开机自启（纯编译期判定：仅 release + custom-protocol 支持）。
///
/// 设置页据此在 dev 构建下禁用「开机自启」开关并给出「请用 release 版」的明确提示 ——
/// 让用户**看得到原因与出路**，而不是开关点了没反应、或显示已开启却开机后球是坏的。
#[tauri::command]
pub fn autostart_supported() -> bool {
    crate::autostart::registration_supported()
}

/// 设置开机自启：写入/删除注册表 Run 项；仅在注册表操作成功后持久化开关。
/// 返回注册表操作是否成功（失败时前端据此回滚开关）。
///
/// 注意：`set_autostart(true)` 在开发构建下**必定返回 false**（拒绝登记，见 `autostart.rs`）；
/// 而 `set_autostart(false)` 始终执行删除，保证用户能清掉既有的脏项。
#[tauri::command]
pub fn config_set_autostart(enable: bool) -> bool {
    let ok = crate::autostart::set_autostart(enable);
    if ok {
        crate::config::update(|c| c.autostart = enable);
    }
    ok
}

/// 设置全屏隐藏开关（持久化；同步 fullscreen 监视线程的内存缓存实时生效）。
#[tauri::command]
pub fn config_set_fullscreen(enable: bool) -> bool {
    crate::config::update(|c| c.hide_on_fullscreen = enable);
    // 同步常驻全屏监视线程的内存缓存（避免它每轮读盘）。
    crate::fullscreen::mark_enabled(enable);
    true
}

/// 设置提权划词（UI Automation）开关（持久化；trigger_selection 实时读取）。
#[tauri::command]
pub fn config_set_privileged(enable: bool) -> bool {
    if crate::config::try_update(|c| c.privileged_capture = enable).is_err() {
        return false;
    }
    // Revocation remains effective even if the user quickly re-enables the preference.
    if !enable {
        crate::selection_runtime::invalidate();
    }
    true
}

/// 保留旧命令兼容；首版公开构建拒绝开启，只允许持久化关闭。
#[tauri::command]
pub fn config_set_clipboard_fallback(enable: bool) -> Result<(), String> {
    crate::release_policy::validate_automatic_copy_request(enable)?;
    crate::config::try_update(|c| c.clipboard_fallback_enabled = enable)?;
    Ok(())
}

#[tauri::command]
pub fn safe_copy_status() -> crate::safe_copy::State {
    crate::safe_copy::state()
}
#[tauri::command]
pub fn safe_copy_revoke() -> crate::safe_copy::State {
    crate::safe_copy::revoke();
    crate::safe_copy::state()
}
#[tauri::command]
pub async fn safe_copy_authorize(
    window: tauri::WebviewWindow,
) -> Result<crate::safe_copy::State, String> {
    tauri::async_runtime::spawn_blocking(move || crate::safe_copy::request_authorization(window))
        .await
        .map_err(|_| "授权确认失败".to_string())?
}

/// ★ 2026-09-17：设置「面板目标网址」（DeepSeek 页面可切换 —— 镜像 / 自托管 / 排障换源）。
///
/// 只做**基础安全校验**（http(s) + 含域名 + 无空格/引号/尖括号），不穷尽 URL 合法性：
/// 目标是防注入进 `eval` 的 JS 字符串，而非替用户验证网址能否打开。失败返回错误文案（前端提示）。
///
/// ⚠️ 持久化后，**若面板已存在且不在空白页**，会立即把它导航到新网址（即时生效）；
///    若面板尚是空白页（未加载），则留给 `ensure_panel_loaded` 首次懒跳时用新值。
#[tauri::command]
pub fn config_set_panel_url(app: AppHandle, url: String) -> Result<String, String> {
    crate::panel_site::switch(&app, url)
}

#[tauri::command]
pub fn panel_sites_save(
    sites: Vec<crate::panel_site::Site>,
) -> Result<Vec<crate::panel_site::Site>, String> {
    crate::panel_site::save(sites)
}

#[tauri::command]
pub fn context_menu_switch_site(
    app: AppHandle,
    generation: u64,
    url: String,
) -> Result<String, String> {
    crate::context_menu::switch_site(&app, generation, url)
}
/// ★ R13-S4：设置截图**全局热键**。空串 = 关闭热键。
///
/// 返回**实际生效**的 spec（设置页据此刷新输入框）；失败返回错误文案供前端提示并回滚。
///
/// 顺序要紧：**先 `hotkey::apply` 成功、再落盘** —— 否则会出现「配置里存着一个注册不上的
/// 键」这种下次启动才暴露的不一致。`apply` 内部保证事务性（失败回滚旧键）。
#[tauri::command]
pub fn set_hotkey(spec: String) -> Result<String, String> {
    let trimmed = spec.trim().to_string();
    let opt = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    };
    let _data = crate::data_store::write_guard();
    crate::hotkey::apply(opt.as_deref())?;
    crate::config::update_under_data_lock(|c| c.hotkey = opt);
    Ok(crate::hotkey::current().unwrap_or_default())
}

/// ★ R13-S4：当前**实际生效**的热键（`None` = 未注册 —— 可能被其它程序占用，或用户已关闭）。
///
/// 与 `config_get().hotkey`（用户**想要**的 spec）区分：注册失败时两者不一致，
/// 设置页用本命令显示真实状态，避免「显示已启用但其实没注册上」。
#[tauri::command]
pub fn hotkey_status() -> Option<String> {
    crate::hotkey::current()
}

// ---- 「截图提问」S1 命令（遮罩页经 window.__TAURI__.core.invoke 调用）----

/// 框选完成：页内回报窗口内 CSS 坐标（+ DPR + 视口尺寸），载荷见 `screenshot::ShotRect`。
#[tauri::command]
pub async fn shot_commit(
    app: AppHandle,
    capture_id: u64,
    revision: u64,
    rect: crate::screenshot::ShotRect,
) -> Result<crate::screenshot::CapturePayload, String> {
    crate::screenshot::commit(app, capture_id, revision, rect).await
}

/// 取消框选（Esc / 右键 / 误触）。
#[tauri::command]
pub fn shot_cancel(app: AppHandle, capture_id: u64, revision: u64) {
    crate::screenshot::cancel_revision(&app, capture_id, revision);
}

#[tauri::command]
pub fn shot_preview_ready(
    app: AppHandle,
    capture_id: u64,
    revision: u64,
    failed: bool,
) -> Result<bool, String> {
    crate::screenshot::preview_ready(&app, capture_id, revision, failed)
}

#[tauri::command]
pub fn shot_confirm(app: AppHandle, capture_id: u64, revision: u64) -> Result<(), String> {
    crate::screenshot::confirm(&app, capture_id, revision)
}

#[tauri::command]
pub fn shot_reselect(app: AppHandle, capture_id: u64, revision: u64) -> Result<u64, String> {
    crate::screenshot::reselect(&app, capture_id, revision)
}

/// 结果卡：关闭（右上角 × / Esc）。
#[tauri::command]
pub fn result_close(app: AppHandle) {
    crate::result::close(&app);
}

/// 结果卡：「继续读取」。回答可能未完成（回读超时降级）时，重放上一轮回读
/// 就地补齐末轮（见 `readback::resume`）。不重新注入，只补回读。
///
/// 返回状态串（`ok` / `no-resume` / `panel-missing`）供前端即时反馈，
/// 避免「点了没反应」的静默无输出。
#[tauri::command]
pub fn result_resume(app: AppHandle, request: Option<u64>) -> String {
    if request.is_some_and(|id| id != crate::workflow::current().0) {
        return "no-resume".into();
    }
    crate::readback::resume(&app)
}

/// 结果卡：复制**某一轮**的正文到剪贴板（页内把该轮正文一起传上来）。
///
/// ★ R5 起卡片是**多轮**的：`text` 必传（页内按轮次给），缺省时才退回会话末轮。
///   旧口径「复制当前正文」在单轮时代成立，多轮下会复制错轮 —— 这是接口必须显式化的原因。
#[tauri::command]
pub fn result_copy(text: Option<String>) {
    crate::result::copy(text);
}

/// 结果卡：发送输入框里的追问，回答回流成卡片的下一轮。
#[tauri::command]
pub fn result_ask(app: AppHandle, text: String) {
    crate::result::ask(&app, text);
}

/// 结果卡：开始拖动（页内在头部位移超过阈值后调用，见 `poc/result.html`）。
#[tauri::command]
pub fn result_drag_start(app: AppHandle) {
    crate::result::start_drag(&app);
}

/// 结果卡：页内主动请求改尺寸（折叠 / 展开正文、输入框自增高 —— 这些是**纯页内**动作，
/// Rust 侧看不到对应的渲染推送，只能由页内把实测高度报上来）。
#[tauri::command]
pub fn result_resize(app: AppHandle, height: f64) {
    crate::result::resize(&app, height);
}

/// 结果卡：开新会话（右上角「开新会话」图标）—— 清空卡片会话，并让面板开启 DeepSeek 新对话。
///
/// ★ R8-④：面板窗不回调 Tauri（约束 G5），故「开新会话」只能由 Rust 侧 `panel.eval` 触发
///   页内 `__DSFLOAT__.newChat()`，不能在面板脚本里自发调用。
#[tauri::command]
pub fn result_new_session(app: AppHandle) -> Result<(), String> {
    crate::result::new_session(&app)
}

/// 结果卡外链：以系统默认浏览器打开 http(s) 链接，避免卡片 WebView 自导航（SEC1）。
///
/// 仅放行 http:// 与 https://（纵深防御：即便前端校验被绕过，Rust 侧再卡一道）。
/// 外链放行网关（纯函数，便于单测）：仅接受 http(s) 绝对地址。
///
/// **大小写不敏感**：`HTTP://` / `HTTPS://` 亦放行（RFC 3986 规定 scheme 大小写不敏感）。
/// 刻意**不**放行 `javascript:` / `data:` / `blob:` / `file:` / `vbscript:` / 相对路径 / 空串 ——
/// 即便前端校验被绕过，Rust 侧这一道仍会把危险 scheme 拦下。
fn is_allowed_external(url: &str) -> bool {
    let raw = url.trim();
    let lower = raw.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://"))
        && !raw.chars().any(char::is_control)
        && tauri::Url::parse(raw).is_ok_and(|parsed| {
            matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some()
        })
}

#[tauri::command]
pub fn result_open_external(url: String) -> String {
    if !is_allowed_external(&url) {
        return "blocked".into();
    }
    // 通过原生 URL 关联打开；链接整体作为数据传入，不经过 cmd 命令语言。
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let Ok(parsed) = tauri::Url::parse(url.trim()) else {
        return "blocked".into();
    };
    let wide: Vec<u16> = parsed.as_str().encode_utf16().chain(Some(0)).collect();
    // SAFETY: wide 在同步调用期间有效且以 NUL 结束；URL 已校验，不传递命令行参数。
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let status = result.0 as isize;
    if status > 32 {
        "ok".into()
    } else {
        format!("err:无法打开链接（Windows 错误 {status}）")
    }
}

// ★ R13-S5：S1 遗留的 `inspect_upload` **命令**已移除 —— 它自 S1 起就无任何前端调用方
// （探针由托盘菜单在 Rust 侧直接调 `crate::inject::inspect_upload`），是一个**死命令**；
// 且该探针的结论已固化（S3 真机 `fileInputCount=1`），不必再向页面暴露这个入口。
//
// 注：「结果回流」R0 的 `inspect_conversation` 与 R1 的 `begin_watch_diagnostic` **刻意不注册为命令**
// —— 它们只由托盘菜单在 Rust 侧直接调用（`crate::inject::inspect_conversation` /
// `crate::readback::begin_watch_diagnostic`），目前无任何前端调用方，注册即死命令
// （与已被移除的 `shot_begin` 同类）。
// ★ R13-S5：上述三个探针的**托盘菜单项**也已移除；函数保留为排障能力（详见 `setup_tray`）。
// ★ R2 起：结果卡**需要**前端触发（卡片上的复制 / 继续提问 / 关闭 / 拖动），故这一组
//   `result_*` 命令已按 S1 的 `shot_*` 先例正式注册，并同步登记进
//   `capabilities/default.json` 的 `windows`。R3 起改为**一个动作一个命令**
// （原先是 `result_action(action: String)`）：命令名拼错在编译期就会暴露，
// 不必等到真机上「点了没反应」才发现。
// ★ R5 起命令口径随「多轮会话」调整：`result_close`（原 `result_action_close`）、
//   `result_copy(text)`（**必须**带上是哪一轮的正文，多轮下「当前正文」不再是单数）、
//   `result_ask(text)`（文本来自卡片输入框，不再是「把卡片正文再发一遍」）、
//   新增 `result_resize(height)`（页内折叠正文 / 输入框自增高后主动请求改尺寸）。
// （`inspect_upload` 的同款命令是 S1 遗留，同样无调用方；留待 S5 收口一并评估。）

#[cfg(test)]
mod tests {
    use super::{is_allowed_external, panel_shown};

    #[test]
    fn panel_shown_true_only_when_visible_and_not_minimized() {
        // 抽取自 toggle_panel 的判据（「面板唤不回」真机缺陷的修复点），锁定逐位行为。
        assert!(panel_shown(true, false), "可见且未最小化 → 呈现给用户");
        // 最小化：IsWindowVisible 仍为 true，但屏幕上没有它 → 必须判为「不可见」，
        // 否则点球会走「收起」分支、永远唤不回面板。
        assert!(!panel_shown(true, true), "最小化必须视为不可见");
        assert!(!panel_shown(false, false), "不可见 → 不呈现");
        assert!(!panel_shown(false, true), "既不可见又最小化 → 不呈现");
    }

    #[test]
    fn external_allows_http_and_https() {
        for u in [
            "http://example.com",
            "https://example.com",
            "https://example.com/a/b?c=1#d",
            "http://127.0.0.1:8080/x",
        ] {
            assert!(is_allowed_external(u), "应放行：{u}");
        }
    }

    #[test]
    fn external_scheme_is_case_insensitive() {
        // RFC 3986：scheme 大小写不敏感（此前白名单是 starts_with 精确比较，会误拦大写）。
        for u in ["HTTP://example.com", "HTTPS://example.com", "hTtPs://x"] {
            assert!(is_allowed_external(u), "大写 scheme 应放行：{u}");
        }
    }

    #[test]
    fn external_tolerates_surrounding_whitespace() {
        assert!(is_allowed_external("  https://example.com  "));
    }

    #[test]
    fn external_rejects_missing_host_and_embedded_controls() {
        for url in [
            "https://",
            "http://?query",
            "https:example.com",
            "https://example.com/\0x",
            "https://exam\nple.com",
        ] {
            assert!(!is_allowed_external(url), "必须拦截无效地址：{url:?}");
        }
    }

    #[test]
    fn external_preserves_url_data_without_shell_restrictions() {
        assert!(is_allowed_external("https://example.com/?a=1&b=2#代码"));
        assert!(is_allowed_external("https://example.com/?q=%25value%25"));
    }

    #[test]
    fn external_blocks_dangerous_schemes_and_relative() {
        for u in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,<script>",
            "blob:https://x",
            "file:///C:/Windows/System32/calc.exe",
            "vbscript:msgbox(1)",
            "",
            "   ",
            "//example.com",
            "/abs/path",
            "example.com",
            "ftp://example.com",
        ] {
            assert!(!is_allowed_external(u), "必须拦截：{u:?}");
        }
    }
}

#[tauri::command]
pub fn result_retry_send(app: AppHandle, request: u64) -> String {
    crate::readback::retry_send(&app, request)
}

#[tauri::command]
pub fn result_favorite_save(
    revision: u64,
    turn: crate::result::AnswerSnapshot,
) -> Result<crate::favorites::Favorite, String> {
    crate::result::favorite_save(revision, turn)
}
#[tauri::command]
pub fn result_export(
    revision: u64,
    turns: Vec<crate::result::AnswerSnapshot>,
    whole: bool,
) -> Result<String, String> {
    crate::result::export_answers(revision, turns, whole)
}
#[tauri::command]
pub fn favorites_list() -> Result<Vec<crate::favorites::Favorite>, String> {
    crate::favorites::list()
}
#[tauri::command]
pub fn favorites_delete(id: u64) -> Result<(), String> {
    crate::favorites::delete(id)
}
#[tauri::command]
pub fn favorites_export(id: u64) -> Result<String, String> {
    crate::favorites::export_favorite(id)
}

#[tauri::command]
pub fn backup_export(app: AppHandle) -> Result<Option<crate::backup::BackupExportResult>, String> {
    crate::backup::export_dialog(&app)
}

#[tauri::command]
pub fn backup_import_preview(
    app: AppHandle,
) -> Result<Option<crate::backup::BackupPreview>, String> {
    crate::backup::import_preview_dialog(&app)
}

#[tauri::command]
pub fn backup_import_cancel(ticket: u64) {
    crate::backup::cancel_import(ticket);
}

#[tauri::command]
pub fn backup_restore(
    app: AppHandle,
    ticket: u64,
    mode: String,
    confirm_overwrite: bool,
) -> Result<crate::backup::BackupRestoreResult, String> {
    let result = crate::backup::restore(&app, ticket, mode, confirm_overwrite)?;
    crate::safe_copy::revoke();
    crate::selection_runtime::invalidate();
    crate::settings::refresh_after_backup(&app);
    crate::result::refresh_favorites(&app);
    Ok(result)
}

#[tauri::command]
pub fn result_open_exports_dir() -> Result<(), String> {
    crate::favorites::open_exports()
}

#[cfg(test)]
mod public_release_tests {
    #[test]
    fn direct_backend_enable_is_rejected_before_configuration_io() {
        assert!(super::config_set_clipboard_fallback(true).is_err());
    }
}
