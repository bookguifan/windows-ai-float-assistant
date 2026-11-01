# Beta.2.0 Release Preparation

Updated 2026-11-01. This is the current release summary; detailed historical test logs are preserved under [`archive/beta2/`](archive/beta2/).

## Version and Release State

- Planned release and tag: `v0.1.0-beta.2`. The only GitHub release currently published is `v0.1.0-beta.1`.
- `package.json`, `Cargo.toml`, and `tauri.conf.json` remain `0.1.0`; the prerelease suffix belongs to the Git tag and release name.
- PR #5 contains clipboard failure recovery and release-build path remapping. The repository cleanup and release-toolchain verification are being added to that branch; check that CI matches the final PR head before merging.
- Beta.2.0 remains **NO-GO**. This branch is not a frozen distribution candidate, and no tag or release asset was created in this work.

## Build Entry

`scripts/build-release.mjs` is the single release implementation. `release.ps1` is a compatibility wrapper that forwards `-SkipFrontBuild`, `-UseSccache`, and an optional `-NodePath`.

The build requires the `package.json` Node engine (`^24.15.0`) and the exact pnpm version in `packageManager` (`9.15.9`). It puts the entry Node executable first in the child `PATH`, then verifies that `pnpm exec node` resolves to the same executable and version. The lockfile install uses `--frozen-lockfile`; npm fallback is not permitted.

The ordinary build runs frontend checks and unit tests, Rust formatting, strict Clippy, Rust library tests, Debug/Release clipboard policy tests, and a locked Release build with `custom-protocol`. `beta2-isolation` and automatic Ctrl+C fallback stay disabled. Use `--check-toolchain-only` to verify the Node/pnpm pairing without building. `--no-deliver` does not refresh the local delivery directory.

## Repository Cleanup And Local Verification

- Eleven completed Beta.2 reports were moved under `docs/archive/beta2/`; their historical outcomes are retained. The current release summary and known-issues page remain at the top level. No tracked history or Windows license material was deleted; the 594-file `distribution/windows/` set remains pending final binary-to-component mapping.
- `release.ps1` is now a compatibility wrapper around `scripts/build-release.mjs`; npm fallback was removed. The package scripts include four release-toolchain regression tests.
- On this machine, the release entry check passed with Node `v24.22.1`, pnpm `9.15.9`, and `pnpm exec node` resolving to the same Node executable. `pnpm run check` passed. `pnpm run test:unit` passed 400 cases across 15 scripts. A local scan found no broken relative links in 30 Markdown files. `git diff --check` exited 0; Git reported only the repository's configured line-ending normalization notices.
- Rust tests, Playwright, and a full Release build were not rerun for this cleanup: application Rust/TypeScript sources and dependencies did not change. PR #5's prior CI result belongs to its previous head; the new commit's CI is still required.

## Windows Acceptance

The last recorded native run used the earlier isolated diagnostic candidate. It showed WebView2 startup, a visible ball/HWND, UI Ready, manual hide and restore, settings access, normal exit, child cleanup, and unchanged production settings. The delayed-startup visibility race was **not exercised** in that run; it must not be reported as a native pass for the current branch.

No native EXE was started, no real system clipboard was read or written, and no native backup/restore was performed during this repository closeout. The simulated clipboard writer tests do not substitute for real Windows clipboard acceptance. Issue #2 remains open; one successful historical run does not establish that intermittent startup failures are fixed.

## Licensing and Distribution

Keep the complete `distribution/windows/` material until the final binary-to-component mapping is reviewed. It currently contains 594 tracked files, including license texts and MPL source correspondence. Its checksum review is not proof that every final binary component has been mapped; that release gate remains open.

The final green package must be freshly built from the final source SHA and include the required project and third-party notices, source correspondence, component/checksum indexes, and a SHA256 manifest. Do not distribute PDBs, local logs, user data, WebView2 profiles, test caches, or signing credentials. The ignored local `release/` draft is not a frozen candidate and must not be reused as the final package.

## Install and Upgrade

Verify the published SHA256, extract the green package into a new program directory, and close the running assistant before replacing program files. Preserve the user's configuration, favorites, prompts, and WebView2 data; do not clear AppData as an upgrade step. Use the application's backup/restore flow only after the native backup and recovery gates have been completed.

## Remaining Release Gates

- Run the latest PR commit's CI and review its full diff.
- Validate the current-source EXE in one authorized isolated native session, including the delayed startup event, settings, normal exit, and child cleanup.
- Complete authorized real clipboard acceptance, including publish-failure restoration; recovery can be partial and must remain clearly reported.
- Complete native backup/restore tests with disposable data.
- Review final package contents, privacy scan, license mapping, EXE/PDB/source identity, and SHA256 manifest.
- Keep Issue #2 open until its intermittent failure has adequate evidence; do not infer a fix from a single successful run.
