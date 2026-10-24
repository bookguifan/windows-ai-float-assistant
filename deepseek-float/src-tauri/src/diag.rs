//! 独立诊断落盘：把 D1 划词链路的关键步骤写进
//! `%LOCALAPPDATA%\com.deepseekfloat.desktop\d1-debug.log`。
//!
//! 为什么不用 `eprintln!`：stderr 只在 `pnpm tauri dev` 控制台可见，进程退出即 flush，
//! 且 agent 侧读不到用户桌面的控制台输出。落盘后用户复现一次，agent 直接读这个文件即可定位。
//!
//! 用法：在 mousehook / capture / popup / settings / inject 的关键分支里调用
//! `crate::diag::{log,warn,error}("...")` —— 三档级别统一落盘，取代散落的
//! `eprintln!`/`println!`（release GUI 子系统下无控制台，这些打印会静默丢失）。
//! 每行自带"距进程启动的毫秒数"前缀，方便看先后顺序；文件 append 写，可反复复现。
//!
//! ## 与 `tauri_plugin_log` 的分工（避免误用）
//!
//! 本项目存在**两条**日志通道，职责不可混淆：
//!
//! | 通道 | 用途 | 脱敏 | 目标 |
//! |---|---|---|---|
//! | `tauri_plugin_log`（`log`/`info!`/`warn!`/`error!`） | 运行时 / 开发者实时排障 | 否（明文） | 环形缓冲 + 控制台 |
//! | 本模块 `crate::diag::{log,warn,error}` | 用户侧**诊断导出**（报障复现） | 是（`Meta`/`Full` 分档，正文可隐去） | 落盘 `d1-debug.log` |
//!
//! 规则：
//! - 仅需「我自己跑起来看现场」的日志 → 走 `tauri_plugin_log`；
//! - 需要「让用户双击复现一次、把文件发我」的日志 → 走本模块，且其内容必须经过
//!   [`content`] 脱敏包装（含页面文本 / 回答 / 划词原文等隐私字段，**严禁**直接明文落盘）；
//! - 本模块在 release 默认 `Off`（零落盘），不会因误用而泄露用户内容；
//!   但若显式开启 `Full`，落盘内容含正文，仅限用户主动开启的诊断场景。
//!
//! # 开关（R17-P1：release 也能开）
//!
//! 由环境变量 `DSFLOAT_DIAG`（或 `diag.flag` 标记文件，见下节）决定，**只在进程启动时读一次**（运行中改无效）：
//!
//! | `DSFLOAT_DIAG` | debug 构建 | release 构建（= 分发给用户的绿色版） |
//! |---|---|---|
//! | 未设置 / `0` / `off` / `false` | **全量**（与历史行为一致） | **关闭**（零落盘） |
//! | `1` / `true` / `meta` / 其它非空串 | 全量 | **元信息**：正文替换为 `«正文已隐去 len=NN h=HEX»` |
//! | `full` / `2` / `all` | 全量 | **全量**：含正文 |
//!
//! ## 为什么这样分档
//!
//! R1–R15 的**全部**真机缺陷都靠这个文件定位，但 release 默认关闭：一来划词热路径的同步
//! 落盘 I/O 不该常开，二来日志会含用户划词原文与页面文本（隐私）。分档后——用户报障时
//! 只需带一个环境变量启动，我们就能拿到「**不含正文**的现场」；确实需要正文时，再由用户
//! 显式给出 `full`。拼错变量值（例如写成 `DSFLOAT_DIAG=true123`）一律落到 `Meta`：
//! **宁可少记正文，也不能因为拼写错误而意外关闭诊断**。
//!
//! ## 不设环境变量的用户怎么开（绿色版是双击启动的）
//!
//! 双击 exe 的用户没法设环境变量，故另有**文件式开关**：只要
//! `%LOCALAPPDATA%\com.deepseekfloat.desktop\diag.flag` 这个文件**存在**，
//! 就等同于 `DSFLOAT_DIAG=1`（Meta 档）。支持流程：让用户把空文件丢进该目录 → 重启应用。
//! **环境变量优先**（非空即覆盖文件开关），便于开发与自动化脚本；显式 `off` 仍可关掉。
//!
//! ## 正文脱敏的落点
//!
//! 含「页面文本 / 回答预览 / DOM 骨架 / 划词结果」的调用一律走 [`content`] 包装；
//! 只记长度与状态字的调用（如 `capture: read_len=42`）不包装，无需处理。

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// 进程启动时刻（用于相对时间戳）。
static T0: OnceLock<Instant> = OnceLock::new();
/// 行号，辅助肉眼判断顺序（相对时间可能相同）。
static SEQ: AtomicU64 = AtomicU64::new(0);
/// 本进程的诊断级别。`DSFLOAT_DIAG` 只在启动读一次，结果缓存于此。
static LEVEL: OnceLock<Level> = OnceLock::new();
/// 目录创建 + 体积轮转只做一次。
static PREPARED: OnceLock<()> = OnceLock::new();

/// 最近事件环形缓冲（有界）。**始终记录**，不受诊断档位 `enabled()` 门控 ——
/// 即便 release 默认 `Off`，崩溃摘要（[`crash_summary`]）仍需"崩溃前最近 N 条关键事件"
/// 来还原现场（WebView2 原生 abort 不走 panic hook，日志里缺 `run: Exit*` 时，
/// 最近事件序列是判断"谁死、怎么死"的唯一线索）。
///
/// 容量很小（见 [`RECENT_CAP`]），内存与锁开销可忽略；每条按 [`RECENT_LINE_CAP`] 截断，
/// 避免单条过长的现场字符串把缓冲撑爆。
static RECENT: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// 环形缓冲最多保留的事件条数。
const RECENT_CAP: usize = 80;
/// 单条事件在缓冲中的最大字符数（超出截断，保留头 + 标记）。
const RECENT_LINE_CAP: usize = 240;

/// 单行内联正文上限（`Full` 模式）：超过即截断，避免一条日志几万字符把文件撑爆。
const MAX_INLINE_CHARS: usize = 2000;
/// 单文件体积上限：启动时若已超过，先把旧文件改名为 `d1-debug.log.1`（只保留一代）。
///
/// 只在进程启动时检查 —— 每次 `emit` 都去 `metadata()` 会给热路径平白加一次系统调用。
/// 上限 8 MB 对「一次报障复现」的日志量而言绰绰有余。
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;
/// 诊断日志文件名。沿用历史名 —— `docs/` 与既有排查流程都按它检索，改名会断掉锚点。
const FILE_NAME: &str = "d1-debug.log";
/// 文件式开关名（与 `d1-debug.log` 同目录）。存在即等价于 `DSFLOAT_DIAG=1`（Meta 档）。
///
/// 见模块头「不设环境变量的用户怎么开」：绿色版双击启动，支持流程只能靠放文件。
const FLAG_FILE: &str = "diag.flag";

/// 诊断级别。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    /// 关闭：不落盘、不打印（release 的默认档）。
    Off,
    /// 仅元信息：正文替换为 `«正文已隐去 len=NN h=HEX»`。
    Meta,
    /// 全量：含正文（debug 的默认档；release 需显式 `DSFLOAT_DIAG=full`）。
    Full,
}

/// 标记文件 `diag.flag` 是否存在（与日志同目录）。
fn flag_present() -> bool {
    log_path()
        .map(|p| p.with_file_name(FLAG_FILE).exists())
        .unwrap_or(false)
}

/// 解析诊断级别（纯函数，便于单测 —— 全局缓存使其无法在测试里反复设环境变量）。
///
/// `debug` 即 `cfg!(debug_assertions)`。**debug 构建恒为 `Full`**：开发机上不该因为
/// 某个残留环境变量而丢掉正文（audit 的既定策略是「debug 恒开」）。
fn parse_level(raw: &str, debug: bool) -> Level {
    if debug {
        return Level::Full;
    }
    match raw.trim().to_ascii_lowercase().as_str() {
        "full" | "2" | "all" | "verbose" => Level::Full,
        "" | "0" | "false" | "off" | "no" => Level::Off,
        // 其余非空值（`1`/`true`/`meta`/`on`/任意拼写）一律按「只记元信息」处理。
        _ => Level::Meta,
    }
}

/// 综合三处输入得到档位：环境变量 / 标记文件 / 构建类型（纯函数，便于单测）。
///
/// 优先级：**debug > 环境变量（非空即覆盖）> 标记文件 > 关**。
/// 环境变量优先是为了让自动化脚本能确定性地压过用户放的标记文件（例如 `off`）。
fn resolve_level(env_raw: &str, debug: bool, flag: bool) -> Level {
    if debug {
        return Level::Full;
    }
    if !env_raw.trim().is_empty() {
        return parse_level(env_raw, debug);
    }
    if flag {
        Level::Meta
    } else {
        Level::Off
    }
}

/// 当前进程的诊断级别（首次调用时解析环境变量/标记文件并缓存）。
pub fn level() -> Level {
    *LEVEL.get_or_init(|| {
        resolve_level(
            &std::env::var("DSFLOAT_DIAG").unwrap_or_default(),
            cfg!(debug_assertions),
            flag_present(),
        )
    })
}

/// 诊断是否启用（release 未设 `DSFLOAT_DIAG` 时为 `false`）。
///
/// 供调用方**跳过昂贵的现场构造**（如 `deliver` 里 200 字的回答预览、
/// 页面 DOM 骨架字符串的格式化）—— 关闭时这些纯属白干。
pub fn enabled() -> bool {
    level() != Level::Off
}

/// 是否允许记录用户正文 / 页面文本。
pub fn verbose() -> bool {
    level() == Level::Full
}

/// INFO：正常流程里程碑（启动、窗口创建、状态切换）。
pub fn log(line: &str) {
    emit("INFO", line);
}

/// WARN：功能降级 / 可恢复异常（弹窗失败、剪贴板恢复失败、注册表写失败等）。
pub fn warn(line: &str) {
    emit("WARN", line);
}

/// ERROR：关键能力不可用（窗口/钩子/注入通道创建失败）。
pub fn error(line: &str) {
    emit("ERROR", line);
}

/// 把「可能含用户正文 / 页面文本」的字符串交给当前级别处理。
///
/// - `Full`：[原样返回]，超过 [`MAX_INLINE_CHARS`] 则截断并标注总长；
/// - `Meta`：**不返回原文**，替换为 `«正文已隐去 len=NN h=HEX»`；
/// - `Off`：返回空串（整条日志本来也不会落盘，省掉哈希开销）。
///
/// `h` 是 FNV-1a 32 位哈希：**同一段文本每次得到同一个值**，于是「两行日志里的正文
/// 是不是同一段」仍可比对，同时原文不出文件。非加密用途，够用即可。
pub fn content(s: &str) -> String {
    render_content(s, level())
}

/// [`content`] 的实现（级别显式传入，便于单测覆盖三档）。
fn render_content(s: &str, lv: Level) -> String {
    let n = s.chars().count();
    match lv {
        Level::Off => String::new(),
        Level::Meta => format!("«正文已隐去 len={n} h={:08x}»", fnv1a32(s)),
        Level::Full => {
            if n <= MAX_INLINE_CHARS {
                s.to_string()
            } else {
                let head: String = s.chars().take(MAX_INLINE_CHARS).collect();
                format!("{head}…«已截断，共 {n} 字»")
            }
        }
    }
}

/// FNV-1a 32 位（非加密，仅用于「同一段文本 → 同一个短标识」）。
fn fnv1a32(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// 当前 UTC 时间，RFC3339 形式（如 `2026-09-15T08:00:00Z`），用于启动信标等
/// 人在读的日志。零依赖：用 Hinnant 的民用历法算法从 epoch 秒换算，不引 date 库。
pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // civil_from_days：返回 (count of leap years before, month, day) 简化为 (y,m,d)。
    let z = days + 719_468; // days since 0000-03-01
    let era = z / 146_097;
    let doe = (z - era * 146_097) as i64; // [0,146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + (era as i64) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y } as u32;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// 日志文件路径（`%LOCALAPPDATA%\com.deepseekfloat.desktop\d1-debug.log`）。
fn log_path() -> Option<std::path::PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    Some(
        std::path::Path::new(&local)
            .join("com.deepseekfloat.desktop")
            .join(FILE_NAME),
    )
}

/// 建目录 + 超限轮转（进程内只做一次）。
fn prepare_once() {
    PREPARED.get_or_init(|| {
        if let Some(p) = log_path() {
            let _ = std::fs::create_dir_all(p.parent().unwrap_or(std::path::Path::new(".")));
            let too_big = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > MAX_LOG_BYTES;
            if too_big {
                // 只留一代：`d1-debug.log` → `d1-debug.log.1`（覆盖上一代）。
                let _ = std::fs::rename(&p, p.with_extension("log.1"));
            }
        }
    });
}

/// 把一条事件压入最近事件环形缓冲（有界、始终记录）。
///
/// 与 [`emit`] 的文件落盘解耦：缓冲独立于诊断档位，仅为 [`crash_summary`] 提供"崩溃前现场"。
fn push_recent(entry: &str) {
    let capped: String = if entry.chars().count() > RECENT_LINE_CAP {
        let head: String = entry.chars().take(RECENT_LINE_CAP).collect();
        format!("{head}…«截断»")
    } else {
        entry.to_string()
    };
    if let Ok(mut v) = RECENT.lock() {
        if v.len() >= RECENT_CAP {
            v.remove(0); // 丢弃最旧
        }
        v.push(capped);
    }
}

/// 追加一行诊断（`log`/`warn`/`error` 三档共用）。失败（如目录不可写）静默，绝不影响主流程。
///
/// 级别为 [`Level::Off`]（release 未设 `DSFLOAT_DIAG`）时**跳过文件 I/O**，但**仍记录到
/// 最近事件环形缓冲**（见 [`RECENT`]）—— 保证崩溃摘要在 release 下也能还原有限现场。
/// stderr 回显只在 debug 保留：release 是 GUI 子系统、根本没有控制台，
/// 每次多打一行只是白白付出一次系统调用。
fn emit(level: &str, line: &str) {
    let t0 = T0.get_or_init(Instant::now);
    let rel_ms = t0.elapsed().as_millis();
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    // ★ 始终记录最近事件（不受 enabled() 门控），为崩溃摘要提供现场。
    push_recent(&format!("#{seq:<4} +{rel_ms:>6}ms [{level}] {line}"));
    if !enabled() {
        return;
    }
    prepare_once();
    let epoch = epoch_ms();
    let msg = format!("[{epoch}] +{rel_ms:>6}ms #{seq:<4} [{level}] {line}\n");
    if let Some(p) = log_path() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
        {
            let _ = f.write_all(msg.as_bytes());
        }
    }
    // 同时打到 stderr，便于开发时在 dev 控制台实时看到（release 无控制台，故不编译进来）。
    #[cfg(debug_assertions)]
    eprintln!("[D1][{level}] {line}");
}

/// 生成结构化崩溃摘要 `last_crash.json`：崩溃时间、消息、位置、回溯（截断）与最近 N 条事件。
///
/// 由 `lifecycle.rs` 的 panic hook 在写 `panic.log` 后调用。失败（目录不可写）静默，
/// 绝不因诊断子系统本身影响主流程或二次崩溃。
///
/// 字段：
/// - `message` / `location` / `backtrace`：panic 信息（回溯超长时截断，避免文件过大）；
/// - `recent_events`：崩溃前最近 N 条关键事件（环形缓冲快照），用于在无 `panic.log` 原文时
///   还原"死在第几步"。
pub fn crash_summary(message: &str, location: Option<&str>, backtrace: &str) {
    if let Some(p) = log_path() {
        let dir = p.parent().unwrap_or(std::path::Path::new("."));
        let _ = std::fs::create_dir_all(dir);
        let recent: Vec<String> = RECENT.lock().map(|v| v.clone()).unwrap_or_default();
        let bt = if backtrace.chars().count() > 8192 {
            format!(
                "{}…«回溯已截断»",
                backtrace.chars().take(8192).collect::<String>()
            )
        } else {
            backtrace.to_string()
        };
        let obj = serde_json::json!({
            "tool": "deepseek-float",
            "version": env!("CARGO_PKG_VERSION"),
            "crash_time_rfc3339": now_rfc3339(),
            "diag_level": format!("{:?}", level()),
            "message": message,
            "location": location,
            "backtrace": bt,
            "recent_events": recent,
        });
        let _ = std::fs::write(
            dir.join("last_crash.json"),
            serde_json::to_string_pretty(&obj).unwrap_or_default(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_rfc3339_is_well_formed() {
        let s = now_rfc3339();
        assert!(s.len() == 20, "len={} s={s}", s.len());
        assert_eq!(&s[10..11], "T");
        assert_eq!(&s[19..20], "Z");
        // 片段应是数字
        for (a, b) in [(0, 4), (5, 7), (8, 10), (11, 13), (14, 16), (17, 19)] {
            assert!(s[a..b].parse::<u32>().is_ok(), "非数字段 s={s}");
        }
    }

    #[test]
    fn parse_level_debug_is_always_full() {
        // 开发机不该因为残留的环境变量而丢正文。
        for raw in ["", "0", "off", "false", "1", "meta", "full", "???"] {
            assert_eq!(parse_level(raw, true), Level::Full, "raw={raw:?}");
        }
    }

    #[test]
    fn parse_level_release_matrix() {
        for raw in ["", "  ", "0", "false", "off", "no", "OFF", " False "] {
            assert_eq!(parse_level(raw, false), Level::Off, "raw={raw:?}");
        }
        // 拼错/未知 → 保守落在 Meta：开诊断，但不记正文。
        for raw in ["1", "true", "on", "yes", "meta", "ture"] {
            assert_eq!(parse_level(raw, false), Level::Meta, "raw={raw:?}");
        }
        for raw in ["full", "2", "all", "verbose", "FULL"] {
            assert_eq!(parse_level(raw, false), Level::Full, "raw={raw:?}");
        }
    }

    #[test]
    fn resolve_level_precedence() {
        // debug 恒 Full：无视环境变量与标记文件。
        assert_eq!(resolve_level("", true, false), Level::Full);
        assert_eq!(resolve_level("off", true, true), Level::Full);
        // 环境变量非空 → 覆盖标记文件（显式 off 能压过用户放的 diag.flag）。
        assert_eq!(resolve_level("off", false, true), Level::Off);
        assert_eq!(resolve_level("1", false, false), Level::Meta);
        assert_eq!(resolve_level("full", false, false), Level::Full);
        // 环境变量空/纯空白 + 有标记文件 → Meta（文件式开关）。
        assert_eq!(resolve_level("", false, true), Level::Meta);
        assert_eq!(resolve_level("   ", false, true), Level::Meta);
        // 两者都无 → 关（release 默认零落盘）。
        assert_eq!(resolve_level("", false, false), Level::Off);
    }

    #[test]
    fn content_off_yields_nothing() {
        assert_eq!(render_content("机密原文", Level::Off), "");
    }

    #[test]
    fn content_meta_hides_body_but_keeps_length_and_stable_hash() {
        let a = render_content("机密原文", Level::Meta);
        assert!(!a.contains("机密"), "Meta 档不得出现正文：{a}");
        assert!(a.contains("len=4"), "{a}");
        // 同一段文本 → 同一个 h（便于跨日志行比对）。
        assert_eq!(a, render_content("机密原文", Level::Meta));
        // 不同文本 → 不同 h。
        assert_ne!(a, render_content("机密原交", Level::Meta));
    }

    #[test]
    fn content_full_passes_through_and_truncates_long_input() {
        assert_eq!(render_content("原文", Level::Full), "原文");
        let long = "字".repeat(MAX_INLINE_CHARS + 10);
        let out = render_content(&long, Level::Full);
        assert!(out.starts_with(&"字".repeat(MAX_INLINE_CHARS)));
        assert!(out.contains("已截断"));
        assert!(out.contains(&format!("共 {} 字", MAX_INLINE_CHARS + 10)));
    }

    #[test]
    fn fnv1a32_is_stable_and_discriminating() {
        assert_eq!(fnv1a32("abc"), fnv1a32("abc"));
        assert_ne!(fnv1a32("abc"), fnv1a32("abd"));
        // 与已知向量对齐（FNV-1a 32 位标准测试向量）。
        assert_eq!(fnv1a32(""), 0x811c_9dc5);
        assert_eq!(fnv1a32("a"), 0xe40c_292c);
    }

    #[test]
    fn crash_summary_writes_json_with_recent_events() {
        // 在临时目录下放 LOCALAPPDATA，验证 last_crash.json 生成且含最近事件序列。
        let tmp = std::env::temp_dir().join(format!("dsfloat_crashtest_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        std::env::set_var("LOCALAPPDATA", &tmp);
        emit("INFO", "boot ok");
        emit("WARN", "something degraded");
        crash_summary("test panic", Some("src/main.rs:1"), "fake backtrace");
        let p = tmp
            .join("com.deepseekfloat.desktop")
            .join("last_crash.json");
        assert!(p.exists(), "last_crash.json 应被写出");
        let s = std::fs::read_to_string(&p).unwrap();
        assert!(s.contains("\"message\": \"test panic\""), "{s}");
        assert!(s.contains("\"recent_events\""), "{s}");
        assert!(s.contains("boot ok"), "应含最近事件：{s}");
        assert!(s.contains("\"location\": \"src/main.rs:1\""), "{s}");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
