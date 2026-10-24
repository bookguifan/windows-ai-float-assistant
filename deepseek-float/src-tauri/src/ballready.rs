//! 悬浮球「首帧就绪」握手闸门 —— 启动期显形时序的一次性同步原语。
//!
//! ## 背景（真机缺陷：「启动瞬间球空白」）
//! 球窗在 `tauri.conf.json` 里以 `visible:false` 创建。启动期此前**直接**显形
//! （见 `windows::gate_ball_show` 的历史注释）：在 WebView2 尚未绘制首帧时就把窗口显示出来，
//! 用户先看到一个「还没画好」的窗口（白块 / 无 logo 的空白圆），待首帧就绪后才补齐。
//!
//! ## 修法
//! 引入一个**一次性就绪闸门**：启动期 `windows::gate_ball_show` 在后台线程
//! [`wait_ball_ready`] 上等待；`poc/ball.html` 在**首个渲染帧之后**经 `ipc::ball_ready`
//! 调用 [`signal_ball_ready`] 置位。等待方一旦被唤醒（或到达兜底超时）才真正显形。
//!
//! ## 设计要点
//! - **一次就绪**：闸门只从「未就绪」单向翻转为「已就绪」，`signal` 幂等；
//! - **抗伪唤醒**：`wait` 采用「循环 + 截止时刻」而非单次 `wait_timeout`，正确处理
//!   `Condvar` 的 spurious wakeup；
//! - **绝不 panic**：锁中毒（`PoisonError`）时取回内部值继续 —— 本模块处在**启动关键路径**上，
//!   任何 panic 都会让球永久不显形，比「空白」严重得多；
//! - **可测**：[`BallReadyGate`] 与全局 [`BALL_READY`] 解耦，单测直接构造局部实例，互不干扰。

use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 等待 `ball_ready` 的**兜底超时**：超过此时长仍未收到握手信号，则强制显形。
///
/// 必要性：前端握手可能因页面加载失败 / IPC 桥不可用（`__TAURI_INTERNALS__` 改名等）
/// 而**永不到达**；若无超时，球将永久不可见 —— 故 ~3s 后无论是否收到信号都必须显形。
pub(crate) const BALL_READY_TIMEOUT: Duration = Duration::from_millis(3000);

/// 一次性「就绪」闸门：`signal` 置位并唤醒等待者；`wait` 阻塞至置位或超时。
///
/// 语义上等价于一个可带超时等待的 `Once`；此处自实现以支持 `wait(timeout) -> bool`。
pub(crate) struct BallReadyGate {
    /// 就绪标志（`false` = 未就绪，`true` = 已就绪）。与 `cv` 配对使用。
    ready: Mutex<bool>,
    /// 状态变更通知：`signal` 置位后 `notify_all`，唤醒所有 `wait` 中的线程。
    cv: Condvar,
}

impl BallReadyGate {
    /// 构造一个**未就绪**的闸门。
    pub(crate) fn new() -> Self {
        Self {
            ready: Mutex::new(false),
            cv: Condvar::new(),
        }
    }

    /// 置位就绪标志并唤醒所有等待者。**幂等**：重复调用仅第一次生效，但每次都会 `notify_all`
    /// （廉价且确保任何新到的等待者立刻返回）。
    pub(crate) fn signal(&self) {
        match self.ready.lock() {
            Ok(mut g) => *g = true,
            // 锁中毒：取回内部值继续，绝不 panic（启动关键路径）。
            Err(poisoned) => *poisoned.into_inner() = true,
        }
        self.cv.notify_all();
    }

    /// 阻塞至**已就绪**或 `timeout` 到期。
    ///
    /// 返回 `true` = 已就绪（收到过 `signal`），`false` = 超时且仍未就绪。
    ///
    /// 抗伪唤醒：`Condvar` 允许无 `signal` 的伪唤醒，故用循环 + 截止时刻重算剩余等待时长，
    /// 直到「已就绪」或「真正超时」为止。
    pub(crate) fn wait(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut guard = match self.ready.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        // 已就绪（含 signal 先于 wait 到达的情形）→ 立即返回，不等待。
        while !*guard {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let remaining = deadline - now;
            guard = match self.cv.wait_timeout(guard, remaining) {
                Ok((g, _)) => g,
                // 中毒：取回 guard 继续判定（下一轮循环会重新检查 *guard）。
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
        true
    }
}

/// 全局就绪闸门实例（惰性初始化，避免依赖 `Condvar::new` 的 const 稳定性）。
fn gate() -> &'static BallReadyGate {
    static BALL_READY: OnceLock<BallReadyGate> = OnceLock::new();
    BALL_READY.get_or_init(BallReadyGate::new)
}

/// 置位全局就绪闸门（由 `ipc::ball_ready` 在收到前端首帧握手时调用）。幂等，可安全重复调用。
pub(crate) fn signal_ball_ready() {
    gate().signal();
}

/// 等待全局就绪闸门：阻塞至收到握手信号或 `timeout` 到期。
///
/// 返回 `true` = 收到 `ball_ready`，`false` = 超时（调用方须**照常显形**，不能因超时把球藏起来）。
pub(crate) fn wait_ball_ready(timeout: Duration) -> bool {
    gate().wait(timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    // ---- 4+ 用例：闸门的核心性质（超时 / 先置位 / 跨线程唤醒 / 幂等 / 超时后再置位）----

    #[test]
    fn wait_returns_false_on_timeout_without_signal() {
        // 未置位 ⇒ 必须在约 timeout 后返回 false（不能永久阻塞）。
        let gate = BallReadyGate::new();
        let t0 = Instant::now();
        assert!(
            !gate.wait(Duration::from_millis(60)),
            "未置位时 wait 应超时返回 false"
        );
        let elapsed = t0.elapsed();
        assert!(
            elapsed >= Duration::from_millis(40),
            "应大致等满超时时长，实测 {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "不应显著超过超时时长，实测 {elapsed:?}"
        );
    }

    #[test]
    fn wait_returns_true_immediately_when_signaled_before() {
        // signal 先于 wait 到达 ⇒ wait 应立即返回 true（不等待）。
        let gate = BallReadyGate::new();
        gate.signal();
        let t0 = Instant::now();
        assert!(gate.wait(Duration::from_secs(5)), "已置位应返回 true");
        assert!(
            t0.elapsed() < Duration::from_secs(1),
            "置位后 wait 应立即返回，实测 {:?}",
            t0.elapsed()
        );
    }

    #[test]
    fn wait_wakes_on_signal_from_another_thread() {
        // 另一线程稍后置位 ⇒ 等待者应被唤醒并返回 true（早于超时）。
        let gate = Arc::new(BallReadyGate::new());
        let g2 = gate.clone();
        let t0 = Instant::now();
        let h = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            g2.signal();
        });
        assert!(gate.wait(Duration::from_secs(5)), "置位后应被唤醒");
        assert!(
            t0.elapsed() < Duration::from_secs(2),
            "应在合理时间内被唤醒，实测 {:?}",
            t0.elapsed()
        );
        h.join().expect("signal 线程不应 panic");
    }

    #[test]
    fn signal_is_idempotent() {
        // 重复置位不改变语义：仍为「已就绪」，wait 立即返回 true。
        let gate = BallReadyGate::new();
        gate.signal();
        gate.signal();
        assert!(gate.wait(Duration::from_millis(10)), "重复置位后仍应就绪");
    }

    #[test]
    fn signal_after_timeout_takes_effect_for_later_waits() {
        // 冷超时之后，闸门仍可被置位并让**后续** wait 立即返回 true（单向翻转、不回落）。
        let gate = BallReadyGate::new();
        assert!(!gate.wait(Duration::from_millis(30)), "首次应超时");
        gate.signal();
        assert!(
            gate.wait(Duration::from_millis(10)),
            "超时后置位，后续 wait 应就绪"
        );
    }

    #[test]
    fn default_timeout_constant_is_about_three_seconds() {
        // 兜底超时时长的契约：~3s（既给足首帧渲染时间，又不让球久等）。
        assert_eq!(BALL_READY_TIMEOUT, Duration::from_millis(3000));
    }
}
