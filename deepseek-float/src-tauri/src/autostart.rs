//! §8：开机自启（仅 Windows）。
//!
//! 走注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 写入/删除本进程 exe 路径，
//! 不引入额外 crate。`set_autostart` 成功返回 true；任何一步失败都返回 false（降级为不生效）。
//!
//! ★★ 2026-09-24 真机缺陷（「开机自启后悬浮球显示错乱」）后的**关键约束**：
//! **只有「能独立运行」的构建才有资格被登记为开机自启。**
//!
//! - 未开启 `custom-protocol` 的构建（含**全部 debug 构建**，见 `Cargo.toml` `[features]` 注释）
//!   在 `tauri::generate_context!` 看来处于 **dev 模式**：① 窗口 URL 变成 `build.devUrl`
//!   （`http://localhost:5173/poc/*.html`）、② 前端资源**完全不内嵌**。
//! - 开机时 vite dev server **不在运行**，于是球窗加载 `poc/ball.html` 失败 → WebView2 弹出
//!   连接错误页（中文以「嗯……无法访问此网页」开头）。该错误页在 64×64 无边框透明窗里渲染成
//!   **白底 + 巨字 + 原生滚动条**（`ball.html` 的 `overflow:hidden;width:64px;background:transparent`
//!   此时根本没生效）—— 即用户截图所示现象。
//! - 故本模块在这一类构建下：
//!   ① `set_autostart(true)` **拒绝登记**（返回 false，前端据此回滚开关）；
//!   ② `set_autostart(false)` **始终照常执行删除**，保证用户能把既有的脏项关掉；
//!   ③ 提供 [`dirty_self_registration`]：识别「注册表里的项指向本开发产物」的脏项，供启动期清理。

use windows::core::HSTRING;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ, REG_VALUE_TYPE,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "deepseek-float";

/// 当前是否为 debug 构建（**编译期常量**）。
const DEBUG_BUILD: bool = cfg!(debug_assertions);
/// 当前构建是否开启 `custom-protocol`（**编译期常量**；开启后前端资源才内嵌、窗口 URL 才指向内嵌资源）。
const HAS_CUSTOM_PROTOCOL: bool = cfg!(feature = "custom-protocol");

/// 纯函数：给定构建类型，判断**是否有资格**把本程序登记为开机自启。
///
/// 判据 = 「该二进制能否脱离 vite dev server 独立加载本地页面」，即
/// `!debug_build && custom_protocol`（详见模块头）。抽取为纯函数以便单测锁定。
pub fn can_register_autostart(debug_build: bool, custom_protocol: bool) -> bool {
    !debug_build && custom_protocol
}

/// 当前构建是否支持开机自启（把编译期 `cfg!` 结果喂给纯函数 [`can_register_autostart`]）。
pub fn registration_supported() -> bool {
    can_register_autostart(DEBUG_BUILD, HAS_CUSTOM_PROTOCOL)
}

/// 当前是否已注册开机自启。
///
/// 直接读注册表 Run 项（真实状态），由 `config_get` 用来覆盖 `AppConfig::autostart`，
/// 使设置页开关反映真实注册表状态（例如用户已在任务管理器里禁用了启动项）。
pub fn is_autostart() -> bool {
    unsafe {
        let key = HSTRING::from(RUN_KEY);
        let value = HSTRING::from(VALUE_NAME);
        let mut hkey = HKEY(std::ptr::null_mut());
        if RegOpenKeyExW(HKEY_CURRENT_USER, &key, Some(0), KEY_READ, &mut hkey) != ERROR_SUCCESS {
            return false;
        }
        let exists = RegQueryValueExW(hkey, &value, None, None, None, None) == ERROR_SUCCESS;
        let _ = RegCloseKey(hkey);
        exists
    }
}

/// 读取注册表 Run 项里登记的命令行原文（`REG_SZ`）。
///
/// 不存在 / 非 `REG_SZ` / 读取失败 → `None`。仅用于识别「脏项归属」（见 [`dirty_self_registration`]），
/// 不影响 `is_autostart` 的「存在性」语义。
fn registered_command() -> Option<String> {
    unsafe {
        let key = HSTRING::from(RUN_KEY);
        let value = HSTRING::from(VALUE_NAME);
        let mut hkey = HKEY(std::ptr::null_mut());
        if RegOpenKeyExW(HKEY_CURRENT_USER, &key, Some(0), KEY_READ, &mut hkey) != ERROR_SUCCESS {
            return None;
        }

        // ① 先查类型与所需字节数（lpdata=None 时，lpcbdata 回填「含结尾 NUL 的字节数」）。
        let mut ty = REG_VALUE_TYPE::default();
        let mut size: u32 = 0;
        let q = RegQueryValueExW(hkey, &value, None, Some(&mut ty), None, Some(&mut size));
        if q != ERROR_SUCCESS || ty != REG_SZ || size == 0 {
            let _ = RegCloseKey(hkey);
            return None;
        }

        // ② 按需分配缓冲再读一次（回填真实字节数到 size2）。
        let mut buf = vec![0u8; size as usize];
        let mut size2: u32 = size;
        let q2 = RegQueryValueExW(
            hkey,
            &value,
            None,
            None,
            Some(buf.as_mut_ptr()),
            Some(&mut size2),
        );
        let _ = RegCloseKey(hkey);
        if q2 != ERROR_SUCCESS {
            return None;
        }

        // ③ 按 UTF-16LE 解码（长度取回填的真实字节数），去掉结尾 NUL 与首尾空白。
        let byte_len = (size2 as usize).min(buf.len());
        let mut units: Vec<u16> = Vec::with_capacity(byte_len / 2);
        let mut i = 0;
        while i + 1 < byte_len {
            units.push(u16::from_le_bytes([buf[i], buf[i + 1]]));
            i += 2;
        }
        let s = String::from_utf16_lossy(&units);
        let s = s.trim_end_matches('\0').trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

/// 从一条自启命令行里提取目标 exe 路径。
///
/// 命令行形如 `"C:\path\app.exe"`（本程序写入的带引号格式）或 `C:\path\app.exe --flag`：
/// 优先取首个引号对内的内容，否则取首个空白前的内容。纯辅助，便于单测。
fn command_target(command: &str) -> &str {
    let t = command.trim();
    if let Some(rest) = t.strip_prefix('"') {
        rest.split('"').next().unwrap_or("")
    } else {
        t.split_whitespace().next().unwrap_or("")
    }
}

/// 纯函数：判断一条自启命令行是否指向给定 exe（Windows 路径**大小写不敏感**）。
pub fn command_points_to(command: &str, exe: &str) -> bool {
    let target = command_target(command);
    !target.is_empty() && target.eq_ignore_ascii_case(exe.trim())
}

/// ★ 若注册表里的自启项**指向本进程这个（不可独立运行的）开发产物**，返回其命令行原文；否则 `None`。
///
/// 用途（启动期自愈，见 `lifecycle::run`）：dev 构建不该被登记为自启，这条项注定失效，
/// 且旧逻辑会在每次启动时把它**反复写回**（用户手动改也会被覆盖）。故识别出来后**主动清理**。
///
/// 安全边界：当注册表里的项指向**别处**（如另一份 release 安装）时返回 `None` —— 我们不擅自
/// 删除不属于当前开发产物的自启项，尊重既有安装。可独立运行的构建下恒返回 `None`（本函数无意义）。
pub fn dirty_self_registration() -> Option<String> {
    if registration_supported() {
        return None;
    }
    let command = registered_command()?;
    let exe = std::env::current_exe().ok()?;
    if command_points_to(&command, &exe.to_string_lossy()) {
        Some(command)
    } else {
        None
    }
}

/// 设置开机自启开关。enable=true → 写入 Run 项；false → 删除。
pub fn set_autostart(enable: bool) -> bool {
    // ★★ 2026-09-24 真机缺陷修复：**开发构建不得把自己登记为开机自启**（见模块头）。
    //   enable=true → 直接拒绝（前端据 `false` 回滚开关）；
    //   enable=false → **照常执行删除**（跳过本守卫），否则用户无法清掉既有的脏项。
    if enable && !registration_supported() {
        crate::diag::warn(&format!(
            "autostart: 当前为{}构建，无法独立运行，不支持开机自启，已拒绝登记；\
             请改用 release 版（`pnpm release`，含 custom-protocol）后再开启。",
            if DEBUG_BUILD {
                " debug"
            } else {
                "未内嵌前端资源（缺 custom-protocol）的"
            }
        ));
        return false;
    }

    // 取当前 exe 完整路径（带引号包裹，规避路径含空格）
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            crate::diag::warn(&format!("autostart: 取当前 exe 路径失败: {e}"));
            return false;
        }
    };
    let quoted = format!("\"{}\"", exe.display());

    unsafe {
        let key = HSTRING::from(RUN_KEY);
        let value = HSTRING::from(VALUE_NAME);
        let mut hkey = HKEY(std::ptr::null_mut());
        // 需要 KEY_QUERY_VALUE 才能在删除失败时查询「值是否本就存在」（KEY_WRITE 单独不含查询权限）。
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &key,
            Some(0),
            KEY_WRITE | KEY_READ,
            &mut hkey,
        ) != ERROR_SUCCESS
        {
            crate::diag::warn("autostart: 打开 Run 注册表项失败");
            return false;
        }

        let ok = if enable {
            // REG_SZ 的值是 NUL 结尾的 UTF-16 序列；写入时按小端字节序列提供。
            let mut wide: Vec<u16> = quoted.encode_utf16().collect();
            wide.push(0); // REG_SZ 必须 NUL 结尾
            let bytes: &[u8] =
                std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            let r = RegSetValueExW(hkey, &value, None, REG_SZ, Some(bytes));
            if r != ERROR_SUCCESS {
                crate::diag::warn(&format!("autostart: 写入 Run 项失败（code {:#x}）", r.0));
            }
            r == ERROR_SUCCESS
        } else {
            // 删除不存在的值返回 ERROR_FILE_NOT_FOUND → 视为「已关闭」成功；
            // 其它错误（如权限不足 ERROR_ACCESS_DENIED）一律视为失败，避免误报成功。
            let ok = RegDeleteValueW(hkey, &value) == ERROR_SUCCESS
                || RegQueryValueExW(hkey, &value, None, None, None, None) == ERROR_FILE_NOT_FOUND;
            if !ok {
                crate::diag::warn("autostart: 删除 Run 项失败");
            }
            ok
        };

        let _ = RegCloseKey(hkey);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::{can_register_autostart, command_points_to};

    #[test]
    fn can_register_only_for_standalone_builds() {
        // 本真机缺陷的核心判据（纯函数）：只有「release + custom-protocol」才有资格登记自启。
        assert!(
            !can_register_autostart(true, true),
            "debug 构建不得登记自启（否则开机加载 devUrl 失败 → 错误页巨字 + 滚动条）"
        );
        assert!(!can_register_autostart(true, false));
        assert!(
            !can_register_autostart(false, false),
            "未开 custom-protocol → 前端资源未内嵌 → 不得登记自启"
        );
        assert!(
            can_register_autostart(false, true),
            "release + custom-protocol → 可独立运行，允许登记"
        );
    }

    #[test]
    fn command_points_to_handles_quotes_args_and_case() {
        let exe = r"C:\App\deepseek-float.exe";
        // 本程序写入的格式：带引号（可含结尾参数）。
        assert!(command_points_to(&format!("\"{exe}\""), exe));
        assert!(command_points_to(&format!("\"{exe}\" --autostart"), exe));
        // 不带引号 / 带参数。
        assert!(command_points_to(exe, exe));
        assert!(command_points_to(&format!("{exe} -x"), exe));
        // Windows 路径大小写不敏感。
        assert!(command_points_to(r#""c:\app\DEEPSEEK-FLOAT.EXE""#, exe));
        // 首尾空白容忍。
        assert!(command_points_to(&format!("  \"{exe}\"  "), exe));
        // 指向别处 → 视为「不是本产物的脏项」。
        assert!(!command_points_to(r#""C:\Other\app.exe""#, exe));
        assert!(!command_points_to("", exe));
        assert!(!command_points_to("   ", exe));
        assert!(!command_points_to(r#""""#, exe));
    }
}
