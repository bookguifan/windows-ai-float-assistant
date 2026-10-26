//! Bounded, read-only UIA selection on the dedicated MTA worker.
use crate::selection::{join_ranges, Request, MAX_RANGES};
use windows::core::Interface;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GetWindowThreadProcessId, GA_ROOT};

pub use crate::uia_strategy::ReadResult;
use crate::uia_strategy::{self, Attempt, Provider};
use std::cell::RefCell;
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
const MAX_TEXT: usize = 20_000;
fn live(r: &Request) -> bool {
    crate::selection_runtime::valid(r)
        && crate::selection_runtime::now_ms() < r.created_ms.saturating_add(350)
}

/// All interfaces created below are dropped before the apartment guard.
pub fn read(r: &Request) -> ReadResult {
    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return ReadResult::fail("UIA_INTERNAL_ERROR", false);
        }
        let _apartment = Apartment;
        read_inner(r).unwrap_or_else(|s| ReadResult::fail(s, false))
    }
}
unsafe fn checked(
    a: &IUIAutomation,
    walker: &IUIAutomationTreeWalker,
    e: &IUIAutomationElement,
    r: &Request,
    metrics: &RefCell<UiaMetrics>,
) -> Result<(), &'static str> {
    uia_strategy::verify_ownership(
        &NativeOwnership {
            automation: a,
            walker: walker.clone(),
            request: r,
            metrics,
        },
        e,
    )
}
struct NativeOwnership<'a> {
    automation: &'a IUIAutomation,
    walker: IUIAutomationTreeWalker,
    request: &'a Request,
    metrics: &'a RefCell<UiaMetrics>,
}
impl uia_strategy::OwnershipProvider for NativeOwnership<'_> {
    type Element = IUIAutomationElement;
    fn rejected(&self, reason: &'static str, depth: usize) {
        let mut m = self.metrics.borrow_mut();
        m.ownership_reason = reason;
        m.ownership_depth = depth;
    }
    fn live(&self) -> Result<(), &'static str> {
        if !crate::selection_runtime::valid(self.request) {
            let mut m = self.metrics.borrow_mut();
            m.generation_match =
                Some(crate::selection_runtime::generation() == self.request.generation);
            m.foreground_match =
                Some(crate::selection_runtime::target() == Some(self.request.target));
            return Err("UIA_TARGET_CHANGED");
        }
        if !live(self.request) {
            return Err("UIA_TIMEOUT");
        }
        Ok(())
    }
    fn process_matches(&self, e: &Self::Element) -> Result<bool, &'static str> {
        unsafe {
            e.CurrentProcessId()
                .map(|pid| {
                    self.metrics.borrow_mut().observed_pid = Some(pid as u32);
                    pid as u32 == self.request.target.pid
                })
                .map_err(|_| "UIA_WINDOW_UNVERIFIED")
        }
    }
    fn password(&self, e: &Self::Element) -> Result<bool, &'static str> {
        unsafe {
            e.CurrentIsPassword()
                .map(|v| v.as_bool())
                .map_err(|_| "UIA_PROTECTED_CONTROL")
        }
    }
    fn root_matches(&self, e: &Self::Element) -> Result<Option<bool>, &'static str> {
        unsafe {
            let hwnd = e
                .CurrentNativeWindowHandle()
                .map_err(|_| "UIA_WINDOW_UNVERIFIED")?;
            if hwnd.is_invalid() {
                return Ok(None);
            }
            let root = GetAncestor(hwnd, GA_ROOT);
            let mut pid = 0;
            GetWindowThreadProcessId(root, Some(&mut pid));
            let mut m = self.metrics.borrow_mut();
            m.root_hwnd_match = Some(root == HWND(self.request.target.hwnd as *mut _));
            m.root_pid_match = Some(pid == self.request.target.pid);
            Ok(Some(
                m.root_hwnd_match == Some(true) && m.root_pid_match == Some(true),
            ))
        }
    }
    fn parent(&self, e: &Self::Element) -> Result<Self::Element, &'static str> {
        unsafe {
            self.walker
                .GetParentElement(e)
                .map_err(|_| "UIA_WINDOW_UNVERIFIED")
        }
    }
    fn same(&self, a: &Self::Element, b: &Self::Element) -> Result<bool, &'static str> {
        unsafe {
            self.automation
                .CompareElements(a, b)
                .map(|v| v.as_bool())
                .map_err(|_| "UIA_WINDOW_UNVERIFIED")
        }
    }
}

unsafe fn range_text(e: &IUIAutomationElement, r: &Request) -> Result<Attempt, &'static str> {
    let pattern = e
        .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        .or_else(|_| {
            e.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
                .and_then(|p| p.cast())
        });
    let Ok(pattern) = pattern else {
        return Ok(Attempt::Unsupported);
    };
    let selection = pattern.GetSelection().map_err(|_| "UIA_INTERNAL_ERROR")?;
    let count = selection.Length().map_err(|_| "UIA_INTERNAL_ERROR")?;
    if count < 0 || count as usize > MAX_RANGES {
        return Err("UIA_INTERNAL_ERROR");
    }
    let mut ranges = Vec::<IUIAutomationTextRange>::new();
    for i in 0..count {
        if !live(r) {
            return Err("UIA_TIMEOUT");
        }
        let range = selection.GetElement(i).map_err(|_| "UIA_INTERNAL_ERROR")?;
        let mut at = ranges.len();
        let mut duplicate = false;
        for (j, old) in ranges.iter().enumerate() {
            let start = range
                .CompareEndpoints(
                    TextPatternRangeEndpoint_Start,
                    old,
                    TextPatternRangeEndpoint_Start,
                )
                .map_err(|_| "UIA_INTERNAL_ERROR")?;
            let end = range
                .CompareEndpoints(
                    TextPatternRangeEndpoint_End,
                    old,
                    TextPatternRangeEndpoint_End,
                )
                .map_err(|_| "UIA_INTERNAL_ERROR")?;
            if start == 0 && end == 0 {
                duplicate = true;
                break;
            }
            if start < 0 {
                at = j;
                break;
            }
        }
        if !duplicate {
            ranges.insert(at, range);
        }
    }
    let mut parts = Vec::new();
    let mut remaining = MAX_TEXT;
    for range in ranges {
        if !live(r) {
            return Err("UIA_TIMEOUT");
        }
        // UTF-16 budget is bounded before invoking the provider; never GetText(-1).
        let text = range
            .GetText((remaining * 2).min(40_000) as i32)
            .map_err(|_| "UIA_INTERNAL_ERROR")?
            .to_string();
        let text: String = text.chars().take(remaining).collect();
        remaining = remaining.saturating_sub(text.chars().count() + 1);
        parts.push(text);
        if remaining == 0 {
            break;
        }
    }
    match join_ranges(&parts, MAX_TEXT) {
        Ok(text) => Ok(Attempt::Text(text)),
        Err("UIA_EMPTY_SELECTION") => Ok(Attempt::Empty),
        Err(s) => Err(s),
    }
}
#[derive(Default)]
struct UiaMetrics {
    calls: [u32; 6],
    ms: [u64; 6],
    last_stage: &'static str,
    last_error: &'static str,
    candidate: usize,
    role: &'static str,
    depth: usize,
    ownership_reason: &'static str,
    ownership_depth: usize,
    observed_pid: Option<u32>,
    generation_match: Option<bool>,
    foreground_match: Option<bool>,
    root_hwnd_match: Option<bool>,
    root_pid_match: Option<bool>,
}
const STAGES: [&str; 6] = ["focus", "hit", "validate", "recheck", "parent", "pattern"];
struct NativeProvider<'a> {
    automation: IUIAutomation,
    walker: IUIAutomationTreeWalker,
    request: &'a Request,
    metrics: RefCell<UiaMetrics>,
}
impl NativeProvider<'_> {
    fn measured<T>(
        &self,
        stage: usize,
        f: impl FnOnce() -> Result<T, &'static str>,
    ) -> Result<T, &'static str> {
        let start = crate::selection_runtime::now_ms();
        let result = f();
        let mut m = self.metrics.borrow_mut();
        m.calls[stage] += 1;
        m.ms[stage] += crate::selection_runtime::now_ms().saturating_sub(start);
        m.last_stage = STAGES[stage];
        if let Err(s) = &result {
            m.last_error = s;
        }
        result
    }
}
impl Provider for NativeProvider<'_> {
    type Element = IUIAutomationElement;
    fn focused(&self) -> Result<Self::Element, &'static str> {
        self.measured(0, || unsafe {
            self.automation
                .GetFocusedElement()
                .map_err(|_| "UIA_ELEMENT_UNAVAILABLE")
        })
    }
    fn hit(&self) -> Result<Self::Element, &'static str> {
        self.measured(1, || unsafe {
            self.automation
                .ElementFromPoint(POINT {
                    x: self.request.point.0,
                    y: self.request.point.1,
                })
                .map_err(|_| "UIA_TARGET_CHANGED")
        })
    }
    fn validate(&self, e: &Self::Element) -> Result<(), &'static str> {
        self.measured(2, || unsafe {
            checked(
                &self.automation,
                &self.walker,
                e,
                self.request,
                &self.metrics,
            )
        })
    }
    fn recheck(&self, e: &Self::Element) -> Result<(), &'static str> {
        self.measured(3, || {
            let owner = NativeOwnership {
                automation: &self.automation,
                walker: self.walker.clone(),
                request: self.request,
                metrics: &self.metrics,
            };
            uia_strategy::recheck_ownership(&owner, e)
        })
    }
    fn candidate(&self, index: usize, role: &'static str, depth: usize) {
        let mut m = self.metrics.borrow_mut();
        m.candidate = index;
        m.role = role;
        m.depth = depth;
    }
    fn parent(&self, e: &Self::Element) -> Result<Option<Self::Element>, &'static str> {
        self.measured(4, || unsafe {
            let Ok(parent) = self.walker.GetParentElement(e) else {
                return Ok(None);
            };
            match checked(
                &self.automation,
                &self.walker,
                &parent,
                self.request,
                &self.metrics,
            ) {
                Ok(()) => Ok(Some(parent)),
                Err("UIA_TARGET_CHANGED") if crate::selection_runtime::valid(self.request) => {
                    Ok(None)
                }
                Err(s) => Err(s),
            }
        })
    }
    fn text_container(&self, e: &Self::Element) -> bool {
        unsafe {
            e.CurrentControlType()
                .map(|t| t == UIA_DocumentControlTypeId || t == UIA_EditControlTypeId)
                .unwrap_or(false)
        }
    }
    fn same(&self, a: &Self::Element, b: &Self::Element) -> bool {
        unsafe {
            self.automation
                .CompareElements(a, b)
                .map(|v| v.as_bool())
                .unwrap_or(false)
        }
    }
    fn attempt(&self, e: &Self::Element) -> Result<Attempt, &'static str> {
        self.measured(5, || unsafe { range_text(e, self.request) })
    }
}
unsafe fn read_inner(r: &Request) -> Result<ReadResult, &'static str> {
    let started = crate::selection_runtime::now_ms();
    if !live(r) {
        return Err("UIA_TIMEOUT");
    }
    let automation: IUIAutomation =
        CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL).map_err(|_| "UIA_INTERNAL_ERROR")?;
    let walker = automation
        .ControlViewWalker()
        .map_err(|_| "UIA_INTERNAL_ERROR")?;
    let provider = NativeProvider {
        automation,
        walker,
        request: r,
        metrics: RefCell::new(UiaMetrics::default()),
    };
    let result = uia_strategy::read_with(&provider);
    let m = provider.metrics.borrow();
    // One metadata-only summary after the transaction; never log text or titles.
    crate::diag::log(&format!("selection trace request={} target_pid={} pre_strategy_ms={} calls={:?} stage_ms={:?} last_stage={} last_error={} candidate={} role={} depth={} ownership_reason={} ownership_depth={} observed_pid={:?} generation_match={:?} foreground_match={:?} root_hwnd_match={:?} root_pid_match={:?}",
        r.generation, r.target.pid, started.saturating_sub(r.created_ms), m.calls, m.ms, m.last_stage, m.last_error, m.candidate, m.role, m.depth, m.ownership_reason, m.ownership_depth, m.observed_pid, m.generation_match, m.foreground_match, m.root_hwnd_match, m.root_pid_match));
    result
}
