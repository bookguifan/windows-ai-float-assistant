// Pure guards/selection orchestration only. No native windows or clipboard writes.
#[path = "../src/copy_engine.rs"]
#[allow(dead_code)]
mod copy_engine;
#[path = "../src/native_copy_proof.rs"]
#[allow(dead_code)]
mod native_copy_proof;
#[path = "../src/selection.rs"]
#[allow(dead_code)]
mod selection;
use copy_engine::SelectionProof;
use std::cell::Cell;
fn proof(text: &str) -> SelectionProof {
    SelectionProof {
        control: 1,
        start: 0,
        end: text.encode_utf16().count() as u32,
        text: text.into(),
    }
}
fn request() -> selection::Request {
    selection::Request {
        target: selection::Target {
            hwnd: 1,
            pid: 2,
            thread: 3,
        },
        point: (0, 0),
        generation: 4,
        session_id: 1,
        created_ms: 10,
        expires_ms: 20,
    }
}
#[test]
fn exact_native_text_keeps_unicode_newlines_and_indentation() {
    let text = "中文😀\r\n  code <&>";
    let mut reads = 0;
    assert_eq!(
        native_copy_proof::read_stable(
            || true,
            || {
                reads += 1;
                Ok(proof(text))
            }
        )
        .unwrap(),
        text
    );
    assert_eq!(reads, 2);
}
#[test]
fn password_ansi_richedit_and_custom_controls_are_denied() {
    assert!(native_copy_proof::control_metadata_allowed(
        "Edit", true, false
    ));
    for (class, unicode, password) in [
        ("Edit", true, true),
        ("Edit", false, false),
        ("RichEdit50W", true, false),
        ("custom", true, false),
    ] {
        assert!(!native_copy_proof::control_metadata_allowed(
            class, unicode, password
        ));
    }
}
#[test]
fn cross_window_expired_generation_and_permission_reject_before_read() {
    let r = request();
    let other = selection::Target {
        hwnd: 99,
        ..r.target
    };
    for (target, generation, now, enabled) in [
        (Some(other), 4, 11, true),
        (Some(r.target), 4, 20, true),
        (Some(r.target), 5, 11, true),
        (Some(r.target), 4, 11, false),
    ] {
        let reads = Cell::new(0);
        assert!(native_copy_proof::read_stable(
            || enabled && r.valid(target, generation, now),
            || {
                reads.set(reads.get() + 1);
                Ok(proof("synthetic"))
            }
        )
        .is_err());
        assert_eq!(reads.get(), 0);
    }
}
#[test]
fn permission_loss_after_first_read_discards_result() {
    let reads = Cell::new(0);
    assert!(native_copy_proof::read_stable(
        || reads.get() == 0,
        || {
            reads.set(reads.get() + 1);
            Ok(proof("synthetic"))
        }
    )
    .is_err());
    assert_eq!(reads.get(), 1);
}
#[test]
fn same_length_text_change_and_control_change_discard_result() {
    for change_control in [false, true] {
        let mut reads = 0;
        assert!(native_copy_proof::read_stable(
            || true,
            || {
                reads += 1;
                let mut p = proof(if reads == 2 && !change_control {
                    "other text"
                } else {
                    "first text"
                });
                if reads == 2 && change_control {
                    p.control = 2;
                }
                Ok(p)
            }
        )
        .is_err());
    }
}
#[test]
fn native_path_has_no_clipboard_or_synthetic_input_calls() {
    let native = include_str!("../src/native_selection.rs");
    let proof = include_str!("../src/native_copy_proof.rs");
    for source in [native, proof] {
        for forbidden in [
            "SendInput",
            "OpenClipboard",
            "EmptyClipboard",
            "SetClipboardData",
            "copy_chord",
            "safe_copy::",
        ] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
    }
    let runtime = include_str!("../src/selection_runtime.rs");
    assert!(
        runtime.find("native_selection::capture").unwrap()
            < runtime.find("safe_copy::capture").unwrap()
    );
}
