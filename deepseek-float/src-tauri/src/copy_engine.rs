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
/// Multiple writes by a related application are allowed to settle. Once a
/// foreign writer is observed this transaction must stop, never wait it away.
#[derive(Default)]
pub struct CopyObservation {
    candidate: Option<Stamp>,
    since_ms: u64,
    settled: Option<Stamp>,
    rejected: bool,
}
impl CopyObservation {
    pub fn observe(
        &mut self,
        before: u32,
        stamp: Option<Stamp>,
        related: bool,
        now_ms: u64,
    ) -> Result<Option<Stamp>, &'static str> {
        if self.rejected
            || !stamp.is_some_and(|s| {
                crate::selection::compatibility_observation_allowed(before, s, related)
            })
        {
            self.rejected = true;
            self.settled = None;
            return Err("COPY_SOURCE_UNVERIFIED");
        }
        let stamp = stamp.expect("verified observation");
        if stamp.format_count == 0 {
            // Some applications clear/close before publishing their formats. Keep
            // waiting within the existing deadline, but never settle empty data.
            self.candidate = None;
            self.settled = None;
            return Ok(None);
        }
        if self.candidate != Some(stamp) {
            self.candidate = Some(stamp);
            self.since_ms = now_ms;
            self.settled = None;
        }
        if now_ms.saturating_sub(self.since_ms) >= 40 {
            self.settled = Some(stamp);
        }
        Ok(self.settled)
    }
    pub fn settled(&self) -> Option<Stamp> {
        self.settled
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
    fn compatibility(&self) -> bool {
        false
    }
    /// Native backend must establish a window/process relationship, never sequence alone.
    fn related_writer(&self, _stamp: Stamp) -> bool {
        false
    }
    fn valid(&self) -> bool;
    /// Independently read the exact selection from the still-bound native control.
    fn selection_proof(&mut self) -> Result<SelectionProof, &'static str>;
    fn prepare(&mut self) -> Result<(Self::Snapshot, u32), &'static str>;
    fn sequence(&self) -> u32;
    fn send_once(&mut self) -> SendStatus;
    fn wait(&mut self, before: u32) -> Result<Stamp, WaitFailure>;
    /// Reopens a changed clipboard after a wait failure and restores only when its
    /// observed transaction is attributable. Unknown/unstable compatibility writes
    /// are deliberately left untouched and reported as skipped restoration.
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
    let proof = if io.compatibility() {
        None
    } else {
        let proof = io.selection_proof()?;
        if !proof.usable() {
            return Err("COPY_SELECTION_UNVERIFIED");
        }
        Some(proof)
    };
    let (snapshot, before) = io.prepare()?;
    if let Some(proof) = &proof {
        if io.selection_proof()? != *proof {
            return Err("COPY_SELECTION_CHANGED");
        }
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
            return match io.recover_after_wait_failure(
                snapshot,
                before,
                target,
                proof.as_ref().map_or(0, |p| p.control),
            ) {
                Ok(()) => Err(failure.code),
                Err(restore_error) => Err(restore_error),
            };
        }
        Err(failure) => return Err(failure.code),
    };
    let accepted = match &proof {
        Some(proof) => source_allowed(target, before, proof.control, source),
        None => crate::selection::compatibility_source_allowed(
            before,
            source,
            io.related_writer(source),
        ),
    };
    if !accepted {
        return Err("COPY_SOURCE_UNVERIFIED");
    }
    if !io.valid() {
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
    if let Some(proof) = &proof {
        if text != proof.text {
            return Err("COPY_SELECTION_MISMATCH");
        }
        match io.selection_proof() {
            Ok(current) if current == *proof => {}
            Ok(_) => return restore_then_fail(io, snapshot, source, "COPY_SELECTION_CHANGED"),
            Err(error) => return restore_then_fail(io, snapshot, source, error),
        }
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
