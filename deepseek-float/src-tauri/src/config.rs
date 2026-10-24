//! §7：应用级偏好配置（区别于 prompts.json 的提示词数据）。
//!
//! 持久化到 `%LOCALAPPDATA%\com.deepseekfloat.desktop\config.json`，
//! 与 prompts.json 同目录、同生命周期。包含以下偏好：
//!   - `ball_x` / `ball_y`：悬浮球最后位置（位置记忆）
//!   - `autostart`：开机自启（写入注册表 Run 项）
//!   - `hide_on_fullscreen`：全屏时自动隐藏悬浮球
//!   - `privileged_capture`：提权划词（UI Automation 直读选区）
//!   - `clipboard_fallback_enabled`：旧字段兼容；首版公开构建固定禁用
//!
//! 设计同 prompts.rs：内存缓存 + 落盘 + 缺字段默认，避免每次读盘。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    /// 悬浮球逻辑位置（屏幕像素，物理像素由调用方按需换算）。None = 用 tauri.conf 默认。
    #[serde(default)]
    pub ball_x: Option<i32>,
    #[serde(default)]
    pub ball_y: Option<i32>,
    /// 开机自启。
    #[serde(default)]
    pub autostart: bool,
    /// 全屏时隐藏悬浮球。
    #[serde(default)]
    pub hide_on_fullscreen: bool,
    /// 提权划词（UI Automation）。
    #[serde(default)]
    pub privileged_capture: bool,
    /// 兼容旧字段；首版公开构建忽略旧true值，规范化为false。
    #[serde(default, deserialize_with = "deserialize_clipboard_fallback")]
    pub clipboard_fallback_enabled: bool,
    /// 截图全局热键 spec（例 `Ctrl+Alt+1`）；`None` / 空串 = 关闭热键。
    ///
    /// ★ 用 `#[serde(default = "default_hotkey")]` 而非 `#[serde(default)]`：后者对 Option
    ///   缺字段会得到 `None`（= 关闭），而我们要的是「**缺字段 = 用默认热键**」（决策 D1）。
    ///   这也是下面必须**手写** `Default` 的原因 —— `derive(Default)` 对 Option 一律给 `None`，
    ///   会让「无 config.json 的全新安装」与「JSON 缺该字段」得到两种不同结果。
    #[serde(default = "default_hotkey")]
    pub hotkey: Option<String>,
    /// ★ 面板目标网址（可切换）：默认官方 `https://chat.deepseek.com/`；`None`/空 = 用默认。
    ///
    /// 用于「镜像 / 自托管 / 企业网关 / 排障换个源」等诉求。
    /// ⚠️ 注入（`__DSFLOAT__`）与回读选择器按 DeepSeek 官方 DOM 书写：切到**非官方 UI** 时
    ///    正文定位/发送依赖选择器，需同步改写后才可用 —— 本开关对「同源镜像」零成本，
    ///    对「其它产品」只保证面板能打开，不保证划词链路可用。
    #[serde(default)]
    pub panel_url: Option<String>,
    /// 用户保存的自定义站点；缺字段时保持旧配置兼容。
    #[serde(default)]
    pub custom_sites: Vec<crate::panel_site::Site>,
}

fn deserialize_clipboard_fallback<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<bool, D::Error> {
    let requested = bool::deserialize(deserializer)?;
    Ok(crate::release_policy::automatic_copy_allowed(requested))
}

impl AppConfig {
    pub(crate) fn enforce_release_policy(&mut self) {
        self.clipboard_fallback_enabled =
            crate::release_policy::automatic_copy_allowed(self.clipboard_fallback_enabled);
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ball_x: None,
            ball_y: None,
            autostart: false,
            hide_on_fullscreen: false,
            privileged_capture: false,
            clipboard_fallback_enabled: false,
            hotkey: default_hotkey(),
            panel_url: None,
            custom_sites: Vec::new(),
        }
    }
}

/// 官方默认面板网址。
pub const DEFAULT_PANEL_URL: &str = "https://chat.deepseek.com/";

/// ★ 当前应跳转的面板网址（配置有值则用它，否则官方默认）。始终 `trim`。
pub fn panel_url() -> String {
    match load().panel_url.as_deref() {
        Some(u) if !u.trim().is_empty() => u.trim().to_string(),
        _ => DEFAULT_PANEL_URL.to_string(),
    }
}

/// 提取 URL 的 **host**（小写、去端口、去 query/fragment/query）。
/// 供 `panel_state`/`url_needs_login` 做**域名无关**判定 —— 面板切到镜像/自托管时同样适用。
pub fn host_of(u: &str) -> String {
    // ① 去 scheme（取「://」之后；无 scheme 则原样，防御不至于把输入当路径吃掉）。
    let rest = u.split("://").nth(1).unwrap_or(u);
    // ② 先按路径分隔符切出「host[:port]」或「[ipv6]:port」，query/fragment/路径不干扰 host。
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or("");
    // ③ 处理端口与 IPv6 字面量：`[::1]:8080` → `::1`；`host:8080` → `host`。
    //    （旧实现用 `split(['/','?','#',':'])` 直接取第一段，遇 IPv6 `[::1]` 会错切成 `[`。）
    let host = match host_port.strip_prefix('[') {
        Some(h) => h.split(']').next().unwrap_or(""),
        None => host_port.split(':').next().unwrap_or(""),
    };
    host.to_ascii_lowercase()
}

/// URL 是否表示面板停在**登录页**（路径含 `sign_in` 登录段）。
///
/// 与 `host_of` 同口径、成为唯一判定源：`lib::panel_state` 与 `readback::url_needs_login`
/// 都调它，杜绝同一语义两处实现分叉（审核 P2）。刻意按**路径段**匹配而非裸
/// `contains('/sign_in')`：先去掉 query/fragment，再按 `/` 切段 —— query/fragment 里恰好
/// 含 `sign_in` 时不会误判（审核 P2 关注点：`/sign_in` 误掐正常提问），而 `/sign_in`、
/// `/app/sign_in`、`/sign_in/` 等真实登录路径都能命中。
pub fn url_is_sign_in(u: &str) -> bool {
    let no_qf = u.split(['?', '#']).next().unwrap_or(u);
    no_qf.split('/').any(|seg| seg == "sign_in")
}

/// ★ 面板网址合法性校验（注入防护的安全网关）。
///
/// 规则：http(s) 起头 + 至少含一个 `.`（域名） + 不含空格 / 尖括号 / 引号（单双） /
/// 分号 / 换行。
///
/// 与 `lib::config_set_panel_url` 的 `eval("location.href='…'")` 拼接配合：因本函数
/// **禁止 `'` 与 `"`**，校验通过的 URL 永远无法闭合 JS 字符串字面量；同时禁止 `;`，
/// 无法终止语句；禁止 `<` `>` 无法起标记。因此即便走裸字符串拼接，校验通过的 `new`
/// 也天然无法逃逸 JS 上下文（审查 P2-2 的结论：该路径当前不可利用）。
///
/// 抽取为独立函数以便单测覆盖，且让「校验规则」只有这一处实现源。
pub(crate) fn is_valid_panel_url(url: &str) -> bool {
    let u = url.trim();
    if u.is_empty() {
        return false;
    }
    let bad_char = u
        .chars()
        .any(|c| matches!(c, ' ' | '<' | '>' | '"' | '\'' | ';' | '\n' | '\r'));
    (u.starts_with("https://") || u.starts_with("http://")) && u.contains('.') && !bad_char
}

/// 当前面板网址的 host（供登录态判定）。
pub fn panel_host() -> String {
    host_of(&panel_url())
}

/// `hotkey` 的 serde / `Default` 缺省值：决策 D1 规定的默认热键。
fn default_hotkey() -> Option<String> {
    Some("Ctrl+Alt+1".to_string())
}

const FILE_NAME: &str = "config.json";

pub(crate) fn store_path() -> Option<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let dir = PathBuf::from(local).join("com.deepseekfloat.desktop");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(FILE_NAME))
}

fn cache() -> &'static std::sync::Mutex<Option<AppConfig>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<AppConfig>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

/// 读取配置；文件缺失/损坏 → 返回默认。
pub fn load() -> AppConfig {
    let _data = crate::data_store::read_guard();
    load_under_data_lock()
}

/// 调用方已持有 `data_store` 读锁或写锁时读取，避免嵌套获取非重入锁。
pub(crate) fn load_under_data_lock() -> AppConfig {
    if let Some(c) = cache().lock().ok().and_then(|g| g.clone()) {
        return c;
    }
    let c = load_from_disk();
    if let Ok(mut g) = cache().lock() {
        *g = Some(c.clone());
    }
    c
}

fn load_from_disk() -> AppConfig {
    let Some(p) = store_path() else {
        return AppConfig::default();
    };
    let Ok(raw) = std::fs::read_to_string(&p) else {
        return AppConfig::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// 局部更新：只改给出的字段，其余沿用当前值。用于各独立开关的单独保存。
///
/// 原子性：整个「读缓存（或磁盘兜底）→ 改字段 → 落盘 → 回写缓存」都在**同一把缓存锁**内完成。
/// 否则两个线程（如球位置保存的窗口事件线程 与 设置命令的 IPC 线程）会交叉读改写，
/// 导致「拖动刚保存的位置」被一个基于旧快照的开关写入覆盖（丢失更新）。
///
/// 注意：本函数**不得**在内部再调用 `load()`/`save()`（会重入同一把非重入锁 → 死锁）。
pub fn update(f: impl FnOnce(&mut AppConfig)) -> AppConfig {
    let _data = crate::data_store::write_guard();
    update_under_data_lock(f)
}

/// 更新配置；调用方已持有 `data_store` 写锁。
pub(crate) fn update_under_data_lock(f: impl FnOnce(&mut AppConfig)) -> AppConfig {
    // 锁中毒也继续用内部数据，避免在窗口事件/命令线程上 panic。
    let mut guard = match cache().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if guard.is_none() {
        *guard = Some(load_from_disk());
    }
    let cfg_ref = guard.as_mut().expect("上面刚初始化为 Some");
    f(cfg_ref);
    cfg_ref.enforce_release_policy();
    let cfg = cfg_ref.clone();
    // 落盘（保持锁不释放，直到写完）。写盘失败**不**向上传播：本函数在窗口事件线程 /
    // 去抖线程 / IPC 线程都会被调用，返回 Result 只会逼每一处调用点写样板降级代码；
    // 故维持「尽力而为」契约，但不再静默——落盘失败经 diag 告警（否则用户会无感地丢配置）。
    if let Some(p) = store_path() {
        match serde_json::to_string_pretty(&cfg) {
            Ok(json) => {
                if let Err(e) = atomic_write(&p, &json) {
                    crate::diag::warn(&format!("config: 写入 {FILE_NAME} 失败: {e}"));
                }
            }
            Err(e) => crate::diag::warn(&format!("config: 序列化 {FILE_NAME} 失败: {e}")),
        }
    }
    crate::data_store::mark_changed();
    cfg
}

/// 原子写盘：先写临时文件再 `rename` 覆盖，避免异常中断把 config.json 截成半截 JSON
/// （程序以 `process::exit(0)` 退出会跳过析构，写盘中途被杀即可留下坏文件 → 下次启动
///   `load_from_disk` 的 `unwrap_or_default` 会静默回退默认值，用户设置无感丢失）。
/// 临时文件与目标同目录、同卷，`rename` 才能保证原子替换。
fn atomic_write(p: &std::path::Path, json: &str) -> std::io::Result<()> {
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, p)
}

/// 用户显式保存配置时必须返回写盘错误；失败不改变内存缓存。
pub fn try_update(f: impl FnOnce(&mut AppConfig)) -> Result<AppConfig, String> {
    let _data = crate::data_store::write_guard();
    let mut guard = cache().lock().unwrap_or_else(|p| p.into_inner());
    let current = guard.get_or_insert_with(load_from_disk);
    let path = store_path().ok_or("无法定位配置目录（缺少 LOCALAPPDATA）")?;
    commit_at(current, &path, f)?;
    crate::data_store::mark_changed();
    Ok(current.clone())
}

/// 清除进程内缓存；下一次读取会从已提交的数据文件重新加载。
pub(crate) fn clear_cache_after_restore() {
    *cache().lock().unwrap_or_else(|p| p.into_inner()) = None;
}

fn commit_at(
    current: &mut AppConfig,
    path: &std::path::Path,
    f: impl FnOnce(&mut AppConfig),
) -> Result<(), String> {
    let mut candidate = current.clone();
    f(&mut candidate);
    candidate.enforce_release_policy();
    let json =
        serde_json::to_string_pretty(&candidate).map_err(|e| format!("配置序列化失败：{e}"))?;
    atomic_write(path, &json).map_err(|e| format!("配置保存失败：{e}"))?;
    *current = candidate;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_build_ignores_legacy_enabled_fallback_and_persists_disabled() {
        let mut cfg: AppConfig =
            serde_json::from_str(r#"{"clipboard_fallback_enabled":true}"#).unwrap();
        assert!(!cfg.clipboard_fallback_enabled);
        let dir = std::env::temp_dir().join(format!("dsfloat-public-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        commit_at(&mut cfg, &path, |c| c.clipboard_fallback_enabled = true).unwrap();
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(raw["clipboard_fallback_enabled"], false);
        let reloaded: AppConfig = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert!(!reloaded.clipboard_fallback_enabled);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn clipboard_fallback_defaults_off_and_ignores_legacy_opt_in() {
        for json in ["{}", r#"{"clipboard_fallback_enabled":false}"#] {
            let cfg: AppConfig = serde_json::from_str(json).unwrap();
            assert_eq!(
                serde_json::to_value(cfg).unwrap()["clipboard_fallback_enabled"],
                false
            );
        }
        assert_eq!(
            serde_json::to_value(AppConfig::default()).unwrap()["clipboard_fallback_enabled"],
            false
        );
        let cfg: AppConfig =
            serde_json::from_str(r#"{"clipboard_fallback_enabled":true}"#).unwrap();
        assert_eq!(
            serde_json::to_value(cfg).unwrap()["clipboard_fallback_enabled"],
            false
        );
    }

    #[test]
    fn clipboard_fallback_saves_disabled_for_both_requested_choices() {
        let dir = std::env::temp_dir().join(format!("dsfloat-fallback-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let mut cfg = AppConfig::default();
        for enabled in [true, false] {
            commit_at(&mut cfg, &path, |c| c.clipboard_fallback_enabled = enabled).unwrap();
            let fresh: AppConfig = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert!(!fresh.clipboard_fallback_enabled);
            cfg = fresh;
        }
        assert!(commit_at(&mut cfg, &path.join("invalid.json"), |c| c
            .clipboard_fallback_enabled =
            true)
        .is_err());
        assert!(!cfg.clipboard_fallback_enabled);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn checked_config_commit_does_not_mutate_memory_when_disk_write_fails() {
        let dir =
            std::env::temp_dir().join(format!("dsfloat-config-commit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blocked = dir.join("blocked");
        std::fs::write(&blocked, "original").unwrap();
        let mut current = AppConfig::default();
        let result = commit_at(&mut current, &blocked.join("config.json"), |c| {
            c.panel_url = Some("https://new.example/".into())
        });
        assert!(result.is_err());
        assert_eq!(current.panel_url, None);
        assert_eq!(std::fs::read_to_string(&blocked).unwrap(), "original");
        let valid = dir.join("config.json");
        commit_at(&mut current, &valid, |c| {
            c.panel_url = Some("https://new.example/".into())
        })
        .unwrap();
        assert_eq!(
            serde_json::from_str::<AppConfig>(&std::fs::read_to_string(valid).unwrap()).unwrap(),
            current
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn custom_sites_survive_config_roundtrip_and_old_configs_still_load() {
        let old: AppConfig = serde_json::from_str("{}").unwrap();
        let old_json = serde_json::to_value(old).unwrap();
        assert_eq!(old_json["custom_sites"], serde_json::json!([]));
        let cfg: AppConfig = serde_json::from_value(serde_json::json!({
            "panel_url":"https://legacy.example/",
            "custom_sites":[{"label":"工作站点","url":"https://work.example/"}]
        }))
        .unwrap();
        let json = serde_json::to_value(cfg).unwrap();
        assert_eq!(json["custom_sites"][0]["label"], "工作站点");
        assert_eq!(json["panel_url"], "https://legacy.example/");
    }

    #[test]
    fn host_of_handles_scheme_path_query_port_and_case() {
        assert_eq!(host_of("https://chat.deepseek.com/"), "chat.deepseek.com");
        assert_eq!(
            host_of("https://chat.deepseek.com/x?a=b#c"),
            "chat.deepseek.com"
        );
        assert_eq!(
            host_of("http://mirror.example.com:8080/chat"),
            "mirror.example.com"
        );
        assert_eq!(host_of("HTTPS://MiXeD.HOST/path"), "mixed.host");
        // 无 scheme 的裸主机：防御性地取第一段，不应 panic。
        assert_eq!(host_of("host.example/x"), "host.example");
    }

    #[test]
    fn host_of_handles_ipv6_literal() {
        assert_eq!(host_of("http://[::1]:8080/"), "::1");
        assert_eq!(host_of("https://[2001:db8::1]/"), "2001:db8::1");
    }

    #[test]
    fn url_is_sign_in_matches_login_path_not_query() {
        assert!(url_is_sign_in("https://chat.deepseek.com/sign_in"));
        assert!(url_is_sign_in("https://chat.deepseek.com/app/sign_in"));
        assert!(url_is_sign_in("https://chat.deepseek.com/sign_in/"));
        // query / fragment / 无关路径里的 sign_in 不算登录页（审核 P2 要点：不误判正常提问）。
        assert!(!url_is_sign_in(
            "https://chat.deepseek.com/?redirect=/sign_in"
        ));
        assert!(!url_is_sign_in(
            "https://chat.deepseek.com/api/sign_in_helper"
        ));
        assert!(!url_is_sign_in("https://chat.deepseek.com/"));
        assert!(!url_is_sign_in(""));
    }

    #[test]
    fn is_valid_panel_url_accepts_normal_urls() {
        assert!(is_valid_panel_url("https://chat.deepseek.com/"));
        assert!(is_valid_panel_url("http://mirror.example.com:8080/chat"));
        assert!(is_valid_panel_url("  https://chat.deepseek.com/  ")); // 容忍首尾空白
        assert!(is_valid_panel_url(
            "https://chatglm.cn/main/alltoolsdetail?lang=zh"
        ));
    }

    /// ★ 2026-10-13：锁定设置页「常用面板网址」预置列表（`poc/settings.html` 的
    /// `<select id="panel-url-preset">` 与 `PRESET_URLS`）**必须全部通过本校验**。
    ///
    /// 为什么要有这条测试：预置项一旦出现非法 URL（漏 scheme、含空格/引号等），
    /// 用户在下拉里选中后 `config_set_panel_url` 会拒绝并弹「面板网址不合法」——
    /// 而那时用户完全不知道自己选的就是内置项。此测试把「预置列表 ⊆ 合法集合」钉死，
    /// 且是**跨语言**（JS 列表 ↔ Rust 校验）的联动护栏：
    /// 凡在 settings.html 增删预置项，必须同步维护本数组，否则测试失败。
    #[test]
    fn preset_panel_urls_are_all_valid() {
        // 与 poc/settings.html 的 <option value> / PRESET_URLS 逐项对应。
        const PRESET_URLS: &[&str] = &[
            "https://chat.deepseek.com/",
            "https://chatgpt.com/",
            "https://www.kimi.com/",
            "https://gemini.google.com/app",
            "https://chatglm.cn/main/alltoolsdetail?lang=zh",
        ];
        for u in PRESET_URLS {
            assert!(is_valid_panel_url(u), "预置面板网址不合法: {u}");
        }
        // 额外断言：host 解析必须稳定（预置项都是普通域名，不应走到 IPv6/端口分支）。
        assert_eq!(host_of(PRESET_URLS[2]), "www.kimi.com");
        assert_eq!(host_of(PRESET_URLS[3]), "gemini.google.com");
    }

    #[test]
    fn is_valid_panel_url_rejects_injection_chars() {
        // 单引号 / 双引号：无法闭合 location.href='…' 字符串字面量。
        assert!(!is_valid_panel_url("https://a.com/'"));
        assert!(!is_valid_panel_url("https://a.com/\""));
        // 分号：无法终止 JS 语句。
        assert!(!is_valid_panel_url("https://a.com/;alert(1)"));
        // 空格 / 尖括号：无法起标记或插入分隔。
        assert!(!is_valid_panel_url("https://a.com/ x"));
        assert!(!is_valid_panel_url("https://a.com/<script>"));
        assert!(!is_valid_panel_url("https://a.com/>"));
        // 换行：无法断行注入。
        assert!(!is_valid_panel_url("https://a.com/\nalert(1)"));
        assert!(!is_valid_panel_url("https://a.com/\ralert(1)"));
    }

    #[test]
    fn is_valid_panel_url_rejects_scheme_and_domain() {
        assert!(!is_valid_panel_url(""));
        assert!(!is_valid_panel_url("   "));
        assert!(!is_valid_panel_url("ftp://a.com/"));
        assert!(!is_valid_panel_url("javascript:alert(1)"));
        assert!(!is_valid_panel_url("http://localhost")); // 缺 '.' 域名段
        assert!(!is_valid_panel_url("https://nodot/com/")); // 缺 '.'
    }
}
