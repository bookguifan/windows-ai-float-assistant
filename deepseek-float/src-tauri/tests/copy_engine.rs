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
    compatibility: bool,
    related_writer: bool,
    formats: Vec<u32>,
    busy: bool,
    snapshot_rejected: bool,
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
    read_fail: bool,
    empty_text: bool,
    competing_text: bool,
    proof_calls: u32,
    proof_error: bool,
    proof_change_at: u32,
    invalid_proof: bool,
    restore_attempts: u32,
    wait_calls: u32,
    live_sequence: u32,
    wait_error: Option<&'static str>,
    change_during_wait: bool,
    unrestorable_notices: u32,
    recovery_attempts: u32,
}
impl Fake {
    fn good() -> Self {
        Self {
            related_writer: true,
            formats: vec![13],
            valid: true,
            sent: 4,
            stamp: Some(Stamp {
                root_hwnd: 10,
                owner: target(),
                sequence: 9,
                format_count: 1,
            }),
            live_sequence: 7,
            ..Default::default()
        }
    }
}

#[test]
fn complex_formats_never_read_payloads_and_snapshot_errors_cannot_degrade_to_discard() {
    for formats in [vec![13, 0xc000], vec![13, 0xc001], vec![2], vec![15]] {
        let plan = prepare_snapshot::<()>(&formats, || {
            panic!("must not render complex clipboard data")
        });
        assert!(matches!(plan, Ok(PreparedSnapshot::Unrecoverable)));
    }
    for error in [
        "COPY_SNAPSHOT_FAILED",
        "COPY_SNAPSHOT_INVALID",
        "COPY_SNAPSHOT_UNSTABLE",
        "COPY_CLIPBOARD_BUSY",
    ] {
        assert!(matches!(prepare_snapshot::<()>(&[13], || Err(error)), Err(code) if code == error));
    }
    assert!(matches!(
        prepare_snapshot::<()>(&[0], || panic!("invalid metadata must fail first")),
        Err("COPY_SNAPSHOT_FAILED")
    ));
}

#[test]
fn compatibility_copies_new_text_without_a_native_selection_proof() {
    let mut f = Fake {
        compatibility: true,
        proof_error: true,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()).unwrap(), "合成\n  code 😀");
    assert_eq!((f.proof_calls, f.copies, f.restored), (0, 1, 1));
}

#[test]
fn compatibility_accepts_related_cross_process_writer_and_multiple_format_writes() {
    let mut f = Fake {
        compatibility: true,
        proof_error: true,
        ..Fake::good()
    };
    let s = f.stamp.as_mut().unwrap();
    s.owner = Target {
        hwnd: 40,
        pid: 50,
        thread: 60,
    };
    s.sequence = 18;
    s.format_count = 6;
    assert!(run_copy(&mut f, target()).is_ok());
    assert_eq!((f.copies, f.restored), (1, 1));
}

#[test]
fn compatibility_never_adopts_or_restores_an_unrelated_writer() {
    let mut f = Fake {
        compatibility: true,
        related_writer: false,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_SOURCE_UNVERIFIED"));
    assert_eq!((f.reads, f.restore_attempts), (0, 0));
}

#[test]
fn compatibility_keeps_snapshot_timeout_concurrency_and_restore_failure_guards() {
    for scenario in 0..6 {
        let mut f = Fake {
            compatibility: true,
            proof_error: true,
            ..Fake::good()
        };
        match scenario {
            0 => f.snapshot_rejected = true,
            1 => f.timeout = true,
            2 => f.race_read = true,
            3 => f.race_restore = true,
            4 => f.partial_restore = true,
            _ => f.live_sequence = 8,
        }
        assert!(run_copy(&mut f, target()).is_err());
        assert_eq!(f.restored, 0);
        if scenario == 0 || scenario == 5 {
            assert_eq!(f.copies, 0);
        }
    }
}
#[test]
fn compatibility_returns_actual_copy_instead_of_comparing_unavailable_native_text() {
    let mut f = Fake {
        compatibility: true,
        proof_error: true,
        competing_text: true,
        ..Fake::good()
    };
    assert_eq!(
        run_copy(&mut f, target()),
        Ok("SYNTHETIC COMPETITOR TEXT".into())
    );
    assert_eq!((f.proof_calls, f.copies, f.restored), (0, 1, 1));
}
#[test]
fn compatibility_never_reads_old_clipboard_and_cleans_up_invalid_new_text() {
    let mut unchanged = Fake {
        compatibility: true,
        ..Fake::good()
    };
    unchanged.stamp.as_mut().unwrap().sequence = 7;
    assert!(run_copy(&mut unchanged, target()).is_err());
    assert_eq!((unchanged.reads, unchanged.restored), (0, 0));
    let mut empty = Fake {
        compatibility: true,
        empty_text: true,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut empty, target()), Err("COPY_TEXT_INVALID"));
    assert_eq!(empty.restored, 1);
}
#[test]
fn compatibility_waits_through_identified_empty_publication_before_accepting_text() {
    let published = Stamp {
        sequence: 30,
        format_count: 7,
        ..Fake::good().stamp.unwrap()
    };
    let empty = Stamp {
        sequence: 15,
        format_count: 0,
        ..published
    };
    let mut observer = CopyObservation::default();
    assert!(!compatibility_source_allowed(7, empty, true));
    assert_eq!(observer.observe(7, Some(empty), true, 0), Ok(None));
    assert_eq!(observer.observe(7, Some(empty), true, 50), Ok(None));
    assert_eq!(observer.settled(), None);
    assert_eq!(observer.observe(7, Some(published), true, 60), Ok(None));
    assert_eq!(observer.observe(7, Some(published), true, 99), Ok(None));
    assert_eq!(
        observer.observe(7, Some(published), true, 100),
        Ok(Some(published))
    );

    // A further clear by the same writer invalidates the old settled candidate.
    let empty_again = Stamp {
        sequence: 31,
        ..empty
    };
    assert_eq!(observer.observe(7, Some(empty_again), true, 110), Ok(None));
    assert_eq!(observer.settled(), None);
    let next = Stamp {
        sequence: 40,
        ..published
    };
    assert_eq!(observer.observe(7, Some(next), true, 120), Ok(None));
    assert_eq!(observer.observe(7, Some(next), true, 159), Ok(None));
    assert_eq!(observer.observe(7, Some(next), true, 160), Ok(Some(next)));
}

#[test]
fn empty_publication_never_hides_an_unknown_or_foreign_writer() {
    let empty = Stamp {
        format_count: 0,
        ..Fake::good().stamp.unwrap()
    };
    for (observed, related) in [
        (None, true),
        (Some(empty), false),
        (
            Some(Stamp {
                sequence: 7,
                ..empty
            }),
            true,
        ),
        (
            Some(Stamp {
                owner: Target {
                    hwnd: 0,
                    ..empty.owner
                },
                ..empty
            }),
            true,
        ),
        (
            Some(Stamp {
                owner: Target {
                    pid: 0,
                    ..empty.owner
                },
                ..empty
            }),
            true,
        ),
        (
            Some(Stamp {
                owner: Target {
                    thread: 0,
                    ..empty.owner
                },
                ..empty
            }),
            true,
        ),
    ] {
        let mut observer = CopyObservation::default();
        assert_eq!(
            observer.observe(7, observed, related, 0),
            Err("COPY_SOURCE_UNVERIFIED")
        );
        assert_eq!(
            observer.observe(7, Fake::good().stamp, true, 100),
            Err("COPY_SOURCE_UNVERIFIED")
        );
        assert_eq!(observer.settled(), None);
    }
}

#[test]
fn compatibility_never_reads_or_restores_a_still_empty_copy_result() {
    let mut f = Fake {
        compatibility: true,
        ..Fake::good()
    };
    f.stamp.as_mut().unwrap().format_count = 0;
    assert_eq!(run_copy(&mut f, target()), Err("COPY_SOURCE_UNVERIFIED"));
    assert_eq!((f.copies, f.reads, f.restore_attempts), (1, 0, 0));
}

#[test]
fn compatibility_waits_for_multiple_writes_to_settle_and_rejects_a_foreign_writer() {
    let stamp = Fake::good().stamp.unwrap();
    let mut observer = CopyObservation::default();
    assert_eq!(observer.observe(7, Some(stamp), true, 0), Ok(None));
    assert_eq!(observer.observe(7, Some(stamp), true, 39), Ok(None));
    let next = Stamp {
        sequence: 15,
        format_count: 6,
        ..stamp
    };
    assert_eq!(observer.observe(7, Some(next), true, 40), Ok(None));
    assert_eq!(observer.observe(7, Some(next), true, 80), Ok(Some(next)));
    assert_eq!(
        observer.observe(7, Some(next), false, 81),
        Err("COPY_SOURCE_UNVERIFIED")
    );
    assert_eq!(observer.settled(), None);
}
#[test]
fn observed_foreign_writer_cannot_be_forgotten_by_a_later_related_write() {
    let stamp = Fake::good().stamp.unwrap();
    let mut observer = CopyObservation::default();
    assert_eq!(
        observer.observe(7, Some(stamp), false, 0),
        Err("COPY_SOURCE_UNVERIFIED")
    );
    assert_eq!(
        observer.observe(7, Some(stamp), true, 100),
        Err("COPY_SOURCE_UNVERIFIED")
    );
    assert_eq!(observer.settled(), None);
}
#[test]
fn unidentifiable_change_permanently_rejects_transaction_even_after_stable_target_write() {
    let stamp = Fake::good().stamp.unwrap();
    let mut observer = CopyObservation::default();
    assert_eq!(observer.observe(7, Some(stamp), true, 0), Ok(None));
    assert_eq!(observer.observe(7, Some(stamp), true, 40), Ok(Some(stamp)));
    assert_eq!(
        observer.observe(7, None, false, 50),
        Err("COPY_SOURCE_UNVERIFIED")
    );
    assert_eq!(observer.settled(), None);
    assert_eq!(
        observer.observe(7, Some(stamp), true, 100),
        Err("COPY_SOURCE_UNVERIFIED")
    );
    assert_eq!(
        observer.observe(7, Some(stamp), true, 150),
        Err("COPY_SOURCE_UNVERIFIED")
    );
}
#[test]
fn compatibility_wait_failure_does_not_recover_using_only_a_later_related_stamp() {
    let mut f = Fake {
        compatibility: true,
        wait_error: Some("COPY_SOURCE_UNVERIFIED"),
        change_during_wait: true,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_RESTORE_SKIPPED"));
    assert_eq!((f.reads, f.restore_attempts, f.restored), (0, 0, 0));
}
impl ClipboardIo for Fake {
    type Snapshot = ();
    fn compatibility(&self) -> bool {
        self.compatibility
    }
    fn related_writer(&self, _: Stamp) -> bool {
        self.related_writer
    }
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
    fn prepare(&mut self) -> Result<(PreparedSnapshot<()>, u32), &'static str> {
        if self.busy || self.snapshot_rejected {
            Err("COPY_SNAPSHOT_FAILED")
        } else {
            Ok((prepare_snapshot(&self.formats, || Ok(()))?, 7))
        }
    }
    fn unrestorable_copy_started(&mut self) {
        self.unrestorable_notices += 1;
    }
    fn sequence(&self) -> u32 {
        self.live_sequence
    }
    fn send_once(&mut self) -> SendStatus {
        self.copies += 1;
        if self.revoke {
            self.valid = false;
        }
        match self.sent {
            0 | 1 => SendStatus::Rejected,
            4 => SendStatus::Sent,
            _ => SendStatus::Partial,
        }
    }
    fn wait(&mut self, _: u32) -> Result<Stamp, WaitFailure> {
        self.wait_calls += 1;
        if self.change_during_wait {
            self.live_sequence = self.stamp.map_or(9, |stamp| stamp.sequence);
        }
        if let Some(error) = self.wait_error {
            return Err(if self.change_during_wait {
                WaitFailure::changed(error)
            } else {
                WaitFailure::unchanged(error)
            });
        }
        if self.timeout || self.sent == 0 {
            Err(WaitFailure::unchanged("COPY_TIMEOUT"))
        } else {
            self.stamp
                .ok_or(WaitFailure::changed("COPY_SOURCE_UNVERIFIED"))
        }
    }
    fn recover_after_wait_failure(
        &mut self,
        snapshot: (),
        before: u32,
        target: Target,
        focused_control: isize,
    ) -> Result<(), &'static str> {
        self.recovery_attempts += 1;
        let observed = self.stamp.ok_or("COPY_RESTORE_SKIPPED")?;
        // Match the native backend: a failed wait did not retain a trustworthy
        // stable candidate. A later related stamp cannot establish provenance.
        if self.compatibility || !source_allowed(target, before, focused_control, observed) {
            return Err("COPY_RESTORE_SKIPPED");
        }
        self.restore(snapshot, observed)
    }
    fn read(&mut self, _: Stamp) -> Result<String, &'static str> {
        self.reads += 1;
        if self.race_read {
            if let Some(mut stamp) = self.stamp {
                stamp.sequence = stamp.sequence.wrapping_add(1);
                self.stamp = Some(stamp);
            }
            Err("COPY_SOURCE_UNVERIFIED")
        } else if self.read_fail {
            Err("COPY_READ_FAILED")
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
    fn verify_unrestored(&mut self, expected: Stamp) -> Result<(), &'static str> {
        if self.race_restore || self.stamp != Some(expected) {
            Err("COPY_SOURCE_UNVERIFIED")
        } else {
            Ok(())
        }
    }
    fn restore(&mut self, _: (), expected: Stamp) -> Result<(), &'static str> {
        self.restore_attempts += 1;
        if self.race_restore {
            if let Some(mut stamp) = self.stamp {
                stamp.sequence = stamp.sequence.wrapping_add(1);
                self.stamp = Some(stamp);
            }
        }
        if self.race_read
            || self.race_restore
            || self.partial_restore
            || self.stamp != Some(expected)
        {
            Err("COPY_RESTORE_SKIPPED")
        } else {
            self.restored += 1;
            Ok(())
        }
    }
}
#[test]
fn complex_copy_rejects_a_third_party_write_after_read_without_overwriting_it() {
    let mut f = Fake {
        formats: vec![2],
        race_restore: true,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_SOURCE_UNVERIFIED"));
    assert_eq!(
        (
            f.copies,
            f.reads,
            f.restore_attempts,
            f.unrestorable_notices
        ),
        (1, 1, 0, 1)
    );
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
        assert_eq!(f.unrestorable_notices, 0);
    }
}
#[test]
fn authorized_complex_formats_copy_without_attempting_partial_restoration() {
    for formats in [
        vec![2],
        vec![13, 8],
        vec![13, 17],
        vec![13, 0xc000],
        vec![13, 0xc001],
        vec![13, 15],
        vec![13, 49152],
        vec![1],
    ] {
        let mut f = Fake {
            formats,
            ..Fake::good()
        };
        assert_eq!(run_copy(&mut f, target()).unwrap(), "合成\n  code 😀");
        assert_eq!((f.copies, f.reads, f.restore_attempts), (1, 1, 0));
        assert_eq!(f.unrestorable_notices, 1);
    }
}
#[test]
fn complex_formats_do_not_bypass_permission_source_or_read_failures() {
    for scenario in 0..5 {
        let mut f = Fake {
            compatibility: true,
            formats: vec![13, 0xc000],
            ..Fake::good()
        };
        match scenario {
            0 => f.valid = false,
            1 => f.related_writer = false,
            2 => f.read_fail = true,
            3 => f.revoke = true,
            _ => {
                f.wait_error = Some("COPY_SOURCE_UNVERIFIED");
                f.change_during_wait = true;
            }
        }
        assert!(run_copy(&mut f, target()).is_err());
        assert_eq!(f.restore_attempts, 0);
        assert_eq!(f.recovery_attempts, 0);
        assert_eq!(f.copies, if scenario == 0 { 0 } else { 1 });
        assert_eq!(f.unrestorable_notices, if scenario == 0 { 0 } else { 1 });
    }
}
#[test]
fn complex_copy_never_retries_or_claims_recovery_after_timeout_partial_input_or_invalid_text() {
    for scenario in 0..4 {
        let mut f = Fake {
            compatibility: true,
            formats: vec![2],
            ..Fake::good()
        };
        let expected = match scenario {
            0 => {
                f.timeout = true;
                "COPY_TIMEOUT"
            }
            1 => {
                f.sent = 2;
                "COPY_INPUT_FAILED"
            }
            2 => {
                f.empty_text = true;
                "COPY_TEXT_INVALID"
            }
            _ => {
                f.race_read = true;
                "COPY_SOURCE_UNVERIFIED"
            }
        };
        assert_eq!(run_copy(&mut f, target()), Err(expected));
        assert_eq!(
            (
                f.copies,
                f.restore_attempts,
                f.recovery_attempts,
                f.unrestorable_notices
            ),
            (1, 0, 0, 1)
        );
    }
    let mut rejected = Fake {
        formats: vec![2],
        sent: 0,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut rejected, target()), Err("COPY_INPUT_FAILED"));
    assert_eq!(
        (
            rejected.wait_calls,
            rejected.restore_attempts,
            rejected.unrestorable_notices
        ),
        (0, 0, 0)
    );
}
#[test]
fn invalid_format_enumeration_is_not_a_complex_format_permission() {
    for formats in [vec![0], vec![13, 13], (1..=65).collect()] {
        let mut f = Fake {
            formats,
            ..Fake::good()
        };
        assert_eq!(run_copy(&mut f, target()), Err("COPY_SNAPSHOT_FAILED"));
        assert_eq!((f.copies, f.unrestorable_notices), (0, 0));
    }
}
#[test]
fn busy_or_rejected_snapshots_never_send() {
    // This fake covers a preflight rejection; it cannot model a blocked native WM_RENDERFORMAT call.
    for (busy, snapshot_rejected) in [(true, false), (false, true)] {
        let mut f = Fake {
            busy,
            snapshot_rejected,
            ..Fake::good()
        };
        assert!(run_copy(&mut f, target()).is_err());
        assert_eq!(f.copies, 0);
    }
}
#[test]
fn partial_input_and_timeout_never_retry_or_restore() {
    for (sent, timeout) in [(0, false), (4, true)] {
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
fn rejected_input_does_not_wait_for_an_unrelated_clipboard_change() {
    let mut f = Fake {
        sent: 0,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_INPUT_FAILED"));
    assert_eq!((f.wait_calls, f.restore_attempts), (0, 0));
}

#[test]
fn partial_input_with_verified_clipboard_change_restores_and_fails() {
    let mut f = Fake {
        sent: 2,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_INPUT_FAILED"));
    assert_eq!((f.copies, f.restore_attempts, f.restored), (1, 1, 1));
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
            sequence: 9,
            format_count: 1,
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
            format_count: 1,
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
fn multiple_clipboard_changes_are_not_attributed_to_this_copy() {
    let mut f = Fake::good();
    f.stamp.as_mut().unwrap().sequence = 10;
    assert_eq!(run_copy(&mut f, target()), Err("COPY_SOURCE_UNVERIFIED"));
    assert_eq!((f.reads, f.restore_attempts, f.restored), (0, 0, 0));
}

#[test]
fn clipboard_busy_after_a_change_attempts_transaction_safe_restore() {
    let mut f = Fake {
        wait_error: Some("COPY_CAPTURE_BUSY"),
        change_during_wait: true,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_CAPTURE_BUSY"));
    assert_eq!((f.restore_attempts, f.restored), (1, 1));
}

#[test]
fn wait_error_does_not_restore_a_foreign_or_ambiguous_clipboard_change() {
    let mut f = Fake {
        wait_error: Some("COPY_CAPTURE_BUSY"),
        change_during_wait: true,
        ..Fake::good()
    };
    f.stamp.as_mut().unwrap().owner.pid = 99;
    assert_eq!(run_copy(&mut f, target()), Err("COPY_RESTORE_SKIPPED"));
    assert_eq!((f.restore_attempts, f.restored), (0, 0));
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
        assert_eq!(f.restore_attempts, 1);
    }
}
#[test]
fn read_failure_restores_the_snapshot_but_is_never_reported_as_capture_success() {
    let mut f = Fake {
        read_fail: true,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_READ_FAILED"));
    assert_eq!((f.restore_attempts, f.restored), (1, 1));
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
    assert_eq!(
        f.restored, 1,
        "verified transaction cleanup still restores after revoke"
    );
}
#[test]
fn restore_failure_is_not_reported_as_success() {
    let mut f = Fake {
        partial_restore: true,
        ..Fake::good()
    };
    assert!(run_copy(&mut f, target()).is_err());
    assert_eq!(f.restore_attempts, 1);
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
fn same_root_sibling_writer_is_rejected_before_reading() {
    let mut f = Fake {
        competing_text: true,
        ..Fake::good()
    };
    f.stamp.as_mut().unwrap().owner.hwnd = 11;
    assert_eq!(run_copy(&mut f, target()), Err("COPY_SOURCE_UNVERIFIED"));
    assert_eq!(f.reads, 0);
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
fn changed_selection_after_read_restores_verified_copy_but_never_returns_text() {
    let mut f = Fake {
        proof_change_at: 3,
        ..Fake::good()
    };
    assert_eq!(run_copy(&mut f, target()), Err("COPY_SELECTION_CHANGED"));
    assert_eq!((f.copies, f.reads, f.restored), (1, 1, 1));
}
