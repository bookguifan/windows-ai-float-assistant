//! 注入、回读和会话切换共享的任务代次。
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Token(pub u64);

pub struct Epoch(AtomicU64);
impl Epoch {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }
    pub fn current(&self) -> Token {
        Token(self.0.load(Ordering::SeqCst))
    }
    pub fn advance(&self) -> Token {
        Token(self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }
    pub fn accepts(&self, token: Token) -> bool {
        self.current() == token
    }
}
static EPOCH: Epoch = Epoch::new();
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendStatus {
    Sending,
    Reading,
    Uncertain(String),
    Failed(String),
    Complete,
    TimedOut,
    Cancelled,
}
static SEND: Mutex<Option<(Token, SendStatus)>> = Mutex::new(None);
pub fn current() -> Token {
    EPOCH.current()
}
pub fn active(token: Token) -> bool {
    EPOCH.accepts(token)
}

/// 已执行的远端发送无法撤回；此处取消本地尚未执行的回调、重试和回读。
pub fn cancel_all(app: &AppHandle) {
    cancel(app, true);
}

/// 清空会话不渲染旧轮的“已取消”卡，避免它在空态之后迟到重绘。
pub fn cancel_for_new_session(app: &AppHandle) {
    cancel(app, false);
}

fn cancel(app: &AppHandle, notify: bool) {
    let token = EPOCH.advance();
    *SEND.lock().unwrap_or_else(|p| p.into_inner()) = None;
    crate::readback::cancel();
    crate::screenshot::invalidate(app);
    if notify {
        crate::result::cancel_pending(app, token);
    }
    for label in ["panel", "result"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.eval(initialize_script(token));
        }
    }
}

pub fn begin(app: &AppHandle) -> Token {
    cancel_all(app);
    let token = current();
    *SEND.lock().unwrap_or_else(|p| p.into_inner()) = Some((token, SendStatus::Sending));
    token
}

pub fn status(token: Token) -> SendStatus {
    SEND.lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .filter(|(id, _)| *id == token && active(token))
        .map(|(_, state)| state.clone())
        .unwrap_or(SendStatus::Cancelled)
}
pub fn claim_retry(token: Token) -> bool {
    let mut guard = SEND.lock().unwrap_or_else(|p| p.into_inner());
    if !active(token) {
        return false;
    }
    if let Some((id, state)) = guard.as_mut() {
        if *id == token && matches!(state, SendStatus::Failed(_)) {
            *state = SendStatus::Sending;
            return true;
        }
    }
    false
}
pub fn initialize_script(token: Token) -> String {
    format!(
        "window.__dsfloatRequest=Math.max(window.__dsfloatRequest||0,{});",
        token.0
    )
}
pub fn guarded(token: Token, js: &str) -> String {
    format!("(function(){{if(window.__dsfloatRequest!=={}||window.__dsfloatNavigationPending)return 'no-api';return ({});}})()",token.0,js)
}
pub fn update(app: &AppHandle, token: Token, state: SendStatus) {
    {
        let mut guard = SEND.lock().unwrap_or_else(|p| p.into_inner());
        if !active(token) || guard.as_ref().map(|(id, _)| *id) != Some(token) {
            return;
        }
        if let Some((_, previous)) = guard.as_mut() {
            if !transition(previous, state.clone()) {
                return;
            }
        }
    }
    crate::result::send_feedback(app, token, state);
}
pub fn failed(panel: &tauri::WebviewWindow, token: Token, reason: &str) {
    update(panel.app_handle(), token, SendStatus::Failed(reason.into()));
}

fn transition(current: &mut SendStatus, next: SendStatus) -> bool {
    use SendStatus::*;
    let allowed = matches!(
        (&*current, &next),
        (Sending, Reading | Uncertain(_) | Failed(_) | Cancelled)
            | (Uncertain(_), Reading | TimedOut | Cancelled)
            | (Reading, Complete | TimedOut | Failed(_) | Cancelled)
            | (TimedOut, Reading | Cancelled)
    );
    if allowed {
        *current = next;
    }
    allowed
}

pub fn claim_resume(token: Token) -> bool {
    let mut guard = SEND.lock().unwrap_or_else(|p| p.into_inner());
    if !active(token) {
        return false;
    }
    if let Some((id, state)) = guard.as_mut() {
        if *id == token && *state == SendStatus::TimedOut {
            return transition(state, SendStatus::Reading);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advancing_cancels_all_previous_callbacks() {
        let epoch = Epoch::new();
        let first = epoch.advance();
        assert!(epoch.accepts(first));
        let second = epoch.advance();
        assert!(!epoch.accepts(first));
        assert!(epoch.accepts(second));
    }
    #[test]
    fn repeated_cancellation_does_not_revive_an_old_task() {
        let epoch = Epoch::new();
        let original = epoch.current();
        epoch.advance();
        epoch.advance();
        assert!(!epoch.accepts(original));
    }
}

#[cfg(test)]
mod transition_tests {
    use super::*;
    #[test]
    fn uncertain_send_cannot_be_retried_or_marked_complete() {
        let mut state = SendStatus::Sending;
        assert!(transition(
            &mut state,
            SendStatus::Uncertain("等待核对".into())
        ));
        assert!(!transition(&mut state, SendStatus::Sending));
        assert!(!transition(&mut state, SendStatus::Complete));
        assert!(transition(&mut state, SendStatus::Reading));
    }
    #[test]
    fn failed_request_cannot_be_revived_by_late_confirmation() {
        let mut state = SendStatus::Failed("未确认".into());
        assert!(!transition(&mut state, SendStatus::Reading));
        assert!(matches!(state, SendStatus::Failed(_)));
    }
    #[test]
    fn sending_requires_confirmation_and_timeout_only_resumes_reading() {
        let mut state = SendStatus::Sending;
        assert!(!transition(&mut state, SendStatus::Complete));
        assert!(transition(&mut state, SendStatus::Reading));
        assert!(transition(&mut state, SendStatus::TimedOut));
        assert!(!transition(&mut state, SendStatus::Sending));
        assert!(transition(&mut state, SendStatus::Reading));
        assert!(transition(&mut state, SendStatus::Complete));
        assert!(!transition(&mut state, SendStatus::Reading));
    }
}
