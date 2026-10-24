//! 面板网址切换的统一入口；设置和快捷菜单使用相同的校验及取消语义。
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    pub label: String,
    pub url: String,
}
pub fn choices(current: &str) -> Vec<Site> {
    choices_with(current, &crate::config::load().custom_sites)
}
fn choices_with(current: &str, saved: &[Site]) -> Vec<Site> {
    let mut sites: Vec<Site> = [
        ("DeepSeek 官方", "https://chat.deepseek.com/"),
        ("ChatGPT", "https://chatgpt.com/"),
        ("Kimi", "https://www.kimi.com/"),
        ("Gemini", "https://gemini.google.com/app"),
        ("智谱清言", "https://chatglm.cn/main/alltoolsdetail?lang=zh"),
    ]
    .into_iter()
    .map(|(label, url)| Site {
        label: label.into(),
        url: url.into(),
    })
    .collect();
    for site in saved {
        if !sites.iter().any(|s| s.url == site.url) {
            sites.push(site.clone());
        }
    }
    if !sites.iter().any(|s| s.url == current) {
        sites.push(Site {
            label: "当前自定义地址".into(),
            url: current.into(),
        });
    }
    sites
}

/// 新会话必须回到已知首页，不能重开配置中的历史对话深链。
pub fn new_session_url(raw: &str) -> Result<String, String> {
    let url = tauri::Url::parse(raw).map_err(|_| "面板网址无法解析")?;
    let home = match url.host_str().unwrap_or("") {
        "chat.deepseek.com" => "https://chat.deepseek.com/",
        "chatgpt.com" | "chat.openai.com" => "https://chatgpt.com/",
        "www.kimi.com" | "kimi.com" => "https://www.kimi.com/",
        "gemini.google.com" => "https://gemini.google.com/app",
        "chatglm.cn" => "https://chatglm.cn/main/alltoolsdetail?lang=zh",
        "www.qianwen.com" | "qianwen.com" => "https://www.qianwen.com/",
        _ if url.path() == "/" && url.query().is_none() && url.fragment().is_none() => {
            return Ok(url.to_string())
        }
        _ => {
            return Err("旧任务已取消；当前自定义站点的新会话地址待配置，请在面板内新建会话".into())
        }
    };
    Ok(home.into())
}

pub(crate) fn normalize_sites(mut sites: Vec<Site>) -> Result<Vec<Site>, String> {
    if sites.len() > 20 {
        return Err("自定义站点最多 20 个".into());
    }
    let mut urls = std::collections::HashSet::new();
    for site in &mut sites {
        site.label = site.label.trim().to_string();
        if site.label.is_empty() || site.label.chars().count() > 32 {
            return Err("站点名称须为 1 至 32 字".into());
        }
        let raw = site.url.trim();
        if !crate::config::is_valid_panel_url(raw) {
            return Err(format!(
                "站点「{}」的网址无效，请使用 http(s) 地址",
                site.label
            ));
        }
        let url =
            tauri::Url::parse(raw).map_err(|_| format!("站点「{}」的网址无法解析", site.label))?;
        if url.host_str().is_none() {
            return Err("站点网址缺少主机名".into());
        }
        site.url = url.to_string();
        if !urls.insert(site.url.clone()) {
            return Err("站点地址重复，请编辑已有站点".into());
        }
    }
    Ok(sites)
}

pub fn save(sites: Vec<Site>) -> Result<Vec<Site>, String> {
    let sites = normalize_sites(sites)?;
    crate::config::try_update(|config| config.custom_sites = sites.clone())?;
    Ok(sites)
}
pub fn switch(app: &AppHandle, url: String) -> Result<String, String> {
    let _data = crate::data_store::write_guard();
    let url = url.trim().to_string();
    if !crate::config::is_valid_panel_url(&url) {
        return Err("请输入有效的 http(s) 面板网址".into());
    }
    let target = tauri::Url::parse(&url).map_err(|_| "面板网址无法解析")?;
    if url
        == crate::config::load_under_data_lock()
            .panel_url
            .filter(|current| !current.trim().is_empty())
            .unwrap_or_else(|| crate::config::DEFAULT_PANEL_URL.to_string())
    {
        return Ok(url);
    }
    crate::workflow::cancel_all(app);
    if let Some(panel) = app.get_webview_window("panel") {
        let _ = panel.eval("window.__dsfloatNavigationPending=true;");
        if let Err(error) = panel.navigate(target) {
            let _ = panel.eval("window.__dsfloatNavigationPending=false;");
            return Err(format!("切换失败：{error}"));
        }
    }
    crate::config::update_under_data_lock(|config| config.panel_url = Some(url.clone()));
    Ok(url)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn site(label: &str, url: &str) -> Site {
        Site {
            label: label.into(),
            url: url.into(),
        }
    }
    #[test]
    fn saved_sites_are_normalized_and_invalid_lists_are_rejected() {
        let sites = normalize_sites(vec![site(" 工作 ", " https://Work.example ")]).unwrap();
        assert_eq!(sites[0].label, "工作");
        assert_eq!(sites[0].url, "https://work.example/");
        assert!(normalize_sites(vec![
            site("一", "https://same.example"),
            site("二", "https://same.example/")
        ])
        .is_err());
        assert!(normalize_sites(vec![site("", "https://ok.example/")]).is_err());
        assert!(normalize_sites(vec![site(&"名".repeat(33), "https://ok.example/")]).is_err());
        assert!(normalize_sites(vec![site("坏地址", "https://")]).is_err());
        assert!(normalize_sites(
            (0..21)
                .map(|i| site("站点", &format!("https://s{i}.example/")))
                .collect()
        )
        .is_err());
    }
    #[test]
    fn menu_keeps_saved_order_without_duplicate_presets_and_retains_legacy_current() {
        let saved = vec![
            site("乙", "https://b.example/"),
            site("甲", "https://a.example/"),
            site("官方", "https://chat.deepseek.com/"),
        ];
        let choices = choices_with("https://legacy.example/", &saved);
        assert_eq!(choices.len(), 8);
        assert_eq!(choices[5].label, "乙");
        assert_eq!(choices[6].label, "甲");
        assert_eq!(choices[7].url, "https://legacy.example/");
        assert_eq!(choices_with("https://a.example/", &saved).len(), 7);
    }
    #[test]
    fn new_session_does_not_reopen_conversation_deep_links() {
        assert_eq!(
            new_session_url("https://chatgpt.com/c/old?x=1").unwrap(),
            "https://chatgpt.com/"
        );
        assert_eq!(
            new_session_url("https://www.qianwen.com/chat/old").unwrap(),
            "https://www.qianwen.com/"
        );
        assert!(new_session_url("https://custom.example/chat/old").is_err());
    }
    #[test]
    fn custom_site_is_retained_without_duplicating_presets() {
        assert_eq!(choices_with("https://chatgpt.com/", &[]).len(), 5);
        let custom = choices_with("https://example.test/chat", &[]);
        assert_eq!(custom.len(), 6);
        assert_eq!(custom[5].url, "https://example.test/chat");
    }
}
