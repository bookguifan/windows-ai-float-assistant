//! One-shot transaction order shared by native and synthetic backends.
use crate::selection::{source_allowed, Stamp, Target};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionProof {
    pub control: isize,
    pub start: u32,
    pub end: u32,
    pub text: String,
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
    fn send_once(&mut self) -> bool;
    fn wait(&mut self, before: u32) -> Result<Stamp, &'static str>;
    /// Must verify the stamp before AND after reading, under the clipboard lock.
    fn read(&mut self, expected: Stamp) -> Result<String, &'static str>;
    /// Must compare + write under one clipboard lock; never restore a different stamp.
    fn restore(&mut self, snapshot: Self::Snapshot, expected: Stamp) -> Result<(), &'static str>;
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
    if !io.send_once() {
        return Err("COPY_INPUT_FAILED");
    }
    if !io.valid() {
        return Err("COPY_CONTEXT_CHANGED");
    }
    let source = io.wait(before)?;
    if !io.valid() || !source_allowed(target, before, source) {
        return Err("COPY_SOURCE_UNVERIFIED");
    }
    let text = io.read(source)?;
    if !io.valid() {
        return Err("COPY_CONTEXT_CHANGED");
    }
    // A matching owner stamp is not evidence that the bytes came from this selection.
    // Reject before restoration, so competing text is neither returned nor overwritten.
    if text != proof.text {
        return Err("COPY_SELECTION_MISMATCH");
    }
    if io.selection_proof()? != proof {
        return Err("COPY_SELECTION_CHANGED");
    }
    if !io.valid() {
        return Err("COPY_CONTEXT_CHANGED");
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
