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
fn compatibility_fallback_keeps_password_consent_and_live_request_guards() {
    for status in [
        "UIA_PATTERN_UNSUPPORTED",
        "UIA_TARGET_CHANGED",
        "UIA_WINDOW_UNVERIFIED",
        "UIA_EMPTY_SELECTION",
    ] {
        assert!(compatibility_fallback_allowed(status, true, true));
        assert!(!compatibility_fallback_allowed(status, false, true));
        assert!(!compatibility_fallback_allowed(status, true, false));
    }
    assert!(!compatibility_fallback_allowed(
        "UIA_PROTECTED_CONTROL",
        true,
        true
    ));
}
#[test]
fn compatibility_focus_accepts_related_cross_process_controls_but_not_unknown_or_own_windows() {
    assert!(compatible_focus_allowed(true, true, 50, 60, 1, false));
    for (valid, related, pid, thread, modal) in [
        (false, true, 50, 60, false),
        (true, false, 50, 60, false),
        (true, true, 0, 60, false),
        (true, true, 50, 0, false),
        (true, true, 1, 60, false),
        (true, true, 50, 60, true),
    ] {
        assert!(!compatible_focus_allowed(
            valid, related, pid, thread, 1, modal
        ));
    }
}
#[test]
fn compatibility_sequence_is_only_change_evidence_and_restore_requires_exact_stamp() {
    let observed = Stamp {
        root_hwnd: 10,
        owner: Target {
            hwnd: 40,
            pid: 50,
            thread: 60,
        },
        sequence: 18,
        format_count: 6,
    };
    assert!(compatibility_source_allowed(7, observed, true));
    assert!(!compatibility_source_allowed(7, observed, false));
    assert!(!compatibility_source_allowed(18, observed, true));
    assert!(!compatibility_source_allowed(0, observed, true));
    assert!(!restore_allowed(
        observed,
        Stamp {
            sequence: 19,
            ..observed
        }
    ));
    assert!(!restore_allowed(
        observed,
        Stamp {
            owner: target(),
            ..observed
        }
    ));
}
#[test]
fn compatibility_rejects_identified_sensitive_controls_without_restricting_custom_editors() {
    for class in [
        "Edit",
        "RichEdit50W",
        "CustomPasswordBox",
        "CredentialInput",
    ] {
        assert!(!compatibility_control_allowed(class, true));
    }
    for class in [
        "WpsCustomEditor",
        "Chrome_RenderWidgetHostHWND",
        "Edit",
        "RichEdit50W",
    ] {
        assert!(compatibility_control_allowed(class, false));
    }
}
#[test]
fn compatibility_clipboard_owner_may_be_related_cross_process_child_or_hidden_helper() {
    let focus = Target {
        hwnd: 40,
        pid: 50,
        thread: 60,
    };
    let mut observed = Stamp {
        root_hwnd: 10,
        owner: focus,
        sequence: 18,
        format_count: 6,
    };
    assert!(compatibility_writer_related(
        target(),
        focus,
        observed,
        10,
        1
    ));
    observed.root_hwnd = 40;
    observed.owner.hwnd = 41;
    assert!(compatibility_writer_related(
        target(),
        focus,
        observed,
        40,
        1
    ));
    observed.owner.pid = 99;
    assert!(!compatibility_writer_related(
        target(),
        focus,
        observed,
        40,
        1
    ));
    observed.owner.pid = 1;
    assert!(!compatibility_writer_related(
        target(),
        focus,
        observed,
        10,
        1
    ));
}
#[test]
fn source_and_restore_reject_e01_external_writer_even_if_text_matches() {
    let t = target();
    let own = Stamp {
        root_hwnd: 10,
        owner: t,
        sequence: 9,
        format_count: 1,
    };
    let foreign = Stamp {
        root_hwnd: 10,
        owner: Target { pid: 99, ..t },
        sequence: 10,
        format_count: 1,
    };
    assert!(source_allowed(t, 7, t.hwnd, own));
    assert!(!source_allowed(t, 7, t.hwnd, foreign));
    assert!(!source_allowed(t, 7, 11, own));
    assert!(!source_allowed(
        t,
        7,
        t.hwnd,
        Stamp {
            sequence: 10,
            ..own
        }
    ));
    assert!(source_allowed(
        t,
        7,
        t.hwnd,
        Stamp {
            sequence: 9,
            format_count: 3,
            ..own
        }
    ));
    assert!(!source_allowed(
        t,
        7,
        t.hwnd,
        Stamp {
            sequence: 10,
            format_count: 3,
            ..own
        }
    ));
    assert!(!source_allowed(
        t,
        7,
        t.hwnd,
        Stamp {
            sequence: 11,
            format_count: 3,
            ..own
        }
    ));
    assert!(!restore_allowed(own, foreign));
    assert!(!restore_allowed(
        own,
        Stamp {
            root_hwnd: 10,
            sequence: 10,
            ..own
        }
    ));
    assert!(!source_allowed(t, 8, t.hwnd, own));
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

fn toolbar(method: Method) -> ToolbarSelection {
    let mut t = ToolbarSelection::empty();
    t.offer(Session::new(
        request(),
        "中文 😀\n  code <&>\n".into(),
        method,
    ));
    t
}
#[test]
fn toolbar_copy_survives_only_its_explicit_native_click_focus_transfer() {
    let mut t = toolbar(Method::Uia);
    assert!(t.arm(Some(target()), false, true, 1, 100));
    assert!(
        t.allows_popup_focus(1, 101),
        "native toolbar click must not revoke its own session"
    );
    t.expire(None, true, true, 1, 102);
    let action = t
        .take_action(1, None, true, true, 1, 110, true)
        .expect("foreground toolbar must not lose the armed session");
    assert_eq!(
        action.into_session(1, 111).unwrap().raw,
        "中文 😀\n  code <&>\n"
    );
    assert!(t.take_action(1, None, true, true, 1, 112, true).is_none());
}
#[test]
fn native_edit_handoff_survives_more_then_settings_and_preserves_exact_text() {
    let mut t = toolbar(Method::NativeEdit);
    assert!(t.arm(Some(target()), false, true, 1, 100));
    assert!(t.take_action(1, None, true, true, 1, 110, false).is_some());
    assert!(t.arm(None, true, true, 1, 120));
    let session = t
        .take_action(1, None, true, true, 1, 130, true)
        .unwrap()
        .into_session(1, 131)
        .unwrap();
    let mut h = SettingsSelection::empty();
    assert!(h.prepare(session, Some(target()), 1, 132, 99));
    assert_eq!(h.preview(99, 133), Some("中文 😀\n  code <&>\n"));
}
#[test]
fn toolbar_unarmed_ipc_background_and_expired_clicks_cannot_consume() {
    let mut t = toolbar(Method::Uia);
    assert!(!t.allows_popup_focus(1, 50));
    assert!(!t.arm(None, true, true, 1, 50));
    assert!(t
        .take_action(1, Some(target()), false, true, 1, 60, true)
        .is_none());
    assert!(t.arm(Some(target()), false, true, 1, 100));
    assert!(t
        .take_action(1, Some(target()), false, true, 1, 1100, true)
        .is_none());
    assert!(!t.arm(Some(target()), false, true, 1, 8010));
}
#[test]
fn toolbar_replacement_old_ticket_and_late_cleanup_do_not_consume_new_selection() {
    let mut t = toolbar(Method::Uia);
    assert!(t.arm(Some(target()), false, true, 1, 100));
    let mut next = request();
    next.generation = 2;
    next.session_id = 2;
    next.created_ms = 150;
    t.offer(Session::new(next, "new".into(), Method::NativeEdit));
    t.clear_if(1);
    assert_eq!(t.ticket(), Some(2));
    assert!(t.arm(Some(target()), false, true, 2, 160));
    assert!(t
        .take_action(1, Some(target()), false, true, 2, 170, true)
        .is_none());
    assert_eq!(
        t.take_action(2, Some(target()), false, true, 2, 180, true)
            .unwrap()
            .into_session(2, 181)
            .unwrap()
            .raw,
        "new"
    );
}
#[test]
fn toolbar_generation_invalid_target_password_guard_and_other_window_fail_closed() {
    for (fg, popup, alive, g, n) in [
        (None, false, true, 1, 120),
        (Some(target()), false, false, 1, 120),
        (Some(target()), false, true, 2, 120),
        (Some(target()), false, true, 1, 8010),
    ] {
        let mut t = toolbar(Method::NativeEdit);
        assert!(t.arm(Some(target()), false, true, 1, 100));
        assert!(t.take_action(1, fg, popup, alive, g, n, true).is_none());
    }
    // The native target guard supplies false for protected/invalid source controls.
    let mut t = toolbar(Method::NativeEdit);
    assert!(!t.arm(Some(target()), false, false, 1, 100));
}
#[test]
fn ordinary_source_window_action_and_settings_revocation_remain_one_shot() {
    let mut t = toolbar(Method::Uia);
    assert!(t.arm(Some(target()), false, true, 1, 100));
    assert!(t
        .take_action(1, Some(target()), false, true, 1, 101, true)
        .is_some());
    let mut h = SettingsSelection::empty();
    assert!(h.prepare(
        Session::new(request(), "old".into(), Method::NativeEdit),
        Some(target()),
        1,
        100,
        99
    ));
    let old = h.ticket().unwrap();
    h.clear();
    assert!(h.take(99, old, 101).is_none());
    assert!(h.prepare(
        Session::new(request(), "new".into(), Method::Uia),
        Some(target()),
        1,
        102,
        99
    ));
    assert!(h.take(99, old, 103).is_none());
    assert_eq!(h.preview(99, 104), Some("new"));
}

#[test]
fn native_action_is_delegated_once_without_requiring_the_source_to_remain_foreground() {
    let mut t = toolbar(Method::NativeEdit);
    assert!(t.arm(Some(target()), false, true, 1, 100));
    let action = t.take_action(1, None, true, true, 1, 110, true).unwrap();
    let mut h = SettingsSelection::empty();
    assert!(h.prepare_action(action, 1, 111, 99));
    let ticket = h.ticket().unwrap();
    assert_eq!(
        h.take(99, ticket, 112).as_deref(),
        Some("中文 😀\n  code <&>\n")
    );
    assert!(h.take(99, ticket, 113).is_none());
    assert!(!Session::new(request(), "synthetic".into(), Method::NativeEdit).usable(None, 1, 111));
}
#[test]
fn action_cannot_delegate_after_a_new_generation_or_expiry() {
    for (generation, now) in [(2, 111), (1, 8010)] {
        let mut t = toolbar(Method::Uia);
        assert!(t.arm(Some(target()), false, true, 1, 100));
        let a = t.take_action(1, None, true, true, 1, 110, true).unwrap();
        assert!(!SettingsSelection::empty().prepare_action(a, generation, now, 99));
    }
}
#[test]
fn external_windows_wrong_labels_hidden_pages_and_ai_pages_cannot_invoke_toolbar_actions() {
    let local = tauri::Url::parse("http://tauri.localhost/poc/popup.html").unwrap();
    assert!(local_document_page("popup", &local, "popup"));
    assert!(local_page("popup", &local, "popup", true));
    assert!(!local_page("popup", &local, "popup", false));
    assert!(!local_document_page("panel", &local, "popup"));
    assert!(!local_page("panel", &local, "popup", true));
    for u in [
        "https://chat.example/poc/popup.html",
        "http://tauri.localhost/poc/settings.html",
        "http://tauri.localhost.evil/poc/popup.html",
    ] {
        assert!(!local_page(
            "popup",
            &tauri::Url::parse(u).unwrap(),
            "popup",
            true
        ));
    }
}

#[test]
fn popup_waits_for_page_load_and_exact_frontend_ticket_before_showing() {
    let mut page = PopupPageState::default();
    assert_eq!(page.request(41), PopupPageStep::Wait);
    assert_eq!(page.page_finished(), Some(41));
    assert_eq!(page.request(41), PopupPageStep::Prepare);
    assert!(!page.acknowledge(40, Some(41)));
    assert_eq!(page.request(41), PopupPageStep::Wait);
    assert!(page.acknowledge(41, Some(41)));
    assert_eq!(page.request(41), PopupPageStep::Show);
    assert_eq!(page.request(41), PopupPageStep::Wait);
}

#[test]
fn stale_popup_acknowledgement_cannot_show_a_replacement_selection() {
    let mut page = PopupPageState::default();
    page.page_started();
    assert_eq!(page.request(41), PopupPageStep::Wait);
    assert_eq!(page.page_finished(), Some(41));
    assert_eq!(page.request(41), PopupPageStep::Prepare);
    assert_eq!(page.request(42), PopupPageStep::Prepare);
    assert!(!page.acknowledge(41, Some(42)));
    assert_eq!(page.request(42), PopupPageStep::Wait);
    assert!(page.acknowledge(42, Some(42)));
    assert_eq!(page.request(42), PopupPageStep::Show);
}

#[test]
fn cancelled_popup_preparation_cannot_be_reused() {
    let mut page = PopupPageState::default();
    page.page_finished();
    assert_eq!(page.request(41), PopupPageStep::Prepare);
    page.cancel();
    assert!(!page.acknowledge(41, Some(41)));
    assert_eq!(page.request(41), PopupPageStep::Prepare);
}

// Characterization of the whole pure Session/Ticket chain, not a clipboard GUI PASS.
#[test]
fn copy_ticket_keeps_exact_full_or_partial_selection_without_normalizing_raw() {
    for method in [Method::Uia, Method::NativeEdit] {
        for raw in [
            "  N01 Unicode Edit synthetic 中文 😀.\r\n  code <&> preserved.  ",
            "  中文 😀\n    code <&>  ",
            "N01 Unicode Edit synthetic 中文 😀.",
        ] {
            let mut t = ToolbarSelection::empty();
            let s = Session::new(request(), raw.into(), method);
            assert_eq!(
                s.raw.encode_utf16().collect::<Vec<_>>(),
                raw.encode_utf16().collect::<Vec<_>>()
            );
            t.offer(s);
            assert!(t.arm(Some(target()), false, true, 1, 100));
            let s = t
                .take_action(1, Some(target()), false, true, 1, 101, true)
                .unwrap()
                .into_session(1, 102)
                .unwrap();
            assert_eq!(s.raw, raw);
            assert_eq!(
                s.raw.encode_utf16().collect::<Vec<_>>(),
                raw.encode_utf16().collect::<Vec<_>>()
            );
            assert!(t
                .take_action(1, Some(target()), false, true, 1, 103, true)
                .is_none());
        }
    }
}

#[test]
fn diagnostic_rejections_agree_with_action_gate_without_consuming_anything() {
    for (ticket, fg, popup, alive, g, n, reason) in [
        (1, Some(target()), false, true, 1, 101, "accepted"),
        (2, Some(target()), false, true, 1, 101, "ticket-replaced"),
        (
            1,
            Some(target()),
            false,
            true,
            1,
            1100,
            "native-click-expired",
        ),
        (1, Some(target()), false, false, 1, 101, "source-dead"),
        (1, Some(target()), false, true, 2, 101, "session-stale"),
        (1, None, false, true, 1, 101, "foreground-changed"),
    ] {
        let mut t = toolbar(Method::NativeEdit);
        assert_eq!(
            t.action_rejection(1, Some(target()), false, true, 1, 100),
            "missing-native-arm"
        );
        assert!(t.arm(Some(target()), false, true, 1, 100));
        assert_eq!(t.action_rejection(ticket, fg, popup, alive, g, n), reason);
        assert_eq!(
            t.take_action(ticket, fg, popup, alive, g, n, true)
                .is_some(),
            reason == "accepted"
        );
    }
}

#[test]
fn show_time_popup_focus_must_not_expire_before_first_native_click() {
    let mut state = ToolbarSelection::empty();
    state.offer(Session::new(
        request(),
        "synthetic selection".into(),
        Method::NativeEdit,
    ));
    state.begin_show(1, popup_target(), 1, 99).unwrap();
    // show() can temporarily foreground the popup before the first pointer-down.
    state.expire(None, true, true, 1, 100);
    assert_eq!(state.ticket(), Some(1));
    assert!(state.arm(None, true, true, 1, 101));
    assert!(state
        .take_action(1, None, true, true, 1, 102, true)
        .is_some());
}

fn popup_target() -> Target {
    Target {
        hwnd: 99,
        pid: 100,
        thread: 101,
    }
}
fn showing() -> (ToolbarSelection, PopupShow) {
    let mut s = toolbar(Method::NativeEdit);
    let show = s.begin_show(7, popup_target(), 1, 100).unwrap();
    (s, show)
}
#[test]
fn show_return_is_exact_identity_bounded_and_not_an_action_grant() {
    let (s, show) = showing();
    assert!(s.can_return_focus(show, Some(popup_target()), true, 1, 101));
    for fg in [
        Some(target()),
        None,
        Some(Target {
            hwnd: 98,
            ..popup_target()
        }),
        Some(Target {
            pid: 999,
            ..popup_target()
        }),
    ] {
        assert!(!s.can_return_focus(show, fg, true, 1, 101));
    }
    assert!(!s.can_return_focus(show, Some(popup_target()), false, 1, 101));
    assert!(!s.can_return_focus(show, Some(popup_target()), true, 2, 101));
    assert!(!s.can_return_focus(show, Some(popup_target()), true, 1, 450));
    let (mut s, _) = showing();
    assert!(s.take_action(1, None, true, true, 1, 101, true).is_none());
}
#[test]
fn show_survives_only_popup_or_source_not_unrelated_own_windows() {
    for fg in [
        Some(Target {
            hwnd: 97,
            ..popup_target()
        }),
        None,
    ] {
        let (mut s, _) = showing();
        s.expire(fg, false, true, 1, 101);
        assert_eq!(s.ticket(), None);
    }
    let (mut s, _) = showing();
    s.expire(Some(target()), false, true, 1, 101);
    assert_eq!(s.ticket(), Some(1));
    s.expire(None, true, true, 1, 102); // verified popup WebView activation
    assert_eq!(s.ticket(), Some(1));
}
#[test]
fn popup_foreground_keeps_selection_actionable_after_show_focus_lease_expires() {
    let (mut state, show) = showing();
    assert!(state.mark_popup_shown(show));
    state.finish_show(show);

    state.expire(None, true, true, 1, 500);
    assert_eq!(state.ticket(), Some(1));
    assert!(state.arm(None, true, true, 1, 501));
    assert!(state
        .take_action(1, None, true, true, 1, 502, true)
        .is_some());
}

#[test]
fn verified_click_on_shown_toolbar_survives_transient_foreground_mismatch() {
    let (mut state, show) = showing();
    assert!(state.mark_popup_shown(show));
    state.finish_show(show);

    assert!(state.arm_verified_popup_click(true, 1, 501));
    assert_eq!(
        state.action_rejection(1, None, false, true, 1, 502),
        "accepted"
    );
    state.expire(None, false, true, 1, 503);
    assert_eq!(state.ticket(), Some(1));
    assert!(state
        .take_action(1, None, false, true, 1, 504, true)
        .is_some());
}

#[test]
fn stale_foreground_notification_does_not_invalidate_current_selection() {
    assert!(!foreground_event_should_invalidate(101, 202, false));
    assert!(foreground_event_should_invalidate(202, 202, false));
    assert!(!foreground_event_should_invalidate(202, 202, true));
}

#[test]
fn verified_popup_click_does_not_survive_generation_change() {
    let (mut state, show) = showing();
    assert!(state.mark_popup_shown(show));
    state.finish_show(show);
    assert!(state.arm_verified_popup_click(true, 1, 501));

    assert_eq!(
        state.action_rejection(1, None, false, true, 2, 502),
        "session-stale"
    );
    state.expire(None, false, true, 2, 502);
    assert_eq!(state.ticket(), None);
}

#[test]
fn expired_native_down_cannot_act_but_a_new_verified_down_can_rearm() {
    let (mut state, show) = showing();
    assert!(state.mark_popup_shown(show));
    state.finish_show(show);
    assert!(state.arm_verified_popup_click(true, 1, 501));

    assert_eq!(
        state.action_rejection(1, None, false, true, 1, 1501),
        "native-click-expired"
    );
    assert!(state
        .take_action(1, None, false, true, 1, 1501, true)
        .is_none());
    assert!(state.arm_verified_popup_click(true, 1, 1502));
    assert!(state
        .take_action(1, None, false, true, 1, 1503, true)
        .is_some());
}

#[test]
fn shown_popup_foreground_event_is_preserved_after_show_lease_expires() {
    let (mut state, show) = showing();
    assert!(state.mark_popup_shown(show));
    state.finish_show(show);

    let popup_focus_allowed = state.allows_popup_focus(1, 500);
    assert!(popup_focus_allowed);
    assert!(state.retains(None, popup_focus_allowed, true, 1, 500));
    assert!(!state.allows_popup_focus(2, 500));
    assert!(!state.allows_popup_focus(1, 8_100));
}
#[test]
fn shown_popup_session_still_revokes_on_focus_loss_death_generation_change_or_expiry() {
    for (foreground, popup, alive, generation, now) in [
        (
            Some(Target {
                hwnd: 97,
                ..popup_target()
            }),
            false,
            true,
            1,
            500,
        ),
        (None, false, true, 1, 500),
        (None, true, false, 1, 500),
        (None, true, true, 2, 500),
        (None, true, true, 1, 8100),
    ] {
        let (mut state, show) = showing();
        assert!(state.mark_popup_shown(show));
        state.finish_show(show);
        state.expire(foreground, popup, alive, generation, now);
        assert_eq!(state.ticket(), None);
    }
}
#[test]
fn old_show_and_timer_cannot_change_replacement_or_restore_focus() {
    let (mut s, old) = showing();
    let mut r = request();
    r.generation = 2;
    r.session_id = 2;
    r.created_ms = 110;
    s.offer(Session::new(r, "new selection".into(), Method::Uia));
    let new = s.begin_show(8, popup_target(), 2, 120).unwrap();
    s.finish_show(old);
    s.clear_if(old.ticket); // old auto-close's ticket guard
    assert_eq!(s.current_show(2, 121), Some(new));
    assert!(!s.can_return_focus(old, Some(popup_target()), true, 2, 121));
    assert_eq!(s.ticket(), Some(2));
}
#[test]
fn hide_failure_source_death_generation_change_and_expiry_end_show() {
    for (alive, gen, now) in [(false, 1, 101), (true, 2, 101), (true, 1, 8010)] {
        let (mut s, old) = showing();
        s.expire(None, true, alive, gen, now);
        assert_eq!(s.ticket(), None);
        assert!(!s.can_return_focus(old, Some(popup_target()), true, gen, now));
    }
    for fail in [true, false] {
        let (mut s, old) = showing();
        if fail {
            s.clear_if(old.ticket);
        } else {
            s.clear();
        }
        assert_eq!(s.current_show(1, 101), None);
    }
}
#[test]
fn click_and_settings_handoff_end_show_and_do_not_allow_repeat_consumption() {
    let (mut s, old) = showing();
    assert!(s.arm(None, true, true, 1, 101));
    assert!(!s.can_return_focus(old, Some(popup_target()), true, 1, 102));
    let action = s.take_action(1, None, true, true, 1, 103, true).unwrap();
    assert!(s.take_action(1, None, true, true, 1, 104, true).is_none());
    let mut settings = SettingsSelection::empty();
    assert!(settings.prepare_action(action, 1, 105, 200));
    assert_eq!(settings.preview(200, 106), Some("中文 😀\n  code <&>\n"));
    assert_eq!(s.current_show(1, 106), None);
}
#[test]
fn finished_show_cannot_be_reused_and_unarmed_popup_expires() {
    let (mut s, show) = showing();
    s.finish_show(show);
    assert!(!s.can_return_focus(show, Some(popup_target()), true, 1, 101));
    s.expire(None, true, true, 1, 102);
    assert_eq!(s.ticket(), None);
}

#[test]
fn same_ticket_new_show_rejects_old_return_and_old_completion() {
    let (mut s, old) = showing();
    let new = s.begin_show(8, popup_target(), 1, 110).unwrap();
    s.finish_show(old);
    assert_eq!(s.current_show(1, 111), Some(new));
    assert!(!s.can_return_focus(old, Some(popup_target()), true, 1, 111));
    assert!(s.can_return_focus(new, Some(popup_target()), true, 1, 111));
}
#[test]
fn malformed_popup_and_stale_source_cannot_start_show() {
    for popup in [
        Target {
            hwnd: 0,
            ..popup_target()
        },
        Target {
            pid: 0,
            ..popup_target()
        },
        Target {
            thread: 0,
            ..popup_target()
        },
        target(),
    ] {
        let mut s = toolbar(Method::Uia);
        assert!(s.begin_show(7, popup, 1, 100).is_none());
    }
    let mut s = toolbar(Method::Uia);
    assert!(s.begin_show(7, popup_target(), 2, 100).is_none());
    assert!(s.begin_show(7, popup_target(), 1, 8010).is_none());
}

#[test]
fn capture_read_requires_permission_before_and_after_provider_return() {
    use std::cell::Cell;
    let permitted = Cell::new(true);
    let calls = Cell::new(0);
    let read = || {
        calls.set(calls.get() + 1);
        "  中文 😀\r\n  code  "
    };
    assert_eq!(
        read_current(|| permitted.get(), read),
        Some("  中文 😀\r\n  code  ")
    );
    assert_eq!(calls.get(), 1);
    permitted.set(false);
    assert!(read_current(|| permitted.get(), read).is_none());
    assert_eq!(
        calls.get(),
        1,
        "disabled capture must not invoke a provider"
    );
    permitted.set(true);
    assert!(read_current(
        || permitted.get(),
        || {
            permitted.set(false);
            "late text"
        }
    )
    .is_none());
}

#[test]
fn capture_disabled_then_reenabled_cannot_revive_a_previous_generation() {
    use std::cell::Cell;
    let current = Cell::new(1);
    let r = request();
    assert!(read_current(
        || r.valid(Some(target()), current.get(), 100),
        || {
            current.set(2);
            "late synthetic text"
        }
    )
    .is_none());
}

#[test]
fn uia_success_short_circuits_native_and_clipboard_fallbacks() {
    let result = capture_first_available(
        Some("uia"),
        || panic!("native read must not run after UIA succeeds"),
        true,
        || panic!("clipboard fallback must not run after UIA succeeds"),
    );
    assert_eq!(result, Some((Method::Uia, "uia")));
}

#[test]
fn absent_safe_uia_text_still_short_circuits_after_native_success() {
    let order = std::cell::RefCell::new(Vec::new());
    let result = capture_first_available(
        None,
        || {
            order.borrow_mut().push("native");
            Some("native")
        },
        true,
        || {
            order.borrow_mut().push("clipboard");
            Some("clipboard")
        },
    );
    assert_eq!(result, Some((Method::NativeEdit, "native")));
    assert_eq!(*order.borrow(), ["native"]);
}

#[test]
fn native_edit_success_short_circuits_clipboard_fallback() {
    let result = capture_first_available(
        None,
        || Some("native"),
        true,
        || panic!("clipboard fallback must not run after native read succeeds"),
    );
    assert_eq!(result, Some((Method::NativeEdit, "native")));
}

#[test]
fn unconsented_capture_never_invokes_clipboard_fallback() {
    use std::cell::Cell;
    let calls = Cell::new(0);
    let result = capture_first_available(
        None,
        || None::<&str>,
        false,
        || {
            calls.set(calls.get() + 1);
            Some("clipboard")
        },
    );
    assert_eq!(result, None);
    assert_eq!(calls.get(), 0);
}

#[test]
fn authorized_clipboard_fallback_runs_only_after_uia_and_native_fail() {
    use std::cell::RefCell;
    let order = RefCell::new(Vec::new());
    let result = capture_first_available(
        None::<&str>,
        || {
            order.borrow_mut().push("native");
            None
        },
        true,
        || {
            order.borrow_mut().push("clipboard");
            Some("clipboard")
        },
    );
    assert_eq!(result, Some((Method::Clipboard, "clipboard")));
    assert_eq!(*order.borrow(), ["native", "clipboard"]);
}
