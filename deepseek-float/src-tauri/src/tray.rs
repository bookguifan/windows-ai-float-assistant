//! 系统托盘和悬浮球使用同一个自绘菜单。
use crate::error::AppResult;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

pub(crate) fn setup_tray(app: &AppHandle) -> AppResult<()> {
    let mut builder = TrayIconBuilder::new().tooltip("DeepSeek 悬浮助手");
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                crate::context_menu::open(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
