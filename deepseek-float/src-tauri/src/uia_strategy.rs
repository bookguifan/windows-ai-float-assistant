//! Bounded candidate traversal shared by real COM and synthetic providers.
pub enum Attempt {
    Unsupported,
    Empty,
    Text(String),
}
pub struct ReadResult {
    pub text: Option<String>,
    pub status: &'static str,
    pub safe_target: bool,
}
impl ReadResult {
    pub fn fail(status: &'static str, safe_target: bool) -> Self {
        Self {
            text: None,
            status,
            safe_target,
        }
    }
}
pub trait Provider {
    type Element: Clone;
    fn focused(&self) -> Result<Self::Element, &'static str>;
    fn hit(&self) -> Result<Self::Element, &'static str>;
    fn validate(&self, e: &Self::Element) -> Result<(), &'static str>;
    /// Recheck an exact node already proven by validate/parent in this request.
    /// Native providers may avoid repeating the ancestry walk, but must check
    /// live identity/password properties; success still requires full validate.
    fn recheck(&self, e: &Self::Element) -> Result<(), &'static str> {
        self.validate(e)
    }
    fn candidate(&self, _index: usize, _role: &'static str, _depth: usize) {}
    fn parent(&self, e: &Self::Element) -> Result<Option<Self::Element>, &'static str>;
    fn text_container(&self, e: &Self::Element) -> bool;
    fn same(&self, a: &Self::Element, b: &Self::Element) -> bool;
    fn attempt(&self, e: &Self::Element) -> Result<Attempt, &'static str>;
}
pub fn read_with(p: &impl Provider) -> Result<ReadResult, &'static str> {
    // An unknown/protected pointer target must never authorize a focused read.
    let hit = p.hit()?;
    p.validate(&hit)?;
    let mut candidates = Vec::new();
    let focused_is_hit = match p.focused() {
        Ok(e) => {
            let same = p.same(&e, &hit);
            if !same {
                p.validate(&e)?;
            }
            candidates.push((e, 0, if same { "focus-hit" } else { "focus" }));
            same
        }
        Err("UIA_ELEMENT_UNAVAILABLE") => false,
        Err(s) => return Err(s),
    };
    if !focused_is_hit {
        candidates.push((hit.clone(), 0, "hit"));
    }
    let mut provider_error = None;
    let mut status = "UIA_PATTERN_UNSUPPORTED";
    let mut begin = 0;
    // Read the entire frontier before doing any potentially slow parent work.
    // This preserves focus/hit priority and gives each verified branch a turn.
    while begin < candidates.len() {
        let end = candidates.len();
        for (at, (e, depth, role)) in candidates.iter().enumerate().take(end).skip(begin) {
            p.candidate(at, role, *depth);
            p.recheck(e)?;
            if *depth == 0 || p.text_container(e) {
                match p.attempt(e) {
                    Ok(Attempt::Text(raw)) if !raw.trim().is_empty() => {
                        // Never return text on the strength of a cached proof alone.
                        p.validate(e)?;
                        if !p.same(e, &hit) {
                            p.validate(&hit)?;
                        }
                        return Ok(ReadResult {
                            text: Some(raw.chars().take(20_000).collect()),
                            status: "UIA_SUCCESS",
                            safe_target: true,
                        });
                    }
                    Ok(Attempt::Text(_) | Attempt::Empty) => status = "UIA_EMPTY_SELECTION",
                    Ok(Attempt::Unsupported) => {}
                    Err("UIA_INTERNAL_ERROR") => provider_error = Some("UIA_INTERNAL_ERROR"),
                    Err(s) => return Err(s),
                }
            }
        }
        for at in begin..end {
            let (e, depth, _) = candidates[at].clone();
            if depth >= 5 || candidates.len() >= 12 {
                continue;
            }
            match p.parent(&e) {
                Ok(Some(parent)) => {
                    if !candidates.iter().any(|(old, _, _)| p.same(old, &parent)) {
                        candidates.push((parent, depth + 1, "parent"));
                    }
                }
                Ok(None) => {}
                // No text is read from an unverified ancestor. Other branches
                // must still independently prove their own nodes before reading.
                Err(
                    s @ ("UIA_WINDOW_UNVERIFIED"
                    | "UIA_INTERNAL_ERROR"
                    | "UIA_ANCESTRY_LIMIT"
                    | "UIA_ANCESTRY_CYCLE"),
                ) => provider_error = Some(s),
                Err(s) => return Err(s),
            }
        }
        begin = end;
    }
    p.validate(&hit)?;
    Ok(ReadResult::fail(
        provider_error.unwrap_or(status),
        provider_error.is_none(),
    ))
}

/// Metadata-only ownership walk, shared by COM and regression fixtures.
pub trait OwnershipProvider {
    type Element: Clone;
    fn rejected(&self, _reason: &'static str, _depth: usize) {}
    fn live(&self) -> Result<(), &'static str>;
    fn process_matches(&self, e: &Self::Element) -> Result<bool, &'static str>;
    fn password(&self, e: &Self::Element) -> Result<bool, &'static str>;
    fn root_matches(&self, e: &Self::Element) -> Result<Option<bool>, &'static str>;
    fn parent(&self, e: &Self::Element) -> Result<Self::Element, &'static str>;
    fn same(&self, a: &Self::Element, b: &Self::Element) -> Result<bool, &'static str>;
}
/// Only for the exact element already proven earlier in this request. A final
/// verify_ownership is still required before returning text to the caller.
pub fn recheck_ownership<P: OwnershipProvider>(p: &P, e: &P::Element) -> Result<(), &'static str> {
    ownership_live(p, 0)?;
    if !p.process_matches(e)? {
        p.rejected("process-mismatch", 0);
        return Err("UIA_TARGET_CHANGED");
    }
    if p.password(e)? {
        p.rejected("protected-control", 0);
        return Err("UIA_PROTECTED_CONTROL");
    }
    if p.root_matches(e)? == Some(false) {
        p.rejected("root-mismatch", 0);
        return Err("UIA_TARGET_CHANGED");
    }
    ownership_live(p, 0)
}
pub fn verify_ownership<P: OwnershipProvider>(p: &P, e: &P::Element) -> Result<(), &'static str> {
    let mut node = e.clone();
    let mut visited = Vec::new();
    // Independent of the shallow text-candidate search; still time/size bounded.
    for depth in 0..=32 {
        ownership_live(p, depth)?;
        if !p.process_matches(&node)? {
            p.rejected("process-mismatch", depth);
            return Err("UIA_TARGET_CHANGED");
        }
        if p.password(&node)? {
            p.rejected("protected-control", depth);
            return Err("UIA_PROTECTED_CONTROL");
        }
        for old in &visited {
            ownership_live(p, depth)?;
            if p.same(old, &node)? {
                return Err("UIA_ANCESTRY_CYCLE");
            }
        }
        visited.push(node.clone());
        match p.root_matches(&node)? {
            Some(true) => {
                ownership_live(p, depth)?;
                return Ok(());
            }
            Some(false) => {
                p.rejected("root-mismatch", depth);
                return Err("UIA_TARGET_CHANGED");
            }
            None => {}
        }
        if depth == 32 {
            return Err("UIA_ANCESTRY_LIMIT");
        }
        node = p.parent(&node)?;
    }
    Err("UIA_ANCESTRY_LIMIT")
}

fn ownership_live<P: OwnershipProvider>(p: &P, depth: usize) -> Result<(), &'static str> {
    p.live().inspect_err(|s| {
        p.rejected(
            if *s == "UIA_TIMEOUT" {
                "deadline"
            } else {
                "request-invalid"
            },
            depth,
        )
    })
}
