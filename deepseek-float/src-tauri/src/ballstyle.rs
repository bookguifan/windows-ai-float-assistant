//! 悬浮球窗口的 Win32 样式修正 —— 修「球右侧有一片不可见区域吞鼠标点击」。
//!
//! ## 现象与根因（2026-09-14 真机定位，证据闭环）
//! 本工程所有 Tauri 窗口实测都带 `WS_CAPTION` 样式位，**即便** `tauri.conf.json` 写了
//! `decorations: false`（`is_decorated()` 也确实返回 `false`）。Windows 对**带标题栏**的窗口
//! 用 `SM_CXMINTRACK` / `SM_CYMINTRACK` 强制最小尺寸，本机（125% 缩放）实测：
//!
//! | 度量 | 值 | 对球的影响 |
//! |---|---|---|
//! | `SM_CXMINTRACK` | 136 | 球只要 **64 < 136 → 宽度被夹成 136** |
//! | `SM_CYMINTRACK` | 39  | 高度 64 > 39 → **不被夹** |
//!
//! 于是球窗的真实逻辑尺寸变成 **136×64**，而 `poc/ball.html` 只画左侧 64px →
//! **右侧多出 72px 宽的不可见区域，吞掉落在该区域的鼠标点击**（用户真机确认）。
//! 另外 4 个窗口（panel 440 / settings 474 / popup 448 / shot 整屏）本来就宽于 136，故只有球受影响。
//!
//! ## 修法（已用运行时实验验证）
//! 清掉 `WS_CAPTION`（连同 `WS_SYSMENU` / `WS_MINIMIZEBOX` / `WS_MAXIMIZEBOX` 这些标题栏关联位，
//! 合计 `0x00CB0000`）→ `SetWindowPos(SWP_FRAMECHANGED)` 让系统重算非客户区 → 再 `set_size(64×64)`。
//! 实测 `136×64` → **`64×64`**，夹取消失。这些位在 `decorations:false` 的语义下本就该没有。
//!
//! ## 调用时机（★ 必须在 `setup_ball_position` **之前**）
//! `clamp_ball_to_monitor` 依赖窗口尺寸做边界夹取；若此时尺寸还是错的 136×64，
//! 位置夹取会按错误宽度计算，球会被错误地留在/推离屏幕边缘。
//!
//! ## 注意
//! - 只对 **ball** 调用。panel 需要保留 `WS_THICKFRAME`（面板可缩放），清标题栏会破坏其行为。
//! - 幂等：可安全重复调用（第二次会走「标题栏位已不在」分支）。

use tauri::{AppHandle, Manager, Runtime};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY,
    DWMWINDOWATTRIBUTE,
};
// 方案 A-2（不透明窗 + 椭圆裁剪）：`CreateEllipticRgn` / `CreateRectRgn` 造区域，
// `SetWindowRgn` 把区域套到球窗上；`DeleteObject` 仅在 `SetWindowRgn` **失败**时回收区域
// （成功时所有权移交系统，绝不能手动删）。`GetClientRect` 取客户区**物理像素**尺寸。
// ★ 2026-10-13（P0-5）：新增 `GetWindowRgn` 读回校验 —— 见 [`region_matches`]。
// `CreateRectRgn` 同时用于「造零面积空区域」与「造临时空 region 供 GetWindowRgn 读回」。
use windows::Win32::Graphics::Gdi::{
    CreateEllipticRgn, CreateRectRgn, DeleteObject, GetWindowRgn, SetWindowRgn, COMPLEXREGION,
    NULLREGION, SIMPLEREGION,
};
// ★ 2026-10-13（F2/F3）：新增 `IsWindowVisible`（可见性读回自愈的判据）与
// `ShowWindow`/`SW_SHOWNOACTIVATE`（原生显形，**刻意不经 tao** —— tao 的 `show()` 会用其
// 缓存的 window-flags 重建 `GWL_STYLE`，把 `clear_caption_bits` 清掉的 `WS_CAPTION` 放回来）。
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetWindowLongPtrW, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    GWL_EXSTYLE, GWL_STYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SW_SHOWNOACTIVATE, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU,
};

/// 悬浮球的目标逻辑边长 —— 与 `poc/ball.html` 的 `html,body{width:64px;height:64px}` 成对约束，
/// 改一处必须改另一处。
const BALL_LOGICAL: f64 = 64.0;

/// 幂等清除球窗的标题栏样式位（`WS_CAPTION` 及关联位）。返回是否发生改动。
///
/// 抽出来供两处复用：启动期 [`fix`]，以及运行时 [`enforce_captionless`]（对抗被 Tauri 某次
/// 操作临时恢复的 `WS_CAPTION`）。只改样式位、不动尺寸。
///
/// 注：**不再**在改动后重申明透明 —— 方案 A-2 已把球窗改为**不透明窗**（`transparent:false`），
/// 透明通道由 [`apply_ball_shape`] 的椭圆区域负责，此处若再调 blur-behind 会把「镂空洞」打回来，
/// 与目标冲突（见 [`apply_ball_shape`] 模块说明）。
fn clear_caption_bits(hwnd: HWND) -> bool {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let clear = WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0;
        let fixed = style & !clear;
        if fixed == style {
            return false;
        }
        SetWindowLongPtrW(hwnd, GWL_STYLE, fixed as isize);
        // SWP_FRAMECHANGED 让系统重算非客户区（标题栏随之消失）。
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        crate::diag::log(&format!(
            "ballstyle: 清除标题栏位 style=0x{style:08X} -> 0x{fixed:08X}（clear=0x{clear:08X}）"
        ));
        true
    }
}

/// `DWMWA_NCRENDERING_ENABLED`（值 = 1）：查询「本窗**非客户区渲染当前是否开启**」的 DWM 属性。
///
/// 供 [`disable_dwm_frame`] 的**读回前置判断**使用：只有当 NC 渲染确实开着（或读回失败、无从
/// 判断）时才下发关闭指令，令热路径在「已关闭」状态下**零 DWM 调用**。windows 0.61 的同名
/// crate 常量值与语义一致（`DWMWINDOWATTRIBUTE(1)`）。
const DWMWA_NCRENDERING_ENABLED: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(1);

/// 通知 DWM **不要绘制本窗的非客户区**（标题栏 / 边框）。返回**是否真的下发了**设置调用。
///
/// ## 为什么变成「先读回、再按需下发」（2026-09-25 第八次排查，点击热路径去重）
/// 本函数处在 [`enforce_captionless`] → [`crate::panels::flush_ball`] 的**热点路径**上：
/// 球态**每次左键**都会走到 `toggle_panel` → `show_panel`/`hide_panel` → `flush_ball`
/// → `enforce_captionless` → 本函数。改造前本函数**无条件**执行
/// `DwmSetWindowAttribute(DWMWA_NCRENDERING_POLICY, DWMNCRP_DISABLED)` —— 这是点击热路径上
/// **唯一无条件**的 DWM 调用，每点一次就让 DWM 重算一次窗口框架 / 重合成一拍，正是用户所报
/// 「点一下球，背后一闪而逝」的首要根因（本模块其余 clear_caption_bits / make_tool_window
/// 早已是「无变化即提前返回」）。
///
/// 现改为：先 `DwmGetWindowAttribute(DWMWA_NCRENDERING_ENABLED)` 读回；**仅当**读回成功且
/// `enabled != 0`（NC 渲染仍开着）**或**读回失败时才下发关闭指令。于是「已关闭」状态下
/// 本函数是**纯读、零下发**，任你高频调用都不会再触发 DWM 框架重算；同时保留纵深防御 ——
/// 一旦有人把 NC 渲染重新打开，下一拍即自愈。
fn disable_dwm_frame(hwnd: HWND) -> bool {
    let policy = DWMNCRP_DISABLED; // = 1：禁用非客户区渲染
    unsafe {
        // 读回「非客户区渲染当前是否开启」。成功且明确为「已关闭」(enabled == 0) ⇒ 提前返回、
        // 不下发任何 DWM 调用（热路径去重的关键）。
        let mut enabled: u32 = 0;
        let read_ok = DwmGetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_ENABLED,
            &mut enabled as *mut u32 as *mut core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok();
        if read_ok && enabled == 0 {
            return false;
        }
        // 走到这里 ⇒ 要么 NC 渲染确实开着，要么读回失败（无从判断，保守纠偏）——都值得下发一次。
        // 直接返回下发结果（避免 `clippy::let_and_return`：多余的 let 绑定）。
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &policy as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
        .is_ok()
    }
}

/// 把悬浮球窗**裁剪成圆形**（或令其整窗不可见）—— 方案 A-2 的核心。
///
/// ## 为什么需要（2026-09-26，结构性消灭「镂空洞」）
/// 本工程球窗历史上的「透明」由 tao 建窗时的 `DwmEnableBlurBehindWindow`（**空区域**）实现：
/// 空 blur 区域 ⇒ **整窗就是一个「镂空洞」** —— 除 WebView2 画出的白圆外，整块窗口透出桌面 /
/// 背后窗口。真机取证（`scripts/diag-ballflash*.ps1` v1–v4）证明：**按下/长按球时 WebView2 白圆
/// 短暂不呈现 ⇒ 整窗变镂空洞 ⇒ 透出用户背后开着的 DeepSeek 浅色页**（用户所报
/// 「DeepSeek 浅色背景、点击闪、长按一直显示」）。
///
/// ## 修法：不透明窗 + `SetWindowRgn` 椭圆裁剪
/// 1. `tauri.conf.json` 里球窗 `transparent: false` ⇒ 变**不透明窗**，DWM **永远不会把桌面
///    透过来**；即便 WebView2 掉帧，DWM 只会保留窗口自身的上一帧表面（白圆），**不会**露出
///    背后的 DeepSeek 页 —— 从结构上消灭「镂空洞」。
/// 2. 但 `transparent:false` 后窗口默认是个**白色方块**（`poc/ball.html` 已把 `html,body`
///    背景改为不透明白），故用 `SetWindowRgn` 把一个**椭圆区域**套在窗口上，只保留中央圆 ⇒
///    恢复「圆形」外观。代价：圆边为硬边、无抗锯齿（已接受）。
///
/// ## 区域语义（`SetWindowRgn` 的双职责）
/// `SetWindowRgn` 设定的区域**同时**是窗口的「可见区域」与「命中测试区域」：
/// - `visible = true`：椭圆区域 `(0,0,w,h)` ⇒ 只在圆内可见、只在圆内可点（方块四角不吞点击）。
/// - `visible = false`：**零面积空区域** `(0,0,0,0)` ⇒ 整窗不可见、且不拦截任何点击
///   （比单纯 `document.documentElement.visibility='hidden'` 更彻底 —— 后者在不透明白底窗上
///   仍会露出白圆）。
///
/// ## 物理像素 + 区域所有权
/// - 坐标用**物理像素**：`GetClientRect` 取的是客户区在**窗口自身坐标空间**下的矩形（本工程
///   球窗 `decorations:false`，客户区原点即窗口原点），DPI 感知下即为物理像素，与
///   `SetWindowRgn` 期望的窗口坐标一致。取不到时退回「逻辑边长 × 当前缩放」兜底。
/// - `SetWindowRgn` **成功后区域所有权移交系统**，**不要**再 `DeleteObject`（否则悬垂句柄）；
///   仅在设置**失败**时自行 `DeleteObject` 防止 GDI 对象泄漏。
///
/// 幂等、可高频调用：`ball_show` 每次显形都会重套一次（防 Tauri/tao 的某些样式操作清掉 region）。
///
/// **返回值**（★ 2026-10-13 P0-5 新增）：`true` = `SetWindowRgn` 被系统接受。
/// 供调用方（尤其 [`enforce_captionless`]）**如实**判断是否成功，而不是靠日志猜 ——
/// 此前本函数无论成败都打「成功」日志，是排障期的一大误导源。
pub(crate) fn apply_ball_shape<R: Runtime>(app: &AppHandle<R>, visible: bool) -> bool {
    let Some(win) = app.get_webview_window("ball") else {
        return false;
    };
    let Ok(hwnd) = win.hwnd() else {
        crate::diag::warn("ballstyle: apply_ball_shape 取球窗 HWND 失败，跳过");
        return false;
    };
    unsafe {
        // visible ⇒ 椭圆（中央圆）；!visible ⇒ 零面积空区域（整窗不可见、不吞点击）。
        let rgn = if visible {
            let mut rc = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            let (w, h) = if GetClientRect(hwnd, &mut rc).is_ok() {
                (rc.right - rc.left, rc.bottom - rc.top)
            } else {
                // 兜底：客户区读不到时按「逻辑边长 × 缩放」换算物理边长，保证仍是个圆。
                let scale = win.scale_factor().unwrap_or(1.0);
                let px = (BALL_LOGICAL * scale).round() as i32;
                (px, px)
            };
            // ★ 防御：退化椭圆（w 或 h ≤ 0）会让 SetWindowRgn 得到零面积区域 ⇒ 球「莫名消失」。
            //   正常路径下 GetClientRect 给 80×80，不会走到；真走到则以兜底边长重建。
            if w <= 0 || h <= 0 {
                let scale = win.scale_factor().unwrap_or(1.0);
                let px = (BALL_LOGICAL * scale).round().max(1.0) as i32;
                crate::diag::warn(&format!(
                    "ballstyle: apply_ball_shape 读到退化客户区 {w}x{h}，改用兜底 {px}x{px}"
                ));
                CreateEllipticRgn(0, 0, px, px)
            } else {
                CreateEllipticRgn(0, 0, w, h)
            }
        } else {
            CreateRectRgn(0, 0, 0, 0)
        };
        // SetWindowRgn 返回非 0 = 成功（区域所有权随即移交系统）。失败则自行释放区域防 GDI 泄漏。
        let ok = SetWindowRgn(hwnd, Some(rgn), true) != 0;
        if !ok {
            let _ = DeleteObject(rgn.into());
            // ★ 如实告警（此前这里会继续打「成功」日志，掩盖失败）。
            crate::diag::warn(&format!(
                "ballstyle: apply_ball_shape **SetWindowRgn 失败**（visible={visible}）—— 区域未生效"
            ));
        } else {
            crate::diag::log(&format!(
                "ballstyle: apply_ball_shape visible={visible}（{}）",
                if visible {
                    "椭圆区域 ⇒ 圆形外观"
                } else {
                    "零面积空区域 ⇒ 整窗不可见/不吞点击"
                }
            ));
        }
        ok
    }
}

/// 读回球窗当前窗口 region，判断它是否**已经**符合期望形态（椭圆 / 空区域）。
///
/// ## 为什么需要（★ 2026-10-13 P0-5，真机日志确诊后新增）
/// 「隐藏被打回」的根因是：某些窗口操作（`SWP_FRAMECHANGED`、tao 的
/// `set_ignore_cursor_events`/`set_size`/`show` 等）会**静默清掉** `SetWindowRgn` 设的区域，
/// 而清掉之后**窗口没有任何日志**、`clear_caption_bits`/`make_tool_window` 也可能恰好都返回
/// `false` ⇒ 此前「按样式位改动收口」的条件判断**漏掉这一拍**，region 永久丢失。
///
/// 故改为**读回校验**：直接问系统「本窗现在的 region 是什么形状」，只有确实不符才重设。
/// 这样即便破坏源完全未知、也不产生任何日志，也能被本函数兜住。
///
/// ## 判定依据（`GetWindowRgn` 返回的区域类型）
/// - `NULLREGION`（空区域）⇒ 对应「隐藏」用的 `CreateRectRgn(0,0,0,0)`。
/// - `SIMPLEREGION` / `COMPLEXREGION`（有面积的单一/复合区域）⇒ 对应「显示」用的椭圆。
/// - `ERROR`（返回 0）⇒ **无 region**（整窗全可见）或调用失败 ⇒ 一律视为「不符」，触发重设。
///   （这是关键的兜底：region 被清掉后 `GetWindowRgn` 正是返回 `ERROR`。）
///
/// ## GDI 句柄
/// `GetWindowRgn` 要求调用方传入一个**已创建**的 region 句柄作为接收容器；
/// 本函数用 `CreateRectRgn(0,0,0,0)` 造空的，读完**必须** `DeleteObject` 回收
/// （与 `SetWindowRgn` 不同 —— 这里所有权**没有**移交，不删就是 GDI 泄漏）。
///
/// 读回失败（取不到 HWND / `CreateRectRgn` 失败）时返回 `false` ⇒ 调用方会执行一次重设，
/// 属于「宁可多设一次」的安全侧。
fn region_matches<R: Runtime>(win: &tauri::WebviewWindow<R>, visible: bool) -> bool {
    let Ok(hwnd) = win.hwnd() else {
        return false;
    };
    unsafe {
        let probe = CreateRectRgn(0, 0, 0, 0);
        if probe.is_invalid() {
            return false;
        }
        let kind = GetWindowRgn(hwnd, probe);
        // 无论结果如何都要回收——此处所有权未移交（与 SetWindowRgn 语义相反）。
        let _ = DeleteObject(probe.into());
        // 注意：`NULLREGION` / `SIMPLEREGION` / `COMPLEXREGION` 在 windows 0.61 里都是
        // `GDI_REGION_TYPE(i32)` **常量**（值分别 1 / 2 / 3），不是枚举变体 ⇒ **不能**用在
        // `match` 的模式位上（编译器会因「常量非结构模式」而报错）。故改用**比较**判定。
        if kind == NULLREGION {
            // 空区域 ⇒ 对应隐藏态的 `CreateRectRgn(0,0,0,0)`。
            !visible
        } else if kind == SIMPLEREGION || kind == COMPLEXREGION {
            // 有面积的单一/复合区域 ⇒ 对应显示态的椭圆。
            visible
        } else {
            // ERROR（GDI_REGION_TYPE(0)）：无 region（region 已被静默清掉，整窗可见）
            // 或调用失败。两种情形都判「不符」，让调用方重设一次。
            false
        }
    }
}

/// 让球窗成为**工具窗口**：清掉 `WS_EX_APPWINDOW`、置上 `WS_EX_TOOLWINDOW`、并置上
/// `WS_EX_NOACTIVATE`。返回是否发生改动。
///
/// 为什么（2026-09-25 第六次排查，真机窗口枚举）：实测球窗
/// `EXSTYLE=0x00040018` = `WS_EX_APPWINDOW | WS_EX_TOPMOST | WS_EX_ACCEPTFILES`
/// —— **有 `WS_EX_APPWINDOW` 却缺 `WS_EX_TOOLWINDOW`**，尽管 `tauri.conf.json` 已写
/// `skipTaskbar: true`（tao 未把该位落到 EXSTYLE 上）。
/// 按 Windows 规则，「可见 + 无 owner + 有 `WS_EX_APPWINDOW` + 无 `WS_EX_TOOLWINDOW`」的
/// 顶层窗口**会获得任务栏按钮**；而任务栏按钮会给出**截断的窗口标题 tooltip** ——
/// 这正是用户截图中「`DeepSee...`」浅色小框的形态（探针期标题 `BALL9Q2` 短、显示完整；
/// 改回「DeepSeek 悬浮助手」后被截断为 `DeepSee...`）。
/// 悬浮球本就不该出现在任务栏 / Alt+Tab，此修正同时消除该 tooltip。
///
/// **`WS_EX_NOACTIVATE`（2026-09-25 第九次排查，§11.4 终于启用）**：
/// 用户决定性的肉眼证据 —— 闪现文字 = 球窗**自己的 OS 标题**（`DeepSeek 悬浮助手` 被截断成
/// `DeepSee...`）、位置**紧贴球后**、带省略号、且**只在按住左键期间**出现 —— 直接复活 §11.2：
/// 闪现 = 球窗在 **被前台激活（foreground-activate）** 时被 Windows 画出的「活动标题栏」。
/// 给球窗加上 `WS_EX_NOACTIVATE` 后，球**永不被前台激活**（即使在 mousedown 那一刻），
/// Windows 因此**永不绘制活动标题栏** ⇒ 这一闪现根因被消除。
/// 此位在 `make_tool_window` 里一并设置，可同时覆盖 `fix()`（启动期）与
/// [`enforce_captionless`]（运行期热点路径）两处，天然幂等（下方 `if want == ex` 已拦截）。
/// 关于 `startDragging` 拖动兼容性：tao 的拖动实现是向本窗投递
/// `WM_NCLBUTTONDOWN` + `HTCAPTION`（捕获驱动的系统的移动循环），**不依赖窗口被激活**，
/// `WS_EX_NOACTIVATE` 不会阻断该循环，故拖动照常可用（已在 `poc/ball.html` 回归确认）。
///
/// 幂等；仅在确实改动时落日志。注意 tao 的 `set_ignore_cursor_events` 会重算 EXSTYLE，
/// 故运行期由 [`enforce_captionless`] 反复校正。返回是否发生改动。
fn make_tool_window(hwnd: HWND) -> bool {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let want = (ex & !WS_EX_APPWINDOW.0) | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0;
        if want == ex {
            return false;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, want as isize);
        // ★★★ 2026-10-13（region 时序缺陷修复，P0-2 纵深防御）：**去掉 `SWP_FRAMECHANGED`**。
        //   历史成因：2026-09-14 清 `WS_CAPTION` 时确实需要 FRAMECHANGED 让系统重算非客户区
        //   （那是 GWL_**STYLE** 中影响 NC 尺寸的位，见 `clear_caption_bits`）。
        //   但本函数改的是 GWL_**EXSTYLE**（扩展样式：TOOLWINDOW/NOACTIVATE/APPWINDOW），
        //   **不影响非客户区尺寸，无需重算**；而 `SWP_FRAMECHANGED` 会**清掉窗口 region**
        //   （`SetWindowRgn` 设的椭圆/空区域），直接造成用户所报「显隐悬浮球按钮不生效」。
        //   仅保留 SWP_NOMOVE|NOSIZE|NOZORDER|NOACTIVATE（不改变位置/尺寸/Z 序/激活态）。
        //   纵深防御：即便将来有人在别处漏了「末尾重套 region」，这里也不再主动破坏它。
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        // 注：方案 A-2 后**不再**在此重申明 blur-behind 透明（那会把「镂空洞」打回来）；
        // 圆形外观由 [`apply_ball_shape`] 的椭圆区域负责，且由 `ball_show` 每次显形重套
        // （★ 2026-10-13：重套已移至 `ball_show` **末尾**，此前在开头会被本函数的
        //   SWP_FRAMECHANGED 清掉 —— 今已双双修正）。
        crate::diag::log(&format!(
            "ballstyle: 球窗改工具窗口 EXSTYLE=0x{ex:08X} -> 0x{want:08X}（清 WS_EX_APPWINDOW、置 WS_EX_TOOLWINDOW ⇒ 无任务栏按钮/tooltip）"
        ));
        true
    }
}

/// 以**原生 Win32** 重新申请球窗尺寸（`SetWindowPos`），刻意**绕过 tao 的 `set_size`**。
///
/// ## 为什么不能用 tao 的 `set_size`（★ 2026-10-13 F3，tao 源码级确诊）
/// tao 的 `set_inner_size`（`tao-0.35.3/src/platform_impl/windows/window.rs`）内部会调用
/// `WindowState::set_window_flags(|f| f.set(WindowFlags::MAXIMIZED, false))`，
/// 从而进入 `apply_diff` —— 而该函数末尾有一段**只依赖 `new`、与 diff 是否为空无关**的
/// **无条件隐藏**：
/// ```text
/// if !new.contains(WindowFlags::VISIBLE) {
///     let _ = ShowWindow(window, SW_HIDE);
/// }
/// ```
/// 球窗以 `visible:false` 创建（`tauri.conf.json`），且我们**从不调用 tao 的 `show()`**
/// （全走原生 `ShowWindow`，见 [`crate::panels::ball_show`]），故 tao 缓存的 `window_flags`
/// 里 `VISIBLE` 位**恒为 0** ⇒ **每一次 `set_size` 都会把球窗真正隐藏（`WS_HIDE`）**。
///
/// 这正是「悬浮球隐藏后无法再显示」缺陷的破坏源 ③ / ④（另两处是
/// [`crate::panels::ball_hide`] / [`crate::panels::ball_show`] 里的
/// `set_ignore_cursor_events`）。原生 `SetWindowPos` 只改尺寸、**不触碰任何 tao flag**。
///
/// ## 尺寸换算
/// 球窗 `decorations:false` + `shadow:false` ⇒ 窗口尺寸即客户区尺寸，
/// 可直接按「逻辑边长 × 缩放」取物理像素（与 tao 在 `undecorated_with_shadows()==false`
/// 时的实际行为等价）。
///
/// 返回是否成功下发。
fn resize_ball_native<R: Runtime>(win: &tauri::WebviewWindow<R>, hwnd: HWND) -> bool {
    let scale = win.scale_factor().unwrap_or(1.0);
    let px = (BALL_LOGICAL * scale).round().max(1.0) as i32;
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            px,
            px,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .is_ok()
    }
}

/// 供窗口模块在「显球 / 冲刷 / 弹右键菜单」前调用：若球窗标题栏位被某次操作恢复则重新清除，
/// 纠正被连带夹宽的尺寸，并再次禁用 DWM 非客户区渲染。幂等、可高频调用。
///
/// 与 [`fix`] 的区别：不读回校验、无日志噪声（仅在**确实发现标题栏位重现**时记一条），
/// 因此可以放进 [`crate::panels::flush_ball`] 这类热点路径。
///
/// **为什么还要纠正尺寸**：`SM_CXMINTRACK` 夹取由 `WS_CAPTION` 触发。标题栏位一旦重现，
/// 球窗宽度会被系统同时夹回 136px（真机确认）—— 只清位不设尺寸会让宽度停在 136，
/// 球的右侧重新出现吞点击的死区。故标题栏位重现时**连尺寸一起纠正**。
///
/// **热点路径去重（2026-09-25 第八次排查）**：本函数处于 `flush_ball` 热点路径上
/// （球态**每次左键**都会跑 `toggle_panel` → `show_panel`/`hide_panel` → `flush_ball` →
/// 本函数）。改造前此处对 [`disable_dwm_frame`] 的调用是**无条件**的，即每点一次就向 DWM
/// 下发一次 `DWMWA_NCRENDERING_POLICY` → DWM 重算一次窗口框架/重合成一拍，正是「点一下球，
/// 背后一闪而逝」的首要根因。现 [`disable_dwm_frame`] 内部已改为**先经 `DWMWA_NCRENDERING_ENABLED`
/// 读回、仅在 NC 渲染确实开着（或读回失败）时才下发**，故本函数在「已关闭」常态下**零 DWM 调用**；
/// 仍保留对它的调用，以维持「有人把 NC 渲染打开时下一拍自愈」的纵深防御。
pub(crate) fn enforce_captionless<R: Runtime>(app: &AppHandle<R>) {
    let Some(win) = app.get_webview_window("ball") else {
        return;
    };
    let Ok(hwnd) = win.hwnd() else {
        return;
    };
    // ★★★ 2026-10-13（region 时序缺陷修复 P0-4，真机日志确诊）：
    //   本函数会经 `clear_caption_bits` 下发 `SetWindowPos(SWP_FRAMECHANGED)` —— 而该调用会
    //   **清掉球窗的窗口 region**（`apply_ball_shape` 设的椭圆/零面积空区域）。
    //   真机证据（d1-debug.log）：`ball_hide` 完成于 +9394ms，仅 7ms 后即出现
    //     `ballstyle: 清除标题栏位 style=0x04CB0000 -> 0x04000000`
    //     `ballstyle: 运行时检测到球窗标题栏位被恢复，已重新清除`
    //   —— 触发源是 `windows.rs` 的 `Focused`/`Resized` 事件处理器
    //   （菜单关闭 → 焦点回到球 → enforce_captionless），把刚设的「零面积空区域」清掉
    //   ⇒ 球又露出来 ⇒ 用户看到「隐藏悬浮球不生效」。
    let region_at_risk = clear_caption_bits(hwnd);
    if region_at_risk {
        crate::diag::log(
            "ballstyle: 运行时检测到球窗标题栏位被恢复，已重新清除（并纠正可能被夹宽的尺寸）",
        );
        // 标题栏位重现 ⇒ 夹取同时生效，窗口宽度已被拉大；重新申请 64×64 收回死区。
        // ★★ 2026-10-13（F3）：改用原生 [`resize_ball_native`] —— 属**加固**，非修 bug。
        //    ⚠️ 更正：早期注释称「tao 的 `set_size()` 会触发 `ShowWindow(SW_HIDE)`」——**是错的**。
        //    `set_inner_size` 内部只 `set(MAXIMIZED, false)`，而球窗从不最大化 ⇒ `diff` 为空 ⇒
        //    `apply_diff` 在早退分支 `if diff == WindowFlags::empty() { return; }` 直接返回，
        //    **从不**触发 `SW_HIDE`（详见下方 F2 注释的「更正」段）。
        //    改原生的价值：球窗的尺寸修正不再经过 tao 的 `WindowState`，杜绝将来
        //    `MAXIMIZED` 或其他位一旦变化时被卷入 `apply_diff`（潜在破坏源）。
        if !resize_ball_native(&win, hwnd) {
            crate::diag::warn(
                "ballstyle: 原生 SetWindowPos 尺寸纠偏失败（球右侧可能重现吞点击死区）",
            );
        }
    }
    // 条件判断在 disable_dwm_frame 内部完成 ⇒ 已关闭 NC 渲染时这里**不产生任何 DWM 调用**。
    let _ = disable_dwm_frame(hwnd);
    // 3) 工具窗口位同样会被 tao 的 `set_window_flags`（`show`/`hide`/`set_ignore_cursor_events`）
    //    按它缓存的 EXSTYLE 重算掉，故每条热点路径都重新校正一次。幂等、仅在真改动时落日志。
    let tool_changed = make_tool_window(hwnd);

    // 本函数出口收口用的「权威期望态」：球当前**应当**是否可见。
    // ★ 判据必须是逻辑态 [`crate::panels::ball_is_hidden`] —— **不可**用 `win.is_visible()`：
    //   方案 A-2 下隐藏靠「零面积空区域」实现，窗口其实仍 `WS_VISIBLE`，`is_visible()` 会恒为 true。
    let want_visible = !crate::panels::ball_is_hidden();

    // ★★★ 2026-10-13（F2：**可见性**读回自愈 —— 与下方 P0-5 的 region 自愈同位置、同理念）
    //
    // 【它修的是哪条通道】P0-1～P0-5 全都在修 **region**（椭圆/空区域被 `SWP_FRAMECHANGED` 清掉）。
    //   但真机又暴露出**另一条完全独立**的通道：`WS_VISIBLE` 被清掉。
    //   region 管不了 `WS_HIDE` 的窗口 —— 此时套椭圆毫无意义，球照样不可见，
    //   而日志还会满口「已恢复可见」。这正是用户所报「隐藏后无法再显示」。
    //
    // 【这条通道的根因（tao 0.35.3 源码级确诊）】tao 的 `WindowFlags::apply_diff`
    //   （`platform_impl/windows/window_state.rs`）在 `diff` **非空**时，末尾会执行一段
    //   **只依赖 `new`、与 diff 内容无关**的**无条件隐藏**：
    //       if !new.contains(WindowFlags::VISIBLE) { ShowWindow(window, SW_HIDE); }
    //   而球窗以 `visible:false` 创建（`tauri.conf.json`）、且我们**从不调 tao 的 `show()`**
    //   （全走原生 `ShowWindow`，见 [`crate::panels::ball_show`]）
    //   ⇒ tao 缓存的 `window_flags` 里 `VISIBLE` 位**恒为 0**
    //   ⇒ **只要 diff 非空**，就会重放一次 `SW_HIDE`。
    //
    // 【真正的破坏源只有 2 处（独立验证已修正早期误判）】
    //   ① [`crate::panels::ball_hide`] 的 `set_ignore_cursor_events(true)`
    //   ② [`crate::panels::ball_show`] 的 `set_ignore_cursor_events(false)`
    //   二者都**翻转** `IGNORE_CURSOR_EVENT` 位 ⇒ `diff` 必非空 ⇒ 必然走到 `SW_HIDE`。
    //   这也解释了缺陷为何「时好时坏」：球在**首次隐藏之前**该位为 false，故那时的
    //   `set_ignore_cursor_events(false)` 是 diff 为空的 **no-op**（显示正常 ——
    //   真机日志 `+5792ms` 那次 `ball_show` 无 `ShowWindow` 行可证）；而**一旦隐藏过**
    //   （该位置真），此后每次调用都翻转 ⇒ 重放 `SW_HIDE` ⇒ 把同函数内刚做的显形救援
    //   当场撤销（日志 `+19329ms` 那次 `ShowWindow` 突然出现可证）。
    //
    //   ⚠️ **更正**：早期曾把 `set_size()`（本函数与 [`fix`] 内）也列为破坏源 —— **是错的**。
    //   `set_inner_size` 内部只 `set(MAXIMIZED, false)`，而球窗从不最大化 ⇒ `diff` 为空 ⇒
    //   在 `apply_diff` 的早退分支 `if diff == WindowFlags::empty() { return; }` 直接返回，
    //   **从不**触发 `SW_HIDE`。F3 把它改为原生 [`resize_ball_native`] 属**加固**
    //   （消除对 tao 窗口 API 的依赖），而非修复既有缺陷。
    //
    //   F3 已删除 ①②；但**破坏源仍可能再出现**（tao 升级、后续新代码，乃至
    //   [`crate::panels::setup_ball_position`] 里球窗的 `set_position` —— 当前因
    //   `MAXIMIZED` 位恒 false 而 diff 为空、属**潜伏**破坏源），故此处保留**读回自愈**
    //   作为兜底 —— 这才是「根治」而非「打补丁」。
    //
    // 【判据：直接问 OS】`IsWindowVisible` 就是 `WS_VISIBLE` 的真实状态，
    //   不依赖任何标志位 / 缓存推断 —— 与 region 自愈用 `GetWindowRgn` 读回同理，
    //   都是「按结果校验」而非「按起因推断」。
    //   常态（可见性未被破坏）**零额外 Win32 调用**，热路径开销与 P0-5 相同。
    //
    // 【为什么 `want_visible == false` 时刻意不做 SW_HIDE】零面积空区域已令整窗不可见；
    //   而保留 `WS_VISIBLE` 正是方案 A-2「合成表面常驻、show 侧不撕裂」的设计前提。
    //
    // 【顺序】放在 region 自愈**之前** —— 保持「region 最后落笔」的既有纪律
    //   （`ShowWindow` 本身并不清 region，但让它排在 region 之后会新增一个潜在破坏点；
    //     放前面与既有约定一致、代价为零）。
    let os_visible = unsafe { IsWindowVisible(hwnd).as_bool() };
    if want_visible && !os_visible {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        crate::diag::log(
            "ballstyle: 可见性自愈 —— IsWindowVisible=false 但逻辑态为可见，已原生 ShowWindow(SW_SHOWNOACTIVATE)",
        );
    }

    // ★★★ 2026-10-13（P0-5，**替换**原 P0-4 的「按样式位改动条件收口」）：
    //
    // 【为什么废弃条件收口】P0-4 写的是 `if region_at_risk || tool_changed { 重套 }`，
    // 把「重套 region」绑定在「样式位是否被改」上。真机日志证明**这个前提不成立**：
    //   `#60 apply_ball_shape visible=false` → `#61 ball_hide` → **空白 2.4 秒** →
    //   `#62 apply_ball_shape visible=true`
    // 这一拍既无 `清除标题栏位` 也无 `收口` —— 即 `clear_caption_bits` 与 `make_tool_window`
    // **都返回了 false**，条件不成立 ⇒ 收口被跳过，可 region 确实已被清掉。
    // 统计坐实：`ball_hide:` 共 7 次，而收口 `visible=false` 仅 3 次 ⇒ **4 次隐藏无保护**。
    // 根因：**region 的破坏源与「样式位是否被改动」并不一一对应** —— 任何不经过本函数的路径
    // （tao 的 `set_ignore_cursor_events`/`set_size`、窗口事件触发的一次 `SWP_FRAMECHANGED`、
    // 乃至系统自身的 NC 重算）都可能静默清掉 region，且不留任何日志。
    //
    // 【现方案：读回校验 + 自愈】不再猜「谁破坏了 region」，而是直接问系统
    // `GetWindowRgn`「现在是什么形状」（见 [`region_matches`]）：
    //   · 符合当前逻辑态 ⇒ 什么都不做（**常态零额外 Win32 调用**，热路径开销与 P0-4 相同）；
    //   · 不符（含 `GetWindowRgn` 返回 ERROR = region 已被清掉）⇒ 重设一次。
    // 这样无论破坏源是否已知、是否留日志，都能被兜住 —— 这是「按结果校验」而非「按起因推断」。
    if !region_matches(&win, want_visible) {
        let ok = apply_ball_shape(app, want_visible);
        crate::diag::log(&format!(
            "ballstyle: enforce_captionless 自愈 —— region 读回与逻辑态不符(clear={region_at_risk} tool={tool_changed} want={want_visible})，已重套（ok={ok}）"
        ));
    }
}

/// 修正悬浮球窗口的样式位与尺寸。幂等。
pub fn fix(app: &AppHandle) {
    let Some(win) = app.get_webview_window("ball") else {
        crate::diag::warn("ballstyle: 找不到 ball 窗口，跳过样式修正");
        return;
    };
    let hwnd = match win.hwnd() {
        Ok(h) => h,
        Err(e) => {
            crate::diag::warn(&format!("ballstyle: 取 HWND 失败: {e}"));
            return;
        }
    };

    // 1) 清掉标题栏样式位 —— 这是 SM_CXMINTRACK 夹取的触发条件。
    if !clear_caption_bits(hwnd) {
        crate::diag::log("ballstyle: 标题栏位已不在，无需修改");
    }

    // 1b) 纵深防御：令 DWM **不渲染本窗非客户区**（标题栏 / 边框）。
    //     真机探针 2026-09-25：闪现内容实测为**球窗自身标题** `BALL9Q2` —— 即某次操作下
    //     标题栏被画了出来。即便 `WS_CAPTION` 位被临时恢复，只要 DWM 侧禁用非客户区渲染，
    //     标题栏就画不出来。幂等。
    //     返回值现为 bool（是否真的下发）；启动期 NC 渲染通常为「开」，条件判断会真实下发一次。
    let _ = disable_dwm_frame(hwnd);

    // 1c) 工具窗口位（2026-09-25 第六次排查）：`tauri.conf.json` 里写着 `skipTaskbar: true`，
    //     但真机窗口枚举实测 `EXSTYLE=0x00040018` = `WS_EX_APPWINDOW | WS_EX_TOPMOST |
    //     WS_EX_ACCEPTFILES` —— 有 `WS_EX_APPWINDOW`、**缺** `WS_EX_TOOLWINDOW`，即该配置
    //     根本没落到 EXSTYLE 上，球被 Windows 当成普通应用窗口**塞进了任务栏**
    //     （任务栏按钮会给出**截断到省略号**的窗口标题 tooltip —— 正是用户截图里
    //     `DeepSee...` 浅色小框的形态）。悬浮球本就不该出现在任务栏 / Alt+Tab。
    let _ = make_tool_window(hwnd);

    // 1d) 方案 A-2：**不再**重申明 blur-behind 透明 —— 球窗已改为不透明窗，透明通道改由
    //     [`apply_ball_shape`] 的椭圆区域负责（由 `ball_show` 显形时套用）。此处若再打
    //     blur-behind 会把「整窗镂空」打回来，正是本次要消灭的根因。

    // 2) 重新申请尺寸（此前会被夹成 136 逻辑像素宽）。
    // ★★ 2026-10-13（F3）：改用原生 [`resize_ball_native`] —— 属**加固**，非修 bug。
    //    ⚠️ 更正两处早期误述：
    //      · 早期注释称本调用「把 `gate_ball_show` 刚显形的球窗又隐藏回去」—— 因果写反了：
    //        本函数（`fix`）在 `gate_ball_show` **之前**执行（见 `lifecycle.rs` 的 setup 顺序）。
    //      · 早期注释称 tao 的 `set_size()` 会触发 `ShowWindow(SW_HIDE)` —— **是错的**
    //        （`set_inner_size` 只 `set(MAXIMIZED,false)`，球窗从不最大化 ⇒ `diff` 为空 ⇒
    //         `apply_diff` 早退，从不 `SW_HIDE`；详见 [`enforce_captionless`] 的 F2 注释）。
    //    改原生的价值是消除球窗对 tao 窗口 API 的依赖，杜绝潜在破坏源。
    if !resize_ball_native(&win, hwnd) {
        crate::diag::warn("ballstyle: 原生 SetWindowPos 尺寸修正失败");
    }

    // 3) 读回校验 —— 与 screenshot.rs 同口径：别把结论押在时序/假设上。
    match (win.inner_size(), win.scale_factor()) {
        (Ok(s), Ok(scale)) if scale > 0.0 => {
            let lw = s.width as f64 / scale;
            let lh = s.height as f64 / scale;
            if (lw - BALL_LOGICAL).abs() > 0.5 || (lh - BALL_LOGICAL).abs() > 0.5 {
                crate::diag::warn(&format!(
                    "ballstyle: 尺寸仍不符 physical={}x{} scale={scale:.3} logical={lw:.0}x{lh:.0}（期望 {BALL_LOGICAL:.0}）",
                    s.width, s.height
                ));
            } else {
                crate::diag::log(&format!(
                    "ballstyle: 尺寸已修正 physical={}x{} scale={scale:.3} logical={lw:.0}x{lh:.0}",
                    s.width, s.height
                ));
            }
        }
        _ => crate::diag::warn("ballstyle: 读回 inner_size / scale_factor 失败"),
    }
}

// ======================= 测试（QA 2026-09-25 新增，仅测试代码） =======================
//
// 把本次修复的**命门性质**变成可执行断言：「NC 渲染已关闭时，`disable_dwm_frame`
// 零 DWM 下发（提前返回 false）」。
//
// 做法：用 `CreateWindowExW` 建一个**隐藏的顶层 STATIC 窗口**当 HWND 靶子（不 ShowWindow），
// 先真实下发一次把 NC 渲染关掉，读回确认属性已为 0，再第二次调用断言其走提前返回分支。
//
// 鲁棒性：无桌面会话 / CI 环境下窗口创建或 DWM 读回可能失败 —— 此时**优雅跳过并打印原因**，
// 绝不 panic 成假失败。
#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
    };

    /// 读回 `DWMWA_NCRENDERING_ENABLED`（1 = 开，0 = 关）。
    fn read_nc_enabled(hwnd: HWND) -> Result<u32, String> {
        let mut v: u32 = u32::MAX;
        unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_ENABLED,
                &mut v as *mut u32 as *mut core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(v)
    }

    /// 创建一个**隐藏的顶层** `STATIC` 窗口当 HWND 靶子（不 `ShowWindow`）。
    fn make_target_hwnd() -> Result<HWND, String> {
        unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("Static"),
                w!("dsfloat-dwm-probe"),
                WS_OVERLAPPED,
                0,
                0,
                64,
                64,
                None,
                None,
                None,
                None,
            )
            .map_err(|e| e.to_string())
        }
    }

    /// 命门：NC 渲染已关闭 ⇒ `disable_dwm_frame` 再调用应为**零下发**（提前返回 false）。
    #[test]
    fn disable_dwm_frame_skips_dispatch_when_nc_already_disabled() {
        let hwnd = match make_target_hwnd() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[skip] 无法创建靶窗口（无桌面会话 / CI？），跳过本测试：{e}");
                return;
            }
        };

        // 基线：隐藏顶层窗新建时，NC 渲染默认应为「开」（=1）。
        let base = read_nc_enabled(hwnd);

        // 第一次调用：基因为「开」时应真的下发并返回 true。
        let first = disable_dwm_frame(hwnd);
        eprintln!(
            "[probe] hwnd={hwnd:?} 基线 NC 渲染={base:?} 首次 disable_dwm_frame 返回={first}"
        );
        if let Ok(b) = base {
            if b != 0 {
                assert!(
                    first,
                    "基线 NC 渲染为开({b})时，首次 disable_dwm_frame 应真的下发并返回 true，实测 {first}"
                );
            }
        }

        // 读回：真正禁用后属性应为 0；不可读则优雅跳过（不 panic）。
        let after = match read_nc_enabled(hwnd) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[skip] 下发后 DWMWA_NCRENDERING_ENABLED 不可读，跳过：{e}");
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return;
            }
        };

        // ★ 命门断言 1：禁用后读回应为 0。
        assert_eq!(
            after, 0,
            "禁用后 DWMWA_NCRENDERING_ENABLED 应读回 0，实测 {after}（基线 {base:?} / 首次下发返回 {first}）"
        );

        // ★ 命门断言 2：NC 渲染已关闭 ⇒ 再次调用必须走提前返回、返回 false（零下发）。
        let second = disable_dwm_frame(hwnd);
        eprintln!("[probe] 下发后 NC 渲染读回={after} 第二次 disable_dwm_frame 返回={second}（期望 false=零下发）");
        assert!(
            !second,
            "NC 渲染已关闭时 disable_dwm_frame 应提前返回 false（零下发），实测 {second}"
        );

        // ★ 命门断言 3：第二次调用未把状态改回「开」。
        let final_v = read_nc_enabled(hwnd).unwrap_or(9999);
        assert_eq!(final_v, 0, "第二次调用后 NC 渲染应仍为关，实测 {final_v}");

        // 回归断言（2026-09-25 QA 追加）：本次根因修复的命门位 ——
        // `make_tool_window` 必须给目标窗置上 `WS_EX_NOACTIVATE`。
        // 该位缺失 ⇒ 「按住即闪 OS 标题」的根因（球窗被前台激活而绘制活动标题栏）未消除。
        // 复用一个已建好、隐藏的 HWND 靶子，断言其 EXSTYLE 经 `make_tool_window` 后含该位。
        let _changed = make_tool_window(hwnd);
        let ex_after = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
        eprintln!(
            "[probe] make_tool_window 返回={_changed} 后 EXSTYLE=0x{ex_after:08X}（期望含 WS_EX_NOACTIVATE=0x{:08X}）",
            WS_EX_NOACTIVATE.0
        );
        assert!(
            (ex_after & WS_EX_NOACTIVATE.0) != 0,
            "make_tool_window 应置上 WS_EX_NOACTIVATE，实测 EXSTYLE=0x{ex_after:08X}（期望含 0x{:08X}）",
            WS_EX_NOACTIVATE.0
        );

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}
