//! One-shot transaction order shared by native and synthetic backends.
use crate::selection::{source_allowed, Stamp, Target};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionProof {
    pub control: isize,
    pub start: u32,
    pub end: u32,
    pub text: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendStatus {
    Rejected,
    Sent,
    Partial,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitFailure {
    pub code: &'static str,
    pub clipboard_changed: bool,
}
impl WaitFailure {
    pub const fn unchanged(code: &'static str) -> Self {
        Self {
            code,
            clipboard_changed: false,
        }
    }
    pub const fn changed(code: &'static str) -> Self {
        Self {
            code,
            clipboard_changed: true,
        }
    }
}
impl SelectionProof {
    fn usable(&self) -> bool {
        self.control != 0
            && self.start < self.end
            && self.end.saturating_sub(self.start) as usize == self.text.encode_utf16().count()
            && self.text.chars().count() <= 20_000
            && self.text.trim().chars().count() >= 2
    }
}
pub trait ClipboardIo {
    type Snapshot;
    fn valid(&self) -> bool;
    /// Independently read the exact selection from the still-bound native control.
    fn selection_proof(&mut self) -> Result<SelectionProof, &'static str>;
    fn prepare(&mut self) -> Result<(Self::Snapshot, u32), &'static str>;
    fn sequence(&self) -> u32;
    fn send_once(&mut self) -> SendStatus;
    fn wait(&mut self, before: u32) -> Result<Stamp, WaitFailure>;
    /// Reopens a changed clipboard after a wait failure and restores only when its
    /// exact owner and sequence delta still prove this selection's copy transaction.
    fn recover_after_wait_failure(
        &mut self,
        snapshot: Self::Snapshot,
        before: u32,
        target: Target,
        focused_control: isize,
    ) -> Result<(), &'static str>;
    /// Must verify the stamp before AND after reading, under the clipboard lock.
    fn read(&mut self, expected: Stamp) -> Result<String, &'static str>;
    /// Must compare + write under one clipboard lock; never restore a different stamp.
    fn restore(&mut self, snapshot: Self::Snapshot, expected: Stamp) -> Result<(), &'static str>;
}
fn restore_then_fail<I: ClipboardIo>(
    io: &mut I,
    snapshot: I::Snapshot,
    source: Stamp,
    reason: &'static str,
) -> Result<String, &'static str> {
    match io.restore(snapshot, source) {
        Ok(()) => Err(reason),
        Err(restore_error) => Err(restore_error),
    }
}
pub fn run_copy(io: &mut impl ClipboardIo, target: Target) -> Result<String, &'static str> {
    if !io.valid() {
        return Err("COPY_UNAUTHORIZED_OR_STALE");
    }
    let proof = io.selection_proof()?;
    if !proof.usable() {
        return Err("COPY_SELECTION_UNVERIFIED");
    }
    let (snapshot, before) = io.prepare()?;
    if io.selection_proof()? != proof {
        return Err("COPY_SELECTION_CHANGED");
    }
    if !io.valid() || before == 0 || io.sequence() != before {
        return Err("COPY_CONTEXT_CHANGED");
    }
    let sent = io.send_once();
    if sent == SendStatus::Rejected {
        return Err("COPY_INPUT_FAILED");
    }
    let source = match io.wait(before) {
        Ok(source) => source,
        Err(failure) if failure.clipboard_changed => {
            return match io.recover_after_wait_failure(snapshot, before, target, proof.control) {
                Ok(()) => Err(failure.code),
                Err(restore_error) => Err(restore_error),
            };
        }
        Err(failure) => return Err(failure.code),
    };
    if !io.valid() || !source_allowed(target, before, proof.control, source) {
        if !source_allowed(target, before, proof.control, source) {
            return Err("COPY_SOURCE_UNVERIFIED");
        }
        return restore_then_fail(io, snapshot, source, "COPY_CONTEXT_CHANGED");
    }
    if sent == SendStatus::Partial {
        return restore_then_fail(io, snapshot, source, "COPY_INPUT_FAILED");
    }
    let text = match io.read(source) {
        Ok(text) => text,
        Err(error) => return restore_then_fail(io, snapshot, source, error),
    };
    if !io.valid() {
        return restore_then_fail(io, snapshot, source, "COPY_CONTEXT_CHANGED");
    }
    // A matching owner stamp is not evidence that the bytes came from this selection.
    // Reject before restoration, so competing text is neither returned nor overwritten.
    if text != proof.text {
        return Err("COPY_SELECTION_MISMATCH");
    }
    match io.selection_proof() {
        Ok(current) if current == proof => {}
        Ok(_) => return restore_then_fail(io, snapshot, source, "COPY_SELECTION_CHANGED"),
        Err(error) => return restore_then_fail(io, snapshot, source, error),
    }
    if !io.valid() {
        return restore_then_fail(io, snapshot, source, "COPY_CONTEXT_CHANGED");
    }
    io.restore(snapshot, source)?;
    if !io.valid() {
        return Err("COPY_CONTEXT_CHANGED");
    }
    if text.chars().count() > 20_000 || text.trim().chars().count() < 2 {
        return Err("COPY_TEXT_INVALID");
    }
    Ok(text)
}
