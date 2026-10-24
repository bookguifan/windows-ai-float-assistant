//! Local favorites and Markdown exports. All paths honor LOCALAPPDATA.
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
fn nonce() -> u64 {
    NONCE.fetch_add(1, Ordering::Relaxed)
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Favorite {
    pub id: u64,
    pub title: String,
    pub question: String,
    pub answer_markdown: String,
    pub source: String,
    pub created_at: u64,
}
pub fn root() -> Result<PathBuf, String> {
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
            serde_json::from_slice(&bytes).map_err(|e| format!("收藏文件损坏，原文件已保留：{e}"))
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
    let mut items = load_at(path)?;
    if let Some(existing) = items
        .iter()
        .find(|f| f.question == item.question && f.answer_markdown == item.answer_markdown)
    {
        return Ok(existing.clone());
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
    export_at(&root()?.join("exports"), title, body).map(|p| p.to_string_lossy().into_owned())
}
pub fn export_favorite(id: u64) -> Result<String, String> {
    let item = list()?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or("收藏不存在或已删除")?;
    export(
        &item.title,
        &markdown(
            &item.title,
            &item.source,
            &[(item.question, item.answer_markdown)],
        ),
    )
}
pub fn open_exports() -> Result<(), String> {
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
            title: "标题".into(),
            question: "问题".into(),
            answer_markdown: "| a |\n| --- |\n| b |\n\n```\ncode\n```".into(),
            source: "来源".into(),
            created_at: 0,
        }
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
