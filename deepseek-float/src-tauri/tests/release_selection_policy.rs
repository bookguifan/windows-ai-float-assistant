#[path = "../src/release_policy.rs"]
#[allow(dead_code)]
mod release_policy;

#[test]
fn release_build_exposes_only_the_guarded_copy_path() {
    let expected = cfg!(all(
        not(debug_assertions),
        feature = "custom-protocol",
        target_os = "windows"
    )) || cfg!(all(
        debug_assertions,
        not(feature = "custom-protocol"),
        target_os = "windows"
    ));
    for authorized in [false, true] {
        for target in [false, true] {
            for live in [false, true] {
                assert_eq!(
                    release_policy::safe_copy_allowed(authorized, target, live),
                    expected && authorized && target && live
                );
            }
        }
    }
}

#[test]
fn ordinary_release_builds_the_guarded_path_without_enabling_it_by_config() {
    let ordinary_release = cfg!(all(
        not(debug_assertions),
        feature = "custom-protocol",
        target_os = "windows"
    ));
    let expected = ordinary_release
        || cfg!(all(
            debug_assertions,
            not(feature = "custom-protocol"),
            target_os = "windows"
        ));
    assert_eq!(release_policy::safe_copy_available(), expected);
    assert!(!release_policy::automatic_copy_allowed(true));
    assert!(!release_policy::safe_copy_allowed(false, true, true));
    assert!(!release_policy::safe_copy_allowed(true, false, true));
    assert!(!release_policy::safe_copy_allowed(true, true, false));
}

#[test]
fn ordinary_release_does_not_enable_research_or_isolation_features() {
    if cfg!(all(
        not(debug_assertions),
        feature = "custom-protocol",
        target_os = "windows"
    )) {
        assert!(!std::hint::black_box(cfg!(
            feature = "experimental-clipboard-fallback"
        )));
        assert!(!std::hint::black_box(cfg!(feature = "beta2-isolation")));
    }
}

#[test]
fn legacy_config_and_backup_true_cannot_unlock_legacy_copy() {
    for requested in [false, true] {
        assert!(!release_policy::automatic_copy_allowed(requested));
    }
    assert!(release_policy::validate_automatic_copy_request(true).is_err());
}
#[test]
fn ordinary_release_excludes_the_diagnostic_command_and_handler() {
    let guard = "#[cfg(any(debug_assertions, feature = \"beta2-isolation\"))]";
    let ipc = include_str!("../src/ipc.rs");
    assert!(ipc.contains(&format!("{guard}\n#[tauri::command]\npub fn popup_trace(")));
    let lifecycle = include_str!("../src/lifecycle.rs");
    assert!(lifecycle.contains(&format!("{guard}\n            crate::ipc::popup_trace,")));
    assert!(lifecycle.contains(
        "#[cfg(feature = \"beta2-isolation\")]\n            crate::ipc::ball_diagnostic,"
    ));
}

#[test]
fn every_copy_entry_and_side_effect_is_guarded_in_the_backend() {
    let src = include_str!("../src/safe_copy.rs");
    for start in [
        "pub fn request_authorization(",
        "fn epoch()",
        "unsafe fn snapshot()",
        "unsafe fn copy_chord(",
        "fn capture_inner(",
        "fn restore(",
    ] {
        let body = &src[src.find(start).expect(start)..];
        assert!(
            body[..body.find("safe_copy_available()").unwrap()].len() < 240,
            "unguarded {start}"
        );
    }
    assert!(include_str!("../src/capture.rs")
        .contains("if !crate::release_policy::automatic_copy_allowed(true)"));
}
