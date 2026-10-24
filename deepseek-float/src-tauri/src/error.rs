//! 统一错误类型（crate 级）。
//!
//! 背景：本工程历史上并存三类「错误表达」，彼此不可组合：
//!   1. `Box<dyn std::error::Error>` —— `lib.rs` 的 setup 子函数用它，类型被擦除，
//!      调用方无法判别错误种类，也无法在边界处统一成型；
//!   2. `Result<_, String>` —— `prompts::save` 用它，错误被字符串化，
//!      既不能 `?` 链式传播，也无法 match 分支，还容易被误当成正常返回值；
//!   3. `bool`「成功位」—— `autostart::set_autostart` / `capture::set_clipboard_text` /
//!      `mousehook::start` 用它。**这些刻意保留**：它们是跨 IPC / FFI 的对外契约
//!      （前端只关心成败并据此回滚开关），改成 Result 只会徒增两侧适配代码。
//!
//! 本模块把 (1)(2) 收敛为 `AppError` + `AppResult<T>`：错误对象自带
//! 「哪一步失败（context）+ 底层原因（source）」，既能 `?` 传播，也能在 IPC 边界
//! 统一渲染成用户可见文案（见 `settings_save` 等命令）。
//!
//! 与 `diag` 的分工（互补、不重叠）：
//!   - `AppError`：描述**会向上传播**的错误，由调用方决定如何处置；
//!   - `diag::{log,warn,error}`：记录**就地吞掉、只做降级**的异常
//!     （拖拽落盘失败、剪贴板被占用、注册表写入失败等），不影响控制流。

use std::fmt;

/// 应用统一错误。
#[derive(Debug)]
pub enum AppError {
    /// Tauri 框架层失败（窗口 / 菜单 / 托盘 / 事件构建）。
    Tauri(tauri::Error),
    /// 文件系统读写失败；`context` 指明是哪一步。
    Io {
        context: &'static str,
        source: std::io::Error,
    },
    /// JSON 序列化 / 反序列化失败；`context` 指明是哪一步。
    Json {
        context: &'static str,
        source: serde_json::Error,
    },
    /// 运行期必需资源缺失（如 `LOCALAPPDATA` 不可用 → 定位不到配置目录）。
    Unavailable(&'static str),
}

/// crate 级统一 `Result` 别名。
pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    /// 构造带上下文的 I/O 错误。
    pub fn io(context: &'static str, source: std::io::Error) -> Self {
        Self::Io { context, source }
    }

    /// 构造带上下文的 JSON 错误。
    pub fn json(context: &'static str, source: serde_json::Error) -> Self {
        Self::Json { context, source }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Tauri(e) => write!(f, "Tauri 框架错误: {e}"),
            AppError::Io { context, source } => write!(f, "{context}: {source}"),
            AppError::Json { context, source } => write!(f, "{context}: {source}"),
            AppError::Unavailable(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::Tauri(e) => Some(e),
            AppError::Io { source, .. } => Some(source),
            AppError::Json { source, .. } => Some(source),
            AppError::Unavailable(_) => None,
        }
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Tauri(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn io_error_keeps_context_and_cause() {
        let e = AppError::io(
            "写入 prompts.json 失败",
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        );
        let s = e.to_string();
        assert!(s.contains("写入 prompts.json 失败"), "应含上下文: {s}");
        assert!(s.contains("denied"), "应含底层原因: {s}");
        assert!(e.source().is_some(), "应暴露 source 链");
    }

    #[test]
    fn json_error_keeps_context() {
        let raw = serde_json::from_str::<serde_json::Value>("{bad").unwrap_err();
        let e = AppError::json("解析配置失败", raw);
        assert!(e.to_string().contains("解析配置失败"));
        assert!(e.source().is_some());
    }

    #[test]
    fn unavailable_has_no_source() {
        let e = AppError::Unavailable("无法定位配置目录（缺少 LOCALAPPDATA 环境变量）");
        assert_eq!(
            e.to_string(),
            "无法定位配置目录（缺少 LOCALAPPDATA 环境变量）"
        );
        assert!(e.source().is_none());
    }
}
