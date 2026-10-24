//! 截图任务的纯状态与坐标换算；不依赖窗口或 Win32，便于确定性回归。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotMode {
    /// 只贴图，不自动发送。
    Ask,
    /// 贴图后发送转录提示词。
    Ocr,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CaptureGeometry {
    /// 遮罩客户区在虚拟桌面的物理原点和尺寸。
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SelectionRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DesktopRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl CaptureGeometry {
    pub fn desktop_rect(self, rect: SelectionRect) -> Option<DesktopRect> {
        if self.width <= 0
            || self.height <= 0
            || !self.scale.is_finite()
            || self.scale <= 0.0
            || [rect.x, rect.y, rect.width, rect.height]
                .iter()
                .any(|v| !v.is_finite())
            || rect.width <= 0.0
            || rect.height <= 0.0
        {
            return None;
        }
        let edges = [
            rect.x * self.scale,
            rect.y * self.scale,
            (rect.x + rect.width) * self.scale,
            (rect.y + rect.height) * self.scale,
        ];
        if edges.iter().any(|v| !v.is_finite()) {
            return None;
        }
        // 两端分别取整，再裁剪到本次遮罩，保留反向拖动/边缘拖出后的真实像素范围。
        let left = edges[0].round().clamp(0.0, self.width as f64) as i32;
        let top = edges[1].round().clamp(0.0, self.height as f64) as i32;
        let right = edges[2].round().clamp(0.0, self.width as f64) as i32;
        let bottom = edges[3].round().clamp(0.0, self.height as f64) as i32;
        let width = right - left;
        let height = bottom - top;
        if width < 8 || height < 8 {
            return None;
        }
        // 检查两端，避免虚拟桌面坐标相加溢出。
        self.x.checked_add(right)?;
        self.y.checked_add(bottom)?;
        Some(DesktopRect {
            x: self.x.checked_add(left)?,
            y: self.y.checked_add(top)?,
            width,
            height,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preparing,
    Selecting,
    CapturingPreview,
    Previewing,
    Delivering,
    Finishing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BallRestore {
    pub restore: bool,
    pub generation: u64,
}

#[derive(Debug)]
struct Session {
    id: u64,
    mode: ShotMode,
    geometry: Option<CaptureGeometry>,
    ball_restore: BallRestore,
    phase: Phase,
    revision: u64,
    png: Option<Vec<u8>>,
    workers: u32,
    viewed: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CommittedCapture {
    pub id: u64,
    pub mode: ShotMode,
    pub geometry: CaptureGeometry,
}

pub(crate) struct DraftCapture {
    pub capture: CommittedCapture,
    pub png: Vec<u8>,
}

pub(crate) struct CaptureState {
    next_id: u64,
    current: Option<Session>,
}

pub(crate) fn should_restore_ball(
    restore_ball: bool,
    generation_at_start: u64,
    current_generation: u64,
) -> bool {
    restore_ball && generation_at_start == current_generation
}

impl CaptureState {
    pub const fn new() -> Self {
        Self {
            next_id: 0,
            current: None,
        }
    }

    pub fn active(&self) -> bool {
        self.current.is_some()
    }

    pub fn current_id(&self) -> Option<u64> {
        self.current.as_ref().map(|s| s.id)
    }

    pub fn preparing(&self, id: u64) -> bool {
        self.current
            .as_ref()
            .is_some_and(|s| s.id == id && s.phase == Phase::Preparing)
    }

    pub fn begin(&mut self, mode: ShotMode) -> Option<u64> {
        if self.active() {
            return None;
        }
        // IPC 编号保留在 JS 安全整数范围内；耗尽时拒绝而非复用旧编号。
        if self.next_id >= 9_007_199_254_740_991 {
            return None;
        }
        self.next_id += 1;
        let id = self.next_id;
        self.current = Some(Session {
            id,
            mode,
            geometry: None,
            ball_restore: BallRestore {
                restore: false,
                generation: 0,
            },
            phase: Phase::Preparing,
            revision: 1,
            png: None,
            workers: 0,
            viewed: false,
        });
        Some(id)
    }

    pub fn ready(
        &mut self,
        id: u64,
        geometry: CaptureGeometry,
        restore_ball: bool,
        visibility_generation: u64,
    ) -> bool {
        let Some(s) = self
            .current
            .as_mut()
            .filter(|s| s.id == id && s.phase == Phase::Preparing)
        else {
            return false;
        };
        s.geometry = Some(geometry);
        s.ball_restore = BallRestore {
            restore: restore_ball,
            generation: visibility_generation,
        };
        s.phase = Phase::Selecting;
        true
    }

    pub fn claim_preview(&mut self, id: u64, revision: u64) -> Option<CommittedCapture> {
        let s = self
            .current
            .as_mut()
            .filter(|s| s.id == id && s.revision == revision && s.phase == Phase::Selecting)?;
        let geometry = s.geometry?;
        s.phase = Phase::CapturingPreview;
        s.workers += 1;
        Some(CommittedCapture {
            id,
            mode: s.mode,
            geometry,
        })
    }

    pub fn matches(&self, id: u64, revision: u64) -> bool {
        self.current
            .as_ref()
            .is_some_and(|s| s.id == id && s.revision == revision && s.phase != Phase::Finishing)
    }

    pub fn previewing(&self, id: u64, revision: u64) -> bool {
        self.current
            .as_ref()
            .is_some_and(|s| s.id == id && s.revision == revision && s.phase == Phase::Previewing)
    }

    pub fn store_preview(&mut self, id: u64, revision: u64, png: Vec<u8>) -> bool {
        if png.is_empty() || png.len() > 4 * 1024 * 1024 {
            return false;
        }
        let Some(s) = self
            .current
            .as_mut()
            .filter(|s| s.id == id && s.revision == revision && s.phase == Phase::CapturingPreview)
        else {
            return false;
        };
        s.png = Some(png);
        s.viewed = false;
        s.phase = Phase::Previewing;
        true
    }

    pub fn preview_failed(&mut self, id: u64, revision: u64) -> bool {
        let Some(s) = self.current.as_mut().filter(|s| {
            s.id == id
                && s.revision == revision
                && matches!(s.phase, Phase::CapturingPreview | Phase::Previewing)
        }) else {
            return false;
        };
        s.png = None;
        s.phase = Phase::Previewing;
        true
    }

    pub fn preview_failed_if_capturing(&mut self, id: u64, revision: u64) {
        if self.current.as_ref().is_some_and(|s| {
            s.id == id && s.revision == revision && s.phase == Phase::CapturingPreview
        }) {
            self.preview_failed(id, revision);
        }
    }

    pub fn delivery_failed(&mut self, id: u64, revision: u64) -> bool {
        let Some(s) = self
            .current
            .as_mut()
            .filter(|s| s.id == id && s.revision == revision && s.phase == Phase::Delivering)
        else {
            return false;
        };
        s.phase = Phase::Previewing;
        s.png = None;
        s.viewed = false;
        true
    }

    pub fn mark_viewed(&mut self, id: u64, revision: u64) -> bool {
        let Some(s) = self.current.as_mut().filter(|s| {
            s.id == id && s.revision == revision && s.phase == Phase::Previewing && s.png.is_some()
        }) else {
            return false;
        };
        s.viewed = true;
        true
    }

    pub fn reselect(&mut self, id: u64, revision: u64) -> Option<u64> {
        let s = self.current.as_mut().filter(|s| {
            s.id == id && s.revision == revision && s.phase == Phase::Previewing && s.workers == 0
        })?;
        s.revision = s.revision.checked_add(1)?;
        s.png = None;
        s.viewed = false;
        s.phase = Phase::Selecting;
        Some(s.revision)
    }

    pub fn confirm(&mut self, id: u64, revision: u64) -> Option<DraftCapture> {
        let s = self.current.as_mut().filter(|s| {
            s.id == id
                && s.revision == revision
                && s.phase == Phase::Previewing
                && s.workers == 0
                && s.viewed
        })?;
        let png = s.png.take()?;
        s.phase = Phase::Delivering;
        s.workers += 1;
        Some(DraftCapture {
            capture: CommittedCapture {
                id,
                mode: s.mode,
                geometry: s.geometry?,
            },
            png,
        })
    }

    pub fn worker_done(&mut self, id: u64) -> Option<BallRestore> {
        let s = self
            .current
            .as_mut()
            .filter(|s| s.id == id && s.workers > 0)?;
        s.workers -= 1;
        (s.phase == Phase::Finishing && s.workers == 0).then_some(s.ball_restore)
    }

    /// 立即失效并清空 draft；worker 尚在运行时不允许释放任务所有权。
    pub fn cancel(&mut self, id: u64) -> Option<BallRestore> {
        let s = self
            .current
            .as_mut()
            .filter(|s| s.id == id && s.phase != Phase::Finishing)?;
        s.phase = Phase::Finishing;
        s.png = None;
        (s.workers == 0).then_some(s.ball_restore)
    }

    /// worker 或初始化失败进入清理；清理期间继续阻止下一次截图。
    pub fn finish(&mut self, id: u64) -> Option<BallRestore> {
        let s = self
            .current
            .as_mut()
            .filter(|s| s.id == id && s.phase != Phase::Finishing)?;
        s.phase = Phase::Finishing;
        s.png = None;
        (s.workers == 0).then_some(s.ball_restore)
    }

    pub fn release(&mut self, id: u64) -> bool {
        if self
            .current
            .as_ref()
            .is_some_and(|s| s.id == id && s.phase == Phase::Finishing && s.workers == 0)
        {
            self.current = None;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_requires_explicit_confirmation_and_consumes_once() {
        let mut state = CaptureState::new();
        let id = ready(&mut state, ShotMode::Ocr);
        assert!(state.confirm(id, 1).is_none());
        state.claim_preview(id, 1).unwrap();
        assert!(state.store_preview(id, 1, vec![1, 2, 3]));
        state.worker_done(id);
        assert!(state.confirm(id, 1).is_none());
        assert!(state.mark_viewed(id, 1));
        let draft = state.confirm(id, 1).unwrap();
        assert_eq!(draft.png, vec![1, 2, 3]);
        assert_eq!(draft.capture.mode, ShotMode::Ocr);
        assert!(state.confirm(id, 1).is_none());
    }

    #[test]
    fn reselect_preserves_mode_and_rejects_old_worker_revision() {
        let mut state = CaptureState::new();
        let id = ready(&mut state, ShotMode::Ocr);
        state.claim_preview(id, 1).unwrap();
        assert!(state.store_preview(id, 1, vec![1]));
        state.worker_done(id);
        assert_eq!(state.reselect(id, 1), Some(2));
        assert!(!state.store_preview(id, 1, vec![9]));
        assert!(state.confirm(id, 1).is_none());
        let capture = state.claim_preview(id, 2).unwrap();
        assert_eq!(capture.mode, ShotMode::Ocr);
        assert_eq!(capture.geometry.desktop_rect(selection()).unwrap().x, 2020);
        assert_eq!(state.finish(id), None);
        assert!(!state.release(id));
        assert_eq!(
            state.worker_done(id),
            Some(BallRestore {
                restore: true,
                generation: 4
            })
        );
        assert!(state.release(id));
    }

    #[test]
    fn invalidated_worker_cannot_cache_or_release_early() {
        let mut state = CaptureState::new();
        let id = ready(&mut state, ShotMode::Ask);
        state.claim_preview(id, 1).unwrap();
        assert_eq!(state.cancel(id), None);
        assert!(!state.release(id));
        assert!(!state.store_preview(id, 1, vec![1]));
        assert_eq!(state.begin(ShotMode::Ask), None);
        assert_eq!(
            state.worker_done(id),
            Some(BallRestore {
                restore: true,
                generation: 4
            })
        );
        assert!(state.release(id));
    }

    #[test]
    fn screenshot_cleanup_does_not_override_a_newer_user_visibility_choice() {
        assert!(should_restore_ball(true, 4, 4));
        assert!(!should_restore_ball(true, 4, 5));
        assert!(!should_restore_ball(true, 4, 6));
        assert!(!should_restore_ball(false, 4, 4));
    }

    #[test]
    fn cancelled_capture_keeps_its_visibility_snapshot_for_delayed_cleanup() {
        let mut state = CaptureState::new();
        let id = ready(&mut state, ShotMode::Ask);
        let restore = state.cancel(id).expect("capture has no active worker");
        assert_eq!(restore.generation, 4);
        assert!(should_restore_ball(restore.restore, restore.generation, 4));
        assert!(!should_restore_ball(restore.restore, restore.generation, 5));
        assert!(state.release(id));
    }

    #[test]
    fn oversize_or_failed_preview_cannot_confirm_but_can_reselect() {
        let mut state = CaptureState::new();
        let id = ready(&mut state, ShotMode::Ask);
        state.claim_preview(id, 1).unwrap();
        assert!(!state.store_preview(id, 1, vec![0; 4 * 1024 * 1024 + 1]));
        state.preview_failed_if_capturing(id, 1);
        state.worker_done(id);
        assert!(!state.mark_viewed(id, 1));
        assert!(state.confirm(id, 1).is_none());
        assert_eq!(state.reselect(id, 1), Some(2));
    }

    #[test]
    fn failed_delivery_retains_task_for_reselect_without_reusing_png() {
        let mut state = CaptureState::new();
        let id = ready(&mut state, ShotMode::Ask);
        state.claim_preview(id, 1).unwrap();
        state.store_preview(id, 1, vec![1]);
        state.worker_done(id);
        state.mark_viewed(id, 1);
        state.confirm(id, 1).unwrap();
        assert!(state.delivery_failed(id, 1));
        assert!(state.reselect(id, 1).is_none());
        state.worker_done(id);
        assert!(state.confirm(id, 1).is_none());
        assert_eq!(state.reselect(id, 1), Some(2));
    }

    fn geometry(x: i32, y: i32, scale: f64) -> CaptureGeometry {
        CaptureGeometry {
            x,
            y,
            width: 1920,
            height: 1080,
            scale,
        }
    }

    fn selection() -> SelectionRect {
        SelectionRect {
            x: 80.0,
            y: 40.0,
            width: 160.0,
            height: 80.0,
        }
    }

    #[test]
    fn secondary_monitor_origin_is_applied() {
        assert_eq!(
            geometry(1920, 0, 1.25).desktop_rect(selection()),
            Some(DesktopRect {
                x: 2020,
                y: 50,
                width: 200,
                height: 100
            })
        );
    }

    #[test]
    fn negative_monitor_origin_and_fractional_scale_are_applied() {
        assert_eq!(
            geometry(-1920, -1080, 1.5).desktop_rect(selection()),
            Some(DesktopRect {
                x: -1800,
                y: -1020,
                width: 240,
                height: 120
            })
        );
    }

    #[test]
    fn selection_is_clipped_to_the_capture_surface() {
        let rect = SelectionRect {
            x: -10.0,
            y: 1000.0,
            width: 2000.0,
            height: 100.0,
        };
        assert_eq!(
            geometry(1920, 0, 1.0).desktop_rect(rect),
            Some(DesktopRect {
                x: 1920,
                y: 1000,
                width: 1920,
                height: 80
            })
        );
    }

    #[test]
    fn rounding_both_edges_preserves_the_selected_pixel_extent() {
        let rect = SelectionRect {
            x: 0.4,
            y: 0.4,
            width: 8.0,
            height: 8.0,
        };
        assert_eq!(
            geometry(0, 0, 1.25).desktop_rect(rect),
            Some(DesktopRect {
                x: 1,
                y: 1,
                width: 10,
                height: 10
            })
        );
    }

    #[test]
    fn invalid_or_tiny_selections_do_not_capture_pixels() {
        for rect in [
            SelectionRect {
                width: 0.0,
                ..selection()
            },
            SelectionRect {
                width: -1.0,
                ..selection()
            },
            SelectionRect {
                width: 2.0,
                ..selection()
            },
            SelectionRect {
                x: f64::NAN,
                ..selection()
            },
            SelectionRect {
                height: f64::INFINITY,
                ..selection()
            },
            SelectionRect {
                x: 3000.0,
                ..selection()
            },
        ] {
            assert_eq!(geometry(0, 0, 1.0).desktop_rect(rect), None);
        }
        assert_eq!(geometry(0, 0, 0.0).desktop_rect(selection()), None);
        assert_eq!(geometry(i32::MAX, 0, 1.0).desktop_rect(selection()), None);
    }

    fn ready(state: &mut CaptureState, mode: ShotMode) -> u64 {
        let id = state.begin(mode).expect("start capture");
        assert!(state.ready(id, geometry(1920, 0, 1.25), true, 4));
        id
    }

    #[test]
    fn rejected_reentry_preserves_mode_and_geometry() {
        let mut state = CaptureState::new();
        let id = ready(&mut state, ShotMode::Ocr);
        assert_eq!(state.begin(ShotMode::Ask), None);
        let capture = state.claim_preview(id, 1).expect("first commit");
        assert_eq!(capture.mode, ShotMode::Ocr);
        assert_eq!(capture.geometry.desktop_rect(selection()).unwrap().x, 2020);
    }

    #[test]
    fn duplicate_commit_and_cancel_after_commit_are_ignored() {
        let mut state = CaptureState::new();
        let id = ready(&mut state, ShotMode::Ask);
        assert!(state.claim_preview(id, 1).is_some());
        assert!(state.claim_preview(id, 1).is_none());
        assert_eq!(state.cancel(id), None);
        assert!(state.active());
        assert_eq!(state.begin(ShotMode::Ocr), None);
    }

    #[test]
    fn finishing_keeps_capture_busy_until_window_cleanup_completes() {
        let mut state = CaptureState::new();
        let id = ready(&mut state, ShotMode::Ask);
        assert_eq!(
            state.cancel(id),
            Some(BallRestore {
                restore: true,
                generation: 4
            })
        );
        assert_eq!(state.cancel(id), None);
        assert!(state.active());
        assert_eq!(state.begin(ShotMode::Ocr), None);
        assert!(state.release(id));
        assert!(!state.active());
    }

    #[test]
    fn delayed_messages_cannot_commit_cancel_or_finish_a_new_capture() {
        let mut state = CaptureState::new();
        let old = ready(&mut state, ShotMode::Ask);
        assert_eq!(
            state.cancel(old),
            Some(BallRestore {
                restore: true,
                generation: 4
            })
        );
        assert!(state.release(old));
        let new = ready(&mut state, ShotMode::Ocr);
        assert_ne!(old, new);
        assert!(state.claim_preview(old, 1).is_none());
        assert_eq!(state.cancel(old), None);
        assert_eq!(state.finish(old), None);
        assert!(!state.release(old));
        assert_eq!(state.claim_preview(new, 1).unwrap().mode, ShotMode::Ocr);
    }

    #[test]
    fn preparing_cannot_commit_and_failure_can_release_it() {
        let mut state = CaptureState::new();
        let id = state.begin(ShotMode::Ask).unwrap();
        assert!(state.preparing(id));
        assert!(state.claim_preview(id, 1).is_none());
        assert_eq!(
            state.finish(id),
            Some(BallRestore {
                restore: false,
                generation: 0
            })
        );
        assert!(!state.preparing(id));
        assert!(state.release(id));
        assert!(state.begin(ShotMode::Ocr).is_some());
    }

    #[test]
    fn worker_completion_restores_only_the_ball_hidden_by_its_capture() {
        let mut state = CaptureState::new();
        let id = state.begin(ShotMode::Ask).unwrap();
        assert!(state.ready(id, geometry(0, 0, 1.0), false, 4));
        assert!(state.claim_preview(id, 1).is_some());
        assert_eq!(state.finish(id), None);
        assert_eq!(
            state.worker_done(id),
            Some(BallRestore {
                restore: false,
                generation: 4
            })
        );
        assert!(state.release(id));
    }
}
