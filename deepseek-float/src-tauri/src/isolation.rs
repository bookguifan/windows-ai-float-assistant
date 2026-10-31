//! Explicit test-build paths. Initialization precedes singleton, logs and Tauri windows.
//! No environment-only redirection and no fallback from test mode to production paths.
use std::{
    ffi::{OsStr, OsString},
    fs::{File, OpenOptions},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::OnceLock,
};

const FLAG: &str = "--beta2-isolated-data-root";
const REPARSE_POINT: u32 = 0x400;
const DIRECTORY_FLAGS: u32 = 0x0200_0000 | 0x0020_0000;
static PATHS: OnceLock<IsolationPaths> = OnceLock::new();

#[derive(Debug)]
pub struct IsolationPaths {
    pub root: PathBuf,
    pub config_dir: PathBuf,
    pub log_dir: PathBuf,
    pub webview2_dir: PathBuf,
    pub diagnostics_dir: PathBuf,
    pub backup_dir: PathBuf,
    pub temp_dir: PathBuf,
    // Deny delete sharing for the entire ancestor chain throughout this process.
    _pins: Vec<File>,
}

fn normalized_absolute(root: &Path) -> Result<PathBuf, String> {
    let s = root
        .to_str()
        .ok_or("isolation path must be valid Unicode")?
        .replace('/', "\\");
    let bytes = s.as_bytes();
    if bytes.len() < 4 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return Err(
            "require a drive-absolute path; UNC/device/drive-relative paths are not supported"
                .into(),
        );
    }
    let tail = s[3..].trim_end_matches('\\');
    if tail.is_empty() {
        return Err("drive root is forbidden".into());
    }
    for part in tail.split('\\') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part.chars().any(|c| c < ' ' || "<>:\"|?*".contains(c))
        {
            return Err("ambiguous, traversal, ADS or invalid path component".into());
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err("reserved Windows device name".into());
        }
    }
    Ok(PathBuf::from(format!(
        "{}:\\{}",
        (bytes[0] as char).to_ascii_uppercase(),
        tail
    )))
}

fn folded(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_uppercase()
}

fn overlaps(a: &Path, b: &Path) -> bool {
    let a = folded(a);
    let b = folded(b);
    a == b || a.starts_with(&(b.clone() + "\\")) || b.starts_with(&(a + "\\"))
}

fn pin_directory(path: &Path) -> Result<File, String> {
    let meta =
        std::fs::symlink_metadata(path).map_err(|e| format!("inspect {}: {e}", path.display()))?;
    if !meta.is_dir() || meta.file_attributes() & REPARSE_POINT != 0 {
        return Err(format!(
            "not a plain directory (reparse points forbidden): {}",
            path.display()
        ));
    }
    let pin = OpenOptions::new()
        .read(true)
        .share_mode(1 | 2)
        .custom_flags(DIRECTORY_FLAGS)
        .open(path)
        .map_err(|e| format!("pin {}: {e}", path.display()))?;
    if pin.metadata().map_err(|e| e.to_string())?.file_attributes() & REPARSE_POINT != 0 {
        return Err(format!("reparse race at {}", path.display()));
    }
    Ok(pin)
}

// Resolve OS aliases (including 8.3 names) after rejecting and pinning reparse ancestors.
fn canonical_directory(path: &Path) -> Result<PathBuf, String> {
    let canonical =
        std::fs::canonicalize(path).map_err(|e| format!("resolve {}: {e}", path.display()))?;
    let text = canonical.to_str().ok_or("canonical path is not Unicode")?;
    normalized_absolute(Path::new(text.strip_prefix(r"\\?\").unwrap_or(text)))
}
impl IsolationPaths {
    fn prepare(root: &Path, protected: &[PathBuf]) -> Result<Self, String> {
        let root = normalized_absolute(root)?;
        for blocked in protected {
            if overlaps(&root, blocked) {
                return Err(format!("overlaps protected data: {}", blocked.display()));
            }
        }
        // Parent must exist. Never create an unvalidated ancestor chain.
        let parent = root.parent().ok_or("missing parent")?;
        let mut ancestors: Vec<_> = parent.ancestors().collect();
        ancestors.reverse();
        let mut pins = Vec::new();
        for dir in ancestors {
            pins.push(pin_directory(dir)?);
        }
        let exists = match std::fs::symlink_metadata(&root) {
            Ok(_) => {
                pins.push(pin_directory(&root)?);
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.to_string()),
        };
        let root = if exists {
            canonical_directory(&root)?
        } else {
            canonical_directory(parent)?.join(root.file_name().ok_or("missing final component")?)
        };
        for blocked in protected {
            let physical = if blocked.exists() {
                canonical_directory(blocked)?
            } else {
                blocked.clone()
            };
            if overlaps(&root, &physical) {
                return Err(format!(
                    "physical path overlaps protected data: {}",
                    physical.display()
                ));
            }
        }
        match std::fs::symlink_metadata(&root) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&root).map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        }
        pins.push(pin_directory(&root)?);
        if std::fs::read_dir(&root)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
        {
            return Err("isolation root is not empty; historical data must never be reused".into());
        }
        let probe = root.join(".write-permission-probe");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
            .map_err(|e| format!("root not writable: {e}"))?;
        drop(file);
        std::fs::remove_file(probe).map_err(|e| e.to_string())?;
        let config_dir = root.join("config");
        let log_dir = root.join("logs");
        let webview2_dir = root.join("webview2");
        let diagnostics_dir = root.join("diagnostics");
        let backup_dir = root.join("backups");
        let temp_dir = root.join("temp");
        for dir in [
            &config_dir,
            &log_dir,
            &webview2_dir,
            &diagnostics_dir,
            &backup_dir,
            &temp_dir,
        ] {
            std::fs::create_dir(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
            pins.push(pin_directory(dir)?);
        }
        let paths = Self {
            root,
            config_dir,
            log_dir,
            webview2_dir,
            diagnostics_dir,
            backup_dir,
            temp_dir,
            _pins: pins,
        };
        paths.validate()?;
        Ok(paths)
    }

    pub fn validate(&self) -> Result<(), String> {
        for dir in [
            &self.root,
            &self.config_dir,
            &self.log_dir,
            &self.webview2_dir,
            &self.diagnostics_dir,
            &self.backup_dir,
            &self.temp_dir,
        ] {
            if !dir.starts_with(&self.root) {
                return Err("derived path escaped root".into());
            }
            let _check = pin_directory(dir)?;
        }
        Ok(())
    }
}

fn parse_root(args: Vec<OsString>, test_build: bool) -> Result<Option<PathBuf>, String> {
    let mut args = args.into_iter().skip(1);
    let mut root = None;
    while let Some(arg) = args.next() {
        if arg == FLAG {
            if !test_build {
                return Err("isolation option requires the beta2-isolation test build".into());
            }
            if root.is_some() {
                return Err("duplicate isolation root".into());
            }
            root = Some(PathBuf::from(
                args.next().ok_or("missing isolation root argument")?,
            ));
        } else if test_build || arg.to_string_lossy().starts_with(FLAG) {
            return Err(format!(
                "unsupported isolation argument: {}",
                arg.to_string_lossy()
            ));
        }
    }
    if test_build && root.is_none() {
        return Err("test build requires --beta2-isolated-data-root".into());
    }
    Ok(root)
}

fn protected_roots() -> Result<Vec<PathBuf>, String> {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{
            FOLDERID_LocalAppData, FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
        },
    };
    let mut roots = Vec::new();
    for id in [FOLDERID_LocalAppData, FOLDERID_RoamingAppData] {
        let raw = unsafe { SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None) }
            .map_err(|e| format!("Known Folder unavailable: {e}"))?;
        let value = unsafe { raw.to_string() };
        unsafe { CoTaskMemFree(Some(raw.0.cast())) };
        let base = PathBuf::from(value.map_err(|e| e.to_string())?);
        // Deny every assistant identifier directory, including historical test runs.
        roots.push(base.join("com.deepseekfloat.desktop"));
        for entry in std::fs::read_dir(&base).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("com.deepseekfloat.")
            {
                roots.push(entry.path());
            }
        }
    }
    Ok(roots)
}

pub fn active() -> bool {
    cfg!(feature = "beta2-isolation")
}

/// The sole initialization path used by the application and the non-UI probe.
fn validate_identifier(identifier: &str, test_build: bool) -> Result<(), String> {
    if test_build && !identifier.starts_with("com.deepseekfloat.beta2.isolated.") {
        return Err(
            "test candidate requires its own com.deepseekfloat.beta2.isolated.* identifier".into(),
        );
    }
    Ok(())
}

pub fn initialize(
    args: impl IntoIterator<Item = OsString>,
    identifier: &str,
) -> Result<(), String> {
    validate_identifier(identifier, active())?;
    let Some(root) = parse_root(args.into_iter().collect(), active())? else {
        return Ok(());
    };
    if PATHS.get().is_some() {
        return Err("isolation already initialized".into());
    }
    // WebView2 environment settings override API arguments. Reject inherited overrides.
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("WEBVIEW2_")
        {
            return Err(format!(
                "inherited WebView2 override must be removed: {}",
                key.to_string_lossy()
            ));
        }
    }
    let paths = IsolationPaths::prepare(&root, &protected_roots()?)?;
    PATHS
        .set(paths)
        .map_err(|_| "isolation already initialized".to_string())?;
    let p = validated_paths()?;
    // Explicit API data_directory remains authoritative; these also contain child-process temp files.
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &p.webview2_dir);
    std::env::set_var("TEMP", &p.temp_dir);
    std::env::set_var("TMP", &p.temp_dir);
    // Exercise the real atomic config writer, within the validated root only.
    // Fresh diagnostics candidates do not register global input or enable capture.
    crate::config::try_update(|config| {
        config.autostart = false;
        config.hotkey = None;
        config.privileged_capture = false;
        config.clipboard_fallback_enabled = false;
    })?;
    Ok(())
}

fn validated_paths() -> Result<&'static IsolationPaths, String> {
    let paths = PATHS
        .get()
        .ok_or("test isolation not initialized; refusing production fallback")?;
    paths.validate()?;
    Ok(paths)
}

// Consumers must never convert an isolated path error into a default configuration.
fn required_paths() -> &'static IsolationPaths {
    validated_paths().unwrap_or_else(|error| {
        eprintln!("ISOLATION_PATH_REJECTED: {error}");
        std::process::exit(78)
    })
}
fn legacy_business_dir(local: Option<&OsStr>) -> Option<PathBuf> {
    local.map(|p| PathBuf::from(p).join("com.deepseekfloat.desktop"))
}

pub fn business_dir() -> Option<PathBuf> {
    if active() {
        return Some(required_paths().config_dir.clone());
    }
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let dir = legacy_business_dir(Some(OsStr::new(&local)))?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

pub fn diagnostics_dir() -> Option<PathBuf> {
    if active() {
        Some(required_paths().diagnostics_dir.clone())
    } else {
        // Diagnostics lookup was read-only before isolation was introduced.
        let local = std::env::var("LOCALAPPDATA").ok()?;
        legacy_business_dir(Some(OsStr::new(&local)))
    }
}
pub fn log_dir() -> Option<PathBuf> {
    if active() {
        Some(required_paths().log_dir.clone())
    } else {
        None
    }
}
pub fn backup_dir() -> Option<PathBuf> {
    if active() {
        Some(required_paths().backup_dir.clone())
    } else {
        None
    }
}

pub fn singleton_names() -> Result<(String, String), String> {
    if !active() {
        return Ok((
            "Local\\com.deepseekfloat.desktop.singleton.mutex".into(),
            "Local\\com.deepseekfloat.desktop.singleton.event".into(),
        ));
    }
    let p = validated_paths()?;
    // Stable FNV-1a namespace; no production mutex or event is ever opened.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in folded(&p.root).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let prefix = format!("Local\\com.deepseekfloat.beta2.isolated.{hash:016x}");
    Ok((format!("{prefix}.mutex"), format!("{prefix}.event")))
}

pub fn configure_context<R: tauri::Runtime>(
    context: &mut tauri::Context<R>,
) -> Result<Vec<tauri::utils::config::WindowConfig>, String> {
    if !active() {
        return Ok(Vec::new());
    }
    let p = validated_paths()?;
    if !context
        .config()
        .identifier
        .starts_with("com.deepseekfloat.beta2.isolated.")
    {
        return Err(
            "test candidate requires its own com.deepseekfloat.beta2.isolated.* identifier".into(),
        );
    }
    let mut deferred = Vec::new();
    for window in &mut context.config_mut().app.windows {
        window.data_directory = Some(p.webview2_dir.clone());
        if window.create {
            deferred.push(window.clone());
            // tauri-runtime 2.11.3 drops WindowConfig.data_directory during conversion.
            // Prevent Tauri auto-build from mkdir-ing LocalData/identifier before setup.
            window.create = false;
        }
    }
    Ok(deferred)
}

/// Apply the directory after from_config, before the manager can create a fallback.
/// Ordinary builds keep their original Tauri automatic creation path.
pub fn create_configured_windows<R: tauri::Runtime>(
    app: &tauri::App<R>,
    windows: &[tauri::utils::config::WindowConfig],
) -> Result<(), String> {
    if !active() {
        if !windows.is_empty() {
            return Err("deferred windows require beta2-isolation".into());
        }
        return Ok(());
    }
    for window in windows {
        let p = validated_paths()?;
        if window.data_directory.as_ref() != Some(&p.webview2_dir) {
            return Err(format!(
                "window {} data directory escaped isolation",
                window.label
            ));
        }
        tauri::WebviewWindowBuilder::from_config(app.handle(), window)
            .map_err(|e| e.to_string())?
            .data_directory(p.webview2_dir.clone())
            .build()
            .map_err(|e| format!("isolated window {}: {e}", window.label))?;
        crate::diag::log(&format!(
            "[isolation-window] label={} data_directory={}",
            window.label,
            p.webview2_dir.display()
        ));
    }
    Ok(())
}

pub fn webview_builder<'a, R: tauri::Runtime, M: tauri::Manager<R>, L: Into<String>>(
    manager: &'a M,
    label: L,
    url: tauri::WebviewUrl,
) -> tauri::WebviewWindowBuilder<'a, R, M> {
    let builder = tauri::WebviewWindowBuilder::new(manager, label, url);
    if active() {
        builder.data_directory(
            validated_paths()
                .expect("invalid isolation: refusing WebView creation")
                .webview2_dir
                .clone(),
        )
    } else {
        builder
    }
}

/// Inspect actual application bindings, not just the requested command-line root.
/// This function creates no app, runtime, window or WebView2 process.
pub fn probe_manifest<R: tauri::Runtime>(
    context: &tauri::Context<R>,
) -> Result<serde_json::Value, String> {
    let p = validated_paths()?;
    let files = [
        crate::config::store_path(),
        Some(crate::favorites::store()?),
        crate::prompts::store_path(),
    ];
    for file in &files {
        if !file.as_ref().is_some_and(|f| f.starts_with(&p.config_dir)) {
            return Err("business store not isolated".into());
        }
    }
    for window in &context.config().app.windows {
        if window.data_directory.as_ref() != Some(&p.webview2_dir) {
            return Err(format!("window {} not isolated", window.label));
        }
    }
    let diagnostic_log = crate::diag::log_path().ok_or("diagnostic log path unavailable")?;
    if !diagnostic_log.starts_with(&p.diagnostics_dir) {
        return Err("diagnostic log escaped isolation".into());
    }
    let config = crate::config::load();
    if config.autostart
        || config.hotkey.is_some()
        || config.privileged_capture
        || config.clipboard_fallback_enabled
    {
        return Err("unsafe isolated startup configuration".into());
    }
    if crate::autostart::registration_supported()
        || crate::autostart::is_autostart()
        || crate::autostart::dirty_self_registration().is_some()
    {
        return Err("isolated autostart guard failed".into());
    }
    if crate::favorites::export("synthetic", "synthetic").is_ok()
        || crate::favorites::open_exports().is_ok()
    {
        return Err("isolated external export guard failed".into());
    }
    Ok(serde_json::json!({
        "mode": "beta2-isolation", "native_app_started": false,
        "identifier": context.config().identifier, "root": p.root,
        "config": files[0], "favorites": files[1], "prompts": files[2],
        "logs": log_dir(), "diagnostics": diagnostics_dir(), "diagnostic_log": diagnostic_log, "backups": backup_dir(),
        "state_log": p.diagnostics_dir.join("poc2-state.log"), "panic_log": p.diagnostics_dir.join("panic.log"), "crash_summary": p.diagnostics_dir.join("last_crash.json"),
        "restore_journal": p.config_dir.join(".backup-restore-journal"), "fresh_config_written_by_real_store": p.config_dir.join("config.json").is_file(),
        "webview2": p.webview2_dir, "temp": p.temp_dir,
        "configured_windows": context.config().app.windows.iter().map(|w| serde_json::json!({"label":w.label,"data_directory":w.data_directory,"create":w.create})).collect::<Vec<_>>(),
        "singleton": singleton_names()?, "external_file_dialogs": "disabled", "autostart_writes": "disabled",
        "automatic_ctrl_c": crate::release_policy::automatic_copy_allowed(true),
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    fn fresh() -> PathBuf {
        std::env::temp_dir().join(format!(
            "beta2-path-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }
    #[test]
    fn valid_root_derives_every_persistent_directory() {
        let root = fresh();
        let p = IsolationPaths::prepare(&root, &[]).unwrap();
        for dir in [
            &p.config_dir,
            &p.log_dir,
            &p.webview2_dir,
            &p.diagnostics_dir,
            &p.backup_dir,
            &p.temp_dir,
        ] {
            assert!(dir.starts_with(&p.root));
            assert!(dir.is_dir());
        }
        assert_eq!(
            p.config_dir.join("config.json"),
            p.root.join("config/config.json")
        );
        assert_eq!(p.webview2_dir, p.root.join("webview2"));
    }
    #[test]
    fn missing_root_is_production_only_without_test_feature() {
        assert_eq!(parse_root(vec!["app".into()], false).unwrap(), None);
        assert!(parse_root(vec!["app".into()], true).is_err());
    }
    #[test]
    fn override_is_rejected_by_ordinary_build() {
        assert!(parse_root(
            vec!["app".into(), FLAG.into(), fresh().into_os_string()],
            false
        )
        .is_err());
    }
    #[test]
    fn duplicate_or_missing_argument_is_rejected() {
        assert!(parse_root(vec!["app".into(), FLAG.into()], true).is_err());
        assert!(parse_root(
            vec![
                "app".into(),
                FLAG.into(),
                fresh().into_os_string(),
                FLAG.into(),
                fresh().into_os_string()
            ],
            true
        )
        .is_err());
    }
    #[test]
    fn relative_drive_relative_and_traversal_are_rejected() {
        for bad in [
            "relative",
            "C:relative",
            "\\rooted",
            "C:\\safe\\..\\other",
            "C:\\safe\\.\\other",
        ] {
            assert!(
                IsolationPaths::prepare(Path::new(bad), &[]).is_err(),
                "{bad}"
            );
        }
    }
    #[test]
    fn unc_device_ads_and_ambiguous_names_are_rejected() {
        for bad in [
            "\\\\server\\share\\test",
            "\\\\?\\C:\\test",
            "C:\\test:stream",
            "C:\\test.\\child",
            "C:\\CON\\child",
            "C:\\test \\child",
        ] {
            assert!(
                IsolationPaths::prepare(Path::new(bad), &[]).is_err(),
                "{bad}"
            );
        }
    }
    #[test]
    fn official_root_ancestors_descendants_and_case_variants_are_rejected() {
        let official = fresh().join("com.deepseekfloat.desktop");
        for root in [
            official.clone(),
            official.join("test"),
            official.parent().unwrap().to_path_buf(),
            PathBuf::from(official.to_string_lossy().to_uppercase()),
        ] {
            assert!(IsolationPaths::prepare(&root, std::slice::from_ref(&official)).is_err());
            assert!(!official.exists());
        }
    }
    #[test]
    fn existing_nonempty_root_is_rejected_without_touching_content() {
        let root = fresh();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("sentinel"), b"keep").unwrap();
        assert!(IsolationPaths::prepare(&root, &[]).is_err());
        assert_eq!(std::fs::read(root.join("sentinel")).unwrap(), b"keep");
    }
    #[test]
    fn existing_empty_root_is_accepted_and_second_use_is_rejected() {
        let root = fresh();
        std::fs::create_dir(&root).unwrap();
        let first = IsolationPaths::prepare(&root, &[]).unwrap();
        assert!(IsolationPaths::prepare(&root, &[]).is_err());
        drop(first);
    }
    #[test]
    fn file_root_is_rejected() {
        let root = fresh();
        std::fs::write(&root, b"keep").unwrap();
        assert!(IsolationPaths::prepare(&root, &[]).is_err());
    }
    #[test]
    fn junction_escape_is_rejected_and_target_is_unchanged() {
        let parent = fresh();
        let target = fresh();
        std::fs::create_dir(&parent).unwrap();
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("sentinel"), b"keep").unwrap();
        let link = parent.join("junction");
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(IsolationPaths::prepare(&link.join("escape"), &[]).is_err());
        assert_eq!(std::fs::read(target.join("sentinel")).unwrap(), b"keep");
        assert!(!target.join("escape").exists());
        std::fs::remove_dir(&link).unwrap();
    }
    #[test]
    fn pinned_root_cannot_be_renamed() {
        let root = fresh();
        let p = IsolationPaths::prepare(&root, &[]).unwrap();
        assert!(std::fs::rename(&root, root.with_extension("moved")).is_err());
        assert!(p.validate().is_ok());
    }
    #[test]
    fn production_directory_selection_stays_legacy() {
        let base = Path::new("C:\\SyntheticLocal");
        assert_eq!(
            legacy_business_dir(Some(base.as_os_str())),
            Some(base.join("com.deepseekfloat.desktop"))
        );
        assert!(legacy_business_dir(None).is_none());
    }

    #[test]
    fn short_path_alias_cannot_create_a_child_inside_protected_data() {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetShortPathNameW(long: *const u16, short: *mut u16, capacity: u32) -> u32;
        }
        let parent = fresh();
        std::fs::create_dir(&parent).unwrap();
        let official = parent.join("ProtectedApplicationLongDirectoryName");
        std::fs::create_dir(&official).unwrap();
        let wide: Vec<_> = official.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut buffer = vec![0u16; 32768];
        let length =
            unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
        assert!(length > 0 && length < buffer.len() as u32);
        let alias = PathBuf::from(String::from_utf16(&buffer[..length as usize]).unwrap());
        let target = alias.join("must-not-create");
        assert!(IsolationPaths::prepare(&target, std::slice::from_ref(&official)).is_err());
        assert!(!official.join("must-not-create").exists());
    }

    #[test]
    fn identifier_validation_is_test_only_and_requires_distinct_namespace() {
        assert!(validate_identifier("com.deepseekfloat.desktop", false).is_ok());
        assert!(validate_identifier("com.deepseekfloat.desktop", true).is_err());
        assert!(validate_identifier("com.deepseekfloat.beta2.clean20261031", true).is_err());
        assert!(validate_identifier("com.deepseekfloat.beta2.isolated.test", true).is_ok());
    }
}
