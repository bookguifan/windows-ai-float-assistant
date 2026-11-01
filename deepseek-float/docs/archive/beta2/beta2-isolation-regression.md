# Beta.2.0 isolation regression and merge preparation

Status (2026-11-01): the final isolated candidate completed one explicitly authorized native run. WebView2, ball visibility/HWND, explicit UI Ready, normal exit (true exit code 0), child cleanup and unchanged production business data/autostart passed. Settings was BLOCKED when an unattributed external-path event stopped the run. No second run is authorized in this closeout round. Beta.2 remains **NO-GO**.

## Confirmed directory creation path

With the locked Tauri 2.11.5 / tauri-runtime 2.11.3 dependencies, conversion from WindowConfig to WebviewAttributes omits data_directory. Tauri builds create=true configuration windows before the application's setup hook. Its WebviewManager supplies LocalData/identifier when the runtime attribute is None and calls create_dir_all there. Setting the configuration field and WEBVIEW2_USER_DATA_FOLDER alone can therefore isolate the actual profile while still creating an empty default identifier directory.

The minimal test-mode fix defers only originally automatic windows, then creates them first in application setup with an explicit builder data_directory after from_config. Original create=false windows remain disabled; ordinary builds retain the original automatic window path. Dynamic windows already use the shared explicit builder. Invalid paths continue to fail closed before building a window. No dependency or lockfile version was changed.

The regression uses the real Tauri setup and manager with MockRuntime. Its fallback path is confined to a disposable fixture. Removing the explicit data_directory setter creates the forbidden sibling directory and fails the test; restoring the setter passes. This is non-native evidence, not HWND, WebView2 or UI Ready acceptance. A test-only Common Controls v6 manifest fixes the integration-test executable's TaskDialogIndirect import; it is not linked into the shipping executable.

## Historical local test matrix (2026-10-31)

| Check | Result |
|---|---|
| cargo test --locked, ordinary default configuration | 431 passed, 12 ignored, 0 failed; counts summed across targets, including repeated modules |
| beta2-isolation console-path and mock-manager integration targets | 7 passed, 0 failed |
| beta2-isolation resolver unit subset | 15 passed, 0 failed; overlaps default unit coverage |
| Experimental Debug policy | 4 passed |
| custom-protocol + experimental feature Release policy | 4 passed; automatic Ctrl+C stays disabled |
| Ordinary custom-protocol Release window configuration | 1 passed |
| Frontend units | 232 custom assertions + 160 Node test cases passed; 392 combined checks |
| Playwright | 22 passed, local browser pages |
| Frontend checks and build | exit 0 |
| fmt check, all-target/all-feature strict Clippy | exit 0 |
| Ordinary Release check and isolation feature check | exit 0 |
| git diff --check | exit 0 |

These are separate matrices, not a unique grand total. Ignored native tests were not executed. The full default Rust suite and targeted isolation tests were run; a full all-features test execution is not claimed. Some legacy storage/diagnostic unit fixtures assume ordinary paths and have not been migrated to run under test isolation. All-feature Clippy compiles those targets but does not execute them. No clipboard, A01/N05, real backup UI, native assistant or screenshot test was performed in this round. Initial preparation/linker failures and deliberate RED/mutation runs remain in the local evidence and are distinct from the final passing matrix.

## Path contract

| Data | Isolated target |
|---|---|
| config / favorites / prompts | ROOT/config/{config,favorites,prompts}.json |
| Transaction/restore journal | ROOT/config/.backup-restore-journal |
| Application/plugin logs | ROOT/logs |
| Diagnostics, state, panic and crash summary | ROOT/diagnostics |
| Configured and dynamic WebView2 windows | ROOT/webview2 |
| Test backup destination | ROOT/backups |
| Process/child temporary files | ROOT/temp |
| Single-instance objects | Local namespace derived from the validated root; no production mutex/event |
| Autostart and external export/file dialogs | disabled |

The frozen console probe and a subsequent native run must use different fresh roots. A console probe writes a fresh test configuration, so its root is consumed and must never be reused for native acceptance. Probe results describe actual store/context bindings, while manager behavior is separately covered by the mock regression. Runtime-wide writes outside the root still require native observation.

## Current local verification (2026-11-01)

| Check | Actual current result |
|---|---|
| Final cargo test --locked | 431 passed, 12 ignored, 0 failed, summed across targets |
| Isolated integration, Debug / Release | 7 / 7 passed; separate overlapping matrices |
| Isolated resolver unit subset | 15 passed |
| Experimental Debug / guarded Release policy | 4 / 4 passed; Release automatic Ctrl+C remains disabled |
| Ordinary Release bootstrap | 1 passed |
| Frontend units | 232 custom assertions + 160 Node tests = 392 combined checks passed |
| Playwright | 22 passed, local browser pages |
| Frontend checks/build, final fmt, all-target/all-feature strict Clippy | exit 0 |
| Ordinary Release check/build and isolated feature check | exit 0 |
| git diff --check | exit 0 before staging; repeated on staged commits |

These counts were extracted from this closeout run, not copied from the historical matrix. Ignored native tests remain unexecuted. The final full default suite and Release isolation matrix were run after the privacy-test change. No native assistant was started in this closeout round.

Both integration targets exercise ASCII case aliases and independently canonicalize the requested root before comparing physical paths. This preserves exact-root and containment assertions while accommodating Windows path spellings such as hosted-runner TEMP aliases; raw, case-sensitive prefix comparison is not a physical containment check.

## Native evidence and attribution (2026-11-01)

The final diagnostic candidate was started once under explicit authorization. The ball was visible with a valid PID-associated HWND, and application logs recorded the first-frame ball_ready handshake. The user selected the normal application exit; a retained process handle and Process.ExitCode both returned 0. WebView2 children cleared approximately 1.5 seconds later. All 4,304 production-directory entries, including config/favorites/prompts hashes, matched; autostart baselines matched. The new identifier-derived Known Folder was absent before and after. This run did not reproduce the previously confirmed empty default-directory creation defect.

The observer captured a transient `%LOCALAPPDATA%/Huorong/AppStore/DisplayIcons/deepseek-float.exe.png` create, two changes and delete. Classification: **UNATTRIBUTED_WRITE**. The location is in Huorong's icon-cache namespace, but the FileSystemWatcher record contains no PID, process image or stack. Candidate timing and the vendor path do not prove the writer. Neither CONFIRMED_APP_WRITE nor CONFIRMED_EXTERNAL_WRITE is supported. Keep the original captured global event stream and historical strict-gate FAIL; do not relabel it PASS. This unresolved event alone does not establish a production code defect.

The clarified isolation contract is: **persistent writes by the test candidate and controlled child processes must not escape the isolation root; production business data and autostart must remain unchanged**. Application-owned WebView2 user-data-dir was observed inside the new root. This evidence and unchanged Known Folders are useful, but they are not process-attributed proof for every write. Full application isolation remains unresolved pending acceptable attribution/risk disposition; no unconditional PASS is claimed. A maintainer must accept the documented residual risk before merge.

The old test WebView2 tree had five directory-only historical mtime metadata differences, without changed names, sizes or file hashes; their timestamps predated the run and there were no corresponding runtime watcher events. Origin remains UNKNOWN. Do not infer candidate reuse from these differences. The historical production config rewrite remains a separate open incident.

Settings was blocked by the stop rule and observation constraints, not demonstrated broken. The hidden settings window's creation is not evidence of an open interactive settings page. No old candidate's settings PASS is reused. Screenshots remain unavailable; Win32, explicit Ready logs and AX are allowed alternatives, with their separate claims recorded.

## Minimal settings regression plan (not executed)

A separate local plan reserves a new, currently absent root and binds the existing frozen final candidate by EXE/PDB hashes and PDB GUID/Age. Before a future authorized start, recheck source-to-candidate binding, identity, root ancestors, empty/fresh state, production hashes, autostart and all identifier Known Folders. If relevant code changes invalidate that candidate binding, freeze a new candidate first. Probe roots and prior native roots must never be reused.

After separate explicit one-shot authorization: start continuous observers before the candidate; verify ball HWND/visibility and explicit Ready; open settings, use AX text and non-mutating focus navigation to verify interaction, close settings normally, then exit through the application. Retain the same process handle for true exit code, track all controlled children, and compare production/identifier trees and actual WebView2 profile paths. If the ball is absent from returned interaction-window objects, ask the user to open settings or exit; do not fabricate window objects. Do not change values, invoke A01/N05/clipboard/backup, or repeat startup.

Confirmed candidate/child root escape, production changes, 0x8000FFFF, ball_ready timeout or invalid HWND stop the test. Unattributed vendor-cache events remain visible and unresolved; the next explicit authorization must cover their documented handling, rather than silently treating them as application writes or hiding them. No settings PASS or full isolation PASS is claimed before evidence exists.

## Merge gates versus release gates

PR #3 remains Draft until final pushed SHA CI, maintainer review, necessary new-candidate settings regression and an acceptable disposition for the unattributed event are complete. A passed historical CI run proves only its historical SHA; new commits require new CI. This closeout authorizes local commits and a normal fast-forward push to the existing feature branch, but not force push, main merge, tag or release. No branch-protection or reviewer-approval claim may be inferred from local review alone.

The isolation implementation and executable regressions should be committed together so no intermediate test feature can fall back to production data. The test manifest, CI, probe and configuration declarations are part of that atomic change. Portable acceptance and gate documentation is a separate commit. Keep PDBs, EXEs, private paths, raw logs, configuration copies, test profiles and local launch plans outside Git. Candidate identity remains distinct from a clean-HEAD build and source mapping must be rechecked after commits.

These are code-merge blockers: failed final CI, actionable code-review defects, missing maintainer approval, missing necessary settings regression, confirmed application root escape, or an undisposed isolation risk. They are separate from publication approval. Do not request actual merge authorization until all required merge gates are met.

Beta.2.0 release continues **NO-GO**, even after any future development merge:

| Remaining gate | Required follow-up |
|---|---|
| A01/N01 toolbar display lifecycle | Explicit native transaction/first-click matrix; keep historical FAIL |
| N05 clipboard correctness | Separately authorized synthetic native clipboard matrix; keep the historical mismatch unresolved |
| Native backup export/restore | Separately authorized valid/invalid/native transaction matrix; no automatic restoration |
| Issue #2 intermittent startup P1 | Keep OPEN; capture the first triggering stack with matching symbols in an authorized investigation |
| Historical production config rewrite | Preserve current evidence, investigate writer without overwriting/restoring originals |
| Release privacy and licenses | Review release-build logs, private metadata, notices, resource rights and provenance for the final artifact |
| Final EXE/PDB/source mapping | Build and freeze release artifacts from the final source/configuration, record hashes and GUID/Age locally; no public diagnostic PDB |
| Other existing native gaps | Preserve N03/N04/N07/N08, WM_COPY UNKNOWN and historical FAIL/BLOCKED until independently closed |

A single successful startup is not a fix for an intermittent P1 and cannot close Issue #2. A main merge does not authorize publication or relabel any remaining release gate PASS.


SOURCE-PROVENANCE.json and SOURCE-MANIFEST.csv describe the historical source import/publication overlay, including the then-current visibility and LF-only normalization claim. They are not a claim that later Git commits changed no implementation or that the current release artifact has been mapped. Preserve that import evidence; use Git history and the separate final release-artifact review for current provenance.

## 维护者实用验收与 PR #3 合并（2026-11-01）

PR #3 已按维护者明确授权合并，main 为 `05eaf6b37381a12fc5f676011b16a2ef1bed3f2c`；后续分支 `feature/beta2-final-polish`。最终 PR HEAD `3d2bc21` 的 CI #9 成功。维护者接受 Huorong 事件为 `UNATTRIBUTED_WRITE / ACCEPTED_NONBLOCKING_RISK`，不要求个人项目额外第三方 Review。上文 Draft、未获授权及严格门禁未处置均是历史阶段记录；历史 FAIL/UNKNOWN 和原始事件流继续保留，不改成严格隔离 PASS。

新增一次单独授权的冻结候选设置回归：WebView2、球窗/HWND、明确 ball_ready 通过；人工打开/保存“全屏隐藏” false→true/关闭设置，AX 与磁盘重读佐证，记 MANUAL_PASS；正常菜单退出，两路真实退出码均 0，受控 WebView2 无残留。正式 config/favorites/prompts SHA256、自启项不变，没有新 identifier Known Folder。没有执行 A01/N05/备份，也未重新启动。冻结候选不等于后续修改产物。

当前发布计划、验证范围与安装升级草稿见 [Beta.2.0 发布准备](../../beta2-release-preparation.md)。Issue #2 仍 OPEN；Beta.2.0 发布仍 NO-GO，合并不等于发布。
