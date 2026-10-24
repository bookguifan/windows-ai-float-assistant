//! §11 单实例互斥：Win32 命名互斥体（Mutex）+ 命名事件（Event），零新增依赖。
//!
//! 语义：首个实例创建命名互斥体与命名事件并常驻；后续实例 `CreateMutexW` 得到
//! `ERROR_ALREADY_EXISTS`，判定为 `Secondary` → 向命名事件 `SetEvent` 唤醒首实例
//! （把悬浮球显示出来）后自行退出。
//!
//! 为什么用 `Local\` 前缀：`Local\` 属于**当前登录会话**的命名空间，对「每用户桌面应用」
//! 是正确的语义；`Global\` 跨会话且在某些权限下不可用，不必用。

use windows::core::HSTRING;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE,
};

const MUTEX_NAME: &str = "Local\\com.deepseekfloat.desktop.singleton.mutex";
const EVENT_NAME: &str = "Local\\com.deepseekfloat.desktop.singleton.event";

/// 单实例判定结果。
pub enum Acquire {
    /// 首实例：拿到命名互斥体的所有权，继续正常启动。
    Primary,
    /// 后续实例：已有实例在运行，应唤醒它后退出。
    Secondary,
}

/// 句柄存放：存**裸地址**（`isize`）而非 `HANDLE` 本体。
///
/// `HANDLE` 在 windows 0.61 里是 `pub struct HANDLE(pub *mut core::ffi::c_void)`，
/// 是 `!Send + !Sync`，无法放进 `OnceLock<HANDLE>`（编译不过），故存裸地址。
///
/// 为什么用 `OnceLock` 而非 `LazyLock`：项目 `rust-version = "1.77.2"`，
/// `LazyLock` 是 1.80+，会被 clippy 的 `incompatible_msrv` 拦下。
///
/// 为何不 `CloseHandle`：windows-rs 的 `HANDLE` **没有实现 `Drop`**，不会自动关闭；
/// 且句柄与其内核对象在进程存活期间一直有效 —— 这正是「单实例」语义本身：
/// 只要首实例还活着，命名对象就存在，后续实例的 `CreateMutexW` 必返回 `ERROR_ALREADY_EXISTS`。
static MUTEX_HANDLE: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
static EVENT_HANDLE: std::sync::OnceLock<isize> = std::sync::OnceLock::new();

/// 获取单实例：返回 `Primary`（首实例）/ `Secondary`（后续实例）/ `None`（降级，不阻塞启动）。
pub fn acquire() -> Option<Acquire> {
    unsafe {
        let mutex_name = HSTRING::from(MUTEX_NAME);
        let h = match CreateMutexW(None, false, &mutex_name) {
            Ok(h) => h,
            Err(e) => {
                crate::diag::warn(&format!(
                    "single-instance: CreateMutexW 失败，跳过单实例保护（不影响启动）: {e}"
                ));
                return None; // 降级：不阻塞启动
            }
        };
        // ★ 成功路径不会触发 Error::from_win32()（windows-rs 的 ok_or_else 是惰性的），
        //   故此处 GetLastError() 仍是 CreateMutexW 留下的错误码：命名对象已存在 → ALREADY_EXISTS。
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        let _ = MUTEX_HANDLE.set(h.0 as isize); // 故意不 CloseHandle：句柄存在即判据
        if already {
            return Some(Acquire::Secondary);
        }
        // 首实例：创建「自动复位」事件，供唤醒线程阻塞等待。
        match CreateEventW(None, false, false, &HSTRING::from(EVENT_NAME)) {
            Ok(ev) => {
                let _ = EVENT_HANDLE.set(ev.0 as isize);
            }
            Err(e) => crate::diag::warn(&format!(
                "single-instance: CreateEventW 失败，第二实例将无法唤醒本实例: {e}"
            )),
        }
        Some(Acquire::Primary)
    }
}

/// 第二实例调用：打开首实例已创建的命名事件并触发一次，唤醒首实例。
///
/// 不用 `OpenEventW`（那会牵扯 `SYNCHRONIZATION_ACCESS_RIGHTS` / `EVENT_MODIFY_STATE`
/// 常量的引入）；直接 `CreateEventW` 打开同名对象即返回同一内核对象。
pub fn wake_primary() -> bool {
    unsafe {
        match CreateEventW(None, false, false, &HSTRING::from(EVENT_NAME)) {
            Ok(ev) => match SetEvent(ev) {
                Ok(()) => true,
                Err(e) => {
                    crate::diag::warn(&format!("single-instance: SetEvent 失败: {e}"));
                    false
                }
            },
            Err(e) => {
                crate::diag::warn(&format!("single-instance: 打开命名事件失败: {e}"));
                false
            }
        }
    }
}

/// 首实例在 setup 内调用：起等待线程，被第二实例唤醒时显示悬浮球。
pub fn watch(app: tauri::AppHandle) {
    let Some(&raw) = EVENT_HANDLE.get() else {
        return;
    };
    std::thread::spawn(move || {
        let h = windows::Win32::Foundation::HANDLE(raw as *mut core::ffi::c_void);
        loop {
            // 自动复位事件 → 每次 SetEvent 只唤醒一轮等待，无需手动 ResetEvent。
            // 线程阻塞在 INFINITE 等待上，空闲 CPU 为 0。
            let r = unsafe { WaitForSingleObject(h, INFINITE) };
            if r.0 != WAIT_OBJECT_0.0 {
                // ★ 必须 break，不能 continue：句柄失效时 WaitForSingleObject 会**立即**返回
                //   WAIT_FAILED(0xFFFFFFFF)，若在此 continue 就变成 100% 占满一核的忙等
                //   （恰好与「空闲 CPU 接近 0%」的验收项相反）。线程退出后不再接收唤醒信号，
                //   属可接受的降级（第二实例被拉起时球不会被顶到前台）。
                crate::diag::warn(&format!(
                    "single-instance: 等待命名事件失败（code {:#x}），唤醒线程退出",
                    r.0
                ));
                break;
            }
            // 第二实例被拉起：仅把悬浮球显示出来。刻意**不打开 panel**（避免启动瞬间闪窗）。
            // ⚠️ 也刻意**不调用 set_focus()**：本项目已验证 show() 后 set_focus 会触发
            //    Win SetForegroundWindow 冲突致 webview abort（宿主 exit -1）。
            // ★ 统一走 `reveal_ball`（逻辑显隐 + 三层冲刷，含无标题栏确认），替换此前裸 `show()`。
            crate::panels::reveal_ball(&app);
            crate::diag::log("single-instance: 收到第二实例唤醒信号，已显示悬浮球");
        }
    });
}
