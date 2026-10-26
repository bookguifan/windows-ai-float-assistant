//! `deepseek-float` 库根：仅做模块声明与再导出，职责单一的代码已拆到子模块。
//!
//! 拆分去向：
//! - `windows.rs`（声明为模块 `panels`，避免与外部 `windows` crate 重名）→ 面板/悬浮球窗口生命周期、位置记忆、划词常驻窗 + 鼠标钩子、诊断窗口；
//! - `tray.rs` → 系统托盘与球态右键菜单；
//! - `ipc.rs` → 全部类型化 `#[tauri::command]`；
//! - `lifecycle.rs` → 应用入口 `run` + 调试期自检 / 压测入口；
//! - `readback.rs` → 回答回读桥（其内部再拆 `analyze` / `watch` / `deliver` / `snapshot`）。

mod autostart;
mod backup;
mod ballready;
mod ballstyle;
mod capture;
mod config;
mod context_menu;
mod data_store;
mod diag;
mod error;
mod fullscreen;
mod hotkey;
mod inject;
mod ipc;
mod lifecycle;
mod mousehook;
mod workflow;
// 模块名取 `panels` 而非 `windows`：外部 `windows` crate（`windows::Win32::…`，见 `hotkey.rs` /
// `inject.rs` 等）若被本地 `mod windows` 遮蔽，会导致全仓库 `windows::Win32::…` 解析失败。
// 源文件仍叫 `windows.rs`（描述其内容：窗口生命周期）。
mod copy_engine;
mod native_copy_proof;
mod panel_site;
#[path = "windows.rs"]
mod panels;
mod popup;
mod prompts;
mod readback;
mod release_policy;
mod result;
mod safe_copy;
mod screenshot;
mod screenshot_state;
mod selection;
mod selection_runtime;
mod settings;
mod single_instance;
mod tray;
mod uia;
mod uia_strategy;

// 对外再导出窗口公开项：既有 `crate::show_panel` / `crate::hide_panel` /
// `crate::ensure_panel_loaded` / `crate::redact_url` 等调用点（ipc / popup / result /
// screenshot / tray / snapshot）走 `crate::<名>`，无需改成 `crate::panels::<名>`。
// `flush_ball` 亦在此再导出：`popup::hide` 需在工具条 hide 后冲刷透明球表面
// （消「首次点球，球背后透出工具条旧帧」）。
// 其余窗口函数仅经 `crate::panels::<名>` 调用，不必在此再导出。
pub(crate) use panels::{ensure_panel_loaded, flush_ball, hide_panel, redact_url, show_panel};

// crate 入口（由 `main.rs` 调用 `deepseek_float_lib::run()`）。
pub use lifecycle::run;
mod favorites;
