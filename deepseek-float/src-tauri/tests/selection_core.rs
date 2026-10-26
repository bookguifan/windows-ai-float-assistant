// This integration test imports the complete pure module; other tests exercise its remaining API.
#[allow(dead_code)]
#[path = "../src/selection.rs"]
mod selection;
use selection::*;

fn target() -> Target {
    Target {
        hwnd: 10,
        pid: 20,
        thread: 30,
    }
}
fn request() -> Request {
    Request {
        target: target(),
        point: (1, 2),
        generation: 1,
        session_id: 1,
        created_ms: 10,
        expires_ms: 510,
    }
}

#[test]
fn request_requires_same_window_pid_thread_generation_and_time() {
    let r = request();
    assert!(r.valid(Some(target()), 1, 100));
    for t in [
        Target {
            hwnd: 11,
            ..target()
        },
        Target {
            pid: 21,
            ..target()
        },
        Target {
            thread: 31,
            ..target()
        },
    ] {
        assert!(!r.valid(Some(t), 1, 100));
    }
    assert!(!r.valid(None, 1, 100));
    assert!(!r.valid(Some(target()), 2, 100));
    assert!(!r.valid(Some(target()), 1, 511));
}
#[test]
fn queue_keeps_only_latest_pending_request() {
    let mut q = Latest::default();
    q.replace(request());
    let mut next = request();
    next.generation = 2;
    q.replace(next.clone());
    assert_eq!(q.take().unwrap().generation, 2);
    assert!(q.take().is_none());
}
#[test]
fn ranges_keep_indent_unicode_and_equal_text_in_different_ranges() {
    assert_eq!(
        join_ranges(
            &["第一行\n  code 😀".into(), "相同".into(), "相同".into()],
            100
        )
        .unwrap(),
        "第一行\n  code 😀\n相同\n相同"
    );
    assert!(join_ranges(&vec!["x".into(); 33], 100).is_err());
    assert!(join_ranges(&["   ".into()], 100).is_err());
    assert_eq!(join_ranges(&["a😀b".into()], 2).unwrap(), "a😀");
}
#[test]
fn authorizations_never_follow_preferences_or_legacy_config() {
    let mut a = Consent::default();
    assert!(!a.allows());
    a.grant(POLICY_VERSION - 1);
    assert!(!a.allows());
    a.grant(POLICY_VERSION);
    assert!(a.allows());
    let epoch = a.epoch();
    a.revoke();
    assert!(!a.allows());
    a.grant(POLICY_VERSION);
    assert!(!a.matches(epoch));
}
#[test]
fn source_and_restore_reject_e01_external_writer_even_if_text_matches() {
    let t = target();
    let own = Stamp {
        root_hwnd: 10,
        owner: t,
        sequence: 8,
    };
    let foreign = Stamp {
        root_hwnd: 10,
        owner: Target { pid: 99, ..t },
        sequence: 9,
    };
    assert!(source_allowed(t, 7, own));
    assert!(!source_allowed(t, 7, foreign));
    assert!(!restore_allowed(own, foreign));
    assert!(!restore_allowed(
        own,
        Stamp {
            root_hwnd: 10,
            sequence: 9,
            ..own
        }
    ));
    assert!(!source_allowed(t, 8, own));
}
#[test]
fn snapshot_rejects_rich_unknown_and_incomplete_text_formats() {
    assert!(formats_allowed(&[]));
    assert!(formats_allowed(&[13, 1, 7, 16]));
    for formats in [&[2][..], &[13, 49152], &[13, 15], &[1], &[13, 13]] {
        assert!(!formats_allowed(formats));
    }
}
#[test]
fn session_requires_current_request_and_preserves_raw() {
    let s = Session::new(request(), "line\n  indent".into(), Method::Uia);
    assert!(s.usable(Some(target()), 1, 100));
    assert!(!s.usable(Some(target()), 2, 100));
    assert_eq!(s.raw, "line\n  indent");
    assert_eq!(s.normalized, "line indent");
}

#[test]
fn native_copy_guard_rejects_password_and_virtual_controls() {
    assert!(copy_control_allowed("edit", 0));
    assert!(copy_control_allowed("richedit50w", 0));
    assert!(!copy_control_allowed("edit", 0x20));
    assert!(!copy_control_allowed("richedit50w", 0x20));
    for class in [
        "Chrome_RenderWidgetHostHWND",
        "CASCADIA_HOSTING_WINDOW_CLASS",
        "custom",
        "button",
    ] {
        assert!(!copy_control_allowed(class, 0));
    }
}
#[test]
fn composition_preserves_raw_newlines_and_whitespace() {
    let raw = "  code\n\tindent 😀\n";
    assert_eq!(compose("解释", raw), format!("解释\n\n{raw}"));
    assert_eq!(compose("处理 {text}", raw), format!("处理 {raw}"));
    let s = Session::new(request(), "\u{FEFF}  word".into(), Method::Clipboard);
    assert_eq!(s.normalized, "word");
    assert_eq!(s.method, Method::Clipboard);
}
#[test]
fn blocked_worker_only_leaves_one_latest_pending_request() {
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;
    let slot = Arc::new(Mutex::new(Latest::empty()));
    slot.lock().unwrap().replace(request());
    let (started, ready) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let worker_slot = slot.clone();
    let worker = std::thread::spawn(move || {
        let active = worker_slot.lock().unwrap().take().unwrap();
        started.send(active.generation).unwrap();
        gate.recv_timeout(Duration::from_secs(2)).unwrap();
        worker_slot.lock().unwrap().take().unwrap().generation
    });
    assert_eq!(ready.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
    for generation in 2..=1000 {
        let mut next = request();
        next.generation = generation;
        slot.lock().unwrap().replace(next);
    }
    release.send(()).unwrap();
    assert_eq!(worker.join().unwrap(), 1000);
    assert!(slot.lock().unwrap().take().is_none());
}
#[test]
fn timeout_and_window_return_cannot_revive_an_old_generation() {
    let r = request();
    assert!(!r.valid(Some(target()), 2, 100));
    assert!(!r.valid(Some(target()), 1, r.expires_ms));
    let s = Session::new(r, "synthetic".into(), Method::Uia);
    assert!(!s.usable(Some(target()), 1, 8010));
}

#[test]
fn ordinary_selection_must_still_reject_settings_as_source_window() {
    let s = Session::new(request(), "synthetic selection".into(), Method::Uia);
    assert!(!s.usable(
        Some(Target {
            hwnd: 99,
            pid: 98,
            thread: 97
        }),
        1,
        100
    ));
}
#[test]
fn explicit_settings_handoff_preserves_raw_and_is_single_use() {
    let mut h = SettingsSelection::default();
    let raw = "代码\n  indent 😀\n";
    assert!(h.prepare(
        Session::new(request(), raw.into(), Method::Uia),
        Some(target()),
        1,
        100,
        99
    ));
    assert_eq!(h.preview(99, 101), Some(raw));
    assert_eq!(h.take(99, 1, 102).as_deref(), Some(raw));
    assert!(h.take(99, 1, 103).is_none());
}
#[test]
fn handoff_rejects_stale_or_different_origin_without_weakening_normal_session() {
    let mut h = SettingsSelection::default();
    for (t, g, n) in [
        (Some(target()), 2, 100),
        (None, 1, 100),
        (Some(target()), 1, 8010),
    ] {
        assert!(!h.prepare(
            Session::new(request(), "synthetic".into(), Method::Uia),
            t,
            g,
            n,
            99
        ));
        assert!(h.preview(99, n).is_none());
    }
}
#[test]
fn handoff_is_bound_to_destination_and_expires_at_deadline() {
    let mut h = SettingsSelection::default();
    assert!(h.prepare(
        Session::new(request(), "synthetic".into(), Method::Uia),
        Some(target()),
        1,
        100,
        99
    ));
    assert!(h.take(100, 1, 101).is_none());
    assert_eq!(h.preview(99, 101), Some("synthetic"));
    assert!(h.preview(99, 60100).is_none());
    assert!(h.expire(60100));
    assert!(h.take(99, 1, 60101).is_none());
}
#[test]
fn clear_revoke_close_or_focus_loss_cannot_replay_handoff() {
    let mut h = SettingsSelection::default();
    assert!(h.prepare(
        Session::new(request(), "synthetic".into(), Method::Uia),
        Some(target()),
        1,
        100,
        99
    ));
    assert!(h.clear());
    assert!(h.take(99, 1, 101).is_none());
    assert!(!h.clear());
}
#[test]
fn replacement_never_reuses_previous_selection_and_invalid_replacement_clears_it() {
    let mut h = SettingsSelection::default();
    assert!(h.prepare(
        Session::new(request(), "old".into(), Method::Uia),
        Some(target()),
        1,
        100,
        99
    ));
    assert!(h.prepare(
        Session::new(request(), "new".into(), Method::Uia),
        Some(target()),
        1,
        101,
        99
    ));
    assert_eq!(h.preview(99, 102), Some("new"));
    assert!(!h.prepare(
        Session::new(request(), "invalid".into(), Method::Uia),
        None,
        1,
        103,
        99
    ));
    assert!(h.take(99, 1, 104).is_none());
}
#[test]
fn invalid_destination_empty_or_excessive_text_cannot_create_transfer() {
    let mut h = SettingsSelection::default();
    for (text, dest) in [
        ("valid".to_string(), 0),
        (" ".to_string(), 99),
        ("x".repeat(20001), 99),
    ] {
        assert!(!h.prepare(
            Session::new(request(), text, Method::Uia),
            Some(target()),
            1,
            100,
            dest
        ));
    }
}

#[test]
fn old_settings_ticket_cannot_consume_replacement_snapshot() {
    let mut h = SettingsSelection::default();
    assert!(h.prepare(
        Session::new(request(), "old".into(), Method::Uia),
        Some(target()),
        1,
        100,
        99
    ));
    let old = h.ticket().unwrap();
    assert!(h.prepare(
        Session::new(request(), "new".into(), Method::Uia),
        Some(target()),
        1,
        101,
        99
    ));
    let new = h.ticket().unwrap();
    assert_ne!(old, new);
    assert!(h.take(99, old, 102).is_none());
    assert_eq!(h.preview(99, 102), Some("new"));
    assert_eq!(h.take(99, new, 103).as_deref(), Some("new"));
}
#[test]
fn cancellation_revision_increases_and_ticket_is_not_reused() {
    let mut h = SettingsSelection::default();
    h.prepare(
        Session::new(request(), "first".into(), Method::Uia),
        Some(target()),
        1,
        100,
        99,
    );
    let first = h.ticket().unwrap();
    h.clear();
    let cancelled = h.revision();
    assert!(cancelled > first);
    h.prepare(
        Session::new(request(), "second".into(), Method::Uia),
        Some(target()),
        1,
        101,
        99,
    );
    assert!(h.ticket().unwrap() > cancelled);
}
