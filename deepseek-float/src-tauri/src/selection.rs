//! Pure selection policy. No Win32 calls, clipboard access or persisted grants.
pub const POLICY_VERSION: u32 = 1;
pub const MAX_RANGES: usize = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub hwnd: isize,
    pub pid: u32,
    pub thread: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub target: Target,
    pub point: (i32, i32),
    pub generation: u64,
    pub session_id: u64,
    pub created_ms: u64,
    pub expires_ms: u64,
}
impl Request {
    pub fn valid(&self, target: Option<Target>, generation: u64, now: u64) -> bool {
        self.target.hwnd != 0
            && self.target.pid != 0
            && self.target.thread != 0
            && target == Some(self.target)
            && generation == self.generation
            && now >= self.created_ms
            && now < self.expires_ms
    }
}
#[derive(Default)]
pub struct Latest(Option<Request>);
impl Latest {
    pub const fn empty() -> Self {
        Self(None)
    }
    pub fn replace(&mut self, request: Request) {
        self.0 = Some(request);
    }
    pub fn take(&mut self) -> Option<Request> {
        self.0.take()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Uia,
    NativeEdit,
    Clipboard,
}
#[derive(Clone)]
pub struct Session {
    pub request: Request,
    pub raw: String,
    pub normalized: String,
    pub method: Method,
}
impl Session {
    pub fn new(request: Request, raw: String, method: Method) -> Self {
        let normalized = raw
            .split(|c: char| c.is_whitespace() || c == '\u{FEFF}')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        Self {
            request,
            raw,
            normalized,
            method,
        }
    }
    pub fn usable(&self, target: Option<Target>, generation: u64, now: u64) -> bool {
        // Capturing has a short deadline; successful sessions live at most eight seconds.
        let mut request = self.request.clone();
        request.expires_ms = request.created_ms.saturating_add(8000);
        request.valid(target, generation, now)
    }
}
/// Explicit one-shot delegation to a single settings window, never persisted.
#[derive(Default)]
pub struct SettingsSelection {
    grant: Option<SettingsGrant>,
    revision: u64,
}
struct SettingsGrant {
    window: isize,
    issued_ms: u64,
    expires_ms: u64,
    raw: String,
}
impl SettingsSelection {
    pub const fn empty() -> Self {
        Self {
            grant: None,
            revision: 0,
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn ticket(&self) -> Option<u64> {
        self.grant.as_ref().map(|_| self.revision)
    }
    pub fn prepare(
        &mut self,
        session: Session,
        target: Option<Target>,
        generation: u64,
        now: u64,
        window: isize,
    ) -> bool {
        self.clear();
        if self.revision == u64::MAX
            || window == 0
            || !session.usable(target, generation, now)
            || session.normalized.chars().count() < 2
            || session.raw.chars().count() > 20_000
        {
            return false;
        }
        self.grant = Some(SettingsGrant {
            window,
            issued_ms: now,
            expires_ms: now.saturating_add(60_000),
            raw: session.raw,
        });
        true
    }
    pub fn prepare_action(
        &mut self,
        action: ToolbarAction,
        generation: u64,
        now: u64,
        window: isize,
    ) -> bool {
        let Some(session) = action.into_session(generation, now) else {
            self.clear();
            return false;
        };
        let source = session.request.target;
        self.prepare(session, Some(source), generation, now, window)
    }
    pub fn preview(&self, window: isize, now: u64) -> Option<&str> {
        self.grant
            .as_ref()
            .filter(|g| g.window == window && now >= g.issued_ms && now < g.expires_ms)
            .map(|g| g.raw.as_str())
    }
    pub fn take(&mut self, window: isize, ticket: u64, now: u64) -> Option<String> {
        self.expire(now);
        if self.ticket() != Some(ticket) {
            return None;
        }
        self.preview(window, now)?;
        let raw = self.grant.take()?.raw;
        self.revision = self.revision.saturating_add(1);
        Some(raw)
    }
    pub fn clear(&mut self) -> bool {
        let had = self.grant.take().is_some();
        self.revision = self.revision.saturating_add(1);
        had
    }
    pub fn expire(&mut self, now: u64) -> bool {
        if self
            .grant
            .as_ref()
            .is_some_and(|g| now < g.issued_ms || now >= g.expires_ms)
        {
            self.clear()
        } else {
            false
        }
    }
}
#[derive(Default)]
pub struct Consent {
    version: u32,
    epoch: u64,
    enabled: bool,
}
impl Consent {
    pub const fn empty() -> Self {
        Self {
            version: 0,
            epoch: 0,
            enabled: false,
        }
    }
    pub fn grant(&mut self, version: u32) {
        self.epoch = self.epoch.wrapping_add(1);
        self.version = version;
        self.enabled = true;
    }
    pub fn revoke(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.enabled = false;
    }
    pub fn allows(&self) -> bool {
        self.enabled && self.version == POLICY_VERSION
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn matches(&self, epoch: u64) -> bool {
        self.allows() && self.epoch == epoch
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub root_hwnd: isize,
    pub owner: Target,
    pub sequence: u32,
}
pub fn source_allowed(target: Target, before: u32, stamp: Stamp) -> bool {
    target.hwnd != 0
        && target.pid != 0
        && target.thread != 0
        && stamp.owner.hwnd != 0
        && stamp.root_hwnd == target.hwnd
        && stamp.owner.pid == target.pid
        && stamp.owner.thread == target.thread
        && before != 0
        && stamp.sequence != 0
        && stamp.sequence != before
}
pub fn restore_allowed(expected: Stamp, current: Stamp) -> bool {
    expected == current && expected.sequence != 0
}
pub fn formats_allowed(formats: &[u32]) -> bool {
    formats.is_empty()
        || (formats.contains(&13)
            && formats.len() <= 4
            && formats.iter().all(|f| matches!(f, 1 | 7 | 13 | 16))
            && formats
                .iter()
                .enumerate()
                .all(|(i, f)| !formats[..i].contains(f)))
}
pub fn join_ranges(parts: &[String], max: usize) -> Result<String, &'static str> {
    if parts.len() > MAX_RANGES || max == 0 {
        return Err("UIA_INTERNAL_ERROR");
    }
    let mut out = String::new();
    let mut count = 0;
    for part in parts.iter().filter(|p| !p.trim().is_empty()) {
        if !out.is_empty() && count < max {
            out.push('\n');
            count += 1;
        }
        for ch in part.chars().take(max.saturating_sub(count)) {
            out.push(ch);
            count += 1;
        }
        if count == max {
            break;
        }
    }
    if out.trim().is_empty() {
        Err("UIA_EMPTY_SELECTION")
    } else {
        Ok(out)
    }
}
pub fn compose(instruction: &str, raw: &str) -> String {
    let instruction = instruction.trim();
    if instruction.contains("{text}") {
        instruction.replace("{text}", raw)
    } else if instruction.is_empty() {
        raw.into()
    } else {
        format!("{instruction}\n\n{raw}")
    }
}

/// ES_PASSWORD has defined meaning here only for the verified native edit classes.
pub fn copy_control_allowed(class: &str, style: u32) -> bool {
    (class == "edit" || class.starts_with("richedit")) && style & 0x20 == 0
}

/// Native pointer-down proof, distinct from an ordinary/background selection request.
pub struct ToolbarAction {
    session: Session,
}
impl ToolbarAction {
    pub fn into_session(self, generation: u64, now: u64) -> Option<Session> {
        self.session
            .usable(Some(self.session.request.target), generation, now)
            .then_some(self.session)
    }
}
#[derive(Default)]
pub struct ToolbarSelection {
    pending: Option<Session>,
    armed: Option<(u64, u64)>,
    interacting: bool,
}
impl ToolbarSelection {
    pub const fn empty() -> Self {
        Self {
            pending: None,
            armed: None,
            interacting: false,
        }
    }
    pub fn offer(&mut self, session: Session) {
        self.pending = Some(session);
        self.armed = None;
        self.interacting = false;
    }
    pub fn target(&self) -> Option<Target> {
        self.pending.as_ref().map(|s| s.request.target)
    }
    pub fn ticket(&self) -> Option<u64> {
        self.pending.as_ref().map(|s| s.request.session_id)
    }
    pub fn clear(&mut self) {
        self.pending = None;
        self.armed = None;
        self.interacting = false;
    }
    pub fn clear_if(&mut self, ticket: u64) {
        if self.ticket() == Some(ticket) {
            self.clear();
        }
    }
    pub fn retains(
        &self,
        foreground: Option<Target>,
        popup: bool,
        alive: bool,
        generation: u64,
        now: u64,
    ) -> bool {
        alive
            && self.pending.as_ref().is_some_and(|s| {
                s.usable(Some(s.request.target), generation, now)
                    && (foreground == Some(s.request.target) || popup && self.interacting)
            })
    }
    pub fn arm(
        &mut self,
        foreground: Option<Target>,
        popup: bool,
        alive: bool,
        generation: u64,
        now: u64,
    ) -> bool {
        self.armed = None;
        if !self.retains(foreground, popup, alive, generation, now) {
            return false;
        }
        self.interacting = true;
        self.armed = self.ticket().map(|t| (t, now));
        true
    }
    pub fn allows_popup_focus(&self, generation: u64, now: u64) -> bool {
        self.interacting
            && self
                .pending
                .as_ref()
                .is_some_and(|s| s.usable(Some(s.request.target), generation, now))
    }
    #[allow(clippy::too_many_arguments)] // Explicit independent inputs of the pure native action gate.
    pub fn take_action(
        &mut self,
        ticket: u64,
        foreground: Option<Target>,
        popup: bool,
        alive: bool,
        generation: u64,
        now: u64,
        consume: bool,
    ) -> Option<ToolbarAction> {
        let (armed, at) = self.armed?;
        if armed != ticket || self.ticket() != Some(ticket) {
            return None;
        }
        self.armed = None;
        if now < at || now - at >= 1000 || !self.retains(foreground, popup, alive, generation, now)
        {
            return None;
        }
        let session = if consume {
            self.pending.take()?
        } else {
            self.pending.as_ref()?.clone()
        };
        Some(ToolbarAction { session })
    }
    pub fn expire(
        &mut self,
        foreground: Option<Target>,
        popup: bool,
        alive: bool,
        generation: u64,
        now: u64,
    ) {
        if !self.retains(foreground, popup, alive, generation, now) {
            self.clear();
        }
    }
}

/// Local app documents only; never authorize an external AI page to consume a toolbar session.
pub fn local_page(label: &str, url: &tauri::Url, expected: &str, visible: bool) -> bool {
    if label != expected || !visible {
        return false;
    }
    let app = url.scheme() == "tauri" && url.host_str() == Some("localhost")
        || matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost");
    let dev = cfg!(debug_assertions)
        && url.scheme() == "http"
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
        && url.port() == Some(5173);
    (app || dev) && url.path() == format!("/poc/{expected}.html")
}
