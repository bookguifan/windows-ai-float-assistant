# Beta.2.0 Release Preparation

Updated 2026-11-02. This is the current release summary; detailed historical test logs are preserved under [`archive/beta2/`](archive/beta2/).

## Version and Release State

- Planned release and tag: `v0.1.0-beta.2`. The only GitHub release currently published is `v0.1.0-beta.1`.
- `package.json`, `Cargo.toml`, and `tauri.conf.json` remain `0.1.0`; the prerelease suffix belongs to the Git tag and release name.
- PR #5 is merged into `main`; the current base is `2ecaba3b8b0d71aa69e38e0975f9916055c13cde`. Changes on `fix/beta2-ball-ready-diagnostics` require their own PR and CI result; earlier CI does not validate this branch's final SHA.
- Beta.2.0 remains **NO-GO**. This branch is not a frozen distribution candidate, and no tag or release asset was created in this work.

## Build Entry

`scripts/build-release.mjs` is the single release implementation. `release.ps1` is a compatibility wrapper that forwards `-SkipFrontBuild`, `-UseSccache`, and an optional `-NodePath`.

The build requires the `package.json` Node engine (`^24.15.0`) and the exact pnpm version in `packageManager` (`9.15.9`). It puts the entry Node executable first in the child `PATH`, then verifies that `pnpm exec node` resolves to the same executable and version. The lockfile install uses `--frozen-lockfile`; npm fallback is not permitted.

The ordinary build runs frontend checks and unit tests, Rust formatting, strict Clippy, Rust library tests, Debug/Release clipboard policy tests, and a locked Release build with `custom-protocol`. `beta2-isolation` and automatic Ctrl+C fallback stay disabled. Use `--check-toolchain-only` to verify the Node/pnpm pairing without building. `--no-deliver` does not refresh the local delivery directory.

## Repository Cleanup And Local Verification

- Eleven completed Beta.2 reports were moved under `docs/archive/beta2/`; their historical outcomes are retained. The current release summary and known-issues page remain at the top level. No tracked history or Windows license material was deleted; the 594-file `distribution/windows/` set remains pending final binary-to-component mapping.
- `release.ps1` is now a compatibility wrapper around `scripts/build-release.mjs`; npm fallback was removed. The package scripts include four release-toolchain regression tests.
- On this machine, the release entry check passed with Node `v24.22.1`, pnpm `9.15.9`, and `pnpm exec node` resolving to the same Node executable. `pnpm run check` passed. `pnpm run test:unit` passed 400 cases across 15 scripts. A local scan found no broken relative links in 30 Markdown files. `git diff --check` exited 0; Git reported only the repository's configured line-ending normalization notices.
- Rust tests, Playwright, and a full Release build were not rerun for this cleanup: application Rust/TypeScript sources and dependencies did not change. The successful PR #5 CI run documented earlier applies only to its old head; the latest head must have its own passing CI before merge.

## Windows Acceptance

The latest recorded native smoke run used the isolated diagnostic candidate built from the 11-path dirty worktree on `2ecaba3b8b0d71aa69e38e0975f9916055c13cde`. It verified WebView2 page/IPC handshake, `ball_ready`, a visible HWND, manual hide/restore, settings open/close, normal exit code `0`, observed WebView2 child cleanup, and unchanged formal configuration/autostart. It was not an ordinary release candidate. Hiding occurred about 42 minutes after startup, so the delayed-startup visibility race was **not exercised**; Issue #2 remains open.

No further native EXE was started in this closeout. A01 end-to-end selection, real Windows clipboard, native backup/restore and file picker remain untested. The simulated clipboard tests do not substitute for real Windows clipboard acceptance. A01 selection verification must not click the toolbar's explicit Copy action. Real clipboard acceptance must run in a disposable VM/Sandbox with host clipboard redirection disabled and a known sentinel created inside the guest; never read or overwrite the maintainer's unknown host clipboard. The `beta2-isolation` diagnostic candidate disables native file dialogs, so Windows file-picker verification requires an ordinary Release candidate inside an isolated Windows account/VM.

## Licensing and Distribution

Keep the complete `distribution/windows/` material until the final binary-to-component mapping is reviewed. It currently contains 594 tracked files, including license texts and MPL source correspondence. Its checksum review is not proof that every final binary component has been mapped; that release gate remains open.

The final green package must be freshly built from the final source SHA and include the required project and third-party notices, source correspondence, component/checksum indexes, and a SHA256 manifest. Do not distribute PDBs, local logs, user data, WebView2 profiles, test caches, or signing credentials. The ignored local `release/` draft is not a frozen candidate and must not be reused as the final package.

## Install and Upgrade

Verify the published SHA256, extract the green package into a new program directory, and close the running assistant before replacing program files. Preserve the user's configuration, favorites, prompts, and WebView2 data; do not clear AppData as an upgrade step. Use the application's backup/restore flow only after the native backup and recovery gates have been completed.

## Remaining Release Gates

- Run the latest PR commit's CI and review its full diff.
- Validate a source-current candidate in one authorized isolated native session; cover A01 selection without Copy, the delayed startup visibility race, settings, normal exit, and child cleanup.
- Complete real clipboard acceptance only in a disposable Windows VM/Sandbox with host clipboard redirection disabled and a known in-guest sentinel; report complete, partial, or failed restoration accurately.
- Complete native backup/restore tests with disposable data.
- Review final package contents, privacy scan, license mapping, EXE/PDB/source identity, and SHA256 manifest.
- Keep Issue #2 open until its intermittent failure has adequate evidence; do not infer a fix from a single successful run.

## 2026-11-02 Latest Native Smoke Evidence

- Candidate source identity: dirty worktree on branch `fix/beta2-ball-ready-diagnostics`, base HEAD `2ecaba3b8b0d71aa69e38e0975f9916055c13cde`, 11 modified paths. Diagnostic Release EXE SHA256 `22A1837B523FABD63D5376F59B3D82E7D16D76A3F94251C914A4A73020E176CC`; PDB SHA256 `08F1F8B95390DDAC3932A65F1B72DC4FA6FB1B6C35C2579B420DEEFEA7FD9EEB`, GUID/Age `1195a12b-9292-4085-9f4c-5ee3013ae619/1`.
- WebView2 navigation and page IPC completed; `ball_ready` arrived about 171 ms after start and returned successfully about 198 ms after start. HWND `6622952` was valid and visible. The user confirmed hide/wait/show, settings open/close without saving, and normal app exit; the process handle recorded exit code `0`.
- The monitor observed 16 WebView2 descendants and recorded zero remaining after 10 seconds. All 251 listed runtime files were under the isolated root; formal config/favorites/prompts hashes and the HKCU Run value were unchanged. This is a basic native smoke result, not final-release or A01 proof.
- The hide action occurred about 42 minutes after launch, after the delayed startup callback had completed. Therefore the startup hide/show race remains unverified. A01, real clipboard, native backup/restore, ordinary Release packaging and final binary-to-license mapping remain open. Issue #2 is still OPEN; Beta.2.0 remains **NO-GO**.

## 2026-11-02 Native Isolation Launch Follow-up

- A prior authorized launch attempt exited with code `78` before WebView2 initialization because its launcher removed inherited `WEBVIEW2_*` variables and then re-added `WEBVIEW2_USER_DATA_FOLDER`. The application correctly rejected that override. This was a test-plan error, not evidence of a WebView2 runtime defect.
- The corrected launcher removes inherited `WEBVIEW2_*` only from the candidate child environment and sets none; the application's validated isolation paths select the WebView2 directory. The single-use JSON plan update now handles optional metadata fields absent from the plan. Process-tree monitoring uses a tested Win32 Toolhelp snapshot because WMI process enumeration is denied in this environment. No production application source or candidate build inputs changed.
- Targeted results: isolation paths `7/7`, Tauri mock window bootstrap `1/1`, ordinary-build window configuration `1/1`, launcher no-launch regression PASS, and `git diff --check` PASS.
- A corrected isolated diagnostic candidate was started once on 2026-11-02 at 01:54:23 UTC (startup ID `ddea9b0cbb344a338e86b24a0d06f253`, PID `46148`), EXE SHA256 `16FBFAAF6CAFA51D4D99F847BA3F0204D1D8793F02A5D37928DA07DB45B8ADDA`. PDB SHA256 is `E0E4328DB9C9AE7EF8258127478EC3F89C679514A698946D8B9EB908B28BCE7D`, GUID/Age `3a63cd30-8dab-4c40-8fbe-daa4bffdde87 / 1`.
- Runtime created WebView2 data under the planned isolated root, and the identifier-derived LocalAppData directory remained absent. The Win32 monitor recorded a valid visible HWND (`267448`) and setup entry. The Release diagnostic channel was off (`DSFLOAT_DIAG` unset), so no `d1-debug.log` or `[ball-ready]` signal is available; UI Ready remains UNKNOWN, settings are BLOCKED, and no feature pass is inferred from window existence.
- The user reported that the app could not exit through its normal UI. After verifying the executable path, the authorized isolated PID `46148` was force-terminated. Its process handle recorded exit code `-1` at `2026-11-02 02:10:41 UTC`; this is an operator-terminated result, so normal exit is FAIL and it is not reported as a crash.
- The monitor observed 21 descendant PIDs during the run and zero remaining 10 seconds after main-process termination. The isolated run created 102 files, including the WebView2 `EBWebView` profile. The identifier-derived LocalAppData folder remained absent.
- Post-run SHA256 values exactly match the preflight baseline: config `CC5EC62C4A89C09E0E26DB0F9DB327F18E7ECCEAA5462BF60EB2639B6B84CB6E`, favorites `31090B94842B85AEAFA1AA218A68AD2C596B4F30FE8101E18D23C44B39ECE9FB`, and prompts `461D059D3B1EB536073C4107A42088E58F1501533462563C5A550F6710BFB2F6`. The `deepseek-float` HKCU Run value remained absent.
- A valid visible HWND was observed, but `DSFLOAT_DIAG` was not enabled in the Release launch plan, so no `[ball-ready]` signal or diagnostic HRESULT was captured. UI Ready and WebView2 initialization therefore remain UNKNOWN; settings are BLOCKED. This observation-plan gap does not prove a WebView2 failure. The candidate did start to a visible native window, so no source rollback was made. No restart was performed.
- Final native result: visible HWND PASS; WebView2/UI Ready UNKNOWN; settings BLOCKED; normal exit FAIL (authorized force termination, exit code `-1`); monitored child cleanup PASS; formal data and autostart unchanged PASS. One launch was used. Issue #2 remains OPEN and Beta.2.0 remains NO-GO. Evidence: `%TEMP%\float-beta2-native-acceptance-20261102-b0711db770564ea2876e0ed5e44bb3b8\native-run-ddea9b0cbb344a338e86b24a0d06f253` (`preflight-baseline.json`, `post-run.json`, `process-window-monitor.jsonl`).

## 2026-11-02 Diagnostic Native Run

- Evidence reconciliation: the app log reports a ball_ready timeout and has no Rust receipt, while the same run's process monitor changes ui_ready_signal_seen to true at approximately 2.6 seconds. The records conflict, so final UI Ready is **UNKNOWN**, not PASS. The exact page/IPC failure stage remains undetermined.
- PID 16588 was the only launch under that authorization and was manually terminated after the normal exit entry could not be found. Exit code -1 is not a crash result. Formal business files and autostart remained unchanged.
- Added opt-in, isolated-build ball-page navigation and IPC stage diagnostics. Only allowlisted event names are logged; page content and exception details are excluded. The ordinary build does not register the diagnostic command or inject the script.
- The launch allowance is exhausted. A new diagnostic candidate may be prepared after targeted checks, but native startup requires separate authorization. Issue #2 remains OPEN; Beta.2.0 remains **NO-GO**.

- The frozen candidate was reused without rebuilding (EXE SHA256 `16FBFAAF6CAFA51D4D99F847BA3F0204D1D8793F02A5D37928DA07DB45B8ADDA`; PDB SHA256 `E0E4328DB9C9AE7EF8258127478EC3F89C679514A698946D8B9EB908B28BCE7D`; GUID/Age `3a63cd30-8dab-4c40-8fbe-daa4bffdde87/1`). One isolated launch ran as PID `16588` from `2026-11-02 02:29:49 UTC`.
- `DSFLOAT_DIAG=1` was set only in the candidate child environment. The log confirmed Meta mode and wrote within the isolated root. The launcher removed inherited `WEBVIEW2_*` variables and set none; WebView2 data directory remained configured by the app under the isolation root.
- The native process and visible HWND (`201992`) were observed. The diagnostic log explicitly records the `ball_ready` handshake timing out at about 3 seconds, without a received handshake. UI Ready is **FAIL**; WebView2 control initialization and page navigation remain **UNKNOWN** because there is no success acknowledgement or HRESULT. The first monitor version falsely marked Ready by matching any `[ball-ready]` line; this was corrected to require the actual “收到 ball_ready” receipt and covered by no-launch tests.
- Hide/restore and settings were not tested after the handshake timeout. The user could not find the tray/exit entry, so normal exit is **BLOCKED**. The verified test PID was then terminated under the one-run authorization; exit code `-1` is an operator-termination result, not evidence of a crash. Child cleanup is **UNKNOWN** because two stale tracked PIDs had been reused by unrelated Codex processes; those processes were verified outside the candidate tree and left untouched.
- Formal config/favorites/prompts hashes and the `deepseek-float` Run value were unchanged. Isolation passed. The candidate identifier Known Folder was not created. Beta.2.0 remains **NO-GO**; Issue #2 remains OPEN.

## 2026-11-02 Diagnostic Candidate Native Regression

- One isolated Release launch completed on the diagnostic candidate from dirty worktree HEAD `2ecaba3b8b0d71aa69e38e0975f9916055c13cde` (11 uncommitted paths). EXE SHA256: `22A1837B523FABD63D5376F59B3D82E7D16D76A3F94251C914A4A73020E176CC`; PDB SHA256: `08F1F8B95390DDAC3932A65F1B72DC4FA6FB1B6C35C2579B420DEEFEA7FD9EEB`, GUID/Age `1195a12b-9292-4085-9f4c-5ee3013ae619/1`. This is a diagnostic candidate, not a release artifact.
- WebView2 navigation, page JavaScript, IPC availability, first-frame callback, Rust `ball_ready`, and the ready invocation all completed; the handshake arrived about 171 ms after startup and the invocation returned successfully at about 198 ms. The candidate reached a visible valid HWND (`6622952`) and UI Ready. No HRESULT failure was recorded.
- The user manually confirmed hide/wait/show, opening and closing settings without saving, and then exiting through the app. The logs record ball hide/show and the app's `ExitRequested`; the process handle recorded exit code `0`. All 16 observed WebView2 descendants had exited within 10 seconds.
- 251 runtime files were listed, all within the new isolated root; the identifier-derived LocalAppData directory was not created. Formal config, favorites, prompts hashes and the HKCU Run value were unchanged. No matching Application Error 1000 or WER 1001 event was found during the run.
- This run validates basic manual hide/restore but does not prove the delayed-startup callback race: the hide action occurred about 42 minutes after startup, after the startup show callback had already completed. Issue #2 remains OPEN; a single successful run does not establish that intermittent startup failures are fixed.
- A01 native selection, real clipboard, backup/restore, ordinary release packaging/privacy/licensing, and final source-to-binary release mapping remain untested. Beta.2.0 remains **NO-GO**.

## 2026-11-02 Diagnostic Branch Automation Results

These are local results for the uncommitted diagnostic branch based on `2ecaba3b8b0d71aa69e38e0975f9916055c13cde`; they are not GitHub CI results and do not identify a release candidate.

- Toolchain: Node `v24.22.1`, pnpm `9.15.9`; the release toolchain check confirmed `pnpm exec node` resolves to the selected Node executable.
- Frontend: `pnpm run check` passed; `pnpm run test:unit` passed 406 cases across 15 scripts; Playwright passed `22/22`. The first Playwright attempt was blocked by sandbox `spawn EPERM`; the rerun completed successfully.
- Rust: `cargo test --locked` passed `450`, failed `0`, ignored `12`. `cargo fmt --check` passed. Strict Clippy passed for the default Debug and `beta2-isolation` configurations; ordinary Release `cargo check` passed.
- Isolation and window bootstrap: the first path-test run used the ordinary identifier and correctly failed closed (`5/7`). With the CI test identifier `com.deepseekfloat.beta2.isolated.ci`, Debug and Release path tests each passed `7/7`, and mock window bootstrap passed `1/1` in each configuration. The new diagnostic stage allowlist test passed `1/1`; Debug and Release clipboard policy tests each passed `4/4`.
- The no-launch PowerShell launcher preflight and `git diff --check` passed. The preflight test used a non-executable fixture and confirmed that no candidate was started.
- An exploratory full `beta2-isolation` library-test run had one unrelated existing backup test failure: its restore assertion expects `autostart=true`, while the isolation feature deliberately forces autostart off. No backup behavior was changed to hide that mismatch; the CI isolation gate runs the focused path/bootstrap tests above.
- No EXE was started during this automation closeout. A01 native selection, real system clipboard, native backup/restore, the ordinary Release file picker, final package privacy review, and binary-to-license mapping remain untested. Issue #2 remains OPEN and Beta.2.0 remains **NO-GO**.
