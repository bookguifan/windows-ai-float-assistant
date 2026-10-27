// Other suites exercise the remaining pure selection API.
#[path = "../src/copy_engine.rs"]
mod copy_engine;
#[allow(dead_code)]
#[path = "../src/selection.rs"]
mod selection;
use copy_engine::*;
use selection::*;
fn target() -> Target {
    Target {
        hwnd: 10,
        pid: 20,
        thread: 30,
    }
}
#[derive(Default)]
struct Fake {
    formats: Vec<u32>,
    busy: bool,
    delayed: bool,
    valid: bool,
    sent: u32,
    copies: u32,
    reads: u32,
    restored: u32,
    stamp: Option<Stamp>,
    race_read: bool,
    race_restore: bool,
    timeout: bool,
    revoke: bool,
    partial_restore: bool,
    empty_text: bool,
    competing_text: bool,
    proof_calls: u32,
    proof_error: bool,
    proof_change_at: u32,
    invalid_proof: bool,
}
impl Fake {
    fn good() -> Self {
        Self {
            formats: vec![13],
            valid: true,
            sent: 4,
            stamp: Some(Stamp {
                root_hwnd: 10,
                owner: target(),
                sequence: 8,
            }),
            ..Default::default()
        }
    }
}
impl ClipboardIo for Fake {
    type Snapshot = ();
    fn valid(&self) -> bool {
        self.valid
    }
    fn selection_proof(&mut self) -> Result<SelectionProof, &'static str> {
        self.proof_calls += 1;
        if self.proof_error {
            return Err("COPY_SELECTION_UNVERIFIED");
        }
        let text = "合成\n  code 😀".to_string();
        Ok(SelectionProof {
            control: if self.invalid_proof { 0 } else { 10 },
            start: if self.proof_calls == self.proof_change_at {
                1
            } else {
                0
            },
            end: text.encode_utf16().count() as u32,
            text,
        })
    }
    fn prepare(&mut self) -> Result<((), u32), &'static str> {
        if self.busy || self.delayed || !formats_allowed(&self.formats) {
            Err("COPY_FORMAT_UNSAFE")
        } else {
            Ok(((), 7))
        }
    }
    fn sequence(&self) -> u32 {
        7
    }
    fn send_once(&mut self) -> bool {
        self.copies += 1;
        if self.revoke {
            self.valid = false;
        }
        self.sent == 4
    }
    fn wait(&mut self, _: u32) -> Result<Stamp, &'static str> {
        if self.timeout {
            Err("COPY_TIMEOUT")
        } else {
            self.stamp.ok_or("COPY_SOURCE_UNVERIFIED")
        }
    }
    fn read(&mut self, _: Stamp) -> Result<String, &'static str> {
        self.reads += 1;
        if self.race_read {
            Err("COPY_SOURCE_UNVERIFIED")
        } else {
            Ok(if self.competing_text {
                "SYNTHETIC COMPETITOR TEXT".into()
            } else if self.empty_text {
                String::new()
            } else {
                "合成\n  code 😀".into()
            })
        }
    }
    fn restore(&mut self, _: (), _: Stamp) -> Result<(), &'static str> {
        if self.race_restore || self.partial_restore {
            Err("COPY_RESTORE_SKIPPED")
        } else {
            self.restored += 1;
            Ok(())
        }
    }
}
#[test]
fn unconsented_context_never_snapshots_or_sends() {
    let mut f = Fake::good();
    f.valid = false;
    assert!(run_copy(&mut f, target()).is_err());
    assert_eq!(f.copies, 0);
    assert_eq!(f.reads, 0);
}
#[test]
fn unicode_and_all_text_variants_restore_once() {
    for formats in [vec![], vec![13], vec![13, 1, 7, 16]] {
        let mut f = Fake {
            formats,
            ..Fake::good()
        };
        assert_eq!(run_copy(&mut f, target()).unwrap(), "合成\n  code 😀");
        assert_eq!((f.copies, f.reads, f.restored), (1, 1, 1));
    }
}
#[test]
fn unsafe_snapshots_never_send() {
    for formats in [vec![2], vec![13, 49152], vec![13, 15], vec![1]] {
        let mut f = Fake {
            formats,
            ..Fake::good()
        };
        assert!(run_copy(&mut f, target()).is_err());
        assert_eq!(f.copies, 0);
    }
}
#[test]
fn busy_and_delayed_snapshots_never_send() {
    for (busy, delayed) in [(true, false), (false, true)] {
        let mut f = Fake {
            busy,
            delayed,
            ..Fake::good()
        };
        assert!(run_copy(&mut f, target()).is_err());
        assert_eq!(f.copies, 0);
    }
}
#[test]
fn partial_input_and_timeout_never_retry_or_restore() {
    for (sent, timeout) in [(0, false), (2, false), (4, true)] {
        let mut f = Fake {
            sent,
            timeout,
            ..Fake::good()
        };
        assert!(run_copy(&mut f, target()).is_err());
        assert_eq!(f.copies, 1);
        assert_eq!(f.reads, 0);
        assert_eq!(f.restored, 0);
    }
}
#[test]
fn e01_foreign_owner_is_rejected_before_reading() {
    let mut f = Fake {
        stamp: Some(Stamp {
            root_hwnd: 10,
            owner: Target {
                pid: 99,
                ..target()
            },
            sequence: 8,
        }),
        ..Fake::good()
    };
    assert!(run_copy(&mut f, target()).is_err());
    assert_eq!(f.reads, 0);
    assert_eq!(f.restored, 0);
}
#[test]
fn unchanged_sequence_and_unknown_owner_never_return_old_text() {
    for stamp in [
        None,
        Some(Stamp {
            root_hwnd: 10,
            owner: target(),
            sequence: 7,
        }),
    ] {
        let mut f = Fake {
            stamp,
            ..Fake::good()
        };
        assert!(run_copy(&mut f, target()).is_err());
        assert_eq!(f.reads, 0);
        assert_eq!(f.restored, 0);
    }
}
#[test]
fn external_writes_at_read_or_restore_never_overwrite() {
    for (race_read, race_restore) in [(true, false), (false, true)] {
        let mut f = Fake {
            race_read,
            race_restore,
            ..Fake::good()
        };
        assert!(run_copy(&mut f, target()).is_err());
        assert_eq!(f.restored, 0);
    }
}
#[test]
fn revoke_in_flight_does_not_restore_or_return_text() {
    let mut f = Fake {
        revoke: true,
        ..Fake::good()
    };
    assert!(run_copy(&mut f, target()).is_err());
    assert_eq!(f.copies, 1);
    assert_eq!(f.reads, 0);
    assert_eq!(f.restored, 0);
}
#[test]
fn restore_failure_is_not_reported_as_success() {
    let mut f = Fake {
        partial_restore: true,
        ..Fake::good()
    };
    assert!(run_copy(&mut f, target()).is_err());
    assert_eq!(f.restored, 0);
}

#[test]
fn same_process_different_top_level_owner_is_unverified() {
    let mut f = Fake::good();
    f.stamp.as_mut().unwrap().root_hwnd = 99;
    assert!(run_copy(&mut f, target()).is_err());
    assert_eq!(f.reads, 0);
    assert_eq!(f.restored, 0);
}

#[test]
fn empty_copy_does_not_match_nonempty_proof_and_is_not_restored() {
    let mut f = Fake {
        empty_text: true,
        ..Fake::good()
    };
    assert!(run_copy(&mut f, target()).is_err());
    assert_eq!(f.restored, 0);
}

#[test]
fn same_owner_competing_text_is_rejected_before_restoration() {
    let mut f = Fake {
        competing_text: true,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_SELECTION_MISMATCH"));
    assert_eq!(f.restored, 0);
}
#[test]
fn same_root_sibling_competing_text_is_rejected_before_restoration() {
    let mut f = Fake {
        competing_text: true,
        ..Fake::good()
    };
    f.stamp.as_mut().unwrap().owner.hwnd = 11;
    assert_eq!(run_copy(&mut f, target()), Err("COPY_SELECTION_MISMATCH"));
    assert_eq!(f.restored, 0);
}

#[test]
fn missing_or_invalid_selection_proof_never_copies() {
    for (proof_error, invalid_proof) in [(true, false), (false, true)] {
        let mut f = Fake {
            proof_error,
            invalid_proof,
            ..Fake::good()
        };
        assert_eq!(run_copy(&mut f, target()), Err("COPY_SELECTION_UNVERIFIED"));
        assert_eq!((f.copies, f.reads, f.restored), (0, 0, 0));
    }
}
#[test]
fn changed_selection_before_send_never_copies() {
    let mut f = Fake {
        proof_change_at: 2,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_SELECTION_CHANGED"));
    assert_eq!((f.copies, f.reads, f.restored), (0, 0, 0));
}
#[test]
fn changed_selection_after_read_never_restores_or_returns_text() {
    let mut f = Fake {
        proof_change_at: 3,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_SELECTION_CHANGED"));
    assert_eq!((f.copies, f.reads, f.restored), (1, 1, 0));
}
