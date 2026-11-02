# Windows Native Clipboard Acceptance Plan

Status: prepared only. No system clipboard was read or written while preparing this plan. Run this only after separate, explicit authorization for the native copy fallback.

## Safety Setup

- Use a disposable Windows test account or isolated interactive session. Do not begin while the clipboard contains user or work data.
- Before testing, deliberately replace the clipboard with a known recovery sentinel. This is the recovery target; the plan does not claim to serialize or restore arbitrary rich clipboard data.
- Keep the test candidate, app data, logs, and fixture files in the existing isolated test root. Record the candidate SHA256 and the test-session start time.
- Leave the fallback disabled until the native settings confirmation is accepted. Run the Release candidate only against disposable synthetic text and a known clipboard sentinel.
- Stop on an unexpected app-data write, a `COPY_RESTORE_FAILED` / `COPY_RESTORE_SKIPPED` diagnostic, or a mismatch after a failed copy. Do not claim arbitrary rich clipboard restoration is lossless.

The verifier at `scripts/windows-clipboard-acceptance.ps1` is read-only. Its default `Plan` action does not access the clipboard. `ListFormats`, `VerifyText`, and `VerifyFormats` read clipboard state only when passed `-AcknowledgeClipboardRead`; run those actions from an STA PowerShell session. The script contains no clipboard write API.

## Delayed Rendering Limitation

Windows can defer a clipboard format until a reader requests it; the system then sends `WM_RENDERFORMAT` to the clipboard owner. The current in-process `GetClipboardData` snapshot cannot reliably identify that state in advance, and its 100 ms elapsed-time check cannot interrupt a call that has not returned. A blocked owner can therefore stall the selection worker while the snapshot is incomplete. The code does not empty the clipboard until the full snapshot succeeds, but this is not a hard timeout or a guarantee against a stuck owner. Do not test against valuable clipboard contents or treat delayed rendering as a verified rejection. See [Microsoft's WM_RENDERFORMAT documentation](https://learn.microsoft.com/en-us/windows/win32/dataxchg/wm-renderformat).

## Test Data

Use these exact text payloads, including line endings:

| Case | Payload |
|---|---|
| English | `BETA2 clipboard acceptance: English text` |
| Chinese | `BETA2 剪贴板验收：中文文本` |
| Unicode | `BETA2: café / 😀 / 𐐀 / 中文` (`e` + U+0301 combining acute) |
| Multiline | `BETA2 line 1` followed by CRLF and `BETA2 第二行 😀` |
| Recovery sentinel | `BETA2-RECOVERY-SENTINEL-20261101` |

For rich-format setup, use a disposable fixture that places these known values on the clipboard: HTML `<div><b>BETA2 HTML</b> 中文 😀</div>`, RTF `BETA2 \b RTF\b0  中文`, and a 2x2 opaque PNG with red/green/blue/white pixels. Record the formats shown by `ListFormats` before the app action; do not use a document containing private content.

Example read-only checks, after the tester has performed the app action:

```powershell
powershell.exe -NoProfile -STA -File .\scripts\windows-clipboard-acceptance.ps1 -Action ListFormats -AcknowledgeClipboardRead
powershell.exe -NoProfile -STA -File .\scripts\windows-clipboard-acceptance.ps1 -Action VerifyText -ExpectedText 'BETA2 clipboard acceptance: English text' -AcknowledgeClipboardRead
$expected = "BETA2 line 1`r`nBETA2 第二行 😀"
powershell.exe -NoProfile -STA -File .\scripts\windows-clipboard-acceptance.ps1 -Action VerifyText -ExpectedText $expected -AcknowledgeClipboardRead
powershell.exe -NoProfile -STA -File .\scripts\windows-clipboard-acceptance.ps1 -Action VerifyFormats -ExpectedFormat 'HTML Format','Rich Text Format','Bitmap' -AcknowledgeClipboardRead
```

## Acceptance Sequence

1. Record the known recovery sentinel and its expected Unicode text. Record the clipboard format list and app process handle count before interaction.
2. In one app session, use the explicit Copy action with the English, Chinese, Unicode, and multiline payloads. Verify each exact text with `VerifyText`. Successful app copy intentionally replaces the clipboard with Unicode text; it is not expected to preserve the previous HTML, RTF, or image payload on success.
3. For the occupied-clipboard case, seed only disposable known data, have a separate controlled helper hold the clipboard open, then invoke Copy once. Expect a clear failure and unchanged original text/formats after releasing the helper. Do not add retries.
4. Repeat a pre-publication rejection with a known unsupported private format if the fixture can produce one. Expect the writer to refuse before `EmptyClipboard`; compare the before/after format lists and known fixture content.
5. Verify format restoration after a failure that occurs after `EmptyClipboard` using the Rust failure-injection test. The Windows clipboard API has no deterministic, safe way to force that exact `SetClipboardData` failure in the live user session; do not add a production failpoint or claim this native case passed.
6. Review diagnostics for `PublishFailedRestored` versus `RestoreIncomplete`. If recovery is incomplete, stop further clipboard actions and record the missing format IDs.
7. Compare process handle counts across a short, fixed set of copy attempts after allowing the process to settle. Treat a sustained increase as a failure requiring investigation; do not infer a leak from a single transient count.
8. Restore the known recovery sentinel from its fixture, verify it, exit normally, and confirm the test process released its clipboard/window handles. Do not try to reconstruct arbitrary clipboard content.

## Evidence To Record

- Candidate SHA256, branch/HEAD, app PID, test-session time, and isolated data root.
- Exact payload case and observed text result; use hashes rather than logging clipboard contents.
- Clipboard formats before/after each failure case.
- Whether the lock helper was released and the clipboard became available again.
- App diagnostics, process handle counts, normal exit code, and any `RestoreIncomplete` format IDs.
- Confirmation that no formal config, favorites, prompts, autostart entry, or user clipboard data was used.

## Automatic Selection Fallback Acceptance

Status: planned only; this task does not start the EXE or access the system clipboard.

1. Confirm UIA success and standard Unicode Edit success each avoid `SendInput`.
2. In Notepad, with only the known sentinel on the clipboard, authorize the session in settings and select synthetic text. Verify one Ctrl+Insert sequence, one toolbar, exact Unicode text, and restoration of the sentinel.
3. Repeat with ordinary browser text only if the browser exposes a verified standard Edit focus. Do not assume a rendered page has a Win32 Edit control.
4. Test a prepared HTML/RTF/image/multiformat clipboard. The copy fallback must reject before input when any format is outside the exact snapshot whitelist; verify the clipboard formats remain unchanged. Do not claim this covers delayed rendering, which cannot be reliably preflighted or interrupted by the current API path.
5. Simulate clipboard contention and third-party sequence changes with the deterministic fake backend. Native acceptance must confirm that the target performs exactly one clipboard-empty operation plus one concrete format write (sequence delta 2); synthesized text formats reported by `EnumClipboardFormats` do not count as concrete writes. Any other delta rejects attribution. On native contention, verify clear refusal and no retry.
6. Change foreground window during a selection and verify no shortcut is sent after identity validation fails. A verified in-flight clipboard result may be restored only if its exact transaction stamp is still current.
7. WPS remains unsupported unless its focused control is verified as a same-process, standard Unicode `Edit` with stable native selection proof. Never relax cross-process checks to make WPS pass.
8. Revoke the authorization and restart the app; the setting must be off. Legacy config and imported backups must not grant it.

## A01 Native Acceptance

Status: planned only; no EXE was started for A01 in this task. Use one separately authorized isolated app session and a local test page containing only synthetic selectable text.

1. Select a short known text range and confirm one toolbar appears with the matching preview.
2. Open the toolbar menu, switch focus with Alt+Tab, and verify Alt+Tab itself does not force an immediate hide; the existing timer remains responsible for hiding it.
3. Switch to another foreground application, then try the old menu action. It must be rejected because its Session/Ticket is stale.
4. Make a new selection before the old timer expires. Verify the old timer does not hide the new toolbar and the old asynchronous result cannot replace the new preview.
5. Repeat rapid selections and confirm a single current toolbar/session remains actionable; the stale menu and result are rejected.
6. Wait the normal configured timer and confirm the current toolbar hides. Record timer start, hide time, current ticket, and relevant diagnostics.

Run the existing UI and Rust lifecycle tests before this manual check. Do not use real private text, and do not infer native window-count or foreground behavior from DOM tests.

## Backup And Restore Acceptance

Status: deterministic simulated-data coverage is available; native file-picker/restore acceptance remains unrun. Create a fresh, unique test root under `%TEMP%` immediately before a separately authorized run; keep the app's config, favorites, prompts, backup files, and logs inside it. Never point the test build at formal AppData.

Use only synthetic records, for example one favorite titled `BETA2 test favorite`, one test prompt, and non-secret settings. Record SHA256 values for all fixture files before each operation.

1. Export a favorite and verify the resulting UTF-8 Markdown content and that a second export does not overwrite the first.
2. Export settings and a complete backup; inspect the manifest, data version, counts, and expected omission of device-specific state.
3. Cancel the native save/open dialog and confirm all fixture hashes remain unchanged.
4. Preview a valid backup and verify counts/conflict choices before applying it. Test merge and overwrite only against the synthetic root.
5. Try malformed JSON, duplicate IDs, unsupported versions, and oversized input. Each must be rejected before fixture files change.
6. Exercise a forced mid-restore write failure in the Rust transaction tests; confirm the previous fixture files are restored byte-for-byte and any journal is handled as expected.
7. Exit normally and compare the post-run fixture hashes and directory inventory. Keep all generated backups inside the unique test root.

If the isolated build disables native dialogs, do not silently enable them or fall back to the formal user directory. Treat native file-picker coverage as blocked until a separately prepared candidate can use the test root while preserving the same data isolation.

## Current Automated Coverage

The in-memory Win32 facade tests exercise exact Unicode text publication, pre-publication failures, clipboard-busy behavior, publish failure with successful recovery, incomplete recovery reporting, HTML/RTF/DIB restoration, DIB-derived bitmap/palette behavior, unsupported private formats, rapid operations, and the `0x0300`/`0x03FF` GDI-object format boundaries. These are deterministic API-contract tests, not native clipboard evidence.
