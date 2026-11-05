//! Pure selection policy. No Win32 calls, clipboard access or persisted grants.
pub const POLICY_VERSION: u32 = 3;
pub const MAX_RANGES: usize = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub hwnd: isize,
    pub pid: u32,
    pub thread: u32,
}
pub(crate) fn mouse_down_target(
    foreground: Option<Target>,
    hit: Option<Target>,
    own_pid: u32,
) -> Option<Target> {
    foreground
        .filter(|target| target.pid != own_pid)
        .or_else(|| hit.filter(|target| target.pid != own_pid))
}
pub(crate) fn foreground_event_should_invalidate(
    event_hwnd: isize,
    current_foreground: isize,
    preserved: bool,
) -> bool {
    event_hwnd == current_foreground && !preserved
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
/// Keep capture methods ordered. The clipboard path is invoked only after the
/// caller has independently established a safe target and explicit consent.
pub fn capture_first_available<T>(
    uia: Option<T>,
    native: impl FnOnce() -> Option<T>,
    fallback_allowed: bool,
    fallback: impl FnOnce() -> Option<T>,
) -> Option<(Method, T)> {
    if let Some(value) = uia {
        return Some((Method::Uia, value));
    }
    if let Some(value) = native() {
        return Some((Method::NativeEdit, value));
    }
    fallback_allowed
        .then(fallback)
        .flatten()
        .map(|value| (Method::Clipboard, value))
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
    pub format_count: u32,
}
/// Compatibility attribution combines a changed sequence with a separately verified
/// writer relationship. It is bounded evidence, not proof of writer causality.
pub fn compatibility_source_allowed(before: u32, stamp: Stamp, related_writer: bool) -> bool {
    stamp.format_count != 0 && compatibility_observation_allowed(before, stamp, related_writer)
}

/// A verified writer may briefly own an empty clipboard while publishing formats.
/// This is only a pending observation, never a readable/restorable copy result.
pub fn compatibility_observation_allowed(before: u32, stamp: Stamp, related_writer: bool) -> bool {
    related_writer
        && before != 0
        && stamp.sequence != 0
        && stamp.sequence != before
        && stamp.owner.hwnd != 0
        && stamp.owner.pid != 0
        && stamp.owner.thread != 0
        && stamp.format_count <= 64
}

pub fn compatibility_fallback_allowed(status: &str, authorized: bool, live: bool) -> bool {
    authorized && live && status != "UIA_PROTECTED_CONTROL"
}
pub fn compatible_focus_allowed(
    valid: bool,
    related: bool,
    pid: u32,
    thread: u32,
    own_pid: u32,
    modal: bool,
) -> bool {
    valid && related && pid != 0 && thread != 0 && pid != own_pid && !modal
}
pub fn compatibility_control_allowed(class: &str, password_style: bool) -> bool {
    let class = class.to_ascii_lowercase();
    !class.contains("password")
        && !class.contains("credential")
        && !((class == "edit" || class.starts_with("richedit")) && password_style)
}
pub fn compatibility_writer_related(
    target: Target,
    focus: Target,
    stamp: Stamp,
    owner_root: isize,
    own_pid: u32,
) -> bool {
    stamp.owner.pid != own_pid
        && (stamp.root_hwnd == target.hwnd
            || owner_root == target.hwnd
            || stamp.owner.pid == target.pid
            || stamp.owner.pid == focus.pid)
}
pub fn source_allowed(target: Target, before: u32, focused_control: isize, stamp: Stamp) -> bool {
    target.hwnd != 0
        && target.pid != 0
        && target.thread != 0
        && focused_control != 0
        && stamp.owner.hwnd != 0
        && stamp.owner.hwnd == focused_control
        && stamp.root_hwnd == target.hwnd
        && stamp.owner.pid == target.pid
        && stamp.owner.thread == target.thread
        && before != 0
        && stamp.sequence != 0
        && (1..=64).contains(&stamp.format_count)
        // EnumClipboardFormats includes synthesized text formats, which do not
        // correspond one-to-one with SetClipboardData calls. Accept only the
        // exact EmptyClipboard + single-format write transaction.
        && stamp.sequence.wrapping_sub(before) == 2
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
/// False means unsupported formats, not a failed or incomplete snapshot.
/// Invalid enumeration metadata remains a preflight error even with consent.
pub fn snapshot_recoverable(formats: &[u32]) -> Result<bool, &'static str> {
    if formats.len() > 64
        || formats
            .iter()
            .enumerate()
            .any(|(i, f)| *f == 0 || formats[..i].contains(f))
    {
        return Err("COPY_SNAPSHOT_FAILED");
    }
    Ok(formats_allowed(formats))
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
/// Native pointer-down proof, distinct from an ordinary/background selection request.
pub struct ToolbarAction {
    session: Session,
}
impl ToolbarAction {
    pub(crate) fn session(&self) -> &Session {
        &self.session
    }
    pub fn into_session(self, generation: u64, now: u64) -> Option<Session> {
        self.session
            .usable(Some(self.session.request.target), generation, now)
            .then_some(self.session)
    }
}
/// A short, exact-identity lease for one native popup show, never an action grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PopupShow {
    pub ticket: u64,
    pub generation: u64,
    pub source: Target,
    pub popup: Target,
    pub serial: u64,
    started: u64,
    deadline: u64,
}
#[derive(Default)]
pub struct ToolbarSelection {
    pending: Option<Session>,
    armed: Option<(u64, u64)>,
    interacting: bool,
    verified_popup_click: bool,
    showing: Option<PopupShow>,
    shown_ticket: Option<u64>,
}
impl ToolbarSelection {
    pub const fn empty() -> Self {
        Self {
            pending: None,
            armed: None,
            interacting: false,
            verified_popup_click: false,
            showing: None,
            shown_ticket: None,
        }
    }
    pub fn offer(&mut self, session: Session) {
        self.pending = Some(session);
        self.armed = None;
        self.interacting = false;
        self.verified_popup_click = false;
        self.showing = None;
        self.shown_ticket = None;
    }
    pub fn begin_show(
        &mut self,
        serial: u64,
        popup: Target,
        generation: u64,
        now: u64,
    ) -> Option<PopupShow> {
        self.showing = None;
        let s = self.pending.as_ref()?;
        if serial == 0
            || popup.hwnd == 0
            || popup.pid == 0
            || popup.thread == 0
            || popup.hwnd == s.request.target.hwnd
            || !s.usable(Some(s.request.target), generation, now)
        {
            return None;
        }
        let show = PopupShow {
            ticket: s.request.session_id,
            generation,
            source: s.request.target,
            popup,
            serial,
            started: now,
            deadline: now.saturating_add(350),
        };
        self.showing = Some(show);
        Some(show)
    }
    pub fn current_show(&self, generation: u64, now: u64) -> Option<PopupShow> {
        let show = self.showing?;
        (self.ticket() == Some(show.ticket)
            && self.target() == Some(show.source)
            && show.generation == generation
            && now >= show.started
            && now < show.deadline
            && self
                .pending
                .as_ref()?
                .usable(Some(show.source), generation, now))
        .then_some(show)
    }
    pub fn finish_show(&mut self, show: PopupShow) {
        if self.showing == Some(show) {
            self.showing = None;
        }
    }
    pub fn mark_popup_shown(&mut self, show: PopupShow) -> bool {
        if self.ticket() == Some(show.ticket) && (self.showing == Some(show) || self.interacting) {
            self.shown_ticket = Some(show.ticket);
            true
        } else {
            false
        }
    }
    pub fn can_return_focus(
        &self,
        show: PopupShow,
        foreground: Option<Target>,
        alive: bool,
        generation: u64,
        now: u64,
    ) -> bool {
        !self.interacting
            && alive
            && self.current_show(generation, now) == Some(show)
            && foreground == Some(show.popup)
    }
    pub fn target(&self) -> Option<Target> {
        self.pending.as_ref().map(|s| s.request.target)
    }
    pub fn ticket(&self) -> Option<u64> {
        self.pending.as_ref().map(|s| s.request.session_id)
    }
    fn has_live_native_click(&self, now: u64) -> bool {
        self.armed.is_some_and(|(ticket, at)| {
            self.verified_popup_click
                && self.interacting
                && self.ticket() == Some(ticket)
                && now >= at
                && now - at < 1000
        })
    }
    pub fn clear(&mut self) {
        self.pending = None;
        self.armed = None;
        self.interacting = false;
        self.verified_popup_click = false;
        self.showing = None;
        self.shown_ticket = None;
    }
    pub(crate) fn action_rejection(
        &self,
        ticket: u64,
        foreground: Option<Target>,
        popup: bool,
        alive: bool,
        generation: u64,
        now: u64,
    ) -> &'static str {
        let Some((armed, at)) = self.armed else {
            return "missing-native-arm";
        };
        if armed != ticket || self.ticket() != Some(ticket) {
            return "ticket-replaced";
        }
        if now < at || now - at >= 1000 {
            return "native-click-expired";
        }
        if !alive {
            return "source-dead";
        }
        let Some(s) = &self.pending else {
            return "missing-session";
        };
        if !s.usable(Some(s.request.target), generation, now) {
            return "session-stale";
        }
        if foreground != Some(s.request.target)
            && !(popup && self.interacting)
            && !self.has_live_native_click(now)
        {
            return "foreground-changed";
        }
        "accepted"
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
                    && (foreground == Some(s.request.target)
                        || popup
                            && (self.interacting
                                || self.shown_ticket == Some(s.request.session_id)
                                || self.current_show(generation, now).is_some())
                        || self.verified_popup_click)
            })
    }
    #[cfg(test)]
    #[allow(dead_code)] // Used by the source-included selection_core integration fixture.
    pub fn arm(
        &mut self,
        foreground: Option<Target>,
        popup: bool,
        alive: bool,
        generation: u64,
        now: u64,
    ) -> bool {
        self.armed = None;
        self.verified_popup_click = false;
        if !self.retains(foreground, popup, alive, generation, now) {
            return false;
        }
        self.interacting = true;
        self.showing = None; // A real native click ends all scheduled show-time focus returns.
        self.armed = self.ticket().map(|t| (t, now));
        true
    }
    pub fn arm_verified_popup_click(&mut self, alive: bool, generation: u64, now: u64) -> bool {
        self.armed = None;
        self.verified_popup_click = false;
        let shown_ticket = self
            .ticket()
            .is_some_and(|ticket| self.shown_ticket == Some(ticket));
        let current_session_is_usable = alive
            && self.pending.as_ref().is_some_and(|session| {
                session.usable(Some(session.request.target), generation, now)
            });
        if !shown_ticket || !current_session_is_usable {
            return false;
        }
        self.interacting = true;
        self.showing = None;
        self.verified_popup_click = true;
        self.armed = self.ticket().map(|ticket| (ticket, now));
        self.armed.is_some()
    }
    pub fn allows_popup_focus(&self, generation: u64, now: u64) -> bool {
        self.pending.as_ref().is_some_and(|s| {
            s.usable(Some(s.request.target), generation, now)
                && (self.interacting
                    || self.shown_ticket == Some(s.request.session_id)
                    || self.current_show(generation, now).is_some())
        })
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
        if now < at || now - at >= 1000 || !self.retains(foreground, popup, alive, generation, now)
        {
            self.armed = None;
            return None;
        }
        self.armed = None;
        let session = if consume {
            self.showing = None;
            self.interacting = false;
            self.verified_popup_click = false;
            self.shown_ticket = None;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupPageStep {
    Wait,
    Prepare,
    Show,
}

/// Keep the native toolbar hidden until its WebView has installed the current ticket.
#[derive(Default)]
pub struct PopupPageState {
    loaded: bool,
    ticket: Option<u64>,
    preparing: bool,
    prepared: bool,
    displayed: Option<u64>,
}
impl PopupPageState {
    pub const fn empty() -> Self {
        Self {
            loaded: false,
            ticket: None,
            preparing: false,
            prepared: false,
            displayed: None,
        }
    }
    pub fn page_started(&mut self) {
        self.loaded = false;
        self.preparing = false;
        self.prepared = false;
    }
    pub fn page_finished(&mut self) -> Option<u64> {
        self.loaded = true;
        self.ticket
    }
    pub fn request(&mut self, ticket: u64) -> PopupPageStep {
        if ticket == 0 || self.displayed == Some(ticket) {
            return PopupPageStep::Wait;
        }
        if self.ticket != Some(ticket) {
            self.ticket = Some(ticket);
            self.preparing = false;
            self.prepared = false;
        }
        if !self.loaded {
            return PopupPageStep::Wait;
        }
        if self.prepared {
            self.ticket = None;
            self.prepared = false;
            self.displayed = Some(ticket);
            return PopupPageStep::Show;
        }
        if self.preparing {
            return PopupPageStep::Wait;
        }
        self.preparing = true;
        PopupPageStep::Prepare
    }
    pub fn acknowledge(&mut self, ticket: u64, current_ticket: Option<u64>) -> bool {
        if self.loaded
            && self.preparing
            && self.ticket == Some(ticket)
            && current_ticket == Some(ticket)
        {
            self.preparing = false;
            self.prepared = true;
            true
        } else {
            false
        }
    }
    pub fn preparation_failed(&mut self, ticket: u64) {
        if self.ticket == Some(ticket) && self.preparing {
            self.preparing = false;
        }
    }
    pub fn cancel(&mut self) {
        self.ticket = None;
        self.preparing = false;
        self.prepared = false;
        self.displayed = None;
    }
}

/// Local app documents only; never authorize an external AI page to consume a toolbar session.
pub fn local_document_page(label: &str, url: &tauri::Url, expected: &str) -> bool {
    if label != expected {
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
pub fn local_page(label: &str, url: &tauri::Url, expected: &str, visible: bool) -> bool {
    visible && local_document_page(label, url, expected)
}

/// Gate a potentially blocking read with the current permission/request on both sides.
/// None is a rejection, not a successful empty selection.
pub fn read_current<T>(mut current: impl FnMut() -> bool, read: impl FnOnce() -> T) -> Option<T> {
    if !current() {
        return None;
    }
    let result = read();
    current().then_some(result)
}

#[cfg(test)]
mod mouse_down_target_tests {
    use super::{mouse_down_target, Target};

    fn target(hwnd: isize, pid: u32, thread: u32) -> Target {
        Target { hwnd, pid, thread }
    }

    #[test]
    fn falls_back_to_external_hit_window_during_app_to_document_activation() {
        let document = target(10, 20, 30);
        assert_eq!(mouse_down_target(None, Some(document), 1), Some(document));
    }

    #[test]
    fn preserves_an_existing_external_foreground_identity() {
        let foreground = target(10, 20, 30);
        let hit = target(11, 21, 31);
        assert_eq!(
            mouse_down_target(Some(foreground), Some(hit), 1),
            Some(foreground)
        );
    }

    #[test]
    fn rejects_an_application_owned_hit_window_when_no_external_foreground_exists() {
        let own_window = target(10, 1, 30);
        assert_eq!(mouse_down_target(None, Some(own_window), 1), None);
    }
}
