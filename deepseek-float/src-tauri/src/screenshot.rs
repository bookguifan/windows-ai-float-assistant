//! 截图提问/识字：框选 → GDI/PNG → 自动分块挂图；识字模式待附件上传完成后发送提示词。
//!
//! 只覆盖光标所在显示器，不做跨屏长截图。开始时冻结遮罩客户区物理原点、尺寸和缩放，
//! 由 screenshot_state 将选区局部 CSS 坐标转换为虚拟桌面物理坐标。
//!
//! 编号和模式保存在 Rust 状态中，失败重选增加 revision；PNG 最多缓存一份，自动投递仅消费一次。
//! worker 和页面回调均验证代次；取消后等待 worker 退出，再恢复球和划词挂起。
//!
//! 窗口常驻只 show/hide；关闭请求防销毁，页面看门狗补交初始化编号。
//! 捕获/投递失败在原 shot 窗提供重选、取消。历史决策见 docs/screenshot-qa-design.md。
use std::sync::Mutex;
use std::time::Duration;

use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

/// 遮罩窗 label（第 5 个常驻窗口：ball / panel / popup / settings / shot）。
pub const SHOT_LABEL: &str = "shot";

/// 是否正处于「截图中」。既是重入闸门（防连击叠加第二个遮罩），
/// 也是 `fullscreen.rs` 的挂起信号 —— 遮罩覆盖整屏，不能被当成「全屏应用」。
static CAPTURE: Mutex<CaptureState> = Mutex::new(CaptureState::new());
static CAPTURE_TOKEN: Mutex<crate::workflow::Token> = Mutex::new(crate::workflow::Token(0));

fn capture_token() -> crate::workflow::Token {
    *CAPTURE_TOKEN.lock().unwrap_or_else(|p| p.into_inner())
}

fn active(id: u64, revision: u64, token: crate::workflow::Token) -> bool {
    crate::workflow::active(token) && state().matches(id, revision)
}

pub use crate::screenshot_state::ShotMode;
use crate::screenshot_state::{BallRestore, CaptureGeometry, CaptureState, SelectionRect};

fn state() -> std::sync::MutexGuard<'static, CaptureState> {
    CAPTURE.lock().unwrap_or_else(|p| p.into_inner())
}

/// 「截图识字」的固定提示词（贴图后发送）。
const OCR_PROMPT: &str = "请把这张图片里的文字完整、准确地转录出来，保持原有的段落与换行；不要翻译、不要解释、不要总结，也不要添加原文里没有的内容。";

/// 「截图识字」等附件挂上输入框的最长时间（上传是异步的，见设计文档 §5.6 时序陷阱）。
const ATTACH_WAIT_MS: u64 = 30_000;

/// 是否正处于截图中（供 `fullscreen.rs` 轮询判定）。
pub fn capture_active() -> bool {
    state().active()
}

/// setup 内调用：常驻创建一次（`visible=false`），之后只 show/hide，绝不销毁重建。
///
/// 初值给一个占位尺寸即可 —— 真正的显示器物理矩形在 `begin()` 里按**光标所在屏**精确设置（D3）。
pub fn ensure(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(SHOT_LABEL) {
        return Some(w);
    }
    match WebviewWindowBuilder::new(app, SHOT_LABEL, WebviewUrl::App("poc/shot.html".into()))
        .title("DeepSeek 截图遮罩")
        .inner_size(800.0, 600.0) // 占位；begin() 会覆盖为显示器物理矩形
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        // 同 popup：undecorated + resizable 是 TAURI_DRAG_RESIZE_* 子窗口类（Error 1411/1412）的成因。
        .resizable(false)
        .shadow(false)
        // ★ 与 popup 相反：popup 是 focusable(false)（怕抢焦点丢选区），
        //   遮罩则**必须** focusable(true) —— 要接管焦点才收得到 Esc。
        .focusable(true)
        .visible(false)
        .build()
    {
        Ok(w) => {
            crate::diag::log(
                "shot 遮罩窗创建成功（常驻，visible=false，transparent + always_on_top）",
            );
            // ★ 防销毁守卫：遮罩是唯一 focusable(true) 的功能窗，Alt+F4 / 外部 close 会真的
            //   销毁它 —— 既违反「常驻窗绝不销毁重建」（1411/1412 风险），也会绕过 finish()
            //   使截图占用与 mousehook 挂起永久泄漏。改为「关 = 隐藏 + 复位」。
            let app_c = app.clone();
            w.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    crate::diag::log("shot: 收到关闭请求，改为隐藏并复位挂起");
                    finish(&app_c);
                }
            });
            Some(w)
        }
        Err(e) => {
            crate::diag::error(&format!("shot 遮罩窗创建失败: {e}"));
            None
        }
    }
}

/// 开始框选：把遮罩定位到**光标所在显示器**的物理矩形后显示，并挂起两个既有子系统。
///
/// 结构上保证「挂起不泄漏」：`set_suspend(true)` 只发生在 [`begin_inner`] 的**最后一步**，
/// 任何前置失败都在挂起**之前**返回 `Err`，由本函数统一汇流到 `finish()` 复位
/// （硬约束 §8「异常路径也复位」；等价于 RAII guard 的效果，但更贴合本仓库既有风格）。
///
/// 挂起是必需的（设计文档 §5.5）：
///   - `mousehook`：遮罩上的框选「按下 → 松开」位移必然超阈值，不挂起会被当成划词触发；
///   - `fullscreen`：遮罩覆盖整屏，不挂起会被判成「全屏应用」而隐藏悬浮球。
pub fn begin(app: &AppHandle) {
    begin_with_mode(app, ShotMode::Ask);
}

/// 带模式入口（见 [`ShotMode`]）：「截图提问」与「截图识字」共用框选链路，只在贴图后分叉。
pub fn begin_with_mode(app: &AppHandle, mode: ShotMode) {
    let Some(id) = state().begin(mode) else {
        crate::diag::log("shot: 已在截图中，忽略本次触发（不修改当前模式）");
        return;
    };
    *CAPTURE_TOKEN.lock().unwrap_or_else(|p| p.into_inner()) = crate::workflow::current();
    crate::diag::log(&format!(
        "shot: 进入框选（模式={}）",
        if mode == ShotMode::Ocr {
            "截图识字"
        } else {
            "截图提问"
        }
    ));
    // 热键也可从 worker 触发；窗口准备统一在 UI 线程串行执行。
    let app_c = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        if let Err(e) = begin_inner(&app_c, id) {
            crate::diag::warn(&format!("shot: 开始框选失败（{e}），已复位"));
            finish_capture(&app_c, id);
            return;
        }
        arm_watchdog(&app_c, id);
    }) {
        crate::diag::warn(&format!("shot: 无法准备遮罩（{e}）"));
        finish_capture(app, id);
    }
}

/// [`begin`] 的实际步骤。**任何**失败都返回 `Err` 交调用方统一复位 —— 这样就不存在
/// 「半初始化」（例如已挂起划词、但遮罩没显示出来）的状态。
fn begin_inner(app: &AppHandle, id: u64) -> Result<(), String> {
    if !crate::workflow::active(capture_token()) {
        return Err("截图任务已取消".into());
    }
    if !state().preparing(id) {
        return Err("截图准备请求已过期".into());
    }
    let win = ensure(app).ok_or_else(|| "遮罩窗不可用".to_string())?;

    // 光标所在显示器（D3：仅该屏）；取不到则退回主显示器。
    let (cx, cy) = crate::popup::cursor_pos();
    let monitor = app
        .monitor_from_point(cx as f64, cy as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or_else(|| "取不到显示器信息".to_string())?;

    // 纯物理像素定位 / 定尺寸（Monitor::position / size 即物理像素）。
    let pos = *monitor.position();
    let size = *monitor.size();
    let target = PhysicalSize::new(size.width, size.height);
    win.set_position(PhysicalPosition::new(pos.x, pos.y))
        .map_err(|e| format!("set_position 失败: {e}"))?;
    win.set_size(target)
        .map_err(|e| format!("set_size 失败: {e}"))?;
    // ★ 跨屏移动会触发 WM_DPICHANGED，tao 会应用系统建议矩形（含尺寸）→ 读回校验并纠正一次。
    let actual = win.inner_size().unwrap_or(target);
    if actual.width != target.width || actual.height != target.height {
        crate::diag::warn(&format!(
            "shot: 尺寸被系统改写为 {}x{}（目标 {}x{}），已纠正",
            actual.width, actual.height, target.width, target.height
        ));
        win.set_size(target)
            .map_err(|e| format!("纠正遮罩尺寸失败: {e}"))?;
    }

    // 使用实际客户区物理原点，不把局部 CSS 坐标误当桌面坐标。
    let origin = win
        .inner_position()
        .map_err(|e| format!("读取遮罩原点失败: {e}"))?;
    let actual = win
        .inner_size()
        .map_err(|e| format!("读取遮罩尺寸失败: {e}"))?;
    let scale = win
        .scale_factor()
        .map_err(|e| format!("读取遮罩缩放失败: {e}"))?;
    if actual.width != target.width
        || actual.height != target.height
        || origin.x != pos.x
        || origin.y != pos.y
        || !scale.is_finite()
        || scale <= 0.0
    {
        return Err("遮罩几何尚未稳定，请重新截图".into());
    }
    let geometry = CaptureGeometry {
        x: origin.x,
        y: origin.y,
        width: i32::try_from(actual.width).map_err(|_| "遮罩宽度超出范围")?,
        height: i32::try_from(actual.height).map_err(|_| "遮罩高度超出范围")?,
        scale,
    };

    // 收起会盖在遮罩之上的常驻窗：球同为 always_on_top（64×64），会浮在遮罩上方
    // 并吞掉起笔处的拖动（设计文档 §5.5 第 3 行）。
    // ★ 逻辑显隐（2026-09-25 二次修复）：可见性判定用 ball_is_hidden（逻辑隐藏下
    //   is_visible() 恒 true，会误判）；隐藏走 ball_hide（2026-09-26 方案 A-2 起为
    //   「空区域裁剪 + 页面透明 + 点击穿透」）—— 窗口保持 WS_VISIBLE、合成表面常驻，
    //   截图结束恢复时不再闪陈旧帧。★ 球窗已是不透明白底窗（transparent:false），hide 时
    //   被裁成零面积空区域（不可见、不吞点击）⇒ 球不在截图中；穿透亦保证不吞起笔拖动
    //   （原 §5.5 诉求不变）。
    let ball_visible = !crate::panels::ball_is_hidden();
    let visibility_generation = crate::fullscreen::user_ball_visibility_generation();
    let hid = ball_visible && crate::panels::ball_hide(app);
    if !state().ready(id, geometry, hid, visibility_generation) {
        return Err("截图准备已失效".into());
    }
    crate::popup::hide(app);

    // 记录恢复信息后才挂起；eval/show 失败同样通过带编号的清理恢复。
    crate::mousehook::set_suspend(true); // 成对复位见 finish()

    // 复位页内状态：窗口只 show/hide 不重建，页内状态会跨次保留（同 popup 的 __dsfResetFired 思路）。
    win.eval(format!(
        "if(window.__dsfShotReset)window.__dsfShotReset({id},1);"
    ))
    .map_err(|e| format!("复位遮罩失败: {e}"))?;

    win.show().map_err(|e| format!("显示遮罩失败: {e}"))?;
    crate::diag::log(&format!(
        "shot: 遮罩显示 monitor={:?} pos=({}, {}) size={}x{} scale={} | 光标=({},{}) focused={:?}",
        monitor.name(),
        pos.x,
        pos.y,
        size.width,
        size.height,
        monitor.scale_factor(),
        cx,
        cy,
        win.is_focused()
    ));
    Ok(())
}

/// 页面存活看门狗：若 500ms 后页内钩子 `__dsfShotReset` 仍未定义，说明 `poc/shot.html`
/// 没被加载（最典型：`dist/` 未随 vite 新入口重建 → 404）。此时遮罩是一扇**收不到任何
/// 框选回调**的全屏死窗 —— 不自动复位就会永久挡住桌面输入，并让划词一直挂起。
fn arm_watchdog(app: &AppHandle, id: u64) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        let Some(win) = app.get_webview_window(SHOT_LABEL) else {
            return;
        };
        let app_c = app.clone();
        // ★ 传表达式：WebView2 会把表达式结果 JSON 序列化后交给回调（回调在主线程，不得 sleep）。
        // 首次 eval 若早于页面加载，会没有复位函数；在就绪检查时补交编号。
        // 页面复位对同编号幂等，不会重置用户已经开始拖动的选区。
        let called = win.eval_with_callback(format!(
            "!!(window.__dsfShotReset && window.__dsfShotReset({id}))"
        ), move |raw| {
            if raw.trim() == "false" {
                crate::diag::warn(
                    "shot: 遮罩页未就绪（__dsfShotReset 未定义），自动取消本次框选（请确认已 pnpm build）",
                );
                cancel(&app_c, id);
            }
        });
        if called.is_err() {
            crate::diag::warn("shot: 看门狗无法执行页面探针");
        }
    });
}

/// 结束 / 取消框选：隐藏遮罩并**成对**复位所有挂起状态。
///
/// `commit` / `cancel` / `begin` 的任一失败分支都汇流到这里，保证异常路径也不残留挂起（硬约束 §8）。
pub fn finish(app: &AppHandle) {
    let id = state().current_id();
    if let Some(id) = id {
        cancel(app, id);
    }
}

/// 取消后保留 worker 的任务所有权，待它停止投递并退出后统一释放。
pub fn cancel(app: &AppHandle, id: u64) {
    let app_c = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        // 与 begin_inner 的窗口副作用串行，避免准备到一半被另一线程取消。
        if state().current_id() != Some(id) {
            return;
        }
        clear_delivery_marker(&app_c, id);
        let restore_ball = state().cancel(id);
        if let Some(window) = app_c.get_webview_window(SHOT_LABEL) {
            let _ = window.hide();
        }
        if let Some(restore_ball) = restore_ball {
            cleanup_capture(&app_c, id, restore_ball);
        }
    }) {
        crate::diag::warn(&format!("shot: 无法投递取消请求（{e}）"));
    }
}

/// 工作流换代强制失效；worker 退出前保持占用，避免新截图并行抓取或贴图。
pub fn invalidate(app: &AppHandle) {
    let id = state().current_id();
    if let Some(id) = id {
        finish_capture(app, id);
        if let Some(window) = app.get_webview_window(SHOT_LABEL) {
            let _ = window.eval(format!(
                "window.__dsfShotInvalidate&&window.__dsfShotInvalidate({id});"
            ));
            let _ = window.hide();
        }
    }
}

pub fn cancel_revision(app: &AppHandle, id: u64, revision: u64) {
    let app_c = app.clone();
    let _ = app.run_on_main_thread(move || {
        if !state().matches(id, revision) {
            return;
        }
        clear_delivery_marker(&app_c, id);
        let restore = state().cancel(id);
        if let Some(window) = app_c.get_webview_window(SHOT_LABEL) {
            let _ = window.hide();
        }
        if let Some(restore) = restore {
            cleanup_capture(&app_c, id, restore);
        }
    });
}

fn clear_delivery_marker(app: &AppHandle, id: u64) {
    if let Some(panel) = app.get_webview_window("panel") {
        let _ = panel.eval(format!("if(typeof window.__dsfloatShotCapture==='string'&&window.__dsfloatShotCapture.startsWith('{id}/'))window.__dsfloatShotCapture=null;"));
    }
}

fn finish_capture(app: &AppHandle, id: u64) {
    clear_delivery_marker(app, id);
    let restore_ball = state().finish(id);
    if let Some(restore_ball) = restore_ball {
        cleanup_capture(app, id, restore_ball);
    }
}

/// Finishing 状态保持占用，直到隐藏、球恢复和划词恢复全部完成。
/// 旧回调不能释放新截图，延迟恢复球也不会跨越下一次截图。
fn cleanup_capture(app: &AppHandle, id: u64, ball_restore: BallRestore) {
    clear_delivery_marker(app, id);
    if let Some(w) = app.get_webview_window(SHOT_LABEL) {
        let _ = w.eval(format!(
            "window.__dsfShotInvalidate&&window.__dsfShotInvalidate({id});"
        ));
        let _ = w.hide();
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if ball_restore.restore {
            std::thread::sleep(Duration::from_millis(SHOT_BALL_REVEAL_SETTLE_MS));
        }
        let app_for_main = app.clone();
        if let Err(error) = app.run_on_main_thread(move || {
            if ball_restore.restore
                && crate::screenshot_state::should_restore_ball(
                    true,
                    ball_restore.generation,
                    crate::fullscreen::user_ball_visibility_generation(),
                )
            {
                crate::fullscreen::restore_ball_after_capture(
                    &app_for_main,
                    ball_restore.generation,
                );
            } else if ball_restore.restore {
                crate::diag::log(&format!(
                    "shot: 截图 {id} 清理跳过悬浮球恢复，用户显隐选择已更新"
                ));
            }
            crate::mousehook::set_suspend(false);
            state().release(id);
            crate::diag::log(&format!("shot: 截图 {id} 清理完成，挂起已复位"));
        }) {
            crate::diag::warn(&format!("shot: 清理回调未能投递到主线程: {error}"));
            crate::mousehook::set_suspend(false);
            state().release(id);
        }
    });
}
/// 页内框选回报的选区几何（**窗口内 CSS 坐标** + 该页的 `devicePixelRatio` 与视口尺寸）。
///
/// 打包成结构体：既避免命令参数过多（clippy `too_many_arguments`），也让前端 `invoke` 的边界
/// 更明确 —— `invoke('shot_commit', { captureId, rect: { x, y, w, h, dpr, vw, vh } })`。
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct ShotRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub dpr: f64,
    pub vw: f64,
    pub vh: f64,
}

// ---------------------------------------------------------------------------
// S2：屏幕抓取 + PNG 编码（见 docs/screenshot-qa-design.md §5.3 / §5.4）
// ---------------------------------------------------------------------------

/// 单边像素上限：超过先降采样（4K 全屏 RGBA ≈ 33 MB，无闸门会拖垮 base64 传输）。
const MAX_EDGE: i32 = 4096;
/// PNG 体积上限（字节）：降采样后仍超限则拒绝并落 `diag`（§5.4 体积闸门）。
const MAX_PNG_BYTES: usize = 4 * 1024 * 1024;
/// 分块推入页面的块大小：单条 `eval` 脚本不能过大（§5.6）。
const CHUNK_BYTES: usize = 256 * 1024;
/// 遮罩从合成器移除后、开始 BitBlt 前的静默（ms）—— 立刻抓会抓到残留帧（§5.5）。
const SHOT_HIDE_SETTLE_MS: u64 = 120;
/// 退出框选时「遮罩 hide → 球 show」之间的静默（ms）—— 至少跨一帧 DWM 合成（60Hz≈16.7ms），
/// 避免遮罩与球两个透明窗在同一合成帧里被并置而留下残影（与 `show_panel` 隐现第二拍同口径）。
const SHOT_BALL_REVEAL_SETTLE_MS: u64 = 30;
/// 附件轮询的步进间隔（ms）—— 上传是异步的，逐格探测缩略图是否已挂上输入框。
const ATTACH_POLL_STEP_MS: u64 = 300;
/// 投递完成后复核附件是否挂上的静默（ms）—— 只记日志、不阻塞（D2 只贴图不发送）。
const ATTACH_VERIFY_SETTLE_MS: u64 = 600;

/// 抓选区并编码 PNG；此阶段不访问面板或发送 OCR。
fn capture_png(x: i32, y: i32, w: i32, h: i32) -> Result<(Vec<u8>, i32, i32), String> {
    let (mut rgba, mut cw, mut ch) = (grab_rgba(x, y, w, h)?, w, h);
    // 体积闸门①：单边超限 → 反复 2× 盒式降采样到阈值内。
    while (cw > MAX_EDGE || ch > MAX_EDGE) && cw > 8 && ch > 8 {
        let (n, nw, nh) = box_downsample(&rgba, cw, ch);
        rgba = n;
        cw = nw;
        ch = nh;
    }
    let mut png = encode_png(&rgba, cw, ch)?;
    // 体积闸门②：png 仍超 4 MiB → 再降（最多 3 档），仍超则拒绝。
    let mut tries = 0;
    while png.len() > MAX_PNG_BYTES && cw > 16 && ch > 16 && tries < 3 {
        let (n, nw, nh) = box_downsample(&rgba, cw, ch);
        rgba = n;
        cw = nw;
        ch = nh;
        png = encode_png(&rgba, cw, ch)?;
        tries += 1;
    }
    if png.len() > MAX_PNG_BYTES {
        return Err(format!("PNG 体积 {} KiB 超过上限", png.len() / 1024));
    }
    if all_black(&rgba) {
        crate::diag::warn(
            "shot: 抓到的画面**全黑**（独占全屏 DirectX / 硬件叠加层 GDI 抓不到，见设计文档 §5.3 已知限制）",
        );
    }
    Ok((png, cw, ch))
}

/// 轮询「图片是否已挂上输入框」至多 [`ATTACH_WAIT_MS`]（§5.6：上传是异步的）。
///
/// 超时停止发送识字指令；缩略图尚在上传时不能把正文提交到站点。
fn wait_attachment(
    app: &AppHandle,
    id: u64,
    revision: u64,
    token: crate::workflow::Token,
) -> Result<(), String> {
    let Some(panel) = app.get_webview_window("panel") else {
        return Err("面板窗口不存在".into());
    };
    let js = "(function(){try{return !!(window.__DSFLOAT__&&window.__DSFLOAT__.hasAttachment&&window.__DSFLOAT__.hasAttachment());}catch(e){return false;}})()";
    let mut waited = 0u64;
    while waited < ATTACH_WAIT_MS {
        if !active(id, revision, token) {
            return Err("截图任务已取消".into());
        }
        std::thread::sleep(Duration::from_millis(ATTACH_POLL_STEP_MS));
        waited += ATTACH_POLL_STEP_MS;
        if eval_for_token(&panel, id, revision, token, js.to_string(), 1500)
            .as_deref()
            .map(str::trim)
            == Some("true")
        {
            crate::diag::log(&format!(
                "shot-ocr: 图片已挂上输入框（等待 {waited}ms），开始发送转录提示词"
            ));
            return Ok(());
        }
    }
    Err("未确认新图片已上传，已停止发送识字指令。请检查面板附件后重新截图".into())
}

/// 截图识字：发送固定的转录提示词，并把 DeepSeek 的转录结果回读到**结果卡**。
fn send_ocr_prompt(app: &AppHandle) -> Result<(), String> {
    let panel = crate::ensure_panel_loaded(app).ok_or_else(|| "面板窗口未建立".to_string())?;
    crate::inject::inject_and_send_async(panel, OCR_PROMPT.to_string());
    crate::readback::begin_watch(
        app,
        OCR_PROMPT.to_string(),
        None,
        crate::readback::WatchOpts {
            tag: "shot-ocr".to_string(),
            source: "截图识字".to_string(),
            title: "图中文字".to_string(),
            // 截图识字没有「被划中的原句」，故 quote 留空（卡片不显示引用条）。
            ..Default::default()
        },
    );
    crate::diag::log("shot-ocr: 已发送转录提示词并开始回读（结果进结果卡）");
    Ok(())
}

/// GDI `BitBlt(SRCCOPY)` 抓虚拟桌面矩形（**物理像素**）→ **RGBA**（行序 top-down）。
///
/// 用 `CreateDIBSection`（而非 `GetDIBits`）直接拿像素指针，省一次拷贝；`biHeight` 取负值
/// 即 top-down（否则每行倒序，PNG 会上下颠倒）。所有句柄**严格成对释放**。
fn grab_rgba(x: i32, y: i32, w: i32, h: i32) -> Result<Vec<u8>, String> {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, RGBQUAD,
        SRCCOPY,
    };
    if w <= 0 || h <= 0 {
        return Err(format!("非法选区 {w}x{h}"));
    }
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err("GetDC(nullptr) 失败".into());
        }
        let mut mem = HDC::default();
        let mut hbm = HBITMAP::default();
        let mut original_bitmap = HGDIOBJ::default();
        let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
        let result = (|| -> Result<Vec<u8>, String> {
            mem = CreateCompatibleDC(Some(screen));
            if mem.is_invalid() {
                return Err("CreateCompatibleDC 失败".into());
            }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // 负值 = top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0, // BI_RGB
                    ..Default::default()
                },
                bmiColors: [RGBQUAD::default()],
            };
            hbm = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| format!("CreateDIBSection 失败: {e}"))?;
            if hbm.is_invalid() || bits.is_null() {
                return Err("CreateDIBSection 返回空位图".into());
            }
            original_bitmap = SelectObject(mem, HGDIOBJ(hbm.0));
            if original_bitmap.0.is_null() || original_bitmap.0 as isize == -1 {
                return Err("SelectObject 选择截图位图失败".into());
            }
            BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY)
                .map_err(|e| format!("BitBlt 失败: {e}"))?;
            let n = (w as usize) * (h as usize) * 4;
            let src = std::slice::from_raw_parts(bits as *const u8, n);
            // BGRA → RGBA：就地交换 R/B；第 4 字节（BitBlt 下恒为 0）显式写 255。
            let mut out = Vec::with_capacity(n);
            for px in src.chunks_exact(4) {
                out.push(px[2]);
                out.push(px[1]);
                out.push(px[0]);
                out.push(255);
            }
            Ok(out)
        })();
        if !mem.is_invalid() && !original_bitmap.0.is_null() && original_bitmap.0 as isize != -1 {
            let replaced = SelectObject(mem, original_bitmap);
            if replaced.0 != hbm.0 {
                let _ = DeleteDC(mem);
                mem = HDC::default();
            }
        }
        if !hbm.is_invalid() && !DeleteObject(HGDIOBJ(hbm.0)).as_bool() {
            // A failed DeleteObject may mean the bitmap is still selected. Destroy the
            // memory DC first, then retry so this abnormal branch does not leak the DIB.
            if !mem.is_invalid() {
                let _ = DeleteDC(mem);
                mem = HDC::default();
            }
            if !DeleteObject(HGDIOBJ(hbm.0)).as_bool() {
                crate::diag::warn("shot: 销毁内存 DC 后仍无法释放截图 DIB 位图");
            }
        }
        if !mem.is_invalid() {
            let _ = DeleteDC(mem);
        }
        let _ = ReleaseDC(None, screen);
        result
    }
}

/// 2× 盒式降采样（2×2 求平均）—— 纯手写、零依赖。返回 `(缓冲, 新宽, 新高)`。
fn box_downsample(rgba: &[u8], w: i32, h: i32) -> (Vec<u8>, i32, i32) {
    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0u32; 4];
            let mut cnt = 0u32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let sx = x * 2 + dx;
                    let sy = y * 2 + dy;
                    if sx >= w || sy >= h {
                        continue;
                    }
                    let i = ((sy as usize) * (w as usize) + (sx as usize)) * 4;
                    if i + 3 >= rgba.len() {
                        continue;
                    }
                    for (c, a) in acc.iter_mut().enumerate() {
                        *a += rgba[i + c] as u32;
                    }
                    cnt += 1;
                }
            }
            let o = ((y as usize) * (nw as usize) + (x as usize)) * 4;
            for (c, a) in acc.iter().enumerate() {
                out[o + c] = a.checked_div(cnt).unwrap_or(0) as u8;
            }
        }
    }
    (out, nw, nh)
}

/// 全黑启发式（§5.3）：至多采样 ~1024 个像素，全为 0 即认为没抓到画面。
fn all_black(rgba: &[u8]) -> bool {
    let pixels = rgba.len() / 4;
    if pixels == 0 {
        return true;
    }
    let step = (pixels / 1024).max(1) * 4;
    let mut i = 0usize;
    while i + 2 < rgba.len() {
        if rgba[i] != 0 || rgba[i + 1] != 0 || rgba[i + 2] != 0 {
            return false;
        }
        i += step;
    }
    true
}

/// RGBA → PNG（`png` crate，见 §5.4 F11：零新增下载）。
fn encode_png(rgba: &[u8], w: i32, h: i32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w.max(1) as u32, h.max(1) as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| format!("PNG header: {e}"))?;
        writer
            .write_image_data(rgba)
            .map_err(|e| format!("PNG data: {e}"))?;
    }
    Ok(out)
}

/// 标准 base64（`+/` 字母表 + `=` 填充）—— 手写，避免再引依赖。
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = ((chunk[0] as u32) << 16) | (b1 << 8) | b2;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// 在面板里执行表达式并取回 JSON 结果；超时/无法投递返回 `None`。
///
/// ★ 回调在**主线程**执行，故本函数**只能从非主线程调用**（`recv_timeout` 阻塞的是 worker
/// 线程）—— 与 `readback::measure` / `inject` 的纪律一致（F9）。
fn eval_js(panel: &WebviewWindow, js: String, timeout_ms: u64) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if panel
        .eval_with_callback(js, move |raw| {
            let _ = tx.send(raw);
        })
        .is_err()
    {
        return None;
    }
    rx.recv_timeout(Duration::from_millis(timeout_ms)).ok()
}

fn eval_for_token(
    panel: &WebviewWindow,
    id: u64,
    revision: u64,
    token: crate::workflow::Token,
    js: String,
    timeout_ms: u64,
) -> Option<String> {
    if !active(id, revision, token) {
        return None;
    }
    let guarded = format!("(function(){{if(window.__dsfloatShotCapture!=='{id}/{revision}')return 'no-api';return ({js});}})()");
    let result = eval_js(panel, crate::workflow::guarded(token, &guarded), timeout_ms);
    if active(id, revision, token) {
        result
    } else {
        None
    }
}

/// 分块推入 PNG（base64）并挂到 DeepSeek 的 `input[type=file]`（§5.6 路线①）。
///
/// 分块 + `seq` 是**必须**的：4 MiB PNG → 5.4 MiB base64，单条 `eval` 脚本过大；而分块后
/// 若乱序/重复，字节错位会产生**损坏图片**（且不报错），故每块都校验 `seq`。
fn deliver_png(
    app: &AppHandle,
    png: &[u8],
    id: u64,
    revision: u64,
    token: crate::workflow::Token,
) -> Result<(), String> {
    if !active(id, revision, token) {
        return Err("截图任务已取消".into());
    }
    let panel = crate::ensure_panel_loaded(app).ok_or_else(|| "面板窗口未建立".to_string())?;
    let b64 = base64_encode(png);
    // 1) 复位分块缓冲（失败不致命 —— 首次为空白状态）。
    let _ = eval_for_token(
        &panel, id, revision, token,
        "(function(){try{if(window.__DSFLOAT__&&window.__DSFLOAT__.shotReset)window.__DSFLOAT__.shotReset();}catch(e){}})()"
            .to_string(),
        1000,
    );
    // 2) 逐块推入（base64 是 ASCII，按字节分块安全）。
    let bytes = b64.as_bytes();
    let total = bytes.len().div_ceil(CHUNK_BYTES);
    for (i, c) in bytes.chunks(CHUNK_BYTES).enumerate() {
        let s = std::str::from_utf8(c).map_err(|_| "base64 分块非 UTF-8".to_string())?;
        let lit = serde_json::to_string(s).map_err(|_| "JSON 转义失败".to_string())?;
        let js = format!(
            "(function(){{try{{if(!window.__DSFLOAT__)return 'no-api';return window.__DSFLOAT__.shotPush({lit},{i});}}catch(e){{return 'exception:'+(e&&e.message||e);}}}})()"
        );
        match eval_for_token(&panel, id, revision, token, js, 3000)
            .as_deref()
            .map(str::trim)
        {
            Some("true") => {}
            other => return Err(format!("shotPush {}/{} 失败: {other:?}", i + 1, total)),
        }
    }
    // 3) 组装 → File → DataTransfer → file input → 派发 change。
    let js = "(function(){try{if(!window.__DSFLOAT__)return 'no-api';return window.__DSFLOAT__.shotCommit('image/png');}catch(e){return 'exception:'+(e&&e.message||e);}})()";
    match eval_for_token(&panel, id, revision, token, js.to_string(), 5000)
        .as_deref()
        .map(str::trim)
    {
        Some("true") => {}
        other => return Err(format!("shotCommit 失败: {other:?}")),
    }
    // 复核（仅记录，不阻塞 —— D2 只贴图不发送，失败路径用户开面板自然可见）。
    std::thread::sleep(Duration::from_millis(ATTACH_VERIFY_SETTLE_MS));
    let has = eval_for_token(
        &panel, id, revision, token,
        "(function(){try{return !!(window.__DSFLOAT__&&window.__DSFLOAT__.hasAttachment&&window.__DSFLOAT__.hasAttachment());}catch(e){return false;}})()"
            .to_string(),
        2000,
    );
    crate::diag::log(&format!(
        "shot: 附件复核 hasAttachment={:?}（false 未必是失败：可能只是缩略图选择器未命中）",
        has.as_deref().map(str::trim)
    ));
    Ok(())
}

/// 状态与窗口副作用串行进入 UI 线程；只有 worker 可以等待此函数。
fn main_call<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(AppHandle) -> T + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(f(handle));
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())
}
struct WorkerGuard {
    app: AppHandle,
    id: u64,
    revision: u64,
    delivery: bool,
    armed: bool,
}
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let (id, revision, delivery) = (self.id, self.revision, self.delivery);
        let _ = main_call(&self.app, move |app| {
            if delivery {
                state().finish(id);
            } else {
                state().preview_failed_if_capturing(id, revision);
            }
            let restore = state().worker_done(id);
            if let Some(restore) = restore {
                cleanup_capture(&app, id, restore);
            }
        });
    }
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePayload {
    capture_id: u64,
    revision: u64,
}

pub async fn commit(
    app: AppHandle,
    id: u64,
    revision: u64,
    rect: ShotRect,
) -> Result<CapturePayload, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let token = capture_token();
        let capture = main_call(&app, move |app| {
            if !active(id, revision, token) {
                return Err("截图任务已过期".to_string());
            }
            let capture = state()
                .claim_preview(id, revision)
                .ok_or("截图状态已改变")?;
            if let Some(win) = app.get_webview_window(SHOT_LABEL) {
                if let Err(e) = win.hide() {
                    state().preview_failed(id, revision);
                    state().worker_done(id);
                    return Err(e.to_string());
                }
            }
            crate::flush_ball(&app);
            Ok(capture)
        })??;
        let _guard = WorkerGuard {
            app: app.clone(),
            id,
            revision,
            delivery: false,
            armed: true,
        };
        crate::diag::log(&format!(
            "shot: preview id={} revision={revision} dpr={} viewport={}x{}",
            capture.id, rect.dpr, rect.vw, rect.vh
        ));
        let physical = capture
            .geometry
            .desktop_rect(SelectionRect {
                x: rect.x,
                y: rect.y,
                width: rect.w,
                height: rect.h,
            })
            .ok_or("选区无效或小于8像素")?;
        std::thread::sleep(Duration::from_millis(SHOT_HIDE_SETTLE_MS));
        if !active(id, revision, token) {
            return Err("截图任务已取消".into());
        }
        let (png, _, _) = capture_png(physical.x, physical.y, physical.width, physical.height)?;
        main_call(&app, move |_| {
            if !active(id, revision, token) || !state().store_preview(id, revision, png) {
                return Err("截图任务已取消".into());
            }
            Ok(CapturePayload {
                capture_id: id,
                revision,
            })
        })?
    })
    .await
    .map_err(|e| format!("截图失败: {e}"))?
}

pub fn preview_ready(
    app: &AppHandle,
    id: u64,
    revision: u64,
    failed: bool,
) -> Result<bool, String> {
    if !active(id, revision, capture_token()) || !state().previewing(id, revision) {
        return Ok(false);
    }
    if failed {
        state().preview_failed(id, revision);
        let Some(window) = app.get_webview_window(SHOT_LABEL) else {
            finish_capture(app, id);
            return Err("截图窗口不存在".into());
        };
        if let Err(e) = window.show() {
            finish_capture(app, id);
            return Err(e.to_string());
        }
    } else if !state().mark_viewed(id, revision) {
        return Ok(false);
    }
    // 成功时遮罩窗口保持隐藏；前端会立即自动调用 shot_confirm，不再展示图片预览。
    Ok(true)
}

pub fn reselect(app: &AppHandle, id: u64, revision: u64) -> Result<u64, String> {
    if !active(id, revision, capture_token()) {
        return Err("截图任务已取消".into());
    }
    let next = state().reselect(id, revision).ok_or("截图状态已改变")?;
    let Some(window) = app.get_webview_window(SHOT_LABEL) else {
        finish_capture(app, id);
        return Err("截图窗口不存在".into());
    };
    if let Err(e) = window
        .eval(format!(
            "window.__dsfShotReselect&&window.__dsfShotReselect({id},{next});"
        ))
        .and_then(|_| window.show())
    {
        finish_capture(app, id);
        return Err(e.to_string());
    }
    Ok(next)
}

pub fn confirm(app: &AppHandle, id: u64, revision: u64) -> Result<(), String> {
    let token = capture_token();
    if !active(id, revision, token) {
        return Err("截图任务已取消".into());
    }
    let draft = state()
        .confirm(id, revision)
        .ok_or("预览尚未就绪或已确认")?;
    if let Some(panel) = app.get_webview_window("panel") {
        let _ = panel.eval(format!("window.__dsfloatShotCapture='{id}/{revision}';"));
    }
    if let Some(window) = app.get_webview_window(SHOT_LABEL) {
        let _ = window.hide();
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let mut guard = WorkerGuard {
            app: app.clone(),
            id,
            revision,
            delivery: true,
            armed: true,
        };
        let outcome = (|| -> Result<(), String> {
            deliver_png(&app, &draft.png, id, revision, token)?;
            if draft.capture.mode == ShotMode::Ocr {
                wait_attachment(&app, id, revision, token)?;
                main_call(&app, move |app| {
                    if !active(id, revision, token) {
                        return Err("截图任务已取消".into());
                    }
                    send_ocr_prompt(&app)
                })??;
            } else {
                main_call(&app, move |app| {
                    if active(id, revision, token) {
                        let _ = crate::show_panel(&app);
                    }
                })?;
            }
            Ok(())
        })();
        if let Err(error) = outcome {
            crate::diag::warn(&format!("shot: 确认投递失败: {error}"));
            if let Ok(true) = main_call(&app, move |app| {
                if !active(id, revision, token) || !state().delivery_failed(id, revision) {
                    return false;
                }
                state().worker_done(id);
                if let Some(window) = app.get_webview_window(SHOT_LABEL) {
                    let message = serde_json::to_string(&format!(
                        "贴图失败：{error}。请检查面板附件后重新框选"
                    ))
                    .unwrap_or_default();
                    if window.eval(format!("window.__dsfShotDeliveryError&&window.__dsfShotDeliveryError({id},{revision},{message});")).and_then(|_| window.show()).is_err() {
                        finish_capture(&app, id);
                    }
                } else {
                    finish_capture(&app, id);
                }
                true
            }) {
                guard.armed = false;
            }
        }
    });
    Ok(())
}
// ---------------------------------------------------------------------------
// 单测：只覆盖 S2 的**纯函数**（base64 / 降采样 / 全黑 / PNG 编码），不碰 Win32 与窗口
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_payload_returns_only_capture_identity() {
        let payload = CapturePayload {
            capture_id: 17,
            revision: 3,
        };
        assert_eq!(
            serde_json::to_value(payload).expect("serialize capture payload"),
            serde_json::json!({"captureId": 17, "revision": 3})
        );
    }

    /// RFC 4648 §10 标准测试向量。
    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn box_downsample_halves_and_averages() {
        // 2×2 → 1×1：四像素取平均（10/30/50/70 → 40，20/40/60/80 → 50，30/50/70/90 → 60）。
        let px = [
            10, 20, 30, 255, 30, 40, 50, 255, 50, 60, 70, 255, 70, 80, 90, 255,
        ];
        let (out, w, h) = box_downsample(&px, 2, 2);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out, vec![40, 50, 60, 255]);
    }

    #[test]
    fn box_downsample_never_returns_empty() {
        // 1×1 → 仍 1×1（`.max(1)` 保底），不得出现 0 尺寸缓冲。
        let (out, w, h) = box_downsample(&[1, 2, 3, 4], 1, 1);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn all_black_detects_black_and_non_black() {
        assert!(all_black(&[]));
        assert!(all_black(&[0, 0, 0, 255, 0, 0, 0, 255]));
        // 任一处非黑即不算全黑（第 4 字节 alpha 不参与判定）。
        assert!(!all_black(&[0, 0, 0, 255, 1, 0, 0, 255]));
    }

    #[test]
    fn encode_png_has_signature_and_roundtrips() {
        let rgba = [255u8, 0, 0, 255].repeat(4); // 2×2 全红
        let png = encode_png(&rgba, 2, 2).expect("encode_png");
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        // 回读校验：尺寸与像素必须与输入一致（证明编码链路正确）。
        let mut reader = png::Decoder::new(std::io::Cursor::new(&png))
            .read_info()
            .expect("read_info");
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("next_frame");
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(&buf[..4], &[255, 0, 0, 255]);
    }
}
