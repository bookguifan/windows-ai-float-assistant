//! §5.1.1 全局热键（截图触发）—— Win32 `RegisterHotKey` + **独立泵线程**。
//!
//! ## 为什么必须独立泵线程
//!
//! `RegisterHotKey(hwnd = None, …)` 把 `WM_HOTKEY` 投递到**调用线程的消息队列**，因此该线程
//! 必须跑消息循环才能收到；而 `UnregisterHotKey` **必须由注册它的那个线程调用**。
//! 两条约束合起来 ⇒ 注册与注销都要收敛到同一个泵线程上（同 `mousehook.rs` 的范式）。
//!
//! ## 历史教训（本模块是「重新引入」）
//!
//! 上一版热键 `Ctrl+Alt+D` 因「触发时用户仍按着 Ctrl/Alt → 合成出 `Ctrl+Alt+Insert` →
//! 复制失败」被移除（见 `capture.rs` 的 `wait_modifiers_released`）。故：
//!   - 热键回调**只发信号**，真正的截图流程在 worker 线程（同 `mousehook` 的纪律）；
//!   - 截图链路里任何合成按键前仍会走 `capture::wait_modifiers_released`（既有实现）。
//!
//! ## 三个「不做就会坏」的细节
//!
//! 1. **必须带 `MOD_NOREPEAT`**：否则长按 `Ctrl+Alt+1` 会连发 `WM_HOTKEY`，反复弹遮罩；
//! 2. **事务性改注册**：先注销旧键 → 试注册新键 → **失败则把旧键注册回去**，
//!    保证「改键失败反而把原本能用的键也弄没了」不会发生；
//! 3. **失败降级不阻塞启动**：注册失败只落 `diag`，右键菜单 / 托盘入口照常可用。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::Duration;

use windows::Win32::Foundation::{GetLastError, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage, MSG, WM_HOTKEY, WM_QUIT,
};

/// 热键 id（进程内唯一）。
const HOTKEY_ID: i32 = 0xD5F1;
/// 自定义线程消息：请求（重新）注册。`wParam` = 修饰键位（0 = 注销），`lParam` = 主键 vk。
const WM_APP_HOTKEY_SET: u32 = 0x8000 + 0x51;
/// `RegisterHotKey` 在「该热键已被其它程序注册」时置的错误码（1409）。
const ERR_HOTKEY_ALREADY_REGISTERED: u32 = 1409;
/// 等泵线程就绪 / 应答的超时。
const REPLY_TIMEOUT: Duration = Duration::from_secs(3);

static PUMP_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static STARTED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN: AtomicBool = AtomicBool::new(false);
/// 「该截图了」信号（worker 线程消费）。
static SIGNAL: Mutex<Option<Sender<()>>> = Mutex::new(None);
/// 当前**已注册**的 `(修饰键位, 主键)`；`None` = 未注册。改注册失败时用它回滚。
static CURRENT: Mutex<Option<(u32, u32)>> = Mutex::new(None);
/// `apply()` 的应答通道（由泵线程回填结果）。
static APPLY_REPLY: Mutex<Option<Sender<Result<(), String>>>> = Mutex::new(None);

fn log(m: &str) {
    crate::diag::log(&format!("hotkey: {m}"));
}

fn warn(m: &str) {
    crate::diag::warn(&format!("hotkey: {m}"));
}

// ---------------------------------------------------------------------------
// 纯函数：spec 解析 / 回显（可单测，不碰 Win32）
// ---------------------------------------------------------------------------

/// 解析 spec：`Ctrl` / `Alt` / `Shift` / `Win`（**至少一个**）+ 字母 / 数字 / `F1`–`F12`，
/// 以 `+` 分隔、大小写与空白不敏感、修饰键顺序无关。返回 `(修饰键位, 虚拟键码)`。
///
/// ★ 拒绝「无修饰键的裸键」：裸键做全局热键会把该键在整个系统里吞掉，风险过高。
pub fn parse(spec: &str) -> Result<(u32, u32), String> {
    let mut mods = 0u32;
    let mut vk: Option<u32> = None;
    for raw in spec.split('+') {
        let p = raw.trim();
        if p.is_empty() {
            continue;
        }
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL.0,
            "alt" => mods |= MOD_ALT.0,
            "shift" => mods |= MOD_SHIFT.0,
            "win" | "super" | "meta" => mods |= MOD_WIN.0,
            _ => {
                if vk.is_some() {
                    return Err(format!("只能有一个主键，但出现了「{p}」"));
                }
                vk =
                    Some(parse_key(p).ok_or_else(|| {
                        format!("无法识别的键「{p}」（支持 A–Z / 0–9 / F1–F12）")
                    })?);
            }
        }
    }
    if mods == 0 {
        return Err("至少要有一个修饰键（Ctrl / Alt / Shift / Win）".to_string());
    }
    let vk = vk.ok_or_else(|| "缺少主键（A–Z / 0–9 / F1–F12）".to_string())?;
    Ok((mods, vk))
}

/// 主键名 → 虚拟键码（`A`–`Z` = 0x41..、`0`–`9` = 0x30..、`F1`–`F12` = 0x70..）。
fn parse_key(p: &str) -> Option<u32> {
    let b = p.as_bytes();
    if b.len() == 1 {
        let c = b[0].to_ascii_uppercase();
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u32);
        }
    }
    if let Some(n) = p.to_ascii_lowercase().strip_prefix('f') {
        if let Ok(n) = n.parse::<u32>() {
            if (1..=12).contains(&n) {
                return Some(0x70 + n - 1); // VK_F1 = 0x70
            }
        }
    }
    None
}

/// 虚拟键码 → 主键名。
fn key_name(vk: u32) -> String {
    if (0x41..=0x5A).contains(&vk) || (0x30..=0x39).contains(&vk) {
        return (vk as u8 as char).to_string();
    }
    if (0x70..=0x7B).contains(&vk) {
        return format!("F{}", vk - 0x70 + 1);
    }
    format!("VK{vk:#X}")
}

/// 修饰键位 + 主键 → 可读 spec（设置页回显 / 日志）。固定输出顺序 `Ctrl+Alt+Shift+Win+Key`。
pub fn format(mods: u32, vk: u32) -> String {
    let mut s = String::new();
    if mods & MOD_CONTROL.0 != 0 {
        s.push_str("Ctrl+");
    }
    if mods & MOD_ALT.0 != 0 {
        s.push_str("Alt+");
    }
    if mods & MOD_SHIFT.0 != 0 {
        s.push_str("Shift+");
    }
    if mods & MOD_WIN.0 != 0 {
        s.push_str("Win+");
    }
    s.push_str(&key_name(vk));
    s
}

// ---------------------------------------------------------------------------
// 泵线程：注册 / 注销 / 消息循环
// ---------------------------------------------------------------------------

/// 启动热键子系统：建信号通道 + worker + 泵线程，并按 `initial` 注册首个热键。
///
/// `initial` 为 `None` / 空串 ⇒ 不注册（用户把热键关掉了）。返回泵线程是否就绪
/// （**注册失败不影响就绪判定** —— 失败只落日志，托盘/右键菜单仍是有效入口）。
pub fn start<F>(on_hotkey: F, initial: Option<&str>) -> bool
where
    F: Fn() + Send + 'static,
{
    if STARTED.swap(true, Ordering::SeqCst) {
        return true;
    }
    log("hotkey::start 被调用");

    // worker：真正的截图流程是重活（遮罩/抓图/编码），绝不放泵线程（同 mousehook 的纪律）。
    let (tx, rx) = channel::<()>();
    if let Ok(mut g) = SIGNAL.lock() {
        *g = Some(tx);
    }
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            on_hotkey();
        }
    });

    // 初始 spec 在调用线程解析：非法 spec 只需记日志，**不阻塞启动**。
    let initial_parsed = match initial.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => match parse(s) {
            Ok(v) => Some(v),
            Err(e) => {
                warn(&format!(
                    "初始热键「{s}」不合法（{e}）—— 本次不注册，托盘/右键菜单仍可用"
                ));
                None
            }
        },
        None => None,
    };

    let (ready_tx, ready_rx) = channel::<bool>();
    std::thread::spawn(move || unsafe {
        PUMP_THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);
        if let Some((m, v)) = initial_parsed {
            // 失败已有日志（do_register 内部），此处不改变「泵就绪」的语义。
            let _ = do_register(m, v);
        }
        let _ = ready_tx.send(true);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            match msg.message {
                WM_HOTKEY => emit(),
                WM_APP_HOTKEY_SET => {
                    let r = do_register(msg.wParam.0 as u32, msg.lParam.0 as u32);
                    reply(r);
                }
                _ => {}
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // 退出：`UnregisterHotKey` 必须在注册它的线程上调用 ⇒ 就在这里收尾。
        let _ = UnregisterHotKey(None, HOTKEY_ID);
        log("热键泵线程已退出，热键已注销");
    });

    ready_rx.recv_timeout(REPLY_TIMEOUT).unwrap_or(false)
}

/// 应用新的热键 spec（`None` / 空串 = 关闭热键）。返回注册结果供设置页提示。
///
/// 解析在**调用线程**完成（非法 spec 立即失败，不必打扰泵线程）；真正的注册/注销由泵线程执行
/// —— 因为 `UnregisterHotKey` 必须由注册它的线程调用。
pub fn apply(spec: Option<&str>) -> Result<(), String> {
    let (mods, vk) = match spec.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => parse(s)?,
        None => (0, 0),
    };
    let id = PUMP_THREAD_ID.load(Ordering::SeqCst);
    if id == 0 {
        return Err("热键线程未启动".to_string());
    }
    let (tx, rx) = channel::<Result<(), String>>();
    if let Ok(mut g) = APPLY_REPLY.lock() {
        *g = Some(tx);
    }
    unsafe {
        if PostThreadMessageW(
            id,
            WM_APP_HOTKEY_SET,
            WPARAM(mods as usize),
            LPARAM(vk as isize),
        )
        .is_err()
        {
            return Err("无法向热键线程投递改注册请求".to_string());
        }
    }
    rx.recv_timeout(REPLY_TIMEOUT)
        .unwrap_or_else(|_| Err("热键线程无应答（超时）".to_string()))
}

/// 退出清理：给泵线程投递 `WM_QUIT`（与 `mousehook::stop()` 并列调用）。
pub fn shutdown() {
    let id = PUMP_THREAD_ID.load(Ordering::SeqCst);
    if id != 0 && !SHUTDOWN.swap(true, Ordering::SeqCst) {
        unsafe {
            let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

/// 当前生效的热键 spec（`None` = 未注册 / 已关闭）—— 设置页回显用。
pub fn current() -> Option<String> {
    CURRENT
        .lock()
        .ok()
        .and_then(|g| *g)
        .map(|(m, v)| format(m, v))
}

fn emit() {
    let tx = SIGNAL.lock().ok().and_then(|g| g.as_ref().cloned());
    if let Some(tx) = tx {
        let _ = tx.send(());
    }
}

fn reply(r: Result<(), String>) {
    if let Ok(mut g) = APPLY_REPLY.lock() {
        if let Some(tx) = g.take() {
            let _ = tx.send(r);
        }
    }
}

/// 在**泵线程**上执行注册 / 注销（`mods == 0` 或 `vk == 0` ⇒ 注销）。
///
/// 事务性：无论成功失败，最终「已注册的键」都处于**明确且可用**的状态 ——
/// 失败时回滚为旧键，绝不把用户的旧热键一起弄丢。
unsafe fn do_register(mods: u32, vk: u32) -> Result<(), String> {
    let old = CURRENT.lock().ok().and_then(|g| *g);
    if old.is_some() {
        let _ = UnregisterHotKey(None, HOTKEY_ID);
        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
    }
    if vk == 0 {
        log("已注销全局热键（关闭）");
        return Ok(());
    }
    match RegisterHotKey(None, HOTKEY_ID, HOT_KEY_MODIFIERS(mods) | MOD_NOREPEAT, vk) {
        Ok(()) => {
            if let Ok(mut g) = CURRENT.lock() {
                *g = Some((mods, vk));
            }
            log(&format!("已注册全局热键 {}", format(mods, vk)));
            Ok(())
        }
        Err(e) => {
            let code = GetLastError().0;
            // ★ 回滚：把旧键注册回去，避免「改键失败 → 连原本能用的键也没了」。
            if let Some((om, ov)) = old {
                if RegisterHotKey(None, HOTKEY_ID, HOT_KEY_MODIFIERS(om) | MOD_NOREPEAT, ov).is_ok()
                {
                    if let Ok(mut g) = CURRENT.lock() {
                        *g = Some((om, ov));
                    }
                    warn(&format!(
                        "注册新热键失败，已回滚为原热键 {}",
                        format(om, ov)
                    ));
                }
            }
            if code == ERR_HOTKEY_ALREADY_REGISTERED {
                Err("该热键已被其它程序占用，请换一个组合".to_string())
            } else {
                Err(format!("注册失败（错误码 {code}）: {e}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_default_spec() {
        assert_eq!(
            parse("Ctrl+Alt+1").unwrap(),
            (MOD_CONTROL.0 | MOD_ALT.0, 0x31)
        );
    }

    #[test]
    fn parse_is_case_space_insensitive_and_order_free() {
        assert_eq!(
            parse("ctrl + alt + f5").unwrap(),
            (MOD_CONTROL.0 | MOD_ALT.0, 0x74)
        );
        assert_eq!(
            parse("Alt+Ctrl+1").unwrap(),
            (MOD_CONTROL.0 | MOD_ALT.0, 0x31)
        );
        assert_eq!(
            parse("shift+win+a").unwrap(),
            (MOD_SHIFT.0 | MOD_WIN.0, 0x41)
        );
    }

    #[test]
    fn parse_rejects_bad_specs() {
        assert!(parse("").is_err(), "空串");
        assert!(parse("Ctrl+Alt").is_err(), "缺主键");
        assert!(parse("1").is_err(), "裸键（无修饰键）必须拒绝");
        assert!(parse("Ctrl+A+B").is_err(), "两个主键");
        assert!(parse("Ctrl+F13").is_err(), "功能键越界");
        assert!(parse("Ctrl+Enter").is_err(), "不支持的键名");
    }

    #[test]
    fn format_roundtrips_parse() {
        for spec in ["Ctrl+Alt+1", "Ctrl+Shift+F12", "Alt+Win+Z", "Ctrl+F1"] {
            let (m, v) = parse(spec).unwrap();
            assert_eq!(format(m, v), spec, "往返应一致: {spec}");
        }
    }
}
