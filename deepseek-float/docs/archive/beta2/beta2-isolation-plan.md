# Beta.2 explicit test isolation

The `beta2-isolation` build feature requires an explicit drive-absolute, absent or empty data root and a distinct `com.deepseekfloat.beta2.isolated.*` identifier. Ordinary production builds retain their existing paths and window creation behavior. Invalid test isolation exits before fallback or window creation.

The isolation contract is: persistence written by the test candidate and its controlled child processes must remain under the validated root; production business data and autostart entries must remain unchanged. External system/security-tool writes are not application writes unless attributed. Unattributed events remain unresolved, and original captured global events must be retained, never hidden by narrowing observation.

All test persistence derives from the validated root: config, logs, diagnostics, webview2, backups and temp. The resolver rejects protected OS Known Folders, traversal, UNC/device/ambiguous paths and reparse ancestors; held directory handles prevent replacement. Autostart writes, external file dialogs and automatic clipboard fallback are disabled in isolation mode.

Configured windows need special handling with the locked Tauri dependency: changing `WindowConfig.data_directory` alone does not propagate through its runtime conversion. Isolated configured windows are deferred, then created at the start of application setup using `from_config(...).data_directory(...)` before the ball shape/position/Ready gates. Dynamic windows keep the shared explicit builder. Non-isolated windows retain Tauri's automatic setup.

Regression coverage includes the real Tauri manager with MockRuntime and a fallback confined to a disposable fixture, path rejection and actual store bindings. The mock test creates no HWND or WebView2 instance. Common Controls v6 is embedded in integration-test EXEs only because the test-linked Tauri menu code imports TaskDialogIndirect. CI runs the isolation feature with its own embedded identifier as well as ordinary production checks.

The console path probe does not run the application or create native windows. Its success and mock-manager regression are prerequisites, not native acceptance. A new diagnostic EXE/PDB must be frozen separately, and any native regression requires separate explicit authorization. Keep old candidates and unknown historical directories unchanged. Never upload diagnostic PDBs or local user evidence. Beta.2 remains NO-GO until the remaining release gates close.
