#[path = "../src/uia_strategy.rs"]
mod uia_strategy;
use std::cell::Cell;
use uia_strategy::*;
struct Fake {
    focus_missing: bool,
    focus_read_error: bool,
    text: [Option<&'static str>; 3],
    blocked: Option<usize>,
    cross_window: bool,
    chain: bool,
    expired: bool,
    late: bool,
    reads: Cell<usize>,
    checks: Cell<usize>,
}
impl Default for Fake {
    fn default() -> Self {
        Self {
            focus_missing: false,
            focus_read_error: false,
            text: [None, None, None],
            blocked: None,
            cross_window: false,
            chain: false,
            expired: false,
            late: false,
            reads: Cell::new(0),
            checks: Cell::new(0),
        }
    }
}
impl Provider for Fake {
    type Element = usize;
    fn focused(&self) -> Result<usize, &'static str> {
        if self.focus_missing {
            return Err("UIA_ELEMENT_UNAVAILABLE");
        }
        Ok(0)
    }
    fn hit(&self) -> Result<usize, &'static str> {
        Ok(1)
    }
    fn validate(&self, e: &usize) -> Result<(), &'static str> {
        self.checks.set(self.checks.get() + 1);
        if self.cross_window && *e == 1 {
            return Err("UIA_TARGET_CHANGED");
        }
        if self.expired {
            return Err("UIA_TIMEOUT");
        }
        if self.blocked == Some(*e) {
            return Err("UIA_PROTECTED_CONTROL");
        }
        if self.late && self.reads.get() > 0 {
            return Err("UIA_TARGET_CHANGED");
        }
        Ok(())
    }
    fn parent(&self, e: &usize) -> Result<Option<usize>, &'static str> {
        Ok(if self.chain {
            Some(if *e < 2 { 2 } else { *e + 1 })
        } else {
            (*e < 2).then_some(2)
        })
    }
    fn text_container(&self, e: &usize) -> bool {
        *e == 2
    }
    fn same(&self, a: &usize, b: &usize) -> bool {
        a == b
    }
    fn attempt(&self, e: &usize) -> Result<Attempt, &'static str> {
        self.reads.set(self.reads.get() + 1);
        if self.focus_read_error && *e == 0 {
            return Err("UIA_INTERNAL_ERROR");
        }
        Ok(match self.text.get(*e).copied().flatten() {
            Some("") => Attempt::Empty,
            Some(t) => Attempt::Text(t.into()),
            None => Attempt::Unsupported,
        })
    }
}
#[test]
fn focused_element_wins_without_parent_reads() {
    let p = Fake {
        text: [Some("focus"), Some("hit"), Some("parent")],
        ..Default::default()
    };
    assert_eq!(read_with(&p).unwrap().text.as_deref(), Some("focus"));
    assert_eq!(p.reads.get(), 1);
}
#[test]
fn hit_element_fills_focus_pattern_gap() {
    let p = Fake {
        text: [None, Some("hit"), None],
        ..Default::default()
    };
    assert_eq!(read_with(&p).unwrap().text.as_deref(), Some("hit"));
}
#[test]
fn parent_container_is_used_only_after_both_initial_candidates() {
    let p = Fake {
        text: [None, None, Some("parent\n  indent")],
        ..Default::default()
    };
    assert_eq!(
        read_with(&p).unwrap().text.as_deref(),
        Some("parent\n  indent")
    );
    assert_eq!(p.reads.get(), 3);
}
#[test]
fn unsupported_and_empty_are_distinct() {
    assert_eq!(
        read_with(&Fake::default()).unwrap().status,
        "UIA_PATTERN_UNSUPPORTED"
    );
    let p = Fake {
        text: [Some(""), None, None],
        ..Default::default()
    };
    assert_eq!(read_with(&p).unwrap().status, "UIA_EMPTY_SELECTION");
}
#[test]
fn protected_hit_rejects_before_any_text_read() {
    let p = Fake {
        blocked: Some(1),
        ..Default::default()
    };
    assert!(matches!(read_with(&p), Err("UIA_PROTECTED_CONTROL")));
    assert_eq!(p.reads.get(), 0);
}
#[test]
fn deadline_is_checked_before_text_reads() {
    let p = Fake {
        expired: true,
        ..Default::default()
    };
    assert!(matches!(read_with(&p), Err("UIA_TIMEOUT")));
    assert_eq!(p.reads.get(), 0);
}
#[test]
fn changed_target_after_provider_return_rejects_late_text() {
    let p = Fake {
        text: [Some("late"), None, None],
        late: true,
        ..Default::default()
    };
    assert!(matches!(read_with(&p), Err("UIA_TARGET_CHANGED")));
}
#[test]
fn duplicate_parent_is_read_only_once() {
    let p = Fake::default();
    assert!(read_with(&p).unwrap().text.is_none());
    assert_eq!(p.reads.get(), 3);
}

#[test]
fn cross_window_hit_rejects_before_reading() {
    let p = Fake {
        cross_window: true,
        ..Default::default()
    };
    assert!(matches!(read_with(&p), Err("UIA_TARGET_CHANGED")));
    assert_eq!(p.reads.get(), 0);
}
#[test]
fn deep_parent_chain_is_bounded_and_safe_target_is_explicit() {
    let p = Fake {
        chain: true,
        ..Default::default()
    };
    let result = read_with(&p).unwrap();
    assert!(result.safe_target);
    assert!(p.reads.get() <= 12);
    assert!(p.checks.get() < 40);
}

#[test]
fn unavailable_focus_does_not_block_verified_hit() {
    let p = Fake {
        focus_missing: true,
        text: [None, Some("verified hit"), None],
        ..Default::default()
    };
    assert_eq!(read_with(&p).unwrap().text.as_deref(), Some("verified hit"));
}
#[test]
fn failed_focus_pattern_does_not_block_verified_hit() {
    let p = Fake {
        focus_read_error: true,
        text: [None, Some("verified hit"), None],
        ..Default::default()
    };
    assert_eq!(read_with(&p).unwrap().text.as_deref(), Some("verified hit"));
}
#[test]
fn missing_focus_does_not_bypass_protected_hit() {
    let p = Fake {
        focus_missing: true,
        blocked: Some(1),
        ..Default::default()
    };
    assert!(matches!(read_with(&p), Err("UIA_PROTECTED_CONTROL")));
    assert_eq!(p.reads.get(), 0);
}

struct Tree {
    root: usize,
    wrong_root: bool,
    password_at: Option<usize>,
    cycle: bool,
    expired: bool,
}
impl Default for Tree {
    fn default() -> Self {
        Self {
            root: 8,
            wrong_root: false,
            password_at: None,
            cycle: false,
            expired: false,
        }
    }
}
impl OwnershipProvider for Tree {
    type Element = usize;
    fn live(&self) -> Result<(), &'static str> {
        if self.expired {
            Err("UIA_TIMEOUT")
        } else {
            Ok(())
        }
    }
    fn process_matches(&self, _: &usize) -> Result<bool, &'static str> {
        Ok(true)
    }
    fn password(&self, e: &usize) -> Result<bool, &'static str> {
        Ok(self.password_at == Some(*e))
    }
    fn root_matches(&self, e: &usize) -> Result<Option<bool>, &'static str> {
        Ok((*e == self.root).then_some(!self.wrong_root))
    }
    fn parent(&self, e: &usize) -> Result<usize, &'static str> {
        Ok(if self.cycle { 0 } else { e + 1 })
    }
    fn same(&self, a: &usize, b: &usize) -> Result<bool, &'static str> {
        Ok(a == b)
    }
}
#[test]
fn zero_hwnd_nodes_can_reach_verified_window_at_depth_eight() {
    assert_eq!(verify_ownership(&Tree::default(), &0), Ok(()));
}
#[test]
fn deeper_walk_still_rejects_password_ancestor_and_wrong_window() {
    assert_eq!(
        verify_ownership(
            &Tree {
                password_at: Some(7),
                ..Default::default()
            },
            &0
        ),
        Err("UIA_PROTECTED_CONTROL")
    );
    assert_eq!(
        verify_ownership(
            &Tree {
                wrong_root: true,
                ..Default::default()
            },
            &0
        ),
        Err("UIA_TARGET_CHANGED")
    );
}
#[test]
fn ancestry_cycles_and_missing_window_remain_bounded() {
    assert_eq!(
        verify_ownership(
            &Tree {
                cycle: true,
                ..Default::default()
            },
            &0
        ),
        Err("UIA_ANCESTRY_CYCLE")
    );
    assert_eq!(
        verify_ownership(
            &Tree {
                root: 100,
                ..Default::default()
            },
            &0
        ),
        Err("UIA_ANCESTRY_LIMIT")
    );
    assert_eq!(
        verify_ownership(
            &Tree {
                expired: true,
                ..Default::default()
            },
            &0
        ),
        Err("UIA_TIMEOUT")
    );
}

#[test]
fn provider_read_error_cannot_authorize_automatic_copy() {
    let p = Fake {
        focus_read_error: true,
        ..Default::default()
    };
    let result = read_with(&p).unwrap();
    assert!(result.text.is_none());
    assert!(!result.safe_target);
    assert_eq!(result.status, "UIA_INTERNAL_ERROR");
}
#[test]
fn missing_focus_cannot_bypass_cross_window_hit() {
    let p = Fake {
        focus_missing: true,
        cross_window: true,
        ..Default::default()
    };
    assert!(matches!(read_with(&p), Err("UIA_TARGET_CHANGED")));
    assert_eq!(p.reads.get(), 0);
}
#[test]
fn protected_focus_is_not_ignored_for_a_readable_hit() {
    let p = Fake {
        blocked: Some(0),
        text: [None, Some("must not read"), None],
        ..Default::default()
    };
    assert!(matches!(read_with(&p), Err("UIA_PROTECTED_CONTROL")));
    assert_eq!(p.reads.get(), 0);
}
#[test]
fn ownership_depth_boundary_accepts_32_but_not_33() {
    assert_eq!(
        verify_ownership(
            &Tree {
                root: 32,
                ..Default::default()
            },
            &0
        ),
        Ok(())
    );
    assert_eq!(
        verify_ownership(
            &Tree {
                root: 33,
                ..Default::default()
            },
            &0
        ),
        Err("UIA_ANCESTRY_LIMIT")
    );
}
struct FaultyTree {
    pid_matches: bool,
    compare_fails: bool,
    expires: bool,
    calls: Cell<usize>,
}
impl OwnershipProvider for FaultyTree {
    type Element = usize;
    fn live(&self) -> Result<(), &'static str> {
        self.calls.set(self.calls.get() + 1);
        if self.expires && self.calls.get() > 3 {
            Err("UIA_TARGET_CHANGED")
        } else {
            Ok(())
        }
    }
    fn process_matches(&self, _: &usize) -> Result<bool, &'static str> {
        Ok(self.pid_matches)
    }
    fn password(&self, _: &usize) -> Result<bool, &'static str> {
        Ok(false)
    }
    fn root_matches(&self, e: &usize) -> Result<Option<bool>, &'static str> {
        Ok((*e == 8).then_some(true))
    }
    fn parent(&self, e: &usize) -> Result<usize, &'static str> {
        Ok(e + 1)
    }
    fn same(&self, a: &usize, b: &usize) -> Result<bool, &'static str> {
        if self.compare_fails {
            Err("UIA_WINDOW_UNVERIFIED")
        } else {
            Ok(a == b)
        }
    }
}
#[test]
fn ownership_walk_rejects_pid_mismatch_without_traversing() {
    let p = FaultyTree {
        pid_matches: false,
        compare_fails: false,
        expires: false,
        calls: Cell::new(0),
    };
    assert_eq!(verify_ownership(&p, &0), Err("UIA_TARGET_CHANGED"));
    assert_eq!(p.calls.get(), 1);
}
#[test]
fn failed_node_identity_comparison_cannot_prove_window_ownership() {
    let p = FaultyTree {
        pid_matches: true,
        compare_fails: true,
        expires: false,
        calls: Cell::new(0),
    };
    assert_eq!(verify_ownership(&p, &0), Err("UIA_WINDOW_UNVERIFIED"));
}
#[test]
fn request_change_during_ancestry_walk_rejects_result() {
    let p = FaultyTree {
        pid_matches: true,
        compare_fails: false,
        expires: true,
        calls: Cell::new(0),
    };
    assert_eq!(verify_ownership(&p, &0), Err("UIA_TARGET_CHANGED"));
}

struct Fairness {
    same_hit: bool,
    parent_error: bool,
    parent_cost: u32,
    focus_error: bool,
    hit_empty: bool,
    validation_cost: u32,
    now: Cell<u32>,
    hit_reads: Cell<usize>,
    parent_calls: Cell<usize>,
}
impl Default for Fairness {
    fn default() -> Self {
        Self {
            same_hit: false,
            parent_error: false,
            parent_cost: 0,
            focus_error: false,
            hit_empty: false,
            validation_cost: 0,
            now: Cell::new(0),
            hit_reads: Cell::new(0),
            parent_calls: Cell::new(0),
        }
    }
}
impl Fairness {
    fn charge(&self, cost: u32) -> Result<(), &'static str> {
        self.now.set(self.now.get() + cost);
        if self.now.get() >= 350 {
            Err("UIA_TIMEOUT")
        } else {
            Ok(())
        }
    }
}
impl Provider for Fairness {
    type Element = usize;
    fn focused(&self) -> Result<usize, &'static str> {
        Ok(0)
    }
    fn hit(&self) -> Result<usize, &'static str> {
        Ok(if self.same_hit { 0 } else { 1 })
    }
    fn validate(&self, _: &usize) -> Result<(), &'static str> {
        self.charge(self.validation_cost)
    }
    fn parent(&self, e: &usize) -> Result<Option<usize>, &'static str> {
        self.parent_calls.set(self.parent_calls.get() + 1);
        self.charge(self.parent_cost)?;
        if *e == 0 && self.parent_error {
            Err("UIA_WINDOW_UNVERIFIED")
        } else {
            Ok((*e == 1).then_some(2))
        }
    }
    fn text_container(&self, _: &usize) -> bool {
        true
    }
    fn same(&self, a: &usize, b: &usize) -> bool {
        a == b
    }
    fn attempt(&self, e: &usize) -> Result<Attempt, &'static str> {
        if *e == 1 || self.same_hit {
            self.hit_reads.set(self.hit_reads.get() + 1);
            if self.hit_empty {
                Ok(Attempt::Empty)
            } else {
                Ok(Attempt::Text("trusted synthetic hit".into()))
            }
        } else if *e == 2 {
            Ok(Attempt::Text("trusted synthetic parent".into()))
        } else if self.focus_error {
            Err("UIA_INTERNAL_ERROR")
        } else {
            Ok(Attempt::Unsupported)
        }
    }
}
#[test]
fn verified_hit_is_read_before_slow_focus_parent() {
    let p = Fairness {
        parent_cost: 350,
        ..Default::default()
    };
    assert_eq!(
        read_with(&p).unwrap().text.as_deref(),
        Some("trusted synthetic hit")
    );
    assert_eq!(p.parent_calls.get(), 0);
}
#[test]
fn verified_hit_is_read_before_failed_focus_parent() {
    let p = Fairness {
        parent_error: true,
        ..Default::default()
    };
    assert_eq!(
        read_with(&p).unwrap().text.as_deref(),
        Some("trusted synthetic hit")
    );
}
#[test]
fn pattern_failure_does_not_enter_parent_before_other_initial_candidate() {
    let p = Fairness {
        parent_error: true,
        focus_error: true,
        ..Default::default()
    };
    assert_eq!(
        read_with(&p).unwrap().text.as_deref(),
        Some("trusted synthetic hit")
    );
}
#[test]
fn same_hit_focus_avoids_redundant_full_walk_budget_exhaustion() {
    let p = Fairness {
        same_hit: true,
        validation_cost: 80,
        ..Default::default()
    };
    assert_eq!(
        read_with(&p).unwrap().text.as_deref(),
        Some("trusted synthetic hit")
    );
    assert_eq!(p.hit_reads.get(), 1);
}
#[test]
fn local_parent_unavailable_does_not_block_other_verified_parent_branch() {
    let p = Fairness {
        parent_error: true,
        hit_empty: true,
        ..Default::default()
    };
    assert_eq!(
        read_with(&p).unwrap().text.as_deref(),
        Some("trusted synthetic parent")
    );
}
#[test]
fn provider_error_is_not_misreported_as_empty_selection() {
    let p = Fake {
        focus_read_error: true,
        text: [None, Some(""), Some("")],
        ..Default::default()
    };
    let r = read_with(&p).unwrap();
    assert!(r.text.is_none());
    assert!(!r.safe_target);
    assert_eq!(r.status, "UIA_INTERNAL_ERROR");
}

struct ChangingOwner {
    protected: Cell<bool>,
    wrong_pid: Cell<bool>,
    wrong_root: Cell<bool>,
    stale: Cell<bool>,
}
impl Default for ChangingOwner {
    fn default() -> Self {
        Self {
            protected: Cell::new(false),
            wrong_pid: Cell::new(false),
            wrong_root: Cell::new(false),
            stale: Cell::new(false),
        }
    }
}
impl OwnershipProvider for ChangingOwner {
    type Element = usize;
    fn live(&self) -> Result<(), &'static str> {
        if self.stale.get() {
            Err("UIA_TARGET_CHANGED")
        } else {
            Ok(())
        }
    }
    fn process_matches(&self, _: &usize) -> Result<bool, &'static str> {
        Ok(!self.wrong_pid.get())
    }
    fn password(&self, _: &usize) -> Result<bool, &'static str> {
        Ok(self.protected.get())
    }
    fn root_matches(&self, _: &usize) -> Result<Option<bool>, &'static str> {
        Ok(Some(!self.wrong_root.get()))
    }
    fn parent(&self, _: &usize) -> Result<usize, &'static str> {
        panic!("Native root is already available")
    }
    fn same(&self, a: &usize, b: &usize) -> Result<bool, &'static str> {
        Ok(a == b)
    }
}
#[test]
fn lightweight_recheck_rejects_new_password_after_initial_proof() {
    let p = ChangingOwner::default();
    verify_ownership(&p, &0).unwrap();
    p.protected.set(true);
    assert_eq!(recheck_ownership(&p, &0), Err("UIA_PROTECTED_CONTROL"));
}
#[test]
fn lightweight_recheck_rejects_new_pid_after_initial_proof() {
    let p = ChangingOwner::default();
    verify_ownership(&p, &0).unwrap();
    p.wrong_pid.set(true);
    assert_eq!(recheck_ownership(&p, &0), Err("UIA_TARGET_CHANGED"));
}
#[test]
fn lightweight_recheck_rejects_new_native_root_after_initial_proof() {
    let p = ChangingOwner::default();
    verify_ownership(&p, &0).unwrap();
    p.wrong_root.set(true);
    assert_eq!(recheck_ownership(&p, &0), Err("UIA_TARGET_CHANGED"));
}
#[test]
fn lightweight_recheck_rejects_invalid_request_after_initial_proof() {
    let p = ChangingOwner::default();
    verify_ownership(&p, &0).unwrap();
    p.stale.set(true);
    assert_eq!(recheck_ownership(&p, &0), Err("UIA_TARGET_CHANGED"));
}
struct ProtectedAfterRead {
    owner: ChangingOwner,
}
impl Provider for ProtectedAfterRead {
    type Element = usize;
    fn focused(&self) -> Result<usize, &'static str> {
        Ok(0)
    }
    fn hit(&self) -> Result<usize, &'static str> {
        Ok(0)
    }
    fn validate(&self, e: &usize) -> Result<(), &'static str> {
        verify_ownership(&self.owner, e)
    }
    fn recheck(&self, e: &usize) -> Result<(), &'static str> {
        recheck_ownership(&self.owner, e)
    }
    fn parent(&self, _: &usize) -> Result<Option<usize>, &'static str> {
        Ok(None)
    }
    fn text_container(&self, _: &usize) -> bool {
        true
    }
    fn same(&self, a: &usize, b: &usize) -> bool {
        a == b
    }
    fn attempt(&self, _: &usize) -> Result<Attempt, &'static str> {
        self.owner.protected.set(true);
        Ok(Attempt::Text("must not escape".into()))
    }
}
#[test]
fn lightweight_recheck_never_replaces_final_full_protection_validation() {
    let p = ProtectedAfterRead {
        owner: ChangingOwner::default(),
    };
    assert!(matches!(read_with(&p), Err("UIA_PROTECTED_CONTROL")));
}
#[test]
fn protected_parent_does_not_become_a_soft_branch_failure() {
    let p = Fake {
        blocked: Some(2),
        ..Default::default()
    };
    assert!(matches!(read_with(&p), Err("UIA_PROTECTED_CONTROL")));
    assert_eq!(p.reads.get(), 2);
}

// Diagnostics must explain distinct rejections while all remain fail-closed.
struct ExplainedOwner {
    fault: &'static str,
    failures: std::cell::RefCell<Vec<(&'static str, usize)>>,
}
impl OwnershipProvider for ExplainedOwner {
    type Element = usize;
    fn live(&self) -> Result<(), &'static str> {
        match self.fault {
            "request-invalid" => Err("UIA_TARGET_CHANGED"),
            "deadline" => Err("UIA_TIMEOUT"),
            _ => Ok(()),
        }
    }
    fn process_matches(&self, e: &usize) -> Result<bool, &'static str> {
        Ok(!(self.fault == "process-mismatch" && *e == 1))
    }
    fn password(&self, _: &usize) -> Result<bool, &'static str> {
        Ok(self.fault == "protected-control")
    }
    fn root_matches(&self, e: &usize) -> Result<Option<bool>, &'static str> {
        Ok(if self.fault == "root-mismatch" {
            Some(false)
        } else if *e == 2 {
            Some(true)
        } else {
            None
        })
    }
    fn parent(&self, e: &usize) -> Result<usize, &'static str> {
        Ok(e + 1)
    }
    fn same(&self, a: &usize, b: &usize) -> Result<bool, &'static str> {
        Ok(a == b)
    }
    fn rejected(&self, reason: &'static str, depth: usize) {
        self.failures.borrow_mut().push((reason, depth));
    }
}
#[test]
fn ownership_diagnostics_distinguish_failures_without_authorizing_read() {
    for (reason, depth, status) in [
        ("request-invalid", 0, "UIA_TARGET_CHANGED"),
        ("deadline", 0, "UIA_TIMEOUT"),
        ("process-mismatch", 1, "UIA_TARGET_CHANGED"),
        ("root-mismatch", 0, "UIA_TARGET_CHANGED"),
        ("protected-control", 0, "UIA_PROTECTED_CONTROL"),
    ] {
        let p = ExplainedOwner {
            fault: reason,
            failures: Default::default(),
        };
        assert_eq!(verify_ownership(&p, &0), Err(status));
        assert_eq!(
            &*p.failures.borrow(),
            &[(reason, depth)],
            "missing or incorrect reason for {reason}"
        );
    }
}
#[test]
fn valid_ownership_does_not_emit_rejection() {
    let p = ExplainedOwner {
        fault: "none",
        failures: Default::default(),
    };
    assert_eq!(verify_ownership(&p, &0), Ok(()));
    assert!(p.failures.borrow().is_empty());
}
#[test]
fn exact_node_recheck_still_rejects_root_mismatch_with_reason() {
    let p = ExplainedOwner {
        fault: "root-mismatch",
        failures: Default::default(),
    };
    assert_eq!(recheck_ownership(&p, &0), Err("UIA_TARGET_CHANGED"));
    assert_eq!(&*p.failures.borrow(), &[("root-mismatch", 0)]);
}

struct LateTraceOwner {
    current: Cell<usize>,
    failures: std::cell::RefCell<Vec<(&'static str, usize)>>,
    on_final_check: bool,
}
impl OwnershipProvider for LateTraceOwner {
    type Element = usize;
    fn live(&self) -> Result<(), &'static str> {
        if self.current.get() > 0 {
            Err("UIA_TARGET_CHANGED")
        } else {
            Ok(())
        }
    }
    fn process_matches(&self, _: &usize) -> Result<bool, &'static str> {
        Ok(true)
    }
    fn password(&self, _: &usize) -> Result<bool, &'static str> {
        Ok(false)
    }
    fn root_matches(&self, _: &usize) -> Result<Option<bool>, &'static str> {
        if self.on_final_check {
            self.current.set(1);
            Ok(Some(true))
        } else {
            Ok(None)
        }
    }
    fn parent(&self, e: &usize) -> Result<usize, &'static str> {
        self.current.set(e + 1);
        Ok(e + 1)
    }
    fn same(&self, a: &usize, b: &usize) -> Result<bool, &'static str> {
        Ok(a == b)
    }
    fn rejected(&self, reason: &'static str, depth: usize) {
        self.failures.borrow_mut().push((reason, depth));
    }
}
#[test]
fn late_request_rejection_trace_reports_actual_ancestor_depth() {
    let p = LateTraceOwner {
        current: Cell::new(0),
        failures: Default::default(),
        on_final_check: false,
    };
    assert_eq!(verify_ownership(&p, &0), Err("UIA_TARGET_CHANGED"));
    assert_eq!(&*p.failures.borrow(), &[("request-invalid", 1)]);
}
#[test]
fn final_recheck_request_change_is_also_explained() {
    let p = LateTraceOwner {
        current: Cell::new(0),
        failures: Default::default(),
        on_final_check: true,
    };
    assert_eq!(recheck_ownership(&p, &0), Err("UIA_TARGET_CHANGED"));
    assert_eq!(&*p.failures.borrow(), &[("request-invalid", 0)]);
}
