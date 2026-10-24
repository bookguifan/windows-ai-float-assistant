//! 统一「提示词」模型：内置预设（解释/翻译/总结/问问DeepSeek）与用户自定义提示词
//! 放进**同一个可编辑列表**，持久化到 `%LOCALAPPDATA%\com.deepseekfloat.desktop\prompts.json`。
//!
//! 设计要点：
//! - 内置项 `is_builtin = true`：可编辑 label / instruction，但**不可删除**，且缺了会自动补回默认。
//! - 自定义项 `is_builtin = false`：可增删改。
//! - 工具条（poc/popup.html）按此列表动态渲染按钮；「复制」是特殊动作，不在此列表。
//! - 每条提示词自带**交付方式** `show_card`：`true` = 显示结果卡片（**不弹面板**），
//!   `false` = 弹出面板（不起回读、不出卡片）。旧配置缺字段时默认 `true`。
//! - 指令里可含 `{text}` 占位符（替换为选区文本）；不含则按「指令\n\n正文」拼接（见 capture::compose_custom）。
//!
//! 说明：早期「硬编码预设」版（`PresetId` / `compose_prompt`）已随「去 React」提交一并删除，
//!       现统一走本数据驱动路径（`Prompt.instruction` + `capture::compose_custom`）。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Prompt {
    /// 内部唯一 id：内置为固定名（explain/translate/summarize/ask），自定义为 `custom_xxx`。
    pub id: String,
    /// 工具条按钮显示名，≤ MAX_LABEL_CHARS。
    pub label: String,
    /// 提示词指令；允许含 `{text}`；空 = 裸文本原样发送（如「问问DeepSeek」）。
    pub instruction: String,
    /// 内置项：不可删除（但可隐藏）；自定义：可删。
    #[serde(default)]
    pub is_builtin: bool,
    /// 是否在工具条上隐藏（内置项通过「隐藏」而非删除来移除出工具条）。旧配置无此字段时默认 false。
    #[serde(default)]
    pub hidden: bool,
    /// ★ 交付方式：`true` = **显示结果卡片**（不弹面板，回答回读后落在卡片上）；
    ///   `false` = **弹出面板**（用户自己在面板里看回答，不起回读、不出卡片）。
    ///
    /// 旧配置（prompts.json）没有这个字段 → 反序列化时取 `default_show_card()` = **true**，
    /// 即「升级后默认走结果卡」—— 与 R3 的产品主链路一致，用户可在设置页逐条关掉。
    #[serde(default = "default_show_card")]
    pub show_card: bool,
}

/// `show_card` 的字段级默认值：**true**（结果卡优先）。
fn default_show_card() -> bool {
    true
}

/// 内置提示词 id（固定，顺序即工具条默认顺序）。
pub const BUILTIN_IDS: [&str; 4] = ["explain", "translate", "summarize", "ask"];
/// 自定义提示词数量上限（内置 4 个不计入）。
pub const MAX_CUSTOM: usize = 20;
/// 名称长度上限（按 Unicode 字符）。
pub const MAX_LABEL_CHARS: usize = 20;
/// 指令长度上限（含 `{text}` 占位符）。
pub const MAX_INSTRUCTION_CHARS: usize = 2000;
const FILE_NAME: &str = "prompts.json";

pub(crate) fn store_path() -> Option<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let dir = PathBuf::from(local).join("com.deepseekfloat.desktop");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(FILE_NAME))
}

/// 内置提示词统一前置的系统性约束：要求模型忽略历史对话、仅依据本次指令作答。
/// 置于每条内置指令最前，避免模型引用/延续此前会话内容。
const BUILTIN_PREFIX: &str =
    "请忽略此前所有对话内容，不要引用、总结、延续或推测任何历史信息。以下指令为唯一依据。";

/// 内置 4 项的默认值（首次启动 / 文件丢失时的种子）。
pub fn defaults() -> Vec<Prompt> {
    vec![
        Prompt {
            id: "explain".into(),
            label: "解释".into(),
            instruction: format!("{BUILTIN_PREFIX}\n\n请解释下面的内容："),
            is_builtin: true,
            hidden: false,
            show_card: true,
        },
        Prompt {
            id: "translate".into(),
            label: "翻译".into(),
            instruction: format!("{BUILTIN_PREFIX}\n\n请把下面的内容翻译成中文，只输出译文："),
            is_builtin: true,
            hidden: false,
            show_card: true,
        },
        Prompt {
            id: "summarize".into(),
            label: "总结".into(),
            instruction: format!("{BUILTIN_PREFIX}\n\n请用要点总结下面的内容："),
            is_builtin: true,
            hidden: false,
            show_card: true,
        },
        Prompt {
            id: "ask".into(),
            label: "问问DeepSeek".into(),
            instruction: BUILTIN_PREFIX.to_string(),
            is_builtin: true,
            hidden: false,
            show_card: true,
        },
    ]
}

/// 内存缓存：避免每次弹窗/点击都读盘 + 反序列化（单进程内配置只经本模块修改，无外部失效问题）。
fn cache() -> &'static std::sync::Mutex<Option<Vec<Prompt>>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<Vec<Prompt>>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

/// 从磁盘读取；文件不存在/解析失败 → 返回默认内置列表。
/// 命中内存缓存直接返回，未命中才读盘并回填缓存。
pub fn load(_app: &AppHandle) -> Vec<Prompt> {
    let _data = crate::data_store::read_guard();
    load_under_data_lock()
}

/// 读取提示词；调用方已持有 `data_store` 读锁或写锁。
pub(crate) fn load_under_data_lock() -> Vec<Prompt> {
    if let Some(list) = cache().lock().ok().and_then(|g| g.clone()) {
        return list;
    }
    let list = load_from_disk();
    if let Ok(mut g) = cache().lock() {
        *g = Some(list.clone());
    }
    list
}

fn load_from_disk() -> Vec<Prompt> {
    let Some(p) = store_path() else {
        return defaults();
    };
    let Ok(raw) = std::fs::read_to_string(&p) else {
        return defaults();
    };
    let mut list: Vec<Prompt> = serde_json::from_str(&raw).unwrap_or_default();
    sanitize(&mut list);
    list
}

/// 保存到磁盘；返回 sanitize 后的列表（调用方可用它回填 UI）。写盘成功后同步内存缓存。
///
/// 错误统一为 crate 级 [`AppError`](crate::error::AppError)：定位目录失败 → `Unavailable`，
/// 序列化失败 → `Json`，写盘失败 → `Io`，调用方按需渲染（IPC 边界见 `settings_save`）。
pub fn save(_app: &AppHandle, list: &[Prompt]) -> AppResult<Vec<Prompt>> {
    let _data = crate::data_store::write_guard();
    let mut list = list.to_vec();
    sanitize(&mut list);
    let Some(p) = store_path() else {
        return Err(AppError::Unavailable(
            "无法定位配置目录（缺少 LOCALAPPDATA 环境变量）",
        ));
    };
    let json =
        serde_json::to_string_pretty(&list).map_err(|e| AppError::json("序列化提示词失败", e))?;
    let mut guard = cache().lock().unwrap_or_else(|p| p.into_inner());
    write_atomic(&p, &json).map_err(|e| AppError::io("写入 prompts.json 失败", e))?;
    *guard = Some(list.clone());
    crate::data_store::mark_changed();
    Ok(list)
}

/// 清除进程内缓存；下一次读取会从已提交的数据文件重新加载。
pub(crate) fn clear_cache_after_restore() {
    *cache().lock().unwrap_or_else(|p| p.into_inner()) = None;
}

fn write_atomic(path: &std::path::Path, json: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    let result = std::fs::rename(&tmp, path);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// 在内存里把脏数据清掉，同时保证「内置 4 项恒在、自定义限量、id 唯一」。
pub fn sanitize(list: &mut Vec<Prompt>) {
    // 1) trim + 截断 + 丢弃无效项（内置项即使 label 被清空也保留，回退默认名）
    list.retain_mut(|p| {
        p.label = p.label.trim().to_string();
        p.instruction = p.instruction.trim().to_string();
        p.label = p.label.chars().take(MAX_LABEL_CHARS).collect::<String>();
        p.instruction = p
            .instruction
            .chars()
            .take(MAX_INSTRUCTION_CHARS)
            .collect::<String>();
        if p.is_builtin {
            if p.label.is_empty() {
                if let Some(d) = defaults().into_iter().find(|d| d.id == p.id) {
                    p.label = d.label;
                }
            }
            true
        } else {
            !p.label.is_empty()
        }
    });

    // 2) 补缺失 id + 校正内置标记 + 去重（同 id 保留首条）
    let mut seen = std::collections::HashSet::with_capacity(list.len());
    list.retain_mut(|p| {
        if p.id.trim().is_empty() {
            p.id = format!("custom_{}", seen.len());
        }
        if BUILTIN_IDS.contains(&p.id.as_str()) {
            p.is_builtin = true;
        }
        seen.insert(p.id.clone())
    });

    // 3) 确保内置 4 项都在（缺失补默认）
    for d in defaults() {
        if !list.iter().any(|p| p.id == d.id) {
            list.push(d);
        }
    }

    // 4) 自定义数量上限（按现有顺序截断，内置不占名额）
    let mut custom_kept = 0usize;
    list.retain(|p| {
        if p.is_builtin {
            return true;
        }
        custom_kept += 1;
        custom_kept <= MAX_CUSTOM
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_atomic_prompt_write_preserves_the_previous_file() {
        let dir =
            std::env::temp_dir().join(format!("dsfloat-prompts-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prompts.json");
        std::fs::write(&path, "previous").unwrap();
        std::fs::create_dir_all(path.with_extension("json.tmp")).unwrap();
        assert!(write_atomic(&path, "new").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous");
        std::fs::remove_dir_all(path.with_extension("json.tmp")).unwrap();
        write_atomic(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn custom_prompt_with_empty_instruction_is_kept_for_verbatim_sending() {
        let mut list = vec![p("custom_raw", "原样提问", "", false)];
        sanitize(&mut list);
        assert!(list.iter().any(|p| p.id == "custom_raw"));
    }

    fn p(id: &str, label: &str, instr: &str, builtin: bool) -> Prompt {
        Prompt {
            id: id.into(),
            label: label.into(),
            instruction: instr.into(),
            is_builtin: builtin,
            hidden: false,
            show_card: true,
        }
    }

    #[test]
    fn defaults_contains_four_builtins_in_order() {
        let d = defaults();
        assert_eq!(d.len(), 4);
        assert_eq!(
            d.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
            BUILTIN_IDS.to_vec()
        );
        assert!(d.iter().all(|x| x.is_builtin));
        // 每条内置都以系统性约束前缀开头（忽略历史、仅依本次指令）
        assert!(d.iter().all(|x| x.instruction.starts_with(BUILTIN_PREFIX)));
        // 问问DeepSeek 是裸文本：除前缀外无附加指令
        assert_eq!(d[3].instruction, BUILTIN_PREFIX);
    }

    #[test]
    fn sanitize_restores_missing_builtins() {
        let mut v = vec![p("custom_1", "我的", "x", false)];
        sanitize(&mut v);
        assert_eq!(v.len(), 5); // 1 自定义 + 4 内置
        assert!(v.iter().filter(|x| x.is_builtin).count() == 4);
    }

    #[test]
    fn sanitize_keeps_edited_builtin_instruction() {
        let mut v = vec![p("explain", "解释", "换个说法：", true)];
        sanitize(&mut v);
        let explain = v.iter().find(|x| x.id == "explain").unwrap();
        assert_eq!(explain.instruction, "换个说法：");
        assert_eq!(v.len(), 4); // 其余 3 个内置被补齐
    }

    #[test]
    fn sanitize_drops_invalid_customs() {
        let mut v = vec![
            p("explain", "解释", "请解释", true),
            p("custom_a", "", "x", false),  // 空 label → 丢
            p("custom_b", "ok", "", false), // 空指令 = 原样发送，保留
            p("custom_c", "好", "y", false),
        ];
        sanitize(&mut v);
        let customs: Vec<_> = v.iter().filter(|x| !x.is_builtin).collect();
        assert_eq!(customs.len(), 2);
        assert_eq!(customs[0].id, "custom_b");
        assert_eq!(customs[1].id, "custom_c");
    }

    #[test]
    fn sanitize_truncates_overlong_fields() {
        let mut v = vec![p(
            "explain",
            &"啊".repeat(MAX_LABEL_CHARS + 5),
            &"嗯".repeat(MAX_INSTRUCTION_CHARS + 5),
            true,
        )];
        sanitize(&mut v);
        let explain = v.iter().find(|x| x.id == "explain").unwrap();
        assert_eq!(explain.label.chars().count(), MAX_LABEL_CHARS);
        assert_eq!(explain.instruction.chars().count(), MAX_INSTRUCTION_CHARS);
    }

    #[test]
    fn sanitize_caps_custom_count() {
        let mut v: Vec<Prompt> = (0..(MAX_CUSTOM + 3))
            .map(|i| p(&format!("custom_{i}"), &format!("词{i}"), "x", false))
            .collect();
        sanitize(&mut v);
        let customs = v.iter().filter(|x| !x.is_builtin).count();
        assert_eq!(customs, MAX_CUSTOM);
        assert_eq!(v.iter().filter(|x| x.is_builtin).count(), 4);
    }

    #[test]
    fn sanitize_dedupes_by_id_keeping_first() {
        let mut v = vec![
            p("custom_x", "第一", "1", false),
            p("custom_x", "第二", "2", false),
            p("custom_y", "第三", "3", false),
        ];
        sanitize(&mut v);
        let x: Vec<_> = v.iter().filter(|p| p.id == "custom_x").collect();
        assert_eq!(x.len(), 1);
        assert_eq!(x[0].label, "第一");
    }

    #[test]
    fn sanitize_preserves_hidden_flag() {
        let mut v = vec![
            Prompt {
                id: "explain".into(),
                label: "解释".into(),
                instruction: "x".into(),
                is_builtin: true,
                hidden: true,
                show_card: true,
            },
            Prompt {
                id: "custom_z".into(),
                label: "隐藏的".into(),
                instruction: "y".into(),
                is_builtin: false,
                hidden: true,
                show_card: true,
            },
        ];
        sanitize(&mut v);
        // 隐藏的内置项仍保留且 hidden 不变；隐藏的自定义项也保留
        let explain = v.iter().find(|x| x.id == "explain").unwrap();
        assert!(explain.hidden);
        assert!(v.iter().find(|x| x.id == "custom_z").unwrap().hidden);
    }

    #[test]
    fn defaults_are_not_hidden() {
        assert!(defaults().iter().all(|p| !p.hidden));
    }

    // ---- show_card（交付方式：结果卡 / 面板）----

    #[test]
    fn defaults_deliver_by_result_card() {
        // R4：默认「显示结果卡片」（不弹面板）—— 划词主链路走卡片。
        assert!(defaults().iter().all(|p| p.show_card));
    }

    /// 旧 prompts.json 没有 `show_card` 字段 → 必须默认 true（缺字段默认 false 会让升级
    /// 后的用户**静默**改成交付方式：划词不再弹面板，看不出在哪儿答的）。
    #[test]
    fn show_card_defaults_true_when_field_absent() {
        let raw =
            r#"{"id":"explain","label":"解释","instruction":"x","is_builtin":true,"hidden":false}"#;
        let p: Prompt = serde_json::from_str(raw).expect("旧格式应能解析");
        assert!(p.show_card);
    }

    /// 显式关掉（走面板）时 sanitize 不得把它改回 true。
    #[test]
    fn sanitize_preserves_show_card_false() {
        let mut v = vec![Prompt {
            id: "explain".into(),
            label: "解释".into(),
            instruction: "x".into(),
            is_builtin: true,
            hidden: false,
            show_card: false,
        }];
        sanitize(&mut v);
        assert!(!v.iter().find(|p| p.id == "explain").unwrap().show_card);
    }
}
