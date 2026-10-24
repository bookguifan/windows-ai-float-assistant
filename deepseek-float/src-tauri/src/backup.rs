//! Versioned user-data backup and crash-recoverable restore.
//!
//! Export is built from an explicit field whitelist. Restore coordinates the existing config,
//! prompt, and favorites stores; it does not include the WebView profile or the application data
//! directory as a whole.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Manager};

const FORMAT: &str = "deepseek-float-backup";
const VERSION: u32 = 1;
const MAX_BACKUP_BYTES: usize = 10 * 1024 * 1024;
const MAX_PROMPTS: usize = 4 + crate::prompts::MAX_CUSTOM;
const MAX_SITES: usize = 20;
const MAX_FAVORITES: usize = 5_000;
const MAX_ID_CHARS: usize = 128;
const MAX_TITLE_CHARS: usize = 200;
const MAX_QUESTION_CHARS: usize = 100_000;
const MAX_ANSWER_CHARS: usize = 500_000;
const MAX_SOURCE_CHARS: usize = 512;
const JOURNAL_DIR: &str = ".backup-restore-journal";
const JOURNAL_VERSION: u8 = 1;
const STORE_FILES: [&str; 3] = ["config.json", "prompts.json", "favorites.json"];
const MAX_CONFLICT_DETAILS: usize = 12;

static NEXT_TICKET: AtomicU64 = AtomicU64::new(1);
static PENDING_IMPORT: Mutex<Option<PendingImport>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PortableSettings {
    #[serde(default)]
    hide_on_fullscreen: bool,
    #[serde(default)]
    privileged_capture: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clipboard_fallback_enabled: Option<bool>,
    #[serde(default = "default_hotkey")]
    hotkey: Option<String>,
}

impl Default for PortableSettings {
    fn default() -> Self {
        Self {
            hide_on_fullscreen: false,
            privileged_capture: false,
            clipboard_fallback_enabled: None,
            hotkey: default_hotkey(),
        }
    }
}

fn default_hotkey() -> Option<String> {
    Some("Ctrl+Alt+1".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct BackupData {
    prompts: Vec<crate::prompts::Prompt>,
    sites: Vec<crate::panel_site::Site>,
    #[serde(default)]
    current_site: Option<String>,
    #[serde(default)]
    settings: PortableSettings,
    favorites: Vec<crate::favorites::Favorite>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct BackupFile {
    format: String,
    version: u32,
    created_at: String,
    app_version: String,
    data: BackupData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct BackupCounts {
    prompts: usize,
    custom_sites: usize,
    favorites: usize,
    settings: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct BackupExportResult {
    file_name: String,
    path: String,
    counts: BackupCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct BackupPreview {
    ticket: u64,
    file_name: String,
    created_at: String,
    app_version: String,
    current_site: Option<String>,
    counts: BackupCounts,
    conflict_count: usize,
    conflict_details: Vec<String>,
    conflicts_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct BackupRestoreResult {
    mode: String,
    prompts: usize,
    sites: usize,
    favorites: usize,
    settings: usize,
    conflicts: usize,
    restart_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestoreMode {
    Merge,
    Overwrite,
}

impl RestoreMode {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "merge" => Ok(Self::Merge),
            "overwrite" => Ok(Self::Overwrite),
            _ => Err("恢复模式无效，请重新选择合并或覆盖".into()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Overwrite => "overwrite",
        }
    }
}

#[derive(Clone)]
struct DataSnapshot {
    config: crate::config::AppConfig,
    prompts: Vec<crate::prompts::Prompt>,
    favorites: Vec<crate::favorites::Favorite>,
}

struct PendingImport {
    ticket: u64,
    revision: u64,
    backup: BackupFile,
    preview: BackupPreview,
}

struct RestorePlan {
    config: crate::config::AppConfig,
    prompts: Vec<crate::prompts::Prompt>,
    favorites: Vec<crate::favorites::Favorite>,
    counts: BackupCounts,
    conflicts: ConflictReport,
}

#[derive(Default)]
struct ConflictReport {
    count: usize,
    details: Vec<String>,
}

impl ConflictReport {
    fn push(&mut self, detail: String) {
        self.count += 1;
        if self.details.len() < MAX_CONFLICT_DETAILS {
            self.details.push(detail);
        }
    }

    fn truncated(&self) -> bool {
        self.count > self.details.len()
    }
}

fn snapshot_under_data_lock() -> Result<DataSnapshot, String> {
    Ok(DataSnapshot {
        config: crate::config::load_under_data_lock(),
        prompts: crate::prompts::load_under_data_lock(),
        favorites: crate::favorites::list_under_data_lock()?,
    })
}

fn backup_data(snapshot: &DataSnapshot) -> Result<BackupData, String> {
    let settings = PortableSettings {
        hide_on_fullscreen: snapshot.config.hide_on_fullscreen,
        privileged_capture: snapshot.config.privileged_capture,
        clipboard_fallback_enabled: Some(crate::release_policy::automatic_copy_allowed(
            snapshot.config.clipboard_fallback_enabled,
        )),
        hotkey: snapshot.config.hotkey.clone(),
    };
    let data = BackupData {
        prompts: snapshot.prompts.clone(),
        sites: snapshot.config.custom_sites.clone(),
        current_site: snapshot.config.panel_url.clone(),
        settings,
        favorites: snapshot.favorites.clone(),
    };
    validate_data(data)
}

fn make_backup(snapshot: &DataSnapshot) -> Result<BackupFile, String> {
    Ok(BackupFile {
        format: FORMAT.into(),
        version: VERSION,
        created_at: crate::diag::now_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        data: backup_data(snapshot)?,
    })
}

fn counts(data: &BackupData) -> BackupCounts {
    BackupCounts {
        prompts: data.prompts.len(),
        custom_sites: data.sites.len(),
        favorites: data.favorites.len(),
        // Portable preferences; selected site is shown separately.
        settings: 3 + usize::from(data.settings.clipboard_fallback_enabled.is_some()),
    }
}

fn encode_backup(snapshot: &DataSnapshot) -> Result<(Vec<u8>, BackupCounts), String> {
    let backup = make_backup(snapshot)?;
    let counts = counts(&backup.data);
    let bytes = serde_json::to_vec_pretty(&backup).map_err(|e| format!("备份序列化失败：{e}"))?;
    if bytes.len() > MAX_BACKUP_BYTES {
        return Err(format!(
            "备份数据超过 {} MiB 上限，未写入文件",
            MAX_BACKUP_BYTES / 1024 / 1024
        ));
    }
    Ok((bytes, counts))
}

fn decode_backup(bytes: &[u8]) -> Result<BackupFile, String> {
    if bytes.len() > MAX_BACKUP_BYTES {
        return Err(format!(
            "备份文件超过 {} MiB 上限",
            MAX_BACKUP_BYTES / 1024 / 1024
        ));
    }
    let raw: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("备份文件损坏或不是有效 JSON：{e}"))?;
    reject_unknown_fields(
        &raw,
        &["format", "version", "created_at", "app_version", "data"],
        "根对象",
    )?;
    let raw_data = raw.get("data").ok_or("备份缺少 data 字段")?;
    reject_unknown_fields(
        raw_data,
        &["prompts", "sites", "current_site", "settings", "favorites"],
        "data",
    )?;
    if let Some(settings) = raw_data.get("settings") {
        reject_unknown_fields(
            settings,
            &[
                "hide_on_fullscreen",
                "privileged_capture",
                "clipboard_fallback_enabled",
                "hotkey",
            ],
            "data.settings",
        )?;
    }
    reject_array_item_fields(
        raw_data,
        "prompts",
        &[
            "id",
            "label",
            "instruction",
            "is_builtin",
            "hidden",
            "show_card",
        ],
    )?;
    reject_array_item_fields(raw_data, "sites", &["label", "url"])?;
    reject_array_item_fields(
        raw_data,
        "favorites",
        &[
            "id",
            "title",
            "question",
            "answer_markdown",
            "source",
            "created_at",
        ],
    )?;
    let mut backup: BackupFile =
        serde_json::from_value(raw).map_err(|e| format!("备份文件损坏或字段类型不匹配：{e}"))?;
    if backup.format != FORMAT {
        return Err("备份文件格式标识不匹配".into());
    }
    if backup.version > VERSION {
        return Err(format!(
            "备份版本 {} 高于本应用支持的版本 {}，请升级应用后导入",
            backup.version, VERSION
        ));
    }
    if backup.version != VERSION {
        return Err(format!("不支持备份版本 {}", backup.version));
    }
    if !valid_iso8601(&backup.created_at) {
        return Err("备份创建时间格式无效".into());
    }
    if backup.app_version.is_empty() || backup.app_version.chars().count() > 64 {
        return Err("备份来源应用版本无效".into());
    }
    backup.data = validate_data(backup.data)?;
    let normalized = serde_json::to_vec(&backup).map_err(|e| format!("备份校验失败：{e}"))?;
    if normalized.len() > MAX_BACKUP_BYTES {
        return Err("校验后的备份数据超过大小上限".into());
    }
    Ok(backup)
}

fn reject_unknown_fields(
    value: &serde_json::Value,
    allowed: &[&str],
    context: &str,
) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!(
            "备份包含不兼容字段 {context}.{key}，为避免丢失数据已拒绝导入"
        ));
    }
    Ok(())
}

fn reject_array_item_fields(
    data: &serde_json::Value,
    field: &str,
    allowed: &[&str],
) -> Result<(), String> {
    let Some(items) = data.get(field).and_then(serde_json::Value::as_array) else {
        return Ok(());
    };
    for (index, item) in items.iter().enumerate() {
        reject_unknown_fields(item, allowed, &format!("data.{field}[{index}]"))?;
    }
    Ok(())
}

fn validate_data(mut data: BackupData) -> Result<BackupData, String> {
    if data.prompts.len() > MAX_PROMPTS {
        return Err(format!("提示词数量超过 {} 条", MAX_PROMPTS));
    }
    if data.sites.len() > MAX_SITES {
        return Err(format!("自定义站点数量超过 {} 个", MAX_SITES));
    }
    if data.favorites.len() > MAX_FAVORITES {
        return Err(format!("收藏数量超过 {} 条", MAX_FAVORITES));
    }

    let mut prompt_ids = HashSet::with_capacity(data.prompts.len());
    let mut builtin_ids = HashSet::new();
    let mut custom_count = 0usize;
    for prompt in &data.prompts {
        if prompt.id.is_empty()
            || prompt.id.chars().count() > MAX_ID_CHARS
            || !prompt
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        {
            return Err("提示词 ID 格式无效".into());
        }
        if !prompt_ids.insert(prompt.id.clone()) {
            return Err(format!("提示词 ID 重复：{}", prompt.id));
        }
        if prompt.label.is_empty()
            || prompt.label.trim() != prompt.label
            || prompt.label.chars().count() > crate::prompts::MAX_LABEL_CHARS
            || prompt.instruction.trim() != prompt.instruction
            || prompt.instruction.chars().count() > crate::prompts::MAX_INSTRUCTION_CHARS
        {
            return Err(format!("提示词「{}」名称或内容长度无效", prompt.label));
        }
        if contains_credential_assignment(&prompt.label)
            || contains_credential_assignment(&prompt.instruction)
        {
            return Err(format!(
                "提示词「{}」中检测到疑似密钥或认证凭据，请先移除后再备份或导入",
                prompt.label
            ));
        }
        if crate::prompts::BUILTIN_IDS.contains(&prompt.id.as_str()) {
            if !prompt.is_builtin {
                return Err(format!("内置提示词「{}」标记不匹配", prompt.id));
            }
            builtin_ids.insert(prompt.id.as_str());
        } else if prompt.is_builtin {
            return Err(format!("提示词「{}」使用了未知的内置 ID", prompt.id));
        } else {
            custom_count += 1;
        }
    }
    if custom_count > crate::prompts::MAX_CUSTOM {
        return Err(format!(
            "自定义提示词超过 {} 条",
            crate::prompts::MAX_CUSTOM
        ));
    }
    for id in crate::prompts::BUILTIN_IDS {
        if !builtin_ids.contains(id) {
            return Err(format!("备份缺少内置提示词「{id}」"));
        }
    }

    data.sites = crate::panel_site::normalize_sites(data.sites)?;
    for site in &data.sites {
        if contains_credential_assignment(&site.label) {
            return Err(format!(
                "站点「{}」名称中检测到疑似密钥或认证凭据，请先移除后再备份或导入",
                site.label
            ));
        }
        validate_portable_url(&site.url)?;
    }
    if let Some(current) = data.current_site.as_mut() {
        *current = validate_portable_url(current)?;
    }
    if let Some(hotkey) = data.settings.hotkey.as_deref() {
        if hotkey.trim().is_empty() {
            data.settings.hotkey = None;
        } else if hotkey.trim() != hotkey || crate::hotkey::parse(hotkey).is_err() {
            return Err("备份中的截图快捷键无效".into());
        }
    }

    let mut favorite_ids = HashSet::with_capacity(data.favorites.len());
    for item in &data.favorites {
        if item.id == 0 || !favorite_ids.insert(item.id) {
            return Err(format!("收藏 ID 无效或重复：{}", item.id));
        }
        if item.title.chars().count() > MAX_TITLE_CHARS
            || item.question.chars().count() > MAX_QUESTION_CHARS
            || item.answer_markdown.is_empty()
            || item.answer_markdown.chars().count() > MAX_ANSWER_CHARS
            || item.source.chars().count() > MAX_SOURCE_CHARS
        {
            return Err(format!("收藏 {} 的字段超过允许长度或答案为空", item.id));
        }
        if contains_credential_assignment(&item.title)
            || contains_credential_assignment(&item.question)
            || contains_credential_assignment(&item.answer_markdown)
            || contains_credential_assignment(&item.source)
        {
            return Err(format!(
                "收藏「{}」中检测到疑似密钥或认证凭据，请先从收藏内容中移除后再备份或导入",
                item.title
            ));
        }
    }
    Ok(data)
}

fn validate_portable_url(raw: &str) -> Result<String, String> {
    if raw.chars().count() > 4096 {
        return Err("备份中的站点地址超过 4096 字符".into());
    }
    if !crate::config::is_valid_panel_url(raw) {
        return Err("备份中的站点地址无效，请使用有效的 http(s) 地址".into());
    }
    let url = tauri::Url::parse(raw).map_err(|_| "备份中的站点地址无法解析")?;
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err("备份中的站点地址不能包含用户名或密码".into());
    }
    const SENSITIVE_QUERY_KEYS: &[&str] = &[
        "token",
        "access_token",
        "refresh_token",
        "api_key",
        "apikey",
        "key",
        "password",
        "passwd",
        "secret",
        "authorization",
        "auth",
    ];
    let query_has_credential = url
        .query_pairs()
        .any(|(name, _)| SENSITIVE_QUERY_KEYS.contains(&name.to_ascii_lowercase().as_str()));
    let fragment_has_credential = url
        .fragment()
        .and_then(|fragment| tauri::Url::parse(&format!("https://backup.invalid/?{fragment}")).ok())
        .is_some_and(|fragment_url| {
            fragment_url
                .query_pairs()
                .any(|(name, _)| SENSITIVE_QUERY_KEYS.contains(&name.to_ascii_lowercase().as_str()))
        });
    if query_has_credential || fragment_has_credential {
        return Err("备份中的站点地址包含疑似凭据参数，请移除后再备份或导入".into());
    }
    Ok(url.to_string())
}

fn contains_credential_assignment(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "api_key",
        "api-key",
        "apikey",
        "access_token",
        "access-token",
        "refresh_token",
        "refresh-token",
        "password",
        "passwd",
        "client_secret",
        "client-secret",
        "authorization",
        "bearer",
        "ghp_",
        "github_pat_",
        "sk-",
    ];
    let lower = text.to_ascii_lowercase();
    KEYS.iter().any(|key| {
        let mut start = 0;
        while let Some(offset) = lower[start..].find(key) {
            let at = start + offset;
            if key.ends_with('_') || *key == "sk-" {
                let tail = &lower[at + key.len()..];
                if tail
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .count()
                    >= 12
                {
                    return true;
                }
            } else {
                let tail = &lower[at + key.len()..];
                let prefix = &lower[..at];
                let separator = tail.trim_start().chars().next();
                let starts_field = prefix
                    .chars()
                    .last()
                    .map_or(true, |c| !(c.is_ascii_alphanumeric() || c == '_'));
                if starts_field && matches!(separator, Some('=') | Some(':')) {
                    let value = tail.trim_start_matches(|c: char| {
                        c.is_whitespace() || matches!(c, '=' | ':' | '\'' | '"' | '`')
                    });
                    if value
                        .chars()
                        .take_while(|c| {
                            !c.is_whitespace() && !matches!(c, '\'' | '"' | '`' | ',' | ';')
                        })
                        .count()
                        >= 8
                    {
                        return true;
                    }
                }
            }
            start = at + key.len();
            if start >= lower.len() {
                break;
            }
        }
        false
    })
}

fn valid_iso8601(raw: &str) -> bool {
    let b = raw.as_bytes();
    if b.len() < 20
        || b.get(4) != Some(&b'-')
        || b.get(7) != Some(&b'-')
        || b.get(10) != Some(&b'T')
        || b.get(13) != Some(&b':')
        || b.get(16) != Some(&b':')
    {
        return false;
    }
    let digit = |range: std::ops::Range<usize>| {
        range.end <= b.len() && b[range].iter().all(u8::is_ascii_digit)
    };
    if !(digit(0..4)
        && digit(5..7)
        && digit(8..10)
        && digit(11..13)
        && digit(14..16)
        && digit(17..19))
    {
        return false;
    }
    let pair = |at: usize| (b[at] - b'0') * 10 + (b[at + 1] - b'0');
    let month = pair(5);
    let day = pair(8);
    let hour = pair(11);
    let minute = pair(14);
    let second = pair(17);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return false;
    }
    let zone = &raw[19..];
    if zone == "Z" {
        return true;
    }
    let zone = zone.strip_prefix('+').or_else(|| zone.strip_prefix('-'));
    zone.is_some_and(|z| {
        z.len() == 5
            && z.as_bytes()[2] == b':'
            && z.as_bytes()[..2].iter().all(u8::is_ascii_digit)
            && z.as_bytes()[3..].iter().all(u8::is_ascii_digit)
    })
}

fn plan_restore(local: &DataSnapshot, backup: &BackupData, mode: RestoreMode) -> RestorePlan {
    let mut plan = match mode {
        RestoreMode::Overwrite => {
            let mut config = local.config.clone();
            config.panel_url = backup.current_site.clone();
            config.custom_sites = backup.sites.clone();
            config.hide_on_fullscreen = backup.settings.hide_on_fullscreen;
            config.privileged_capture = backup.settings.privileged_capture;
            if let Some(enabled) = backup.settings.clipboard_fallback_enabled {
                config.clipboard_fallback_enabled = enabled;
            }
            config.hotkey = backup.settings.hotkey.clone();
            RestorePlan {
                config,
                prompts: backup.prompts.clone(),
                favorites: backup.favorites.clone(),
                counts: counts(backup),
                conflicts: ConflictReport::default(),
            }
        }
        RestoreMode::Merge => merge_plan(local, backup),
    };
    plan.config.enforce_release_policy();
    plan
}

fn merge_plan(local: &DataSnapshot, backup: &BackupData) -> RestorePlan {
    let mut config = local.config.clone();
    let mut conflicts = ConflictReport::default();
    let defaults = crate::prompts::defaults();
    let mut prompts = local.prompts.clone();
    let mut prompts_restored = 0usize;

    for imported in &backup.prompts {
        if let Some(index) = prompts.iter().position(|p| p.id == imported.id) {
            if prompts[index] == *imported {
                continue;
            }
            if imported.is_builtin
                && defaults
                    .iter()
                    .any(|default| default.id == imported.id && prompts[index] == *default)
            {
                prompts[index] = imported.clone();
                prompts_restored += 1;
                continue;
            }
            conflicts.push(format!(
                "提示词「{}」ID 内容冲突，合并时保留本机版本",
                imported.label
            ));
            continue;
        }
        if let Some(existing) = prompts
            .iter()
            .find(|p| p.label.eq_ignore_ascii_case(&imported.label))
        {
            if same_prompt_content(existing, imported) {
                continue;
            }
            conflicts.push(format!(
                "提示词「{}」名称已存在且内容不同，合并时保留本机版本",
                imported.label
            ));
            continue;
        }
        let custom_count = prompts.iter().filter(|p| !p.is_builtin).count();
        if custom_count >= crate::prompts::MAX_CUSTOM {
            conflicts.push(format!(
                "提示词「{}」因本机自定义提示词已达上限而跳过",
                imported.label
            ));
            continue;
        }
        prompts.push(imported.clone());
        prompts_restored += 1;
    }

    let mut sites = config.custom_sites.clone();
    let mut sites_restored = 0usize;
    for imported in &backup.sites {
        if let Some(existing) = sites.iter().find(|s| same_site_url(&s.url, &imported.url)) {
            if existing.label == imported.label {
                continue;
            }
            conflicts.push(format!(
                "站点地址「{}」已存在，合并时保留本机名称",
                imported.label
            ));
            continue;
        }
        if let Some(existing) = sites
            .iter()
            .find(|s| s.label.eq_ignore_ascii_case(&imported.label))
        {
            conflicts.push(format!(
                "站点名称「{}」对应不同地址，合并时保留本机地址",
                existing.label
            ));
            continue;
        }
        if sites.len() >= MAX_SITES {
            conflicts.push(format!(
                "站点「{}」因本机自定义站点已达上限而跳过",
                imported.label
            ));
            continue;
        }
        sites.push(imported.clone());
        sites_restored += 1;
    }
    config.custom_sites = sites;

    if same_optional_site_url(config.panel_url.as_deref(), backup.current_site.as_deref()) {
        // Already selected locally.
    } else if config.panel_url.is_none() {
        config.panel_url = backup.current_site.clone();
    } else {
        conflicts.push("当前面板地址与备份不同，合并时保留本机地址".into());
    }

    merge_bool_setting(
        &mut config.hide_on_fullscreen,
        backup.settings.hide_on_fullscreen,
        "全屏隐藏设置",
        &mut conflicts,
    );
    merge_bool_setting(
        &mut config.privileged_capture,
        backup.settings.privileged_capture,
        "提权划词设置",
        &mut conflicts,
    );
    if backup
        .settings
        .clipboard_fallback_enabled
        .is_some_and(|value| value != config.clipboard_fallback_enabled)
    {
        conflicts
            .push("自动复制回退与本机设置不同，合并时保留本机选择（不自动开启实验性功能）".into());
    }
    if config.hotkey != backup.settings.hotkey {
        if config.hotkey == crate::config::AppConfig::default().hotkey {
            config.hotkey = backup.settings.hotkey.clone();
        } else {
            conflicts.push("截图快捷键与本机设置不同，合并时保留本机快捷键".into());
        }
    }

    let mut favorites = local.favorites.clone();
    let mut favorites_restored = 0usize;
    for imported in &backup.favorites {
        if let Some(existing) = favorites.iter().find(|f| f.id == imported.id) {
            if existing != imported {
                conflicts.push(format!(
                    "收藏 ID {} 内容不同，合并时保留本机版本",
                    imported.id
                ));
            }
            continue;
        }
        if favorites.iter().any(|f| {
            f.question == imported.question && f.answer_markdown == imported.answer_markdown
        }) {
            conflicts.push(format!(
                "收藏「{}」的问题与回答已存在，合并时保留本机收藏",
                imported.title
            ));
            continue;
        }
        favorites.push(imported.clone());
        favorites_restored += 1;
    }

    let mut settings = 0;
    if config.panel_url != local.config.panel_url {
        settings += 1;
    }
    if config.hide_on_fullscreen != local.config.hide_on_fullscreen {
        settings += 1;
    }
    if config.privileged_capture != local.config.privileged_capture {
        settings += 1;
    }
    if config.hotkey != local.config.hotkey {
        settings += 1;
    }
    RestorePlan {
        config,
        prompts,
        favorites,
        counts: BackupCounts {
            prompts: prompts_restored,
            custom_sites: sites_restored,
            favorites: favorites_restored,
            settings,
        },
        conflicts,
    }
}

fn same_prompt_content(a: &crate::prompts::Prompt, b: &crate::prompts::Prompt) -> bool {
    a.label == b.label
        && a.instruction == b.instruction
        && a.is_builtin == b.is_builtin
        && a.hidden == b.hidden
        && a.show_card == b.show_card
}

fn same_site_url(a: &str, b: &str) -> bool {
    match (tauri::Url::parse(a), tauri::Url::parse(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn same_optional_site_url(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => same_site_url(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn merge_bool_setting(
    local: &mut bool,
    imported: bool,
    label: &str,
    conflicts: &mut ConflictReport,
) {
    if *local == imported {
        return;
    }
    if !*local {
        *local = imported;
    } else {
        conflicts.push(format!("{label}与本机设置不同，合并时保留本机设置"));
    }
}

pub(crate) fn export_dialog(app: &AppHandle) -> Result<Option<BackupExportResult>, String> {
    let snapshot = {
        let _read = crate::data_store::read_guard();
        snapshot_under_data_lock()?
    };
    let (bytes, counts) = encode_backup(&snapshot)?;
    let Some(path) = save_file_dialog(app)? else {
        return Ok(None);
    };
    write_export(&path, &bytes)?;
    Ok(Some(BackupExportResult {
        file_name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: path.to_string_lossy().into_owned(),
        counts,
    }))
}

pub(crate) fn import_preview_dialog(app: &AppHandle) -> Result<Option<BackupPreview>, String> {
    let Some(path) = open_file_dialog(app)? else {
        return Ok(None);
    };
    let bytes = read_limited(&path)?;
    let backup = decode_backup(&bytes)?;
    let (snapshot, revision) = {
        let _read = crate::data_store::read_guard();
        (snapshot_under_data_lock()?, crate::data_store::revision())
    };
    let plan = plan_restore(&snapshot, &backup.data, RestoreMode::Merge);
    let ticket = NEXT_TICKET.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let preview = BackupPreview {
        ticket,
        file_name,
        created_at: backup.created_at.clone(),
        app_version: backup.app_version.clone(),
        current_site: backup.data.current_site.clone(),
        counts: counts(&backup.data),
        conflict_count: plan.conflicts.count,
        conflict_details: plan.conflicts.details.clone(),
        conflicts_truncated: plan.conflicts.truncated(),
    };
    *PENDING_IMPORT.lock().unwrap_or_else(|p| p.into_inner()) = Some(PendingImport {
        ticket,
        revision,
        backup,
        preview: preview.clone(),
    });
    Ok(Some(preview))
}

pub(crate) fn cancel_import(ticket: u64) {
    let mut pending = PENDING_IMPORT.lock().unwrap_or_else(|p| p.into_inner());
    if pending.as_ref().is_some_and(|p| p.ticket == ticket) {
        *pending = None;
    }
}

pub(crate) fn restore(
    _app: &AppHandle,
    ticket: u64,
    raw_mode: String,
    confirm_overwrite: bool,
) -> Result<BackupRestoreResult, String> {
    let mode = RestoreMode::parse(&raw_mode)?;
    if mode == RestoreMode::Overwrite && !confirm_overwrite {
        return Err("覆盖恢复必须再次确认".into());
    }
    let pending = {
        let mut slot = PENDING_IMPORT.lock().unwrap_or_else(|p| p.into_inner());
        match slot.take() {
            Some(p) if p.ticket == ticket => p,
            Some(p) => {
                *slot = Some(p);
                return Err("导入预览已过期，请重新选择备份文件".into());
            }
            None => return Err("导入预览不存在或已取消，请重新选择备份文件".into()),
        }
    };

    let result = {
        let _write = crate::data_store::write_guard();
        if crate::data_store::revision() != pending.revision {
            return Err("预览后本机数据发生了变化；为避免覆盖新修改，请重新导入并预览".into());
        }
        let current = snapshot_under_data_lock()?;
        let plan = plan_restore(&current, &pending.backup.data, mode);
        let root = data_root()?;
        let updates = updates_for(&plan)?;
        let old_actual_hotkey = crate::hotkey::current();
        crate::hotkey::apply(plan.config.hotkey.as_deref())?;
        if let Err(error) = apply_transaction_at(&root, &updates, None) {
            if let Err(revert_error) = crate::hotkey::apply(old_actual_hotkey.as_deref()) {
                crate::diag::warn(&format!(
                    "backup: 恢复失败后回滚运行时热键也失败: {revert_error}"
                ));
            }
            return Err(error);
        }
        crate::config::clear_cache_after_restore();
        crate::prompts::clear_cache_after_restore();
        crate::data_store::mark_changed();
        crate::fullscreen::mark_enabled(plan.config.hide_on_fullscreen);
        BackupRestoreResult {
            mode: mode.name().into(),
            prompts: plan.counts.prompts,
            sites: plan.counts.custom_sites,
            favorites: plan.counts.favorites,
            settings: plan.counts.settings,
            conflicts: plan.conflicts.count,
            // Panel URL is stored but the currently loaded WebView is intentionally left alone;
            // a restart applies it without touching the device's WebView login profile.
            restart_required: true,
        }
    };
    let _ = pending.preview;
    Ok(result)
}

fn updates_for(plan: &RestorePlan) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let config =
        serde_json::to_vec_pretty(&plan.config).map_err(|e| format!("配置序列化失败：{e}"))?;
    let prompts =
        serde_json::to_vec_pretty(&plan.prompts).map_err(|e| format!("提示词序列化失败：{e}"))?;
    let favorites =
        serde_json::to_vec_pretty(&plan.favorites).map_err(|e| format!("收藏序列化失败：{e}"))?;
    Ok(vec![
        ("config.json", config),
        ("prompts.json", prompts),
        ("favorites.json", favorites),
    ])
}

fn data_root() -> Result<PathBuf, String> {
    crate::config::store_path()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .ok_or_else(|| "无法定位应用数据目录（缺少 LOCALAPPDATA）".into())
}

fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("无法打开备份文件：{e}"))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("无法读取备份文件信息：{e}"))?;
    if metadata.len() > MAX_BACKUP_BYTES as u64 {
        return Err(format!(
            "备份文件超过 {} MiB 上限",
            MAX_BACKUP_BYTES / 1024 / 1024
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_BACKUP_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("读取备份文件失败：{e}"))?;
    if bytes.len() > MAX_BACKUP_BYTES {
        return Err(format!(
            "备份文件超过 {} MiB 上限",
            MAX_BACKUP_BYTES / 1024 / 1024
        ));
    }
    Ok(bytes)
}

fn write_export(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_BACKUP_BYTES {
        return Err("备份内容超过大小上限".into());
    }
    let path = if path.extension().is_none() {
        path.with_extension("json")
    } else {
        path.to_path_buf()
    };
    atomic_write(path.as_path(), bytes).map_err(|e| format!("写入备份失败：{e}"))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("目标路径无效"))?;
    fs::create_dir_all(parent)?;
    let tmp = unique_temp_path(
        parent,
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .as_ref(),
    );
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

fn unique_temp_path(parent: &Path, file_name: &str) -> PathBuf {
    let nonce = NEXT_TICKET.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(".{file_name}.{}.{nonce}.tmp", std::process::id()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalEntry {
    file: String,
    existed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalManifest {
    version: u8,
    state: String,
    files: Vec<JournalEntry>,
}

fn prepare_journal(root: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(root).map_err(|e| format!("创建应用数据目录失败：{e}"))?;
    recover_journal_at(root)?;
    let journal = root.join(JOURNAL_DIR);
    fs::create_dir(&journal).map_err(|e| format!("创建恢复日志失败：{e}"))?;
    let mut entries = Vec::with_capacity(STORE_FILES.len());
    for name in STORE_FILES {
        let path = root.join(name);
        let existed = match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() => {
                copy_synced(&path, &journal.join(format!("before-{name}")))
                    .map_err(|e| format!("创建 {name} 回滚快照失败：{e}"))?;
                true
            }
            Ok(_) => return Err(format!("数据文件 {name} 不是普通文件，已取消恢复")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(format!("读取数据文件 {name} 状态失败：{e}")),
        };
        entries.push(JournalEntry {
            file: name.into(),
            existed,
        });
    }
    let manifest = JournalManifest {
        version: JOURNAL_VERSION,
        state: "prepared".into(),
        files: entries,
    };
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|e| format!("恢复日志序列化失败：{e}"))?;
    if let Err(e) = atomic_write(&journal.join("manifest.json"), &manifest_bytes) {
        let _ = fs::remove_dir_all(&journal);
        return Err(format!("写入恢复日志失败：{e}"));
    }
    Ok(journal)
}

fn apply_transaction_at(
    root: &Path,
    updates: &[(&str, Vec<u8>)],
    fail_after_writes: Option<usize>,
) -> Result<(), String> {
    let mut expected: Vec<&str> = updates.iter().map(|(name, _)| *name).collect();
    expected.sort_unstable();
    let mut allowed = STORE_FILES.to_vec();
    allowed.sort_unstable();
    if expected != allowed {
        return Err("恢复事务文件集合无效".into());
    }
    let journal = prepare_journal(root)?;
    let mut written = 0usize;
    for (name, bytes) in updates {
        if fail_after_writes == Some(written) {
            let error = "模拟写入故障".to_string();
            return rollback_after_error(root, &journal, error);
        }
        if let Err(error) = atomic_write(&root.join(name), bytes) {
            return rollback_after_error(root, &journal, format!("写入 {name} 失败：{error}"));
        }
        written += 1;
    }
    if fail_after_writes == Some(written) {
        return rollback_after_error(root, &journal, "模拟提交前故障".into());
    }
    let committed = JournalManifest {
        version: JOURNAL_VERSION,
        state: "committed".into(),
        files: STORE_FILES
            .into_iter()
            .map(|file| JournalEntry {
                file: file.into(),
                existed: false,
            })
            .collect(),
    };
    let manifest_bytes =
        serde_json::to_vec_pretty(&committed).map_err(|e| format!("恢复日志序列化失败：{e}"))?;
    if let Err(error) = atomic_write(&journal.join("manifest.json"), &manifest_bytes) {
        return rollback_after_error(root, &journal, format!("标记事务提交失败：{error}"));
    }
    if let Err(error) = fs::remove_dir_all(&journal) {
        crate::diag::warn(&format!(
            "backup: 已提交恢复事务，但清理日志失败（启动时会重试）: {error}"
        ));
    }
    Ok(())
}

fn rollback_after_error(root: &Path, journal: &Path, error: String) -> Result<(), String> {
    match recover_journal_at(root) {
        Ok(()) => Err(format!("{error}；原有配置已回滚，未完成恢复")),
        Err(rollback) => {
            crate::diag::error(&format!("backup: 恢复事务回滚失败: {rollback}"));
            Err(format!(
                "{error}；回滚尚未完成，恢复日志已保留，请重启应用自动恢复（{}）",
                journal.display()
            ))
        }
    }
}

fn recover_journal_at(root: &Path) -> Result<(), String> {
    let journal = root.join(JOURNAL_DIR);
    if !journal.exists() {
        return Ok(());
    }
    let manifest_path = journal.join("manifest.json");
    if !manifest_path.exists() {
        // No data file is changed until the prepared manifest has been synced.
        fs::remove_dir_all(&journal).map_err(|e| format!("清理未完成的空恢复日志失败：{e}"))?;
        return Ok(());
    }
    let bytes = fs::read(&manifest_path).map_err(|e| format!("读取恢复日志失败：{e}"))?;
    if bytes.len() > 16 * 1024 {
        return Err("恢复日志大小异常，为保护数据未自动修改文件".into());
    }
    let manifest: JournalManifest = serde_json::from_slice(&bytes)
        .map_err(|e| format!("恢复日志损坏，为保护数据未自动修改文件：{e}"))?;
    if manifest.version != JOURNAL_VERSION {
        return Err("恢复日志版本不兼容，为保护数据未自动修改文件".into());
    }
    let names: Vec<&str> = manifest.files.iter().map(|e| e.file.as_str()).collect();
    let mut sorted_names = names.clone();
    sorted_names.sort_unstable();
    let mut expected = STORE_FILES.to_vec();
    expected.sort_unstable();
    if sorted_names != expected
        || names.iter().copied().collect::<HashSet<_>>().len() != STORE_FILES.len()
    {
        return Err("恢复日志文件清单无效，为保护数据未自动修改文件".into());
    }
    match manifest.state.as_str() {
        "committed" => {
            fs::remove_dir_all(&journal).map_err(|e| format!("清理已提交恢复日志失败：{e}"))?;
            Ok(())
        }
        "prepared" => {
            for entry in &manifest.files {
                let target = root.join(&entry.file);
                if entry.existed {
                    let before = journal.join(format!("before-{}", entry.file));
                    if !before.is_file() {
                        return Err(format!(
                            "回滚快照 {} 缺失，恢复日志已保留",
                            before.display()
                        ));
                    }
                    atomic_copy_replace(&before, &target)
                        .map_err(|e| format!("回滚 {} 失败，恢复日志已保留：{e}", entry.file))?;
                } else if target.exists() {
                    fs::remove_file(&target).map_err(|e| {
                        format!("移除新建的 {} 失败，恢复日志已保留：{e}", entry.file)
                    })?;
                }
            }
            fs::remove_dir_all(&journal).map_err(|e| format!("清理回滚日志失败：{e}"))?;
            Ok(())
        }
        _ => Err("恢复日志状态无效，为保护数据未自动修改文件".into()),
    }
}

fn copy_synced(source: &Path, destination: &Path) -> std::io::Result<()> {
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()
}

fn atomic_copy_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| std::io::Error::other("目标路径无效"))?;
    let tmp = unique_temp_path(
        parent,
        destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .as_ref(),
    );
    let result = (|| {
        copy_synced(source, &tmp)?;
        fs::rename(&tmp, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

/// Called before any config/prompt cache is initialized during app setup.
pub(crate) fn recover_interrupted_restore() -> Result<(), String> {
    let Some(config_path) = crate::config::store_path() else {
        return Ok(());
    };
    let root = config_path.parent().ok_or("应用数据目录无效")?;
    let _write = crate::data_store::write_guard();
    recover_journal_at(root)
}

#[cfg(windows)]
fn save_file_dialog(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::{
            Foundation::HWND,
            UI::Controls::Dialogs::{
                CommDlgExtendedError, GetSaveFileNameW, OFN_EXPLORER, OFN_NOCHANGEDIR,
                OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
            },
        },
    };
    let filter: Vec<u16> = "DeepSeek Float 备份 (*.json)\0*.json\0JSON 文件 (*.json)\0*.json\0\0"
        .encode_utf16()
        .collect();
    let title: Vec<u16> = "导出 DeepSeek Float 备份"
        .encode_utf16()
        .chain([0])
        .collect();
    let ext: Vec<u16> = "json\0".encode_utf16().collect();
    let default_name = default_backup_filename();
    let mut file_name: Vec<u16> = default_name.encode_utf16().chain([0]).collect();
    file_name.resize(32_768, 0);
    let owner = app
        .get_webview_window("settings")
        .and_then(|w| w.hwnd().ok())
        .unwrap_or(HWND::default());
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file_name.as_mut_ptr()),
        nMaxFile: file_name.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        lpstrDefExt: PCWSTR(ext.as_ptr()),
        Flags: OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | OFN_OVERWRITEPROMPT,
        ..Default::default()
    };
    let ok = unsafe { GetSaveFileNameW(&mut dialog) }.as_bool();
    if !ok {
        let error = unsafe { CommDlgExtendedError() };
        if error.0 != 0 {
            return Err(format!("Windows 保存对话框失败（错误码 {:#x}）", error.0));
        }
        return Ok(None);
    }
    let end = file_name
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(file_name.len());
    Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
        &file_name[..end],
    ))))
}

#[cfg(not(windows))]
fn save_file_dialog(_app: &AppHandle) -> Result<Option<PathBuf>, String> {
    Err("备份文件对话框仅支持 Windows".into())
}

#[cfg(windows)]
fn open_file_dialog(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::{
            Foundation::HWND,
            UI::Controls::Dialogs::{
                CommDlgExtendedError, GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST,
                OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
            },
        },
    };
    let filter: Vec<u16> = "DeepSeek Float 备份 (*.json)\0*.json\0JSON 文件 (*.json)\0*.json\0\0"
        .encode_utf16()
        .collect();
    let title: Vec<u16> = "导入 DeepSeek Float 备份"
        .encode_utf16()
        .chain([0])
        .collect();
    let mut file_name = vec![0u16; 32_768];
    let owner = app
        .get_webview_window("settings")
        .and_then(|w| w.hwnd().ok())
        .unwrap_or(HWND::default());
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file_name.as_mut_ptr()),
        nMaxFile: file_name.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        ..Default::default()
    };
    let ok = unsafe { GetOpenFileNameW(&mut dialog) }.as_bool();
    if !ok {
        let error = unsafe { CommDlgExtendedError() };
        if error.0 != 0 {
            return Err(format!("Windows 打开对话框失败（错误码 {:#x}）", error.0));
        }
        return Ok(None);
    }
    let end = file_name
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(file_name.len());
    Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
        &file_name[..end],
    ))))
}

#[cfg(not(windows))]
fn open_file_dialog(_app: &AppHandle) -> Result<Option<PathBuf>, String> {
    Err("备份文件对话框仅支持 Windows".into())
}

fn default_backup_filename() -> String {
    let timestamp = crate::diag::now_rfc3339();
    let date = timestamp.get(0..19).unwrap_or("2000-01-01T00:00:00");
    format!(
        "deepseek-float-backup-{}-{}.json",
        date[0..10].replace('-', ""),
        date[11..19].replace(':', "")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "dsfloat-{name}-{}-{}",
            std::process::id(),
            NEXT_TICKET.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn sample_data() -> BackupData {
        BackupData {
            prompts: crate::prompts::defaults(),
            sites: vec![crate::panel_site::Site { label: "工作站点".into(), url: "https://work.example/chat".into() }],
            current_site: Some("https://work.example/chat".into()),
            settings: PortableSettings { hide_on_fullscreen: true, privileged_capture: true, clipboard_fallback_enabled: Some(false), hotkey: Some("Ctrl+Alt+1".into()) },
            favorites: vec![crate::favorites::Favorite {
                id: 42,
                title: "Markdown 收藏".into(),
                question: "问题\n第二行".into(),
                answer_markdown: "| 列 |\n| --- |\n| 值 |\n\n````rust\n  fn main() {\n    println!(\"你好\");\n  }\n````".into(),
                source: "Kimi".into(),
                created_at: 1_700_000_000,
            }],
        }
    }

    fn sample_snapshot() -> DataSnapshot {
        DataSnapshot {
            config: crate::config::AppConfig::default(),
            prompts: crate::prompts::defaults(),
            favorites: vec![],
        }
    }

    fn serialize(data: BackupData) -> Vec<u8> {
        serde_json::to_vec(&BackupFile {
            format: FORMAT.into(),
            version: VERSION,
            created_at: "2026-10-18T12:00:00Z".into(),
            app_version: "0.1.0".into(),
            data,
        })
        .unwrap()
    }

    #[test]
    fn clipboard_fallback_backup_merge_and_overwrite_both_disable_legacy_choice() {
        for local_value in [false, true] {
            for incoming in [None, Some(false), Some(true)] {
                let mut local = sample_snapshot();
                local.config.clipboard_fallback_enabled = local_value;
                let mut data = sample_data();
                data.settings.clipboard_fallback_enabled = incoming;
                let decoded = decode_backup(&serialize(data)).unwrap();
                let merged = plan_restore(&local, &decoded.data, RestoreMode::Merge);
                assert!(!merged.config.clipboard_fallback_enabled);
                if incoming.is_some_and(|value| value != local_value) {
                    assert!(merged
                        .conflicts
                        .details
                        .iter()
                        .any(|detail| detail.contains("自动复制回退")));
                }
                let replaced = plan_restore(&local, &decoded.data, RestoreMode::Overwrite);
                assert!(!replaced.config.clipboard_fallback_enabled);
            }
        }
    }

    #[test]
    fn public_build_backup_modes_cannot_reenable_fallback() {
        for local_enabled in [false, true] {
            for incoming in [None, Some(false), Some(true)] {
                let mut local = sample_snapshot();
                local.config.clipboard_fallback_enabled = local_enabled;
                let mut data = sample_data();
                data.settings.clipboard_fallback_enabled = incoming;
                for mode in [RestoreMode::Merge, RestoreMode::Overwrite] {
                    let plan = plan_restore(&local, &data, mode);
                    assert!(!plan.config.clipboard_fallback_enabled);
                }
            }
        }
    }

    #[test]
    fn clipboard_fallback_backup_exports_explicit_boolean_and_rejects_bad_type() {
        for enabled in [false, true] {
            let mut snapshot = sample_snapshot();
            snapshot.config.clipboard_fallback_enabled = enabled;
            let (bytes, stats) = encode_backup(&snapshot).unwrap();
            let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                json["data"]["settings"]["clipboard_fallback_enabled"],
                false
            );
            assert_eq!(stats.settings, 4);
            json["data"]["settings"]["clipboard_fallback_enabled"] = serde_json::json!("true");
            assert!(decode_backup(&serde_json::to_vec(&json).unwrap()).is_err());
        }
    }

    #[test]
    fn export_whitelist_excludes_device_state_and_keeps_markdown_exact() {
        let mut snapshot = sample_snapshot();
        snapshot.config.ball_x = Some(1234);
        snapshot.config.ball_y = Some(567);
        snapshot.config.autostart = true;
        snapshot.config.panel_url = Some("https://work.example/chat".into());
        snapshot.config.custom_sites = sample_data().sites;
        snapshot.favorites = sample_data().favorites;
        let (bytes, stats) = encode_backup(&snapshot).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(stats.prompts, 4);
        assert_eq!(stats.custom_sites, 1);
        assert_eq!(stats.favorites, 1);
        assert!(json["data"]["settings"]["hide_on_fullscreen"].is_boolean());
        assert!(json["data"]["ball_x"].is_null());
        assert!(json["data"]["autostart"].is_null());
        assert!(!String::from_utf8(bytes.clone()).unwrap().contains("ball_x"));
        let decoded = decode_backup(&bytes).unwrap();
        assert_eq!(
            decoded.data.favorites[0].answer_markdown,
            sample_data().favorites[0].answer_markdown
        );
        assert_eq!(decoded.data.favorites[0].question, "问题\n第二行");
    }

    #[test]
    fn version_one_without_new_optional_settings_fields_remains_compatible() {
        let mut value: serde_json::Value =
            serde_json::from_slice(&serialize(sample_data())).unwrap();
        value["data"]
            .as_object_mut()
            .unwrap()
            .remove("current_site");
        value["data"]["settings"]
            .as_object_mut()
            .unwrap()
            .remove("hotkey");
        let decoded = decode_backup(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(decoded.data.current_site, None);
        assert_eq!(decoded.data.settings.hotkey, default_hotkey());
    }

    #[test]
    fn rejects_wrong_format_future_version_damage_and_invalid_urls() {
        let bytes = serialize(sample_data());
        let mut bad: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        bad["format"] = "other".into();
        assert!(decode_backup(&serde_json::to_vec(&bad).unwrap())
            .unwrap_err()
            .contains("格式标识"));
        bad["format"] = FORMAT.into();
        bad["version"] = (VERSION + 1).into();
        assert!(decode_backup(&serde_json::to_vec(&bad).unwrap())
            .unwrap_err()
            .contains("高于"));
        assert!(decode_backup(b"{broken").unwrap_err().contains("损坏"));
        let mut bad: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        bad["data"]["sites"][0]["url"] = "javascript:alert(1)".into();
        assert!(decode_backup(&serde_json::to_vec(&bad).unwrap())
            .unwrap_err()
            .contains("网址无效"));
        bad["data"]["sites"][0]["url"] = "https://user:password@work.example/".into();
        assert!(decode_backup(&serde_json::to_vec(&bad).unwrap())
            .unwrap_err()
            .contains("用户名或密码"));
        bad["data"]["sites"][0]["url"] = "https://work.example/?access_token=secret".into();
        assert!(decode_backup(&serde_json::to_vec(&bad).unwrap())
            .unwrap_err()
            .contains("疑似凭据"));
        bad["data"]["sites"][0]["url"] = "https://work.example/#access_token=secret".into();
        assert!(decode_backup(&serde_json::to_vec(&bad).unwrap())
            .unwrap_err()
            .contains("疑似凭据"));
    }

    #[test]
    fn rejects_oversized_files_duplicate_ids_and_invalid_records() {
        assert!(decode_backup(&vec![b' '; MAX_BACKUP_BYTES + 1])
            .unwrap_err()
            .contains("上限"));
        let mut data = sample_data();
        let mut duplicate = data.favorites[0].clone();
        duplicate.title = "另一个标题".into();
        data.favorites.push(duplicate);
        assert!(decode_backup(&serialize(data))
            .unwrap_err()
            .contains("重复"));
        let mut data = sample_data();
        data.sites[0].url = "https://work.example/?api_key=secret".into();
        assert!(decode_backup(&serialize(data))
            .unwrap_err()
            .contains("凭据"));
        let mut data = sample_data();
        data.favorites[0].answer_markdown = "api_key = abcdefghijklmnop".into();
        assert!(decode_backup(&serialize(data))
            .unwrap_err()
            .contains("检测到疑似密钥"));
        let mut data = sample_data();
        data.favorites[0].answer_markdown =
            "普通 Markdown 内容\n\n```rust\nlet api_key = \"public placeholder\";\n```".into();
        assert!(decode_backup(&serialize(data)).is_ok());
    }

    #[test]
    fn rejects_suspected_credentials_in_other_exported_text_fields() {
        let mut data = sample_data();
        data.prompts[0].instruction = "api_key = abcdefghijklmnop".into();
        assert!(validate_data(data).unwrap_err().contains("疑似密钥"));

        let mut data = sample_data();
        data.sites[0].label = "password: hunter2secret".into();
        assert!(validate_data(data).unwrap_err().contains("疑似密钥"));

        let mut data = sample_data();
        data.favorites[0].source = "password: hunter2secret".into();
        assert!(validate_data(data).unwrap_err().contains("疑似密钥"));
    }

    #[test]
    fn unknown_nested_fields_are_rejected_instead_of_silently_discarded() {
        let mut value: serde_json::Value =
            serde_json::from_slice(&serialize(sample_data())).unwrap();
        value["data"]["favorites"][0]["future_metadata"] = true.into();
        assert!(decode_backup(&serde_json::to_vec(&value).unwrap())
            .unwrap_err()
            .contains("不兼容字段 data.favorites[0].future_metadata"));
    }

    #[test]
    fn merge_keeps_conflicting_local_records_and_adds_missing_data() {
        let backup = sample_data();
        let mut local = sample_snapshot();
        local.prompts.push(crate::prompts::Prompt {
            id: "custom_conflict".into(),
            label: "导入冲突".into(),
            instruction: "本机内容".into(),
            is_builtin: false,
            hidden: false,
            show_card: true,
        });
        let mut imported = backup.clone();
        imported.prompts.push(crate::prompts::Prompt {
            id: "custom_conflict".into(),
            label: "导入冲突".into(),
            instruction: "备份内容".into(),
            is_builtin: false,
            hidden: false,
            show_card: true,
        });
        imported.prompts.push(crate::prompts::Prompt {
            id: "custom_new".into(),
            label: "新增".into(),
            instruction: "新指令".into(),
            is_builtin: false,
            hidden: false,
            show_card: true,
        });
        let plan = plan_restore(&local, &imported, RestoreMode::Merge);
        assert!(plan.conflicts.count >= 1);
        assert!(plan
            .prompts
            .iter()
            .any(|p| p.id == "custom_conflict" && p.instruction == "本机内容"));
        assert!(plan.prompts.iter().any(|p| p.id == "custom_new"));
        assert_eq!(plan.favorites.len(), 1);
        assert_eq!(
            plan.config.panel_url,
            Some("https://work.example/chat".into())
        );
    }

    #[test]
    fn merge_reports_same_name_different_site_and_prompt_and_keeps_local_values() {
        let imported = sample_data();
        let mut local = sample_snapshot();
        local.config.custom_sites = vec![crate::panel_site::Site {
            label: "工作站点".into(),
            url: "https://other.example/work".into(),
        }];
        local.config.panel_url = Some("https://local.example/".into());
        local.prompts.push(crate::prompts::Prompt {
            id: "custom_local".into(),
            label: "同名提示词".into(),
            instruction: "本机指令".into(),
            is_builtin: false,
            hidden: false,
            show_card: true,
        });
        local.favorites.push(crate::favorites::Favorite {
            id: 42,
            title: "本机收藏".into(),
            question: "本机问题".into(),
            answer_markdown: "本机答案".into(),
            source: "本机".into(),
            created_at: 1,
        });
        let mut imported = imported;
        imported.prompts.push(crate::prompts::Prompt {
            id: "custom_remote".into(),
            label: "同名提示词".into(),
            instruction: "备份指令".into(),
            is_builtin: false,
            hidden: false,
            show_card: true,
        });
        imported.favorites[0].id = 42;
        let plan = plan_restore(&local, &imported, RestoreMode::Merge);
        assert!(plan.conflicts.count >= 4);
        assert_eq!(
            plan.config.custom_sites[0].url,
            "https://other.example/work"
        );
        assert_eq!(
            plan.config.panel_url.as_deref(),
            Some("https://local.example/")
        );
        assert!(plan
            .prompts
            .iter()
            .any(|p| p.id == "custom_local" && p.instruction == "本机指令"));
        assert_eq!(plan.favorites[0].answer_markdown, "本机答案");
    }

    #[test]
    fn overwrite_replaces_migratable_values_but_keeps_device_specific_fields() {
        let mut local = sample_snapshot();
        local.config.ball_x = Some(444);
        local.config.ball_y = Some(555);
        local.config.autostart = true;
        local.config.panel_url = Some("https://old.example/".into());
        let plan = plan_restore(&local, &sample_data(), RestoreMode::Overwrite);
        assert_eq!(
            plan.config.panel_url.as_deref(),
            Some("https://work.example/chat")
        );
        assert_eq!(plan.config.ball_x, Some(444));
        assert_eq!(plan.config.ball_y, Some(555));
        assert!(plan.config.autostart);
        assert!(plan.config.hide_on_fullscreen);
        assert_eq!(plan.favorites[0].id, 42);
    }

    #[test]
    fn failed_cross_file_write_rolls_back_every_existing_file() {
        let root = temp_root("restore-rollback");
        fs::write(root.join("config.json"), b"old-config").unwrap();
        fs::write(root.join("prompts.json"), b"old-prompts").unwrap();
        let updates = vec![
            ("config.json", b"new-config".to_vec()),
            ("prompts.json", b"new-prompts".to_vec()),
            ("favorites.json", b"new-favorites".to_vec()),
        ];
        let error = apply_transaction_at(&root, &updates, Some(1)).unwrap_err();
        assert!(error.contains("已回滚"));
        assert_eq!(fs::read(root.join("config.json")).unwrap(), b"old-config");
        assert_eq!(fs::read(root.join("prompts.json")).unwrap(), b"old-prompts");
        assert!(!root.join("favorites.json").exists());
        assert!(!root.join(JOURNAL_DIR).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepared_journal_recovers_after_simulated_process_exit() {
        let root = temp_root("restore-crash");
        fs::write(root.join("config.json"), b"old-config").unwrap();
        fs::write(root.join("prompts.json"), b"old-prompts").unwrap();
        let journal = prepare_journal(&root).unwrap();
        atomic_write(&root.join("config.json"), b"new-config").unwrap();
        atomic_write(&root.join("prompts.json"), b"new-prompts").unwrap();
        // A crash here leaves a durable prepared manifest; next startup restores all old bytes.
        recover_journal_at(&root).unwrap();
        assert_eq!(fs::read(root.join("config.json")).unwrap(), b"old-config");
        assert_eq!(fs::read(root.join("prompts.json")).unwrap(), b"old-prompts");
        assert!(!root.join(JOURNAL_DIR).exists());
        let _ = journal;
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn committed_journal_only_cleans_up_and_never_reverts_successful_restore() {
        let root = temp_root("restore-committed");
        let updates = vec![
            ("config.json", b"config".to_vec()),
            ("prompts.json", b"prompts".to_vec()),
            ("favorites.json", b"favorites".to_vec()),
        ];
        apply_transaction_at(&root, &updates, None).unwrap();
        assert_eq!(fs::read(root.join("config.json")).unwrap(), b"config");
        assert!(!root.join(JOURNAL_DIR).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn crash_after_commit_keeps_new_data_and_removes_remaining_journal() {
        let root = temp_root("restore-committed-crash");
        fs::write(root.join("config.json"), b"old-config").unwrap();
        let journal = prepare_journal(&root).unwrap();
        atomic_write(&root.join("config.json"), b"new-config").unwrap();
        let committed = JournalManifest {
            version: JOURNAL_VERSION,
            state: "committed".into(),
            files: STORE_FILES
                .into_iter()
                .map(|file| JournalEntry {
                    file: file.into(),
                    existed: false,
                })
                .collect(),
        };
        atomic_write(
            &journal.join("manifest.json"),
            &serde_json::to_vec(&committed).unwrap(),
        )
        .unwrap();
        recover_journal_at(&root).unwrap();
        assert_eq!(fs::read(root.join("config.json")).unwrap(), b"new-config");
        assert!(!journal.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn export_filename_uses_required_timestamp_shape() {
        let name = default_backup_filename();
        assert!(name.starts_with("deepseek-float-backup-"));
        assert!(name.ends_with(".json"));
        let timestamp = name
            .trim_start_matches("deepseek-float-backup-")
            .trim_end_matches(".json");
        assert_eq!(timestamp.len(), 15);
        assert_eq!(&timestamp[8..9], "-");
        assert!(timestamp[..8].chars().all(|c| c.is_ascii_digit()));
        assert!(timestamp[9..].chars().all(|c| c.is_ascii_digit()));
        assert!(valid_iso8601("2026-10-18T12:34:56Z"));
        assert!(!valid_iso8601("not-a-date"));
    }
}
