//! Local favorites and Markdown exports. Isolated builds use the shared business root.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
static STORE_LOCK: Mutex<()> = Mutex::new(());
static NONCE: AtomicU64 = AtomicU64::new(0);
static LAST_ID: AtomicU64 = AtomicU64::new(0);
pub(crate) const MAX_FAVORITES: usize = 5_000;
pub(crate) const MAX_TITLE_CHARS: usize = 200;
pub(crate) const MAX_QUESTION_CHARS: usize = 100_000;
pub(crate) const MAX_ANSWER_CHARS: usize = 500_000;
pub(crate) const MAX_SOURCE_CHARS: usize = 512;
fn nonce() -> u64 {
    NONCE.fetch_add(1, Ordering::Relaxed)
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FavoriteKind {
    #[default]
    Answer,
    Selection,
    Manual,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Favorite {
    pub id: u64,
    #[serde(default)]
    pub kind: FavoriteKind,
    pub title: String,
    pub question: String,
    pub answer_markdown: String,
    pub source: String,
    pub created_at: u64,
}
pub(crate) fn validate_fields(item: &Favorite) -> Result<(), String> {
    if item.title.chars().count() > MAX_TITLE_CHARS
        || item.question.chars().count() > MAX_QUESTION_CHARS
        || item.answer_markdown.is_empty()
        || item.answer_markdown.chars().count() > MAX_ANSWER_CHARS
        || item.source.chars().count() > MAX_SOURCE_CHARS
    {
        return Err(format!("收藏 {} 的字段超过允许长度或答案为空", item.id));
    }
    Ok(())
}
pub fn root() -> Result<PathBuf, String> {
    if crate::isolation::active() {
        return crate::isolation::business_dir()
            .ok_or_else(|| "Isolated business directory is not initialized".into());
    }
    // Preserve production read-only lookup and non-Unicode LOCALAPPDATA support.
    std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("com.deepseekfloat.desktop"))
        .ok_or_else(|| "无法定位本地收藏目录".into())
}
pub(crate) fn store() -> Result<PathBuf, String> {
    Ok(root()?.join("favorites.json"))
}
fn load_at(path: &Path) -> Result<Vec<Favorite>, String> {
    match fs::read(path) {
        Ok(bytes) => {
            let mut items: Vec<Favorite> = serde_json::from_slice(&bytes)
                .map_err(|e| format!("收藏文件损坏，原文件已保留：{e}"))?;
            items.sort_by(|a, b| {
                b.created_at
                    .cmp(&a.created_at)
                    .then_with(|| b.id.cmp(&a.id))
            });
            Ok(items)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(format!("读取收藏失败：{e}")),
    }
}
fn write_at(path: &Path, items: &[Favorite]) -> Result<(), String> {
    let parent = path.parent().ok_or("收藏路径无效")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(".favorites-{}-{}.tmp", std::process::id(), nonce()));
    let result = (|| {
        let bytes = serde_json::to_vec_pretty(items).map_err(|e| e.to_string())?;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(&bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        drop(f);
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result.map_err(|e| format!("保存收藏失败：{e}"))
}
fn save_at(path: &Path, mut item: Favorite) -> Result<Favorite, String> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    if item.answer_markdown.trim().is_empty() {
        return Err("答案为空，无法收藏".into());
    }
    validate_fields(&item)?;
    let mut items = load_at(path)?;
    if let Some(existing) = items.iter().find(|f| {
        f.kind == item.kind
            && f.question == item.question
            && f.answer_markdown == item.answer_markdown
    }) {
        return Ok(existing.clone());
    }
    if items.len() >= MAX_FAVORITES {
        return Err(format!("收藏数量超过 {} 条", MAX_FAVORITES));
    }
    // Microseconds stay exactly representable in JavaScript and survive deletion/restarts.
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;
    let floor = clock.max(
        items
            .iter()
            .map(|f| f.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1),
    );
    let next = LAST_ID.load(Ordering::Relaxed).max(floor);
    item.id = next;
    LAST_ID.store(next.saturating_add(1), Ordering::Relaxed);
    item.created_at = now();
    items.push(item.clone());
    write_at(path, &items)?;
    Ok(item)
}
fn delete_at(path: &Path, id: u64) -> Result<(), String> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut items = load_at(path)?;
    items.retain(|f| f.id != id);
    write_at(path, &items)
}
fn update_at(
    path: &Path,
    expected: &Favorite,
    title: String,
    content: String,
) -> Result<Favorite, String> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut items = load_at(path)?;
    let current = items
        .iter_mut()
        .find(|item| item.id == expected.id)
        .ok_or("收藏不存在或已删除，请取消编辑后刷新列表")?;
    if current != expected {
        return Err("收藏已被修改，请保留草稿并取消编辑后重新打开".into());
    }
    let fields = manual(title, content)?;
    current.title = fields.title;
    current.answer_markdown = fields.answer_markdown;
    validate_fields(current)?;
    let updated = current.clone();
    write_at(path, &items)?;
    Ok(updated)
}
pub fn list() -> Result<Vec<Favorite>, String> {
    let _data = crate::data_store::read_guard();
    list_under_data_lock()
}
pub(crate) fn list_under_data_lock() -> Result<Vec<Favorite>, String> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    load_at(&store()?)
}
pub fn save(item: Favorite) -> Result<Favorite, String> {
    let _data = crate::data_store::write_guard();
    let saved = save_at(&store()?, item)?;
    crate::data_store::mark_changed();
    Ok(saved)
}
fn selection(text: String, source: String) -> Favorite {
    let title = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .chars()
        .take(48)
        .collect();
    Favorite {
        id: 0,
        kind: FavoriteKind::Selection,
        title,
        question: String::new(),
        answer_markdown: text,
        source,
        created_at: 0,
    }
}
pub fn save_selection(text: String, source: String) -> Result<Favorite, String> {
    save(selection(text, source))
}
fn manual(title: String, content: String) -> Result<Favorite, String> {
    if content.trim().is_empty() {
        return Err("内容不能为空".into());
    }
    let mut item = selection(content, "手动新增".into());
    item.kind = FavoriteKind::Manual;
    if !title.trim().is_empty() {
        item.title = title.trim().into();
    }
    validate_fields(&item)?;
    Ok(item)
}
pub fn create_manual(title: String, content: String) -> Result<Favorite, String> {
    save(manual(title, content)?)
}
pub fn update(expected: Favorite, title: String, content: String) -> Result<Favorite, String> {
    let _data = crate::data_store::write_guard();
    let saved = update_at(&store()?, &expected, title, content)?;
    crate::data_store::mark_changed();
    Ok(saved)
}
pub fn delete(id: u64) -> Result<(), String> {
    let _data = crate::data_store::write_guard();
    delete_at(&store()?, id)?;
    crate::data_store::mark_changed();
    Ok(())
}
pub fn markdown(title: &str, source: &str, turns: &[(String, String)]) -> String {
    let mut out = format!(
        "# {}\n\n来源：{}\n",
        title.replace(['\n', '\r'], " "),
        source.replace(['\n', '\r'], " ")
    );
    for (i, (q, a)) in turns.iter().enumerate() {
        out.push_str(&format!(
            "\n## 问题 {}\n\n{}\n\n## 回答 {}\n\n{}\n",
            i + 1,
            q,
            i + 1,
            a
        ));
    }
    out
}
fn favorite_markdown(item: &Favorite) -> String {
    match item.kind {
        FavoriteKind::Answer => markdown(
            &item.title,
            &item.source,
            &[(item.question.clone(), item.answer_markdown.clone())],
        ),
        FavoriteKind::Manual => item.answer_markdown.clone(),
        FavoriteKind::Selection => {
            // Keep untrusted text (including its title) out of Markdown/HTML syntax.
            // A fence longer than every raw backtick run cannot be closed by the selection.
            let longest_run = item
                .answer_markdown
                .split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or(0);
            let fence = "`".repeat(3.max(longest_run + 1));
            let newline = if item.answer_markdown.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            format!("{fence}text\n{}{newline}{fence}\n", item.answer_markdown)
        }
    }
}
fn export_at(dir: &Path, title: &str, body: &str) -> Result<PathBuf, String> {
    if body.trim().is_empty() {
        return Err("没有可导出的答案".into());
    }
    fs::create_dir_all(dir).map_err(|e| format!("创建导出目录失败：{e}"))?;
    let title: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(48)
        .collect();
    let title = if title.is_empty() { "answer" } else { &title };
    for n in 0..10000 {
        let path = dir.join(format!("{title}-{}-{n}.md", now()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                if let Err(e) = f.write_all(body.as_bytes()).and_then(|_| f.sync_all()) {
                    drop(f);
                    let _ = fs::remove_file(&path);
                    return Err(format!("导出写入失败：{e}"));
                }
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("导出失败：{e}")),
        }
    }
    Err("同名导出过多，请清理导出目录".into())
}
pub fn export(title: &str, body: &str) -> Result<String, String> {
    if crate::isolation::active() {
        return Err("Interactive exports are disabled in isolated mode".into());
    }
    export_at(&root()?.join("exports"), title, body).map(|p| p.to_string_lossy().into_owned())
}
pub fn export_favorite(id: u64) -> Result<String, String> {
    if crate::isolation::active() {
        return Err("Interactive exports are disabled in isolated mode".into());
    }
    let item = list()?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or("收藏不存在或已删除")?;
    export(&item.title, &favorite_markdown(&item))
}
pub fn open_exports() -> Result<(), String> {
    if crate::isolation::active() {
        return Err("Interactive exports are disabled in isolated mode".into());
    }
    let dir = root()?.join("exports");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer.exe")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开导出目录失败：{e}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn temp() -> PathBuf {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.test-artifacts")
            .join(format!("favorites-{}-{}", std::process::id(), nonce()));
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn favorite() -> Favorite {
        Favorite {
            id: 0,
            kind: FavoriteKind::Answer,
            title: "标题".into(),
            question: "问题".into(),
            answer_markdown: "| a |\n| --- |\n| b |\n\n```\ncode\n```".into(),
            source: "来源".into(),
            created_at: 0,
        }
    }
    #[test]
    fn manual_entries_preserve_content_derive_title_and_roundtrip_without_colliding_with_selection()
    {
        let dir = temp();
        let path = dir.join("favorites.json");
        let raw = "\n  <b>首行</b>\n```\n完整原文  ";
        let item = manual(" \t".into(), raw.into()).unwrap();
        assert_eq!(item.kind, FavoriteKind::Manual);
        assert_eq!(item.title, "<b>首行</b>");
        assert_eq!(item.source, "手动新增");
        assert!(item.question.is_empty());
        assert_eq!(item.answer_markdown, raw);
        let first = save_at(&path, item.clone()).unwrap();
        assert_eq!(save_at(&path, item).unwrap().id, first.id);
        let selected = save_at(&path, selection(raw.into(), "划词".into())).unwrap();
        assert_ne!(first.id, selected.id);
        assert_eq!(load_at(&path).unwrap().len(), 2);
        assert_eq!(favorite_markdown(&first), raw);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn manual_validation_rejects_blank_or_oversized_input_and_keeps_explicit_titles() {
        assert_eq!(
            manual(" 自定标题 ".into(), " 正文 ".into()).unwrap().title,
            "自定标题"
        );
        assert_eq!(
            manual(String::new(), "字".repeat(80)).unwrap().title,
            "字".repeat(48)
        );
        assert!(manual("标题".into(), " \n\t".into()).is_err());
        assert!(manual("字".repeat(201), "正文".into()).is_err());
        assert!(manual(String::new(), "字".repeat(500_001)).is_err());
    }
    #[test]
    fn editing_updates_in_place_preserves_metadata_and_rejects_stale_or_deleted_entries() {
        let dir = temp();
        let path = dir.join("favorites.json");
        for kind in [
            FavoriteKind::Manual,
            FavoriteKind::Selection,
            FavoriteKind::Answer,
        ] {
            let original = save_at(&path, Favorite { kind, ..favorite() }).unwrap();
            let updated = update_at(
                &path,
                &original,
                " 新标题 ".into(),
                "# 新正文\n  原文  ".into(),
            )
            .unwrap();
            let mut expected = original.clone();
            expected.title = "新标题".into();
            expected.answer_markdown = "# 新正文\n  原文  ".into();
            assert_eq!(updated, expected);
            let before = fs::read(&path).unwrap();
            assert!(update_at(&path, &original, "过期编辑".into(), "不应覆盖".into()).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
            for (title, content) in [
                ("标题".into(), " \t\n".into()),
                ("字".repeat(201), "正文".into()),
                ("标题".into(), "字".repeat(500_001)),
            ] {
                assert!(update_at(&path, &updated, title, content).is_err());
                assert_eq!(fs::read(&path).unwrap(), before);
            }
            delete_at(&path, updated.id).unwrap();
            assert!(update_at(&path, &updated, "不应重建".into(), "正文".into()).is_err());
            assert!(load_at(&path).unwrap().is_empty());
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn roundtrip_duplicate_delete() {
        let dir = temp();
        let p = dir.join("favorites.json");
        let a = save_at(&p, favorite()).unwrap();
        let b = save_at(&p, favorite()).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(load_at(&p).unwrap().len(), 1);
        assert_eq!(
            load_at(&p).unwrap()[0].answer_markdown,
            favorite().answer_markdown
        );
        assert_eq!(load_at(&p).unwrap()[0].source, "来源");
        delete_at(&p, a.id).unwrap();
        assert!(load_at(&p).unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn selection_preserves_raw_text_and_deduplicates_exactly() {
        let dir = temp();
        let path = dir.join("favorites.json");
        let raw = "  <div>标题 & 内容</div>\r\n\t```rust\n代码\n```\n  ";
        let first = save_at(&path, selection(raw.into(), "划词".into())).unwrap();
        assert_eq!(first.kind, FavoriteKind::Selection);
        assert_eq!(first.title, "<div>标题 & 内容</div>");
        assert_eq!(first.question, "");
        assert_eq!(first.answer_markdown, raw);
        assert_eq!(first.source, "划词");
        assert_eq!(load_at(&path).unwrap(), vec![first.clone()]);
        let duplicate = save_at(&path, selection(raw.into(), "另一来源".into())).unwrap();
        assert_eq!(duplicate, first);
        let distinct = save_at(&path, selection(raw.trim().into(), "网页".into())).unwrap();
        assert_ne!(distinct.id, first.id);
        assert_eq!(load_at(&path).unwrap().len(), 2);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn selection_title_is_short_unicode_first_line_and_blank_content_is_refused() {
        assert_eq!(
            selection("\n \t\n  正文首行\n后续".into(), String::new()).title,
            "正文首行"
        );
        let raw = format!("  {}\r\nsecond line", "字".repeat(100));
        let item = selection(raw.clone(), String::new());
        assert_eq!(item.title, "字".repeat(48));
        assert_eq!(item.answer_markdown, raw);
        let dir = temp();
        let path = dir.join("favorites.json");
        assert!(save_at(&path, selection(" \t\r\n".into(), String::new())).is_err());
        assert!(!path.exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn same_content_in_different_kinds_remains_distinct() {
        let dir = temp();
        let path = dir.join("favorites.json");
        let item = selection("same raw content".into(), "网页".into());
        let selected = save_at(&path, item.clone()).unwrap();
        let mut answer = item;
        answer.kind = FavoriteKind::Answer;
        let answered = save_at(&path, answer).unwrap();
        assert_ne!(selected.id, answered.id);
        assert_eq!(load_at(&path).unwrap().len(), 2);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn legacy_json_defaults_to_answer_and_unknown_kind_is_preserved_as_corrupt() {
        let dir = temp();
        let path = dir.join("favorites.json");
        let legacy = br#"[{"id":1,"title":"old","question":"q","answer_markdown":"a","source":"s","created_at":2}]"#;
        fs::write(&path, legacy).unwrap();
        assert_eq!(load_at(&path).unwrap()[0].kind, FavoriteKind::Answer);
        let mut value: serde_json::Value = serde_json::from_slice(legacy).unwrap();
        value[0]["kind"] = "future_kind".into();
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(save_at(&path, favorite()).is_err());
        assert!(delete_at(&path, 1).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn listing_is_newest_first_with_id_breaking_timestamp_ties() {
        let dir = temp();
        let path = dir.join("favorites.json");
        let mut items = vec![favorite(); 3];
        for (item, (id, created_at)) in items.iter_mut().zip([(9, 1), (2, 2), (3, 2)]) {
            item.id = id;
            item.created_at = created_at;
        }
        write_at(&path, &items).unwrap();
        let ids: Vec<_> = load_at(&path).unwrap().iter().map(|item| item.id).collect();
        assert_eq!(ids, vec![3, 2, 9]);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn record_limit_allows_duplicates_and_refuses_new_content_without_writing() {
        let dir = temp();
        let path = dir.join("favorites.json");
        let items: Vec<_> = (1..=5_000)
            .map(|id| Favorite {
                id,
                question: format!("q{id}"),
                ..favorite()
            })
            .collect();
        write_at(&path, &items).unwrap();
        let before = fs::read(&path).unwrap();
        assert_eq!(save_at(&path, items[0].clone()).unwrap(), items[0]);
        assert!(save_at(&path, favorite()).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn save_enforces_backup_character_limits_without_modifying_store() {
        let dir = temp();
        let path = dir.join("favorites.json");
        let boundary = Favorite {
            title: "字".repeat(200),
            question: "问".repeat(100_000),
            answer_markdown: "答".repeat(500_000),
            source: "源".repeat(512),
            ..favorite()
        };
        save_at(&path, boundary.clone()).unwrap();
        let before = fs::read(&path).unwrap();
        for field in ["title", "question", "answer_markdown", "source"] {
            let mut oversized = boundary.clone();
            match field {
                "title" => oversized.title.push('字'),
                "question" => oversized.question.push('问'),
                "answer_markdown" => oversized.answer_markdown.push('答'),
                "source" => oversized.source.push('源'),
                _ => unreachable!(),
            }
            assert!(save_at(&path, oversized).is_err(), "{field}");
            assert_eq!(fs::read(&path).unwrap(), before);
        }
        assert!(save_at(&path, selection("字".repeat(500_001), String::new())).is_err());
        assert!(save_at(&path, selection("raw".into(), "源".repeat(513))).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn selection_export_fences_html_and_embedded_fences_without_metadata_injection() {
        let raw = "<script>example()</script>\r\n`````\n# raw heading\n~~~\n";
        let mut item = selection(raw.into(), "<iframe>source</iframe>".into());
        item.title = "<img src=x>".into();
        let body = favorite_markdown(&item);
        assert_eq!(body, format!("``````text\n{raw}``````\n"));
        let dir = temp();
        let path = export_at(&dir, &item.title, &body).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), body);
        item.answer_markdown = "no trailing newline".into();
        assert_eq!(
            favorite_markdown(&item),
            "```text\nno trailing newline\n```\n"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn answer_export_keeps_existing_markdown_layout() {
        let item = favorite();
        assert_eq!(
            favorite_markdown(&item),
            format!(
                "# 标题\n\n来源：来源\n\n## 问题 1\n\n问题\n\n## 回答 1\n\n{}\n",
                item.answer_markdown
            )
        );
    }
    #[test]
    fn corrupt_store_is_never_overwritten() {
        let dir = temp();
        let p = dir.join("favorites.json");
        fs::write(&p, "broken").unwrap();
        assert!(save_at(&p, favorite()).is_err());
        assert_eq!(fs::read_to_string(&p).unwrap(), "broken");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn export_never_overwrites_and_failure_propagates() {
        let dir = temp();
        let a = export_at(&dir, "title", "问题答案").unwrap();
        let b = export_at(&dir, "title", "第二次").unwrap();
        assert_ne!(a, b);
        assert_eq!(fs::read_to_string(a).unwrap(), "问题答案");
        let blocked = dir.join("file");
        fs::write(&blocked, "x").unwrap();
        assert!(export_at(&blocked, "t", "answer").is_err());
        assert!(save_at(&blocked.join("bad"), favorite()).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn deleting_all_items_never_reuses_a_button_identity() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.test-artifacts")
            .join(format!("favorite-identity-{}", nonce()));
        let p = dir.join("favorites.json");
        let item = Favorite {
            id: 0,
            kind: FavoriteKind::Answer,
            title: "t".into(),
            question: "q".into(),
            answer_markdown: "a".into(),
            source: "s".into(),
            created_at: 0,
        };
        let old = save_at(&p, item.clone()).unwrap();
        delete_at(&p, old.id).unwrap();
        let new = save_at(&p, item).unwrap();
        assert_ne!(old.id, new.id);
        delete_at(&p, old.id).unwrap();
        assert_eq!(load_at(&p).unwrap()[0].id, new.id);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn parallel_saves_do_not_lose_items() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.test-artifacts")
            .join(format!("favorite-parallel-{}", nonce()));
        let p = dir.join("favorites.json");
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let p = p.clone();
                std::thread::spawn(move || {
                    save_at(
                        &p,
                        Favorite {
                            id: 0,
                            kind: FavoriteKind::Answer,
                            title: "t".into(),
                            question: format!("q{n}"),
                            answer_markdown: "a".into(),
                            source: "s".into(),
                            created_at: 0,
                        },
                    )
                    .unwrap()
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(load_at(&p).unwrap().len(), 8);
        fs::remove_dir_all(dir).unwrap();
    }
}
