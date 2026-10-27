#[path = "../src/release_policy.rs"]
#[allow(dead_code)]
mod release_policy;

#[test]
fn runtime_authorization_never_unlocks_a_distribution_build() {
    let expected = cfg!(all(
        debug_assertions,
        not(feature = "custom-protocol"),
        feature = "experimental-clipboard-fallback",
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
fn legacy_config_and_backup_true_cannot_unlock_legacy_copy() {
    for requested in [false, true] {
        assert!(!release_policy::automatic_copy_allowed(requested));
    }
    assert!(release_policy::validate_automatic_copy_request(true).is_err());
}

#[test]
fn every_copy_entry_and_side_effect_is_guarded_in_the_backend() {
    let src = include_str!("../src/safe_copy.rs");
    for start in [
        "pub fn request_authorization(",
        "fn epoch()",
        "unsafe fn snapshot()",
        "unsafe fn copy_chord()",
        "fn capture_inner(",
        "fn restore(",
    ] {
        let body = &src[src.find(start).expect(start)..];
        assert!(
            body[..body.find("experimental_copy_available()").unwrap()].len() < 240,
            "unguarded {start}"
        );
    }
    assert!(include_str!("../src/capture.rs")
        .contains("if !crate::release_policy::automatic_copy_allowed(true)"));
}
