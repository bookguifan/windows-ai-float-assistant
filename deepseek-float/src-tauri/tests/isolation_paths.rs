//! Runs only the console path probe, never the native app executable.
#![cfg(feature = "beta2-isolation")]
use std::{
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static N: AtomicU64 = AtomicU64::new(0);
fn fresh() -> PathBuf {
    std::env::temp_dir().join(format!(
        "beta2-probe-test-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}
fn command() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_beta2-path-probe"));
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("WEBVIEW2_")
        {
            cmd.env_remove(key);
        }
    }
    cmd
}
fn probe(root: &std::path::Path) -> Output {
    command()
        .arg("--beta2-isolated-data-root")
        .arg(root)
        .output()
        .unwrap()
}
#[test]
fn actual_stores_context_and_webview2_bindings_are_inside_root() {
    // Exercise a Windows case alias even when the caller's TEMP is canonical.
    let root = PathBuf::from(fresh().to_str().unwrap().to_ascii_uppercase());
    let output = probe(&root);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let map: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // The resolver expands OS aliases (including 8.3 TEMP names). Compare with
    // the independently resolved requested directory, not its raw input spelling.
    let canonical = std::fs::canonicalize(&root).unwrap();
    let text = canonical.to_str().unwrap();
    let physical_root = PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(text));
    assert_eq!(PathBuf::from(map["root"].as_str().unwrap()), physical_root);
    assert_eq!(map["native_app_started"], false);
    assert_eq!(map["automatic_ctrl_c"], false);
    assert_eq!(map["fresh_config_written_by_real_store"], true);
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("config/config.json")).unwrap()).unwrap();
    assert_eq!(config["autostart"], false);
    assert_eq!(config.get("hotkey"), Some(&serde_json::Value::Null));
    assert_eq!(config["privileged_capture"], false);
    assert_eq!(config["clipboard_fallback_enabled"], false);
    for key in [
        "config",
        "favorites",
        "prompts",
        "logs",
        "diagnostics",
        "backups",
        "webview2",
        "temp",
    ] {
        let actual = PathBuf::from(map[key].as_str().unwrap());
        assert!(
            actual.starts_with(&physical_root),
            "{key}: {actual:?}, requested root: {root:?}, physical root: {physical_root:?}"
        );
    }
    assert_eq!(
        map["configured_windows"][0]["data_directory"],
        map["webview2"]
    );
    assert!(map["identifier"]
        .as_str()
        .unwrap()
        .starts_with("com.deepseekfloat.beta2.isolated."));
    assert_eq!(map["configured_windows"][0]["create"], false);
    assert_eq!(map["external_file_dialogs"], "disabled");
    assert_eq!(map["autostart_writes"], "disabled");
    // Once the probe has reserved the directory, it cannot be reused for a native run.
    let second = probe(&root);
    assert_eq!(second.status.code(), Some(78));
}
#[test]
fn missing_or_relative_path_fails_closed_without_native_start() {
    for output in [
        command().output().unwrap(),
        probe(std::path::Path::new("relative")),
    ] {
        assert_eq!(output.status.code(), Some(78));
        assert!(output.stdout.is_empty());
    }
}
#[test]
fn inherited_webview2_override_is_rejected_before_root_creation() {
    let root = fresh();
    let output = command()
        .env("WEBVIEW2_USER_DATA_FOLDER", "C:\\must-not-use")
        .arg("--beta2-isolated-data-root")
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(78));
    assert!(!root.exists());
}

#[test]
fn inherited_non_path_webview2_override_is_rejected_before_root_creation() {
    let root = fresh();
    let output = command()
        .env(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--remote-debugging-port=0",
        )
        .arg("--beta2-isolated-data-root")
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(78));
    assert!(!root.exists());
}
#[test]
fn actual_known_folder_official_path_is_rejected_without_reading_user_files() {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT},
    };
    let raw =
        unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None) }.unwrap();
    let base = unsafe { raw.to_string() }.unwrap();
    unsafe { CoTaskMemFree(Some(raw.0.cast())) };
    let root = PathBuf::from(base).join("com.deepseekfloat.desktop");
    // Resolve the protected path, but never read real business data or print it
    // in a failed assertion. Production-file integrity belongs to the separately
    // authorized native evidence capture, not the automated test harness.
    let existed = root.exists();
    let output = probe(&root);
    assert_eq!(output.status.code(), Some(78));
    assert_eq!(root.exists(), existed);
    let valid = probe(&fresh());
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
}

#[test]
fn invalid_embedded_identifier_rejects_before_any_directory_creation() {
    let root = fresh();
    let error = deepseek_float_lib::isolation::initialize(
        vec![
            "probe".into(),
            "--beta2-isolated-data-root".into(),
            root.clone().into_os_string(),
        ],
        "com.deepseekfloat.desktop",
    )
    .unwrap_err();
    assert!(error.contains("identifier"));
    assert!(!root.exists());
}

#[test]
fn uninitialized_storage_consumer_exits_instead_of_returning_defaults() {
    const CHILD: &str = "BETA2_UNINITIALIZED_PATH_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let _ = deepseek_float_lib::isolation::business_dir();
        panic!("isolated consumer silently returned missing/default storage");
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("uninitialized_storage_consumer_exits_instead_of_returning_defaults")
        .arg("--nocapture")
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(78),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("ISOLATION_PATH_REJECTED"));
}
