# Changelog

本文件记录本项目所有值得关注的变更。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 2026-10-21 — 开源准备与低风险安全整改

- 结果卡外链改为 Windows 原生 URL 关联打开，移除 `cmd /c start`；增加结构化 HTTP(S) 与控制字符校验，补充两项 Rust 校验测试（本轮未执行）。
- 签名流程不再输出 PFX 参数、签名器原始输出或异常消息；增加四项模拟签名日志测试（本轮未执行），保留原有可选签名政策。
- 剪贴板 UTF-16 读取受实际 `GlobalSize` 分配大小限制，拒绝没有范围内终止符的数据，补充两项纯逻辑测试（本轮未执行）；两份绿色版构建入口加入 Cargo `--locked`。
- CI 增加前端单测、Playwright 和净化白名单检查，改用 Node 24，收窄仓库令牌和 checkout 凭据权限；声明 Node 24.15+ 的 24.x 环境要求。
- 补充安全报告渠道、贡献指南、Issue/PR 模板、第三方许可清单和开源准备报告；校准 README 的截图交互、菜单尺寸、第三方声明与隐私说明，扩充针对性的本地数据/凭据忽略规则。
- 只做源码准备：未发布 EXE、未公开仓库、未改写历史；本轮动态测试与完整审计验证器受环境限制，具体未验证项见报告。

### 2026-10-19 — README 审核与事实校准

- 更正 Git remote / CI 触发说明、前端检查门禁数、WebView2 运行时依赖和当前六站验收状态。
- 删除 `robocopy /MIR` 清理建议，改为项目级 `cargo clean`；明确 `--no-frontend` 仅适用于已核实新鲜的 dist。
- 将 M3、站点兼容方案和阶段性实施记录标注为历史状态，补充 JS 单测与 Playwright 使用说明。

### 2026-10-18 — 仓库维护与绿色版构建

- 使用 `pnpm release --no-frontend` 完成 release 构建；执行前核对 `dist/poc` 的 6 个页面均不早于源码，Cargo release 和四项发布门禁通过。
- 刷新绿色版交付物：7,537,664 字节，SHA256 `27A2D033C10A0589C2DD72F2F6C75415D57F3392DF2FC9D6605930CF1BD34A7A`；同步 README 与交接文档。
- 更新文档索引，区分当前事实源和带日期的历史审计；未修改业务源码、依赖或 Git 忽略规则。
- 清理扫描没有发现 A 类临时残留；构建缓存、历史记录、验收证据及备份数据按清理边界保留。

### 2026-10-18 — 配置与收藏备份恢复

- 设置页新增版本化 JSON 备份导入/导出；白名单仅迁移提示词、自定义站点、当前面板地址、三项常规偏好和收藏，不包含登录态、窗口坐标、自启注册表、诊断日志或整个应用数据目录。
- 导入前校验格式、版本、大小、数量、字段、重复 ID、站点 URL 与疑似凭据，并预览记录数量及合并冲突；支持保留本机优先的合并和二次确认覆盖。
- 配置、提示词和收藏恢复使用共享读写协调及跨文件事务日志；写入失败回滚，启动时识别并恢复中断事务，同时刷新设置页和打开中的收藏夹。
- 新增隔离 Rust 与设置页交互测试；未执行六站真实网站回归。

### 2026-10-18 — 真实站点兼容与业务验收

- 六站输入和回答回读增加语义适配；严格绑定本轮新增提问，不确定发送状态不自动重发，新会话失效旧注入和回读。
- 修复连续相同问题返回相同短答案时被旧答案过滤的问题，同时保留旧节点复用隔离。
- 适配 ChatGPT 新版 CodeMirror 代码块；保留 Rust 回读中的 HTML 换行与缩进，避免收藏和导出代码被压成一行。
- 收紧 ChatGPT 渲染完成条件，逐字渲染或代码缺行占位仍存在时不交付为完整回答。
- 修复 DeepSeek 和 ChatGPT 附件区域超出输入框内层导致上传确认失败的问题；避开拍照专用入口，上传未完成时不发送 OCR 指令。
- 增加逐站真实验收、固定 OCR 图片生成及字符/排版评分工具；实际通过范围与外部站点限制见真实验收报告。

### 2026-10-14 — 首批体验完善

- 提示词保存显示进行中、成功及失败，失败保留最新修改并支持重试；连续编辑按顺序写入，旧初始化数据不能覆盖未保存内容。
- 保存成功返回实际保存列表；提示词原子写入，修正自定义空指令被删除的问题，与界面“留空原样发送”保持一致。
- 设置页管理最多20个自定义站点，支持新增、改名、改址、排序、删除和切换；菜单及下拉同步展示，200×180站点菜单支持滚动。
- 首次读取站点清单成功前禁止保存，读取失败可重试；保存失败不更改后端缓存，延迟旧读取不能覆盖新保存内容。
- 收藏支持标题、问题和Markdown正文搜索、匹配数量、清空筛选；删除状态跨搜索重绘保留，避免重复删除。
- 新增设置DOM、浏览器布局及隔离原生写盘回归脚本 `scripts/e2e-first-batch.mjs`。

### 2026-10-14 — 面板工作流完善

- 右键增加“切换站点”子菜单，标记当前地址；设置与菜单共用换址及旧任务取消逻辑。
- 区分发送中、发送失败、读取中和回答超时；失败重试完整问题，超时仅继续读取。
- 新会话先失效旧注入、回读、恢复点及截图任务，再清空卡片并导航新文档。
- 框选后显示真实 PNG 预览，支持重选与取消；确认前不贴图，重复确认只投递一次。
- 结果卡增加收藏夹、逐轮与整会话 Markdown 导出；保存失败反馈，重名导出不覆盖。
- 补充 DOM、真实浏览器和隔离 WebView 工作流回归，覆盖取消、重试、超时、截图及菜单尺寸。

### 2026-10-13 — 自绘菜单与截图功能修复

- 悬浮球和托盘右键共用圆角菜单，保留显隐、截图、设置、刷新登录页面与退出入口。
- 修复首次右键创建 WebView2 阻塞主线程，改为 setup 预建隐藏窗口；动作按打开代次消费。
- 按遮罩物理原点和缩放换算截图选区，修复副屏坐标错位并裁剪边缘选区。
- 截图模式与任务编号固定，忽略重复和旧回调，投递及恢复完成后才接受下一次截图。
- 菜单压缩为 200×180，保持七项文字操作；修复拖动悬浮球被全局钩子误判为划词。
- 补充截图状态、菜单交互、球点击/拖动与真实 WebView 回归测试。


### 2026-10-03 — ChatGPT 面板 URL 站点可移植性：回读定位八次真机迭代

> **背景**：面板 URL 可配置为第三方站点（`config.json::panel_url`，如 `https://chatgpt.com/`）。
> 但「结果回流」的**会话容器定位**（`locateConversation`）与**尾窗抽取**（`snapshotConversation`）
> 长期假设 DeepSeek 特有的 DOM 形态（单子承载 ≥90%、`ds-*` 类名）。
> 切到 ChatGPT 后回读**系统性失效** —— 表现为结果卡空白 / 交付 UI 外壳 / 交付免责页脚 / 交付角色头。
> 本次以「**读真机日志取证、逐层下钻修复**」的方式，跨 8 次真机运行（批次 2′ → 5′）闭环。

#### 问题演进（八次真机）

| 次序 | 真机现象 | 根因 |
|---|---|---|
| 1 | 交付整页 `<main>`（2323 字符，混 UI 外壳） | 下钻无「择优」路径，停在多分支包装层 |
| 2 | 回读超时（9b 回归） | 同带护栏对 `display:contents`（无几何）候选误判 |
| 3 | 仍超时 | `isWrapper` 恒真 + `hop` 上限 4 不足 |
| 4 | `[L]` 停在 `composer-parent` | `isList` 未排除「含输入框的容器」 |
| 5 | 下钻基准被 composer 残留文本污染 | 未扣除输入区文本长度 |
| 6 | `bandTextLen` 削过头 → 钻进 `HEADER` | 未排除 composer 同带兄弟 |
| 7 | 停在「消息区 + 页脚」合体层 → 交付**页脚** | `isMsgListLike` 未识别「尾块是装饰条」 |
| 8 | 交付「`<h4>ChatGPT 说：</h4>`」+ 空回答 | `tailIsDecor` 作用域错误 ⟹ 每轮采样失败；角色头（`sr-only`）未剥离 |

#### 修复（按批次）

- **批次 2′**（`bcc26ea`）：容器下钻增加「**择优兜底**」路径（多分支候选按文本量/块数择优），
  不再停在混入 UI 外壳的包装层。
- **批次 9b**（`1e1a6ad`）：同带护栏改为**几何感知** —— `display:contents` 等无几何候选不再被误拒。
- **批次 2″**（`f4d2307`）：下钻引入「**list 优先 + 到达即停**」，`hop` 上限 4 → **12**；
  修正 `isWrapper` 恒真导致的误判。
- **批次 2‴ / 3′**（`66a7a82` / `785b99b` / `b385fae`）：`isList` 排除「含输入框的容器」（composer 祖先）；
  下钻基准改用 `bandTextLen`（**扣除输入区残留文本**）；再以 `isComposerSibling`（双条件）
  排除 composer 同带兄弟，避免「削过头钻进页眉」。
- **批次 4′**（`b2f82f4`）：引入 `tailIsDecor(bl)` —— **「尾块是装饰条 ⟹ 本层不是消息列表」**；
  `isMsgListLike` 追加该否决、`spread` 定向放宽（`|| tailIsDecor(kb)`）；
  Rust 侧 `analyze.rs::DECOR_HINTS` 2 → **5 项**（补 ChatGPT 免责页脚文案）作为最后防线。
- **批次 5′**（`48fc479`）：① 把 `tailIsDecor` / `CONV_DECOR_HINTS` **提升到顶层作用域**
  （首版误声明在 `locateConversation` 函数体内，被 `snapshotConversation` 调用时抛
  `ReferenceError` ⟹ 首轮定位成功后每轮采样失败）；② 新增 `isSrOnly(node)` ——
  **剥离视觉隐藏的角色头**（`<h4 class="sr-only">ChatGPT 说：</h4>`），
  `serializeHtml` / `toMarkdown` 与 `aria-hidden` 同列跳过。

#### 新增诊断（真机取证能力）

- `chain=` 容器链条（祖先 4 级 + 自身 + 后代 4 个，各级读 `tag/cls/cc/tl/h`）；
- `drillTrace` 下钻逐层读数（`hN:cc/tl→cc/tl[标记]`）；
- 起始 / 交付前容器骨架 dump（结构性 JSON）。

#### 新增守卫单测

- `tests/business-logic.test.mjs`：**⑤~⑮** 共 11 项（复刻各次真机形态 + **回退对照**验证守卫有效性）；
  其中 ⑮ 用**静态断言**锁死「`tailIsDecor` 必须声明在顶层作用域（2 空格缩进）」这一根因。
- `tests/sanitize.test.mjs`：`serializeHtml` 分区补 `sr-only` 剥离断言（含 `sr-only-*` 变体 + 窄口径不误伤）。
- `src-tauri/src/readback.rs`：+4 Rust 单测（页脚否决 ×2 + `locate_answer` 集成 ×2）。

#### 验证

- `pnpm test:unit`：**232 passed**（sanitize 71 + business-logic 161）。
- `cargo test --lib`：**179 passed / 0 failed**。
- `pnpm check` 四门禁、`cargo fmt --check`：全绿。
- **回退对照**（关键批次 4′ / 5′）：去掉修复后对应单测断言**失败**（复现真机缺陷）⟹ 守卫有效。

#### 文档

`docs/panel-url-site-portability-plan.md`：§9b.7（回归修复的四条通用教训）、§十三 / §13.7 / §13.8（实施记录）。

#### 交付物刷新（2026-10-03 21:40）

按 README 规范 `pnpm release` 重新打包绿色版单文件 exe（`CARGO_INCREMENTAL=0` 强制全量，耗时 **4m21s**）：

- `release/deepseek-float.exe` = **7,084,544 B**（上一版 7,041,024 B，**+43,520**），
  `sha256` = `37745F73020D64C9EC37EBF45DBE5F95030E366E6290A442F73157BFD93EBA9E`，与 `release/HASH.txt` 逐字符一致（大写 hex + CRLF = 66 B）。
- 前置：关闭占用 `src-tauri/target` 的 5 个 dev 进程（**未误杀** WorkBuddy 自身的 MCP node），全程零 `os error 5`。
- 门禁：`pnpm build` ✓ → `cargo build --release --features custom-protocol` ✓ → `pnpm check` 四门禁全过。
- **独立验证（QA）**：双工具复算 SHA256（`sha256sum` + `certutil`）→ **MATCH**；**反向验证**（故意改错哈希 → 应报 MISMATCH）✓；
  PE 头 `MZ` + 内嵌标记 `ball_ready` / `splitV` / `chat.deepseek.com` ✓（证明非白屏）；产物同一性（`release/` 与 `target/release/` 同哈希）✓。
- 同步刷新 `README.md` / `docs/handover.md` / `docs/distribution-plan.md` 的交付物登记。
- ⚠️ 未做 exe **运行时启动烟测** —— 属 README 发布 checklist #8「真机手动项」（需 GUI 会话）。

---

### 2026-09-28（下半场）— 工具条复制保留原文排版（换行 / 缩进）

> 修复「工具条『复制』把选区文本的多行结构压成一行」的缺陷。

#### 问题

工具条「复制」写入剪贴板的文本，来自 `PENDING_TEXT`，而它是 `capture::normalize_text()` 的产物 ——
该函数把 `\n` / `\t` / `\u{00a0}` / `\u{3000}` 全部折叠为单个半角空格。
⇒ 复制的 Markdown **列表、段落、代码块、表格行** 全部塌成一行，结构不可恢复。

（**结果卡「复制」不受影响**：`result.rs::copy()` 直接写 `domToMarkdown()` 的产物，未经过 `normalize_text`；
该路径本就在保留换行，本次未动。）

#### 修复（方案 C：旁路原文缓存）

不扩 `CaptureOutcome` 枚举、不动 `PENDING_TEXT` 与注入链路，只在抓词处**旁路留存一份原文**：

- `capture.rs`：新增 `static LAST_RAW: Mutex<Option<String>>` + `take_last_raw()` / `clear_last_raw()` / `raw_matches_pending()`；
  在 `capture_selection` 的闸门后按「成功写 / 失败清」写入。
- `popup.rs`：工具条「复制」优先取 `LAST_RAW` 原文，取不到则退化为原 `text`（**绝不失败**）。
- `popup.rs`：UIA 直读**成功**分支显式 `clear_last_raw()` ——
  ★ 该路径不产生原文，若不清空会让「复制」误取到**上一条**划词的文本（比丢换行更严重）。

#### 安全性

- **不动**：`CaptureOutcome`、`PENDING_TEXT` 类型、`LAST_CAPTURED` 去重、注入链路、`compose_custom`、超长截断逻辑。
- **自校验**：复制前用 `raw_matches_pending()`（判据：`normalize(raw)` 以 `pending` 为前缀）挡住缓存错配。
  判据取「前缀」而非「全等」—— 因为按用户要求「复制不截断」，
  而 `PENDING_TEXT` 存的是截断版，两者天然不等，全等判据会让每次超长复制**误退化为截断版**。
- **超长不截断**：复制的原文保留全文（注入侧仍截断至 20000，两侧语义不同属刻意设计）。

#### 验证

- `cargo test --lib`：**174 passed / 0 failed**（原 172 + 新增 2：缓存语义 / 自校验判据）。
- `cargo clippy -D warnings`、`cargo fmt --check`：全绿。
- `pnpm check` 四门禁、`pnpm test:unit`（112 项）：全绿。

#### 文档

新增 `docs/copy-preserve-layout-options.md`（三方案对比）、`docs/copy-preserve-layout-c-audit.md`（方案 C 落地审核 + 实施记录）；
原方案 A 标注**已弃用**；该方案及其审核稿于 2026-10-14 清理，最终实施依据保留在方案 C 审核记录中。

#### 二版修复：真机验证暴露「提权划词路径」的修复方向错误

真机验证时用户反馈「复制多行文本仍是单行」。**诊断日志一手证据**：

```
popup: 已复制 161 字符到剪贴板（来源=normalized(无原文)）
```

该日志格式**只有本次改动才有** ⇒ dev 跑的确实是新代码，但**取不到原文**；
且该次抓词**没有任何 `capture:` 日志** ⇒ 未走剪贴板法。查配置 `privileged_capture: true`
⇒ 走的是 **UIA 直读路径**。

- **错误**（一版）：判定「UIA 不产生原文 ⇒ 清空缓存防错配」。**目标对、手段错** ——
  `uia::uia_selection_text()` 的返回值**本身就是原文**（`range.GetText(-1)` 的文本，
  range **内部**换行是保留的），清掉它反而让复制退化得**比 UIA 原生能提供的还差**。
- **正确修法**：UIA 两个成功分支 `clear_last_raw()` → **`set_last_raw(raw)`**，
  两条抓词路径统一把原文写入缓存。同源性由 `raw_matches_pending` 自校验保证。
- **连带清理**：`clear_last_raw()` 生产路径已无调用者 ⇒ 删除该函数（否则 `clippy -D warnings` 报 `never used`）。
- **新增纯观测诊断**（`uia.rs`，零行为变更）：记录选区 `range 数 / 有效段 / 各段字符 / 拼接后换行数`。

**★ 实证有效的边界**（真机日志）：用户关闭提权划词后

```
popup: 已复制 112 字符到剪贴板（来源=raw）      ← ✅ 剪贴板路径修复生效
```

**★ 提权划词「作用不大」的实测数据**（来自用户完整日志统计）：

| 指标 | 次数 |
|---|---|
| UIA 取不到文本 → 回退剪贴板法 | **176** |
| 选区过短 → 回退 | 1 |
| UIA **真正成功** | **2** |

成功率约 **1%**，且成功时因 range 拼接丢换行。**结论：默认保持关闭**（`privileged_capture` 默认已是 `false`）。

**★ 未根治的残留**：`uia.rs` 拼接多个 range 时 `text.push(' ')` 会抹平**段间**换行。
不能简单换成 `'\n'`（多 range 未必是不同行，可能同一行的多个片段，盲插会**引入**不存在的换行），
需先调研 range 几何（`GetBoundingRectangles` 的 top 比较），属独立改造，本次不做。

**验证**：`cargo test --lib` **174 passed / 0 failed**；`clippy -D warnings` / `fmt --check` /
`pnpm check` 四门禁 / `pnpm test:unit`（112 项）全绿。

### 2026-09-28 — 构建根因根治 + 绿色版交付刷新 + 代码签名审计（本批次）

> 本批次的核心是**根治**（不是绕过）构建阻塞，并在此基础上有把握地重出交付物。

#### 根治构建阻塞：`schemars 0.8.22` × `indexmap 1.9.3` 的 `error[E0107]`

- **症状**：`pnpm tauri dev` / `cargo build` 在 `schemars` 编译期硬失败 ——
  `error[E0107]: struct takes 3 generic arguments but 2 generic arguments were supplied`（`schemars/src/lib.rs:12`），
  `error: could not compile 'schemars' (lib) due to 1 previous error`。**整个项目编译不过**。
- **真根因（5 环因果链，已取证）**：
  1. 报错行是 `pub type Map<K, V> = indexmap::IndexMap<K, V>;`（只给 2 个泛型参数）；
  2. `indexmap 1.9.3` 的 `IndexMap<K, V, S = RandomState>` 默认类型参数**只存在于 `#[cfg(has_std)]` 分支**（`map.rs:70-76` 两个 cfg 分支），另一分支的 `IndexMap<K, V, S>` **没有**默认值 ⇒ 报 E0107；
  3. `has_std` 由 `indexmap/build.rs` 产生，有**两条路**：
     `Some(CARGO_FEATURE_STD) => autocfg::emit("has_std")`（**路径 A：声明即生效**）/
     `None => autocfg::new().emit_sysroot_crate("std")`（**路径 B：依赖探测，本机失败**）；
  4. `indexmap` 的 `[features] default` **不含 `std`**（`Cargo.toml:104-107` 只有 `serde-1`/`std`/`test_debug`/`test_low_transition_point`，`default` 为空），全依赖树**无人开启 `std`** ⇒ 只能走路径 B；
  5. 本机 autocfg **探测失败** ⇒ `has_std` 从未定义 ⇒ 走无默认值分支 ⇒ E0107。
- **修复（两处各一行）**：`deepseek-float/src-tauri/Cargo.toml` 在 **`[dependencies]` 与 `[build-dependencies]` 两段各显式声明**
  `indexmap = { version = "1.9", features = ["std"] }`（附长注释记录完整因果链）。
- **★ 为什么必须两处（关键认知）**：Cargo 的 **feature unification 只在「同类依赖」内生效 —— `[dependencies]` 与 `[build-dependencies]` 是两套独立解析**。
  失败的 `schemars` 走的是 **build-dep 链**：`deepseek-float --[build-dependencies]--> tauri-build --> tauri-utils --> schemars --> indexmap`。
  只加 `[dependencies]` 一处时，`cargo tree` 显示 `std` 已挂上、但 build script 的产物里**仍无** `has_std` ⇒ 依旧报错。**两处都加才是根治**。
- **修复的确定性依据**：显式声明 `std` ⇒ `indexmap/build.rs` 走**路径 A**（不再依赖本机 autocfg 探测能力）⇒ 产物 `target/*/build/indexmap-*/output` 中出现 **`cargo:rustc-cfg=has_std`**，`stderr` **为空**（修复前 stderr 打出 `warning: autocfg could not probe for 'std'`）。
- **验证（三组合全过）**：`cargo check`（默认特性）✅ 2m09s / `cargo check --no-default-features` ✅ 7.94s（= dev 组合）/ `cargo check --release --features custom-protocol` ✅。
- **★ 副产品与纪律**：
  - **`RUSTFLAGS="--cfg has_std"` 从此废弃** —— 该法虽能"压住"报错，但会**全局污染所有 crate**，掩盖同类问题；已改用精确声明。
  - **`cargo check --lib` 对本问题是假阴性** —— `schemars` 只在 build-dep 链上被编译，`--lib` 根本不碰它。
  - 排查 build script 的**第一手资料**在 `target/*/build/<crate>-<hash>/stderr` 与 `output`（`cargo build -v` 之外的权威依据）。

#### 交付物刷新（2026-09-28）

- `deepseek-float/release/deepseek-float.exe` = **7,041,024 B**（上一版 7,031,808 B，**+9,216**），
  `sha256 = E7C15FD3578FF5897CEE041754A3F3194140474090BD9248C5135F4887EE1E9B`；
  `release/HASH.txt` 为同值（大写 hex、CRLF、**66 字节**），与 `sha256(exe)` **逐字符一致**；
  `target/release` 构建产物与交付位哈希**一致**（未脱钩）。
- 构建走 `pnpm release`，耗时 **9m39s**；门禁全绿（CSP / 内联 JS / 版本 / 净化白名单 + 内联脚本 5/5）。
- **内容取证**：新代码确已进产物 —— exe 内 `grep -a` 命中 `splitV: 5`、`roles: roles`、`ball_ready` 等标记（`dsfloat_init.js` 经 `include_str!` 逐字嵌入）。
- ⚠️ 本版**未做代码签名**（见下），故仍会触发 SmartScreen。

#### 代码签名审计（结论：需要，但当前无法执行）

- **技术上需要**：绿色版单文件 exe 没有安装器，**Authenticode 签名是唯一可携带的信任凭据**；不签则每个用户首次运行都会看到 SmartScreen 蓝色警告。
- **当前无法执行**：本机 `signtool.exe` 存在，但**无代码签名证书**（无 `.pfx`/`.p12`、`~/.tauri` 不存在、无签名相关环境变量）。
  - **自签名证书无效** —— 只在签发它的那台机器上受信，其他机器 SmartScreen 照样拦。
  - **必须购买 OV/EV 证书**：年费约 **$200–600**、需实名/企业主体，EV 还需硬件令牌。
- **处置**：列为**分发阻塞项**（不影响本轮交付可用性）。脚本侧 `--sign` 已是**非致命**设计（证书缺失只警告、不中止构建）；拿到证书后配好环境变量再 `pnpm release -- --sign`（交付在签名**之后**执行，`HASH.txt` 对应含签名的最终产物）。
- 详见 `deepseek-float/docs/distribution-plan.md` §十。

#### 打包流程与文档修订

- **README 快速开始**：`pnpm tauri build` → **`pnpm release`**；新增警告「**不要用 `pnpm tauri build`**」（`bundle.active=false` 只发绿色版；裸 `cargo build --release` 不带 `custom-protocol` ⇒ 前端资源不内嵌 ⇒ **打开即白屏**）；补 `dist/` 不存在时报 `frontendDist ... doesn't exist` 的说明。
- **README 发布前 checklist**：修订 **#5**（补 `--features custom-protocol`）、**#7**（原 `cargo audit --manifest-path` **跑不通** —— `cargo audit` 不接受该参数，必须 `cd src-tauri && cargo audit`）；新增 **#10 代码签名**项；新增「#7 与 #10 的环境限制」与「已知环境噪音」小节。
- **`docs/handover.md`**：交付物哈希/体积更新；构建命令改为 `pnpm release`；「三条最容易踩的构建前提」→ **四条**（新增：`cargo audit` 必须在 `src-tauri/` 内跑）。
- **`docs/distribution-plan.md`**：新增 **§九**（schemars × indexmap 根治全过程：因果链 / 修复 / feature unification 作用域 / 验证组合 / 副产品与纪律）+ **§十**（代码签名现状 / 为什么需要 / 为什么现在签不了 / 落地方式）。
- **`docs/architecture-summary.md`** 与 `docs/frontend-rust-contract.md` 同步（诊断日志出口、构建前提）。

#### 诊断日志补齐（P0 阶段 A，`uia.rs` / `capture.rs`）

- **零行为变更**：仅给 8 个原先**静默**的失败出口补日志（UIA 四属性 `CtrlType`/`Class`/`Fw`/`Name` + 三探针 `T2`/`VP`/`LP`；剪贴板 `CF_UNICODETEXT` 不可用时 `html=0/1 rtf=0/1` 三态），**不新增 return/break/continue**。
- **默认不落盘**：release 下 `diag::Level` 为 `Off`（`emit()` 直接 return）⇒ 需诊断时显式设 `DSFLOAT_DIAG=full`。
- **副产品**：原先"89 次回退 100% 集中在 `GetCurrentPattern(TextPattern)` 静默失败"的定性依据是「其它出口日志计数全为 0」；补了日志后**该依据消失** —— 下次复现若出现非零计数，**不是新问题，而是原先静默的旧现象**，勿误判为回归。

---

### Added

- **R1-R8 代码级风险修复 + UI 修正 + 绿色版重建（2026-09-15）**：按审核报告 Top 10 风险榜执行代码级修复，全部 `cargo fmt/clippy/test` 与门禁验证通过（125 passed）。
  - **UI**：结果卡右上角折叠/关闭按钮统一为 SVG 图标 16×16 居中，消除文本字符混排导致的比例失调；新增告警横幅 CSS（`.warnflag`）+ JS（`turnFlagMsg`），按优先级提示一条（发送失败 > 登录失效 > 疑似注入）。
  - **悬浮球残影**：新增 `hide_panel()` 统一面板隐藏逻辑，`hide()` 后 `eval("void 0")` 触发悬浮球 WebView2 重绘，消除透明球表面残留面板旧帧（「DeepSeek…好的」渗出）。
  - **R3 注入检测**：`detect_injection()` 纯函数，极保守匹配 `<script`/`javascript:`/`onerror=`/`onload=`；正文照常净化展示，仅额外提示，不强删不误杀。
  - **R4 渲染串行锁**：`RENDER_LOCK` 串行化并发推送的 `set_size`/`show`，避免多路推送竞态。
  - **R5 登录失效标志**：`SESSION_EXPIRED` 原子量，回读判定面板停在登录页时置位，结果卡提示重登。
  - **R6 剪贴板恢复校验**：恢复前比对当前剪贴板内容是否仍为本次写入，避免覆盖用户期间新复制内容。
  - **R7 发送失败标志**：`mark_send_confirmed_failed()` 由 `inject::verify_sent` 最终分支置位，结果卡提示「提问可能尚未发出」，不谎报已发送。
  - **单测**：5 组新测试（注入检测正反例、发送失败标志置位/读即清、Fresh/Update 标志传播、默认值校验），总计 125 passed / 0 failed。
  - **验证**：`cargo fmt` 通过；`cargo clippy` 0 警告；`cargo test` 125 passed；`cargo audit` 439 依赖 0 漏洞。

- **#7 悬浮球「首帧就绪」握手闸门（2026-09-26，新模块）**：
  - 新增 `src-tauri/src/ballready.rs`：一次性就绪门 `BallReadyGate`（`Mutex<bool>` + `Condvar`）+ 全局 `signal_ball_ready()` / `wait_ball_ready(timeout) -> bool` + 兜底超时常量 `BALL_READY_TIMEOUT = Duration::from_millis(3000)`（~3s）。设计约束：`signal` **幂等**；`wait` 采用「循环 + 截止时刻」而非单次 `wait_timeout`，正确处理 `Condvar` **伪唤醒**；锁中毒（`PoisonError`）时经 `into_inner()` 取回内部值继续、**绝不 panic** —— 本模块处在**启动关键路径**上，任何 panic 都会让球**永久不显形**（远比「空白」严重）。含 **6 个单测**。
  - 新增 IPC 命令 `ball_ready`（`src-tauri/src/ipc.rs:104`），转发到 `signal_ball_ready()`；无参数、无返回值，只承担「置位就绪闸门」这一件事。

### Changed

- **`lib.rs` 拆模块重构（2026-09-26，提交 `544dfa2`，代码已合）**：把 1300+ 行的 `lib.rs` 拆为职责单一的子模块，`lib.rs` 收敛到 **46 行**（仅模块声明与再导出）。拆分去向：`windows.rs`(632 行，声明为模块名 `panels` 以避免与外部 `windows` crate 重名) → 面板/悬浮球窗口生命周期、位置记忆、划词常驻窗 + 鼠标钩子、`flush_ball` 冲刷；`tray.rs`(188) → 系统托盘与球态右键菜单；`ipc.rs`(423) → 全部类型化 `#[tauri::command]`；`lifecycle.rs`(586) → 应用入口 `run` + 调试期自检/压测入口。对外调用点（`crate::show_panel` / `hide_panel` / `ensure_panel_loaded` / `redact_url` / `flush_ball`）经 `lib.rs` 统一 `pub(crate) use` 再导出，**无调用点改动**。验证：`cargo fmt --check` / `clippy -D warnings` 0 警告 / `cargo test --lib` **153 passed**。（同一提交还含 `Fixed` 节的「方案 A-2 球窗消镂空洞」；文档同步在 `8397068`。）

- **依赖审计 + 发布前 checklist + 清理历史残留（2026-09-15）**：
  - `cargo audit` 扫描 439 个依赖：0 安全漏洞；7 条 unmaintained/unsound 警告（均间接依赖，无阻断）。
  - README 新增「发布前 checklist」9 项（#1–#7 自动化门禁 + #8–#9 真机验证），固化 `DSFLOAT_SELFTEST_STRESS` 压测入流程。
  - 审核报告 §〇/§四 Top10 同步标记 #4/#6 已修。
  - 删除历史残留：`.workbuddy/`（OCR 试验、截图、诊断脚本、记忆文件，1.23 MB）、`docs/release-checklist.md`（已被 README checklist 取代）、`src-tauri/target/debug/`（编译中间产物）。
  - **绿色版重建**：`deepseek-float/release/deepseek-float.exe` = **6,974,976 B（≈6.65 MB）**，含 R1-R8 全部修复 + 依赖审计 + checklist。

- **R20+ 划词/回流边界增强（2026-09-16，代码已合，尚未重打包 exe）**：
  - **capture.rs**：超长选区改为**截断前 `MAX_CHARS` 并成功返回**（不再整段 `TooLong` 废弃）；「与上次内容相同」**不再阻断**（允许重发，化解「上次注入失败 → 同内容永远无法再触发」的死路）；抓词首轮轮询超时**补投一次 Ctrl+Insert 并重等**，慢应用不再直接 `NoNewCopy`。
  - **inject.rs**：发送重试改**指数退避**（400→800→1600→3000ms 封顶），面板慢加载/慢网络下首次回流成功率更高。
  - **mousehook.rs**：曾试做「双击/连击抑制」，后实证会误伤「单击后紧跟拖动选词」**已回退**，仍只按位移>8px 判定拖动。
  - **prompts.rs**：4 条内置提示词统一加前缀「请忽略此前所有对话内容，不要引用、总结、延续或推测任何历史信息。以下指令为唯一依据。」（仅对全新/无 `prompts.json` 用户生效，不覆盖已有配置）。
  - **result 残影定稿**：面板内 WebView 旧帧串扰透明球，采「开面板时对球 `hide→show` 重建合成表面（~30ms）」；曾试 `InvalidateRect(bErase=true)` 强制重绘，实证造成每次点击都闪，**废止**。
  - ⚠️ **以上改动需重新打包后才进入交付物**；当前 `release/deepseek-float.exe`（6,948,864 B）仍为含「残影隐现 + 首划重试」的前一版。

- **MEDIUM-2 / MEDIUM-3 清理（2026-09-21，代码已合，尚未重打包 exe）**：
  - **MEDIUM-2（死代码）**：移除 `capture::CaptureOutcome::TooLong` 变体（及其 `user_message` 分支、测试引用），并修正提权划词（UIA）超长分支——由返回 `TooLong` 失败改为**截断前 `MAX_CHARS` 字符并成功返回**，与剪贴板路径（`evaluate`）行为对齐，消除两条捕获链路的语义不一致。
  - **MEDIUM-3（守卫残留）**：`trigger_selection` 的「本应用窗口内划词」守卫在跳过抓词并提前返回前，新增 `clear_pending_text()` 清空残留的 `PENDING_TEXT`，避免上一次成功抓词留下的旧文本被后续动作误消费；新增 `clear_pending_text()` 及对应单测。
  - **验证**：`cargo check` / `cargo test --lib` 通过（无新增失败）；`cargo clippy` 0 警告。

- **BUG 修复：划词发送误点历史消息按钮，导致「内容进了输入框却没发 + 上一题被重复触发」（2026-09-20，代码已合，尚未重打包 exe）**：
  - **现象**：划词选中的新内容已填入 DeepSeek 输入框，但「发送」未真正成功，且上一轮问题被重复提交/重新生成。
  - **根因（源码级）**：`dsfloat_init.js` 的 `scoredSenders(input)` 原先在**整个页面** `document.querySelectorAll` 找发送候选。DeepSeek 在每条历史消息气泡上渲染「重新发送 / 重新生成」等动作按钮，其中用户消息的「重新发送」文案含「发送」二字 → `SEND_WORDS` 给 +5 分，与真正 composer 发送按钮同分甚至更高，被 `submit()` 误点：点了会把**上一题重新提交**，而当前输入框里的新内容却没发出。`inject.rs` 的 `verify_sent` 重试链（Click{0..4}→页内回车→OS 回车→None）还会连点多个历史消息按钮后才放弃，最终 `mark_send_confirmed_failed`（日志里的「发送不成功」）。`popup.rs`/`capture.rs` 侧已确认无陈旧文本复用路径（Rust 侧 `TRIGGER_GUARD` 串行化、`send_prompt` 恒用本次新抓取的 `prompt`），排除「旧文本被复用」。
  - **修复**：`scoredSenders` 的候选范围收敛到**输入框所在的 composer 容器**（`composerRootOf`：优先 `closest('form')`，否则取输入框向上首个含可点按钮的最小容器），历史消息气泡在更高层、不会落进来，从根本上排除「重新发送 / 重新生成」等误点；`submit` 点击前把实际点到的按钮文案写入 `window.__DSFLOAT__._diagSendLabel`，Rust 侧 `submit_step` 在 `ST_SENT` 后回读并落 `d1-debug.log`，便于核对是否点错按钮。
  - **验证**：`cargo check` / `cargo clippy --all-targets` 0 警告；`cargo test --lib` 全过（JS 为注入脚本，随 Rust 编译期字符串常量一并打包，无独立单测；行为由 `inspect`/`inspectConversation` 探针与真机复现核对）。
  - ⚠️ **提交归属更正**：本条与上文 `lib.rs` 拆模块重构（方案 A-2 球窗消「镂空洞」，见 `Fixed` 节）同属提交 **`544dfa2`**（2026-09-26 14:38），其文档同步在 **`8397068`**（15:21）。**二者均早于** `Fixed` 节的 `#13 E1/E2`（`31226ab`，19:51）与**晚于** `Added` 节的 `#7 球窗握手`（`83ba5ce`，19:52）。

- **#7 球窗启动显形时序改走「首帧就绪握手」（2026-09-26）**：启动期入口由「直接 `ball_show`」改为 `windows::gate_ball_show`（`src-tauri/src/windows.rs:263-302`）—— ① 先 `apply_ball_shape(app, false)` 维持**零面积空区域**（整窗不可见、且不吞点击）；② **同时**在 OS 层 `ShowWindow(SW_SHOWNOACTIVATE)` 显形以**驱动 WebView2 合成首帧**（隐藏窗下合成器不产渲染帧、页面 `rAF` 被暂停 ⇒ 若让球一直真·隐藏，首帧永不到来、握手必落空）；③ `thread::spawn` 等 `wait_ball_ready(BALL_READY_TIMEOUT)`，收到信号或超时后调 `ball_show`（`windows.rs:192-235`：先 `apply_ball_shape(app, true)` 套椭圆区域、再原生 `ShowWindow`；**刻意不调 Tauri 的 `show()`**，否则会重建 `WS_CAPTION` 出现边框）。球页侧上报：`poc/ball.html:118-129` 的 `notifyBallReady` 三条路径（双 `requestAnimationFrame` + `window` 的 `load` 事件 + `setTimeout(…, 4000)` 兜底），同处一段内联脚本。形状 API 见 `src-tauri/src/ballstyle.rs:167-210` 的 `apply_ball_shape`（`visible` ⇒ `CreateEllipticRgn(0,0,w,h)`；`!visible` ⇒ `CreateRectRgn(0,0,0,0)`）。**硬约束：等待超时必须照常显形**，绝不因超时把球藏起来。

- **#004 页内契约升版：`splitV` 4 → 5 + 新增块角色字段 `roles`（2026-09-26）**：`src-tauri/src/dsfloat_init.js:1646-1648` 的页内契约对象新增 `splitV: 5`（注释写明「日志只要出现 `splitV=5` 即证明新脚本在线；缺它 ⇒ 强制刷新面板页面 / 重建 WebView」；任何页内契约变更都应升版）；`dsfloat_init.js:1677` 新增块角色字段 `roles: roles`（`roles` 由 `:1631` 的 `parts.map(…)` 生成，取值 `assistant`/`user`/`unknown`，与 `texts`/`thinking` 逐项对齐，Rust 侧只在正向识别 `user` 时拒绝交付）。

- **交付物刷新（2026-09-26，提交 `d4afa88`）**：`deepseek-float/release/deepseek-float.exe` = **7,031,808 B**，`sha256 = 16FA5C0344E125F994C38B9272FC8CA53EAEC8CEB11368A6CB0397FA607504B0`；`deepseek-float/release/HASH.txt` 为同值（大写 hex、CRLF、**66 字节**，须 == `sha256(exe)`）。构建走 `pnpm release`（= `node scripts/build-release.mjs`）：前端 `pnpm build` → `cargo build --release --features custom-protocol` → `dist`/`poc` 新鲜度门禁 → `pnpm check` 四门禁 → 签名（可跳过，跳过会打印提示）→ 交付到 `release/deepseek-float.exe` 并写入 `HASH.txt`；本轮耗时约 **5m30s**（前端 2.1s + cargo release 4m58s）。**内容取证**：`dsfloat_init.js` 经 `include_str!` **逐字嵌入** exe，故 `ball_ready` / `scanStopControl` / `splitV: 5` / `roles: roles` 等标记可在 exe 内 `grep -a` 直接命中；**反例**：Rust **函数名**会被内联/丢弃，**不能**当交付标记。附带：`~/.cargo/config.toml` 的 `[build] incremental=true` 会泄漏到 release 构建，故 exe **非逐字节可复现**（强制点在 `build-release.mjs` 里的 `env CARGO_INCREMENTAL=0`），审计判据 = 「内容取证（`grep -a` 标记）+ 交付自洽（`HASH.txt` == `sha256(exe)`）」。
  - ⚠️ **已被取代**：本版交付物已由 **2026-09-28** 的刷新取代（**7,041,024 B** / `E7C15FD3…`，含 schemars×indexmap 根治 + P0 诊断日志），见本文档顶部「2026-09-28」批次。

### Fixed

- **修复：悬浮球「镂空洞」透出背后窗口（闪现背后应用浅色页）（2026-09-26，方案 A-2，提交 `544dfa2`）**：
  - **现象**：单击 / 长按悬浮球时，球背后**闪现背后应用（真机为 DeepSeek 网页端浅色页）**，长按期间持续可见。
  - **根因**：球窗原「透明」由 `DwmEnableBlurBehindWindow`（空区域 `CreateRectRgn(0,0,-1,-1)`）实现 ⇒ **整窗即一个「镂空洞」**，除 WebView2 画的白圆外整块窗口透出桌面/背后窗口；按下与长按时 WebView2 白圆短暂不呈现，整窗遂变镂空洞 ⇒ 透出背后应用。（真机取证已排除拖动、`display:none` reflow、面板遮挡、OS 标题栏等假说：按住 21 拍球 RECT 恒定不动、球 JS 在按住不动期间不调任何 Rust ⇒ 纯 OS/WebView2 按下态。）
  - **修法（方案 A-2，「去掉球窗透明镂空」）**：① `tauri.conf.json` 球窗 `transparent: true → false`（**不透明窗，DWM 永不透出桌面/背后窗口**）；② `ballstyle.rs` 删 `ensure_transparent`，新增 **`apply_ball_shape(app, visible)`** —— `SetWindowRgn` + `CreateEllipticRgn` 把窗口裁成椭圆（区域同时是可见区与命中测试区），隐藏时设空区域 `CreateRectRgn(0,0,0,0)`；③ `windows.rs` 的 `ball_show` 在 `ShowWindow(SW_SHOWNOACTIVATE)` **之前**套用区域、`ball_hide` 以区域清空作隐藏主力（`set_ignore_cursor_events` 与页面 `visibility:hidden` 保留作纵深防御）；④ `poc/ball.html` 的 `html,body` 背景 `transparent → #ffffff` 兜底。
  - **原理**：DWM 对**不透明窗**不做逐像素 alpha 合成 ⇒ 窗口内不存在镂空洞，WebView2 表面短暂不呈现时最多露白底，**不可能透出背后窗口**。
  - **已知代价**：`SetWindowRgn` 为区域裁剪，圆边**硬边、无抗锯齿**（已接受）。实证：`SWP_FRAMECHANGED` 不会清掉 `SetWindowRgn` 设的区域（改 EXSTYLE/STYLE + FRAMECHANGED 后区域仍为 COMPLEX）。

- **BUG 修复：结果卡片回流代码块丢失格式（压成一行）（2026-09-23，代码已合，尚未重打包 exe）**：
  - **现象**：网页端代码块正常换行显示，结果卡片里同一段代码被压成**一整行**（语言标签与代码正文混排）。
  - **根因（源码级）**：DeepSeek 代码块的换行是**结构性的**（每行一个块级样式元素，靠远端 CSS 撑出行 breaks，HTML 里没有真实 `\n` 文本节点）。页内 `serializeHtml` 按原样克隆 DOM 结构（白名单标签、不保留任何属性）；卡片侧 `cloneSafe` 保留 `SPAN` 但剥掉全部 class/样式 → 这些「行元素」在卡片里退化为行内元素，行与行直接拼接；卡片 `.md pre` 虽是 `white-space: pre-wrap`，但没有 `\n` 可保留，代码即被压平。`toMarkdown` 兜底路径的 `PRE` 分支用 `textContent` 提取，同样丢结构性换行。
  - **修复**：`dsfloat_init.js` 两处——① `serializeHtml` 对 `PRE` 特殊处理：取 `code` 元素的 `innerText`（保留**渲染后**的真实换行，语言标签/复制下载按钮在 `code` 外天然排除），规范化输出 `<pre><code>带真实换行的文本</code></pre>`，与远端 DOM 结构解耦；innerText 无换行（未渲染等异常）时退回 `textContent`。② `toMarkdown` 的 `PRE` 分支同样改用 `innerText` 提取。
  - **验证**：`node --check` 通过（JS 为注入脚本，无独立单测；行为由真机复现核对）。

- **方案A 增强：结果卡片代码块语法高亮还原（2026-09-23，代码已合，尚未重打包 exe）**：
  - **目标**：在「换行铁律不变」的前提下，把代码块里**藏在 CSS/类里的语法高亮结构**用**极窄 class 白名单**还原，让卡片用自有 CSS 给 token 上色（承接上一轮的代码块换行修复，进一步补回可读性）。
  - **改动（仅两文件）**：
    - `dsfloat_init.js`：新增 `HLJS_CLASS_RE`/`LANG_CLASS_RE` 常量、`codeLangClass(v)`（仅从 `<code>` 取 `language-<name>`）、`serializeCodeText(node)`（递归序列化代码子树：文本节点转义保留含 `\n`；`<br>`→`\n`；`<span>` 逐 token 校验、**仅保留 `hljs*` token**、其余 unwrap；其他标签 unwrap）。`serializeHtml` 的 `PRE` 特判改为**两段式**：优先 `serializeCodeText` 取高亮结构；若结果**不含 `\n`** 则**回退** `innerText`→`textContent` 纯文本并**清空 code class**（高亮尽力而为、换行强制保证）。`toMarkdown` 的 `PRE` 分支未改（Markdown 无法承载逐 token 高亮，保持纯围栏块）。
    - `poc/result.html`：新增 `safeClass(v)`（仅放行 `hljs*`/`language-*` token，逐 token 过滤）；`cloneSafe` 仅对 `CODE`/`SPAN`/`PRE` 经 `safeClass` 门禁后拷贝 `class`，其它标签一律不拷贝（保持默认剥 class）；`.md pre code` 下新增克制 token 配色（关键词蓝、字符串绿、注释灰斜体、数字橙、标题紫、变量青，共 6 组 ~24 条选择器）。
  - **安全复核（QA 独立，全部 PASS）**：白名单正则 56 条断言全过——放行 `hljs`/`hljs-keyword`/`hljs-built_in`/`hljs-literal`/`hljs-title`/`language-python`/`language-c++`/`hljs-title function_`（过滤为仅 `hljs-title`）；拒绝/剥离 `foo`/大写/`onclick`/`onmouseover=alert(1)`/`javascript:alert(1)`/`hljs"onload="`/`evil-style`/`hljs-keyword onclick=alert(1)`（仅留 `hljs-keyword`）。断言生成 HTML **绝不包含** `javascript:`/`style=`/`<script`/`on*`；换行铁律回退路径（异常结构 mock）确认最终 `<pre><code>` 含真实 `\n` 且 code 无 class。`cloneSafe` 的 `setAttribute` 调用点逐一核对，无 `style`/`data-*`/`id`/事件属性新增放行。
  - **已知限制**：① JS 无自动化单测基建，本次靠独立 node 脚本（复刻逻辑）+ 静态 grep + 语法校验；② 构建产物 `dist/poc/result.html` 是 stale 副本、**未含**新配色 CSS，运行期若从 `dist/` 取卡片页高亮不生效，发布前需重建；③ 真机配色与 `innerText` 回退效果只能在真实 WebView 复现确认。

- **BUG 修复：结果卡片「思考过程」不显示（2026-09-21，代码已合，尚未重打包 exe）**：
  - **现象**：DeepSeek 启用深度思考（R1）时，划词 / 追问的结果卡片只显示最终回答，**思考过程（「已思考」折叠区）整段缺失**。
  - **根因（源码级回归）**：2026-09-20 的判终修复把页内 `splitBlock` 改为——思考中（`pending`，块只有思考、尚无答案元素）时返回 `ans=''`、`think=整块文本`。但 `readback/watch.rs` 的流式推送仍沿用旧语义（注释明写「pending 时 `ans` 其实是思考正文」），造成**四重叠加吞掉思考过程**：① `last_think` 在 pending 时取 `ans`（恒空）；② `changed` 用 `ans != pushed_think`（pending 时 `ans` 恒空 ⇒ 永不相等 ⇒ **思考阶段永不推送**）；③ 推送守卫 `!ans.is_empty()`（pending 时 `ans` 恒空 ⇒ 直接拦截）；④ 即使推送 `thinking = ans`（空）。前端 `poc/result.html` 的 `buildTurn` 逻辑本身正确（`thinking` 非空即渲染 `.think` 折叠区、默认展开），故问题完全在 Rust 侧传空值。
  - **修复**：`watch.rs` 在 `pending` 阶段统一以 `think` 作为思考正文——`changed` 比对改用 `think`、`last_think` 统一取 `think`、推送守卫放行「思考非空」、pending 分支 `thinking` 取 `think` 且 `pushed_think` 同步记 `think`；同步更新了过时注释。非 pending 阶段（已有回答）行为不变。
  - **遗留风险**：思考**完成**后卡片是否保留思考过程，取决于页内 `splitBlock` 的 `bestEl` 扫描能否从助手块里提取 `think`（同块内联假设）。若真机复现「思考中能看到、完成后消失」，需开诊断（放 `diag.flag` 或设 `DSFLOAT_DIAG=full`）复现一次，把 `d1-debug.log` 的「交付前尾窗」发我，据日志进一步加固 `splitBlock` 的 `think` 提取。
  - **验证**：`cargo check` / `cargo clippy --all-targets` 0 警告；`cargo test --lib` **140 passed / 0 failed**。

- **BUG 修复：结果卡片「继续读取」按钮点击后静默无反应（2026-09-23，代码已合，尚未重打包 exe）**：
  - **现象**：超时降级后出现「继续读取」按钮，点击后**毫无反应**（按钮卡在「续读中…」或像没被点）。
  - **排查结论（先排除 ACL 误判）**：能力文件 `capabilities/default.json` 仅放行 `core:default` + `core:window:allow-start-dragging`，**本不含**任何应用自定义命令权限；但经 `cargo check` 验证——本工程 Tauri 构建期**未为 `#[tauri::command]` 生成任何权限描述**（`__app__-permission-files` 为空），即应用命令**默认放行**，故 ACL 不是根因（曾误加 `com.deepseekfloat.desktop:default` 因含点号非法、已还原）。
  - **真正根因**：续读链路是**静默**的——`resume()` 在 `RESUME` 为空（已被新会话覆盖）、或 `begin_watch` 因面板窗口缺失而 early-return 时，**没有任何用户可见反馈**；前端旧 handler 用 `invoke('result_resume')` 一抛了之，`.catch` 只 `console.error` 且不复位按钮，于是「续读中…」卡死、看起来像「点了没反应」。
  - **修复（三处联动）**：① `readback::resume` 改为**返回状态串** `ok` / `no-resume` / `panel-missing`；② `ipc::result_resume` 同步返回 `String`；③ `poc/result.html` 的按钮 handler 改用 `tauri.core.invoke(...).then(status)`——`ok` 等待卡片重渲染、`no-resume`/`panel-missing` 给出明确文案（「没有可续读的内容」/「面板未打开，无法续读」），`.catch` 失败则复位按钮。**彻底消除静默无反应**。
  - **验证**：`cargo check` 通过；`resume()` 与 `result_resume` 返回串改动一致、编译无误。真机复现确认仍需重打包 + 真机验证。

- **#13 E1：回读启动门竞态（2026-09-26）**：`readback/watch.rs` 的启动门新增**第四路「尾形」证据** —— 判据 `located && cur_echo > 0 && !last_echo`（容器已定位、尾窗**含**我方提示词、但回显**不是末块** ⇒ 本轮助手块已开始渲染），专门覆盖**真机现场形态 `[上一轮答案, 回显, 助手块]`**：此时旧三路 `new_echo`/`last_echo`/`streaming` 可能**全 false** ⇒ 本轮永不开始 ⇒ 答案就地超时丢弃（这是「回读启动门竞态」的直接根因）。第四路判据较宽，故新增常量 `STARTUP_TAIL_CONFIRM`（`watch.rs:41`）要求该形态**连续命中** 2 次采样（≈一拍）才判「已开始」，以排除「回显尚未落盘 / 命中的是上一轮同文本提问」的瞬时误判（纯函数 `startup_gate` 二次收敛见 `watch.rs:182-197`，另见 `:454`、`:695`）；单测见 `readback.rs:782` 起。
- **#13 E2：页内 `streaming` 信号失灵兜底（2026-09-26）**：把「**容器增长**」纳入「有进展」—— `readback/watch.rs` 新增纯函数 `container_grew`（`watch.rs:225`、`:456`、`:610`），以 `snapshot.total`（容器内文本总长）/ `snapshot.blocks`（有文本的块数）的**增长**证明「仍在推进」，供 `watch_loop` 自适应截止放宽（避免长回答被 `opts.timeout_ms`（60s）误掐）；刻意只认「增长」（文本变短 = 页面重排/容器切换，不算进展）。页内扫描函数 `scanStopControl` 在 `src-tauri/src/dsfloat_init.js:902`：作用域**收敛到会话容器内**（`dsfloat_init.js:922-923`，容器内优先、全文档兜底），并**去掉 200 上限**（原 `i < 200` 正是探不到「停止生成」控件的漏判根因）；E2 修复注释见 `dsfloat_init.js:894`。单测见 `readback.rs:857` 起。
- **修复：dev 启动「白色悬浮球」（2026-09-26，提交 `8a4eb44`）—— 本轮最重要的修复**：
  - **症状**：`pnpm tauri dev` 启动后，球窗在 +3s 兜底显形时页面尚未绘制 ⇒ 用户看到一个**白色圆球**，约 **15~47s** 后才变成正常的鲸鱼 logo 球。**release 版不受影响**（走自定义协议，不经 Vite）。
  - **根因**：项目根内的 Rust 构建产物目录 `src-tauri/target`（实测 **26 GB / 29192 个文件**）落在 Vite root 内；冷启动时 Vite 对项目根做**一次性递归扫描**，把首个页面请求的 I/O 排在数万目录项之后。
  - **证据**：探针直接挂在 Vite 中间件上计时 —— 不加 ignore：首个页面请求 **46894ms**（复测 46628 / 29247 / 20471ms，波动大）；加上 ignore：**12ms**。五个页面（ball/popup/settings/shot/result）的响应**总是在同一瞬间一起返回** ⇒ 典型的一次性全局阻塞。**反证**：`server.watch: null`（关掉文件监听）**不能**消除该阻塞（实测仍 46628ms）⇒ 真正有效的是 `watch.ignored`（Vite 用它决定递归扫描跳过哪些路径）。
  - **修法**：`deepseek-float/vite.config.ts:38-40` 新增 `watch: { ignored: ['**/src-tauri/**', '**/node_modules/**', '**/.git/**'] }`（同文件 `:13-15` 保留 `host: '127.0.0.1'` / `port: 5173`）。
  - **独立复验**（另一人执行、不复用上述数字）：冷启动首请求 **0.0514s / 0.0716s**；端到端 `ball_ready` = **+480ms / +672ms**；修复前每轮必现的「等待 `ball_ready` 超时」告警，两轮**均未出现**；另用临时 config 复现慢基线 **39.717s**，**证伪**了「OS 文件缓存热」这一替代解释。
  - ⚠️ **归因修正**：`vite.config.ts:7-11` 的旧注释把「球页加载失败 → 球空白」归因于「Vite 只监听 IPv6 回环」；本轮证明 dev 白球的真因是 **26GB 目录扫描**，二者是**两件事** —— IPv4 绑定 `127.0.0.1` 仍是有效的防御性设置，但**不是**白球的根因。

### Added (previous)

- **债务削减批次（P0/P1 落地 + 工程化门禁，2026-09-15）**：按 `docs/audit-2026-09-15.md` 的优先顺序（高分+低成本优先、暂缓 remote/CI）执行首批评修复，全部 `cargo fmt/clippy/test` 与门禁验证通过。
  - **安全 / 数据**：`poc/result.html` Markdown 兜底链接加 URL scheme 白名单（`https?/mailto/#//`，其余退化为纯文本）堵 XSS；`config.rs` 配置写盘改 **temp+rename 原子替换**（避免异常退出把 `config.json` 截成半截）。
  - **性能 / 稳定性**：`fullscreen.rs` 全屏隐藏开关改 `AtomicBool` 内存缓存（轮询不再每 250ms 读盘）；进全屏时**一并隐藏结果卡**、离开全屏按用户关闭状态恢复（避免悬浮在游戏/视频之上）。
  - **可观测性 / 隐私**：`lib.rs` 启动信标加版本指纹（件+dev/release）+ RFC3339 时间戳（`diag::now_rfc3339` 零依赖实现）；导航日志 URL 用 `redact_url` 脱敏（丢弃 query，防一次性 token 明文落盘）。
  - **内存上限**：`result.rs` 合并会话时即对 `turn.text` 截断（非仅序列化时），流式 Update 不再无限累积，会话内存常驻有界。
  - **测试 / 工程化**：`inject.rs` 补 `js_string/parse_js_string/parse_js_bool` 单测；`package.json` 增聚合 `check`（csp+inline-js+version）与 `test:rs`；`e2e-diag.mjs` 硬编码绝对路径改 `__dirname` 相对定位；`package.json` 补 `license` 字段（与 `LICENSE`/`Cargo.toml`/`tauri.conf copyright` 对齐）。
  - **验证**：`cargo fmt --check` 0；`cargo clippy --all-targets` 0 警告；`cargo test` **119 通过**（含新增）；`check:inline-js` 5 段全过；`node --check` e2e 脚本通过。<br>另在验证中发现并修复 `diag::now_rfc3339` 的类型错误（u64/i64）与重复 `mod tests`。
- **R20 真机事故修复：交付的绿色版 exe 一直是「dev 模式」空壳**（2026-09-15）—— 用户报「悬浮球和工具条都显示出错」。
  - **现象**：绿色版 exe 双击后悬浮球是一片空白/显示错乱。**实测复现**：球窗截图整片空白；CDP 查到球窗的文档停在 `about:blank`、没有任何 `.ball` 元素与样式表。
  - **根因（源码级）**：Tauri 用 `custom-protocol` 特性区分生产/dev ——
    `tauri-macros-2.6.3/src/context.rs:155` `dev: cfg!(not(feature = "custom-protocol"))`，
    该 cfg 在 **tauri-macros 这个 proc-macro crate 编译时**求值。而本仓库 `Cargo.toml` **从未声明**该特性，
    构建又一直用**裸 `cargo build --release`**（它不会自动开启）⇒ 产出的 exe 认为自己在 dev：
    **窗口 URL 变成 `build.devUrl`（`http://localhost:5173/poc/*.html`）、前端资源完全不内嵌**。
    R16 首次交付时用的是 `pnpm tauri build`（CLI 会自动加该特性，6.0 MB），R17 之后改成裸 `cargo build --release`
    就引入了这个回归 —— **此后每一版绿色版都不能独立运行**。
  - **运行时机证**：给 WebView2 开 CDP 远程调试端口后读 target 列表 —— 修复前是
    `http://localhost:5173/poc/ball.html`（devUrl），修复后是 **`http://tauri.localhost/poc/*.html`**（6 个页面全部，含内嵌资源）。
  - **修法**：`src-tauri/Cargo.toml` 新增 `[features] custom-protocol = ["tauri/custom-protocol"]`；
    构建改用 `cargo build --release --features custom-protocol`（`package.json` 增 `build:release` 脚本固定这条命令）。
  - ★★ **为什么之前的自动化全都放过了它**：`e2e-diag.mjs` 的 11 条断言**只查日志**，
    而日志（窗口创建、诊断档位、boot）**完全不依赖前端** ⇒ 全绿出厂。**教训：「产物能不能自己活」必须由渲染出的 DOM 事实判定，不能靠日志。**
  - **新增 `deepseek-float/scripts/e2e-shell.mjs`（`pnpm e2e:shell`，12 条）**：脱离 dev server 拉起 release exe，
    经 CDP 直接查页面真相 —— ① `A2` 产物模式（URL 不得指向 devUrl，**这条就是事故的判定式**）；
    ② 悬浮球 DOM/样式表/外边距/溢出/logo 尺寸；③ `A8` 重载后抓 **CSP 违规**（`Log.entryAdded`，
    这是 release 下唯一能看到 violation 的办法）；④ `B1–B4` **逐页**验证 5 个本地页的样式生效性
    （popup 的 `.bar` 用上 flex / settings 的 `#editor-card` 被 `style="…"` 收起 / shot 的 `#mask` 半透明黑 / result 的卡片圆角）。
    ★ 启动前必须 `taskkill` 残留进程 —— 本应用有单实例互斥，残留会让新进程直接退出、调试端口永不出现（踩过）。
  - **CSP 的疑问一并结清**：`e2e-shell` 实测 `style-src 'self'` + nonce **正常工作**（球页样式表 5 条规则、
    `settings` 的 10 处 `style="…"` 属性也生效：`#editor-card` 的 `display:none` 生效），且**无任何 CSP 违规**
    ⇒ R19 的 CSP 收窄**没有**打死页面，本次「显示错乱」与 CSP 无关。
  - **新增诊断工具（`.workbuddy/`）**：`cdp-probe.mjs`（运行时 DOM/CSP 取证）、`cdp-shot.mjs`（逐页截图 + 样式断言）、
    `shot.py`（ctypes `PrintWindow` 截窗口，★ 注意**透明窗口抓不到**，必须用 CDP 的 `Page.captureScreenshot`）。
  - **绿色版已重建交付**：`deepseek-float/release/deepseek-float.exe` = **6,934,528 B（≈6.9 MB）**，
    `sha256` 前 16 位 `13ccc2a8daa28d4b`；**两处 release 目录同步覆盖**（R19 曾因两处同名副本不一致，
    用户双击到旧版而白测一轮）。`e2e-shell` **12/12** 通过。
  - 文档同步：`docs/release-checklist.md` 新增 **§2.5 产物模式**（含事故全过程与判定依据）、
    `README.md` 构建章节加「别用裸 `cargo build --release`」警示、`package.json` 增 `build:release` / `e2e:shell`。
- **R19 CSP 收窄 + 「CSP ↔ 页面需求」一致性门禁**（2026-09-15）：把 `app.security.csp` 从「一条通吃」改为按资源类型逐项声明，并新增一道 CI 门禁把「CSP 打死页面」提前拦下。
  - **改前**：`default-src 'self' 'unsafe-inline' 'unsafe-eval' data: blob: asset: tauri: ipc: http://localhost:* https://localhost:* https://* wss:;` —— 单条 `default-src` 兜底**所有**资源类型，且放行 `https://*` / `wss:` / `'unsafe-eval'`。
  - **改后**：`default-src 'self'; script-src 'self'; style-src 'self'; style-src-attr 'unsafe-inline'; img-src 'self' data: https:; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'self'; frame-src 'none'; form-action 'none'`
  - ★ **两条反直觉机制（本轮源码核实，推翻了 R17 我自己写错的一条）**：
    1. 内联 `<script>` 由 `tauri-codegen/src/context.rs:48`（选 `script:not(:empty)`）**自动补 sha256**；hash 一旦存在，同指令里的 `'unsafe-inline'` 会被浏览器**忽略** ⇒ 「顺手加上 `'unsafe-inline'` 保平安」是**无效**的，内联脚本实际靠 hash 活着。
    2. 内联 `<style>` **不补 hash**，而是由 `tauri-utils/src/html2.rs:75` 打 nonce，运行时（`tauri/src/manager/mod.rs:96`）换成随机值并**显式创建** `style-src`（= `'self' 'nonce-x'`）⇒ **`style="…"` 属性一直是失效的**（nonce 只覆盖元素、不覆盖属性）。R17 笔记称「样式也补 sha256」**有误，已订正**。
  - ★ **顺带修掉一个存量缺陷**：`poc/settings.html` 有 **10 处 `style="…"` 属性**（徽章配色、`#editor-card` 的 `display:none`），按上述机制此前应当一直不生效 ⇒ 新增 `style-src-attr 'unsafe-inline'` 专门放行（**只管属性**，比放宽 `style-src` 窄；且若该指令不被支持，回退到 `style-src` 即等于「维持现状」，**无回归风险**）。
  - ★ **CSP 只在构建产物里生效**：dev 下页面由 vite dev server 提供、不经过 Tauri 资产协议（只有 `tauri/src/manager/mod.rs:435` 的 `get_asset` 会给 `.html` 下发 CSP 头）⇒ **`pnpm dev` 里根本没有 CSP**，「改坏了」在 dev 里**永远测不出来**。这是本项最大的风险来源，也是新门禁存在的唯一理由。
  - **新增 `deepseek-float/scripts/check-csp.mjs`（`pnpm check:csp`）**：读 `tauri.conf.json` 的 CSP + `dist/poc/*.html`，把「页面**实际用到**的资源」与「指令」逐条对齐 —— 内联脚本数、`style="…"` 属性数、外链脚本/样式、内联事件处理器、`<form>`、`fetch`/XHR/WebSocket、`eval` 家族、CSS 外链、`img-src` 是否需要 `https:`/`data:`、`connect-src` 是否含 `ipc:`；另加**防回退基线**（禁 `'unsafe-eval'`、`https://*`、`wss:`、`http://localhost:*`，要求 `default-src` 恰为 `'self'`，要求四条硬化指令存在）。
    - 扫描前**剥离注释**并**识别正则字面量**：`popup.html:130` 的 `/[&<>"']/g` 含引号，若只跟踪引号，状态机会在正则处错位，导致 `popup.html:150` 那句讲 `win.eval(…)` 的**注释**被误判成真实调用（首轮实跑就撞上这个假阳性）⇒ 剥离器自带 **6 条自测**（正则/字符串/模板/块注释/除号/`return` 后正则），它错了后面结论全错。
    - 支持 `--csp="…"` 覆盖入口，便于**负向测试**与「改配置前先试算」。实测负向：去掉 `style-src-attr` ⇒ 精确报出「10 处 `style="…"` 会被拦」；退回旧宽松策略 ⇒ 一次性报出 11 条（4 个通配/危险源 + `default-src` 非 `'self'` + 缺 4 条硬化指令 + `img-src` 缺 `https:`）。
    - 另含两条**防失真**检查：`dist/` 比 `poc/` 旧 → 直接判失败（否则校验的是过期页面）；`dist/poc/` 不存在 → 提示先 `pnpm build`。
  - `.github/workflows/ci.yml` 增一步 `pnpm check:csp`（★ 必须在 `pnpm build` **之后**，它读 `dist/`）；`package.json` 增 `check:csp`；`docs/upgrade-notes.md` 增「CSP 同样依赖 Tauri 内部行为」小节（含三条改写时机与源码位置，供升级 Tauri 时回归）；`docs/release-checklist.md` 新增 **§6**「CSP 收窄 —— 只在打包版生效，必须真机手测」。
  - **二进制取证（不是只改配置文件）**：在 `target/release/deepseek-float.exe` 里 grep —— 新指令 **7/7 全部存在**（`style-src-attr` / `img-src 'self' data: https:` / `object-src 'none'` / `base-uri 'self'` / `frame-src 'none'` / `form-action 'none'` / `connect-src ipc: http://ipc.localhost`），旧串 **3/3 全部消失**（`https://* wss:` / `unsafe-eval` / `blob: asset: tauri:`）。
  - **绿色版已重建交付**：`deepseek-float/release/deepseek-float.exe` = **6,902,272 B（≈6.9 MB）**，含 R7–R19。新构建跑诊断端到端 **11/11 通过**（启动路径与诊断档位无回归）。
  - 本轮**未改任何 Rust 代码**：改动仅 `tauri.conf.json` + 3 个脚本 + 4 份文档。
- **R17 多维度审核（只审不改）+ 首批修复（P1 release 诊断通道 · P2 工程化门禁）**（2026-09-15）：用户要求对架构 / 代码简化 / 性能 / 稳定性 / 安全 / 可观测性 / 测试 / 工程化 / 代码风格做一轮可落地审核 —— 报告落 `deepseek-float/docs/audit-2026-09-15.md`（覆盖 83 文件 / 27,350 行，Rust 22 文件 / 8,415 行全纳）。
  - **审核结论**：**无 P0**（810 次窗口生命周期压测 0 失败、108 单测全绿、release 零警告、空闲 CPU ≈ 0% 且 150s 内存完全持平）。Top10 首位为 **P1：release 构建零日志**。
  - **为什么 P1 是「release 零日志」**：R1–R15 的**全部**真机缺陷都靠 `d1-debug.log` 定位，而该文件在 release 下被 `diag.rs` 的 `if !cfg!(debug_assertions) { return; }` 直接掐断 ⇒ 分发给用户后，用户报「结果卡空白 / 没回流」时我们**一无所有**（无 console、无日志、无崩溃上报）。
- **R17-P1 release 可选诊断通道**（2026-09-15）：`diag.rs` 引入**三档级别**并由环境变量 `DSFLOAT_DIAG` 控制（进程启动解析一次）：
  | `DSFLOAT_DIAG` | debug | release |
  |---|---|---|
  | 未设置 / `0` / `off` | **Full**（与历史一致） | **Off**（零 I/O） |
  | `1` / `true` / `meta` / 任意非空串 | Full | **Meta**：正文替换为 `«正文已隐去 len=NN h=HEX»` |
  | `full` / `2` / `all` | Full | **Full**：含正文 |
  - ★ **debug 恒为 Full**：开发机不该因某个残留环境变量而丢正文。★ **拼错的值保守落 Meta**：宁可少记正文，也不能因拼写错误而**意外关闭**诊断。
  - **正文脱敏的落点**（新增 `diag::content()`）：`readback` 的「起始/交付前/超时前尾窗」「容器骨架」与**回答预览**、`inject` 的 `inspect-upload` / `inspect-conversation` 结果及 36 字首块预览。`content()` 在 `Meta` 档返回 **FNV-1a 32 位哈希 + 长度** —— 原文不出文件，但「两行日志里的正文是不是同一段」仍可比对（这正是那些诊断行的用途）。
  - `readback::deliver` 顺带去掉一次**无条件**的 200 字 `collect`（关闭档与 Meta 档都不再构造）。回答预览改为「`Full` → 前 200 字原文（带「」界定）；`Meta` → 全书长度 + 哈希；`Off` → 空串」。
  - `lib.rs` 的 `tauri-plugin-log` 初始化由 `if cfg!(debug_assertions)` 改为 `if diag::enabled()`，且 **release 降为 `Warn` 级**（只收 Tauri/wry/WebView2 侧异常，不被依赖库 INFO 刷屏）；与 `diag` 分工：`diag` 记我方业务里程碑，plugin-log 记依赖与运行时异常。
  - **体积兜底**：`d1-debug.log` 在**启动时**检查，超过 8 MiB 先改名为 `d1-debug.log.1`（只留一代）—— 只在启动查一次，不给划词热路径平添系统调用。
  - ★ **文件式开关 `diag.flag`**：绿色版是**双击启动**的 GUI 程序，终端用户**设不了环境变量** ⇒ 只有环境变量等于「诊断通道对用户不可达」。故增加第二入口：`%LOCALAPPDATA%\com.deepseekfloat.desktop\diag.flag` 这个文件**存在**即等价于 `DSFLOAT_DIAG=1`（Meta 档）。支持流程 = 「放个空文件 → 重启应用」。优先级：**debug > 环境变量（非空即覆盖，`off` 可强行关）> 标记文件 > 关**，即「文件开关不会挡掉自动化脚本」。新增 `resolve_level()` 纯函数与 8 条断言（含「显式 off 压过标记文件」）。
  - **端到端验证（真机 release exe，非代码层推测）**：新增 `deepseek-float/scripts/e2e-diag.mjs`，拉起 release exe 逐档断言 **11/11 通过** —— 不设变量 **delta=0 B**（零落盘）· `off` **delta=0** · `1` 落盘且启动行报 `档位=Meta` · `full` 报 `档位=Full` · 拼错值 `ture` **仍落盘且落 Meta**（宁开不错关）· `diag.flag` 存在则落盘为 Meta · 移除后**回到 delta=0**。
  - **release 档位下的单测也跑**：`cargo test --release --lib diag` **7 passed**（`debug_assertions=false`，与线上同一份实现）。门禁实测：`cargo test --lib` **115 passed / 0 failed**。
  - **绿色版已重建交付**：`deepseek-float/release/deepseek-float.exe` = **6,902,272 B（≈6.9 MB）**，含 R7–R18 全部修复；探针/压测钩子仍按 `#[cfg(debug_assertions)]` 排除（体积增量来自 release 现在也会编入诊断落盘代码）。
  - 单测 **108 → 114**：`parse_level` 的 debug/release 矩阵、拼错值落 Meta、`content` 三档（Off 空串 / Meta 隐去正文但长度与哈希稳定 / Full 透传且超长截断）、FNV-1a 标准向量（后随 `diag.flag` 追加 `resolve_level` 共 **115**）。
  - **验证法**（已写入 `docs/release-checklist.md` §6）：release 版带 `DSFLOAT_DIAG=1` 跑一次划词 ⇒ 日志出现「开始回读 / 回读完成」等关键行，且**不含**正文。
- **R17-P2 前端 IPC 桥自检**（2026-09-15）：`poc/*.html` 是原生 HTML，通过 Tauri **内部**对象 `window.__TAURI_INTERNALS__.invoke` 调命令（`withGlobalTauri:false`，不宜开启 —— 全局注入会同时进入远端 panel 页，扩大攻击面）。该内部契约一旦在升级中改名，5 个页面会「**加载成功、但按钮静默失效**」，真机上只表现为「点了没反应」。
  - 5 页 shim 统一加探测：建桥**之前**判断 `__TAURI_INTERNALS__.invoke` 是否为函数，缺失时往控制台打一条明确中文错误，并把 `invoke` 换成**已 reject 的 Promise**（错误码 `tauri-ipc-bridge-unavailable` 便于检索），取代原先「看不见的 `TypeError`」。
  - 配套新增 `deepseek-float/docs/upgrade-notes.md`：写清「为什么用内部 API / 三种方案的取舍 / 升级 Tauri 后逐页回归清单 / 将来改 `@tauri-apps/api` 要动哪些地方」，并**明确不建议**在无构建验证条件时把 5 份 shim 抽成共享文件（`poc/` 由 vite 多入口产出到 `dist/poc/`，同目录新建的 `.js` 不会进产物会 404）。
- **R17-P2 工程化门禁：内联脚本语法检查 + release 检查入 CI**（2026-09-15）：
  - 新增 `deepseek-float/scripts/check-inline-js.mjs`（`pnpm check:inline-js`）：把 `poc/*.html` 里每个**无 `src`** 的 `<script>` 交给 Node `vm.Script` **只编译不执行**，语法错误即非 0 退出并报出源文件行号。为什么需要：内联 JS 没有 eslint / tsconfig / 打包期检查兜底，一个手滑的括号就能让某页「加载成功但按钮全失效」。当前 **5 段，失败 0 段**。
  - `.github/workflows/ci.yml` 增加三步：`pnpm check:inline-js`、`pnpm check:version`，以及 **`cargo check --release`**（★ 后者的价值已在 R16 被证明：`#[cfg(debug_assertions)]` 门控的探针在 release 下会变 `dead_code`，只跑 debug 的 CI 永远发现不了）。
  - 新增 `deepseek-float/scripts/check-version.mjs`（`pnpm check:version`）：版本号散在 **四处**（`package.json` / `tauri.conf.json` / `Cargo.toml` / `Cargo.lock`），发版时极易只改一处 ⇒ 安装包名与 exe 版本资源对不上、事后无法追溯。**只校验不同步**（默认只读，避免手滑改坏分发物命名）。已做**负向测试**：把一处改成 `9.9.9` → 非 0 退出并逐处列出。
- **`deepseek-float/docs/release-checklist.md`**（2026-09-15，**已删除**）：内容已并入 `README.md`「发布前 checklist」段（精简为 9 项），原文件不再维护。

### 刻意未做（等拍板，不计入本轮）

- ~~**CSP 收窄**~~ ✅ **已落地（R19，2026-09-15）** —— 本条原列在「等拍板」里（理由：必须真机验证），本轮已按下方机制查证结果改造完成，详见上方 R19 条目。
  > ★ 本条原先写的机制里**有一条是错的**（已订正）：原文称「Tauri 会自动把内联脚本**和样式**的 `sha256-…` 补进 `script-src`/`style-src`」。实际上**只有脚本补 hash**（`tauri-codegen/src/context.rs:48` 选 `script:not(:empty)`）；**样式走 nonce**（`tauri-utils/src/html2.rs:75`），而 nonce 不覆盖 `style="…"` 属性。两句话差之毫厘，结论完全相反 —— 后者才是 `poc/settings.html` 那 10 处属性一直失效的原因。
- **`LICENSE` / `publisher` / `copyright` / 版本号统一**：依赖 `docs/distribution-plan.md` 的 D1/D2 决策（主体信息与许可），需用户拍板。
- **拆大文件**（`readback.rs` 1698 / `dsfloat_init.js` 1456 / `result.html` 1311 行）：本项目注释密度极高，移动代码必须一并搬注释，且文档里有大量按「文件/函数」的交叉引用锚点 ⇒ 收益低、风险中等，暂不动。
- **两个常驻轮询改事件驱动**（250ms 全屏检测 / 400ms 位置 flush）：实测 CPU 影响 ≈ 0，**无实测收益**，暂不动。
- **`cargo-deny` / `pnpm audit` 入 CI**：需要在能跑通的环境里先把 `deny.toml` 的许可白名单试出来，避免加一个从没跑过的必失败步骤。

### Added（上一轮 R16 归档）

- **R16 分发就绪度审计 + 打包基础配置（B1）**（2026-09-15）：用户决定 **「工具需要分发」** ⇒ 签名从「可选」变为「必须」，新方案见 `docs/distribution-plan.md`。
  - **审计结论（三条硬事实）**：① **项目从未产出过安装包**（`target/release/bundle/` 与 `%LOCALAPPDATA%\tauri` 均不存在 ⇒ 没跑过打包步骤）；此前「分发」= 手工拷 `release/deepseek-float.exe`（**2026-09-14 的旧构建**）。② **零签名 / 零更新机制 / 元数据缺失**（`productName` 是英文 slug、`authors=["you"]`、`license=""`、无 publisher/copyright、无 LICENSE）。③ **仓库没有任何远程**（`git remote -v` 为空、无 tag）⇒ `.github/workflows/ci.yml` **至今从未运行**，是「死文件」。
  - **★ 三条硬约束（写进文档）**：`identifier` **永不可改**（它决定 WebView2 用户数据目录 ⇒ 登录态与配置；改 = 已装用户全丢）；`productName` 决定 exe 名/安装目录/开始菜单，**首次公开分发前必须一次定死**；**产品名含「DeepSeek」+ 图标为近似鲸鱼 ⇒ 公开分发有商标风险**，需改名/换图标或至少显著标注「非官方」。
  - **本轮已落地（B1，不依赖任何外部条件）**：`bundle.targets: ["nsis"]`、`windows.webviewInstallMode = embedBootstrapper(silent)`（+约 2 MB，缺 WebView2 自动装）、`nsis.installMode = currentUser`（★ **免 UAC**，装到 `%LOCALAPPDATA%`）、`nsis.languages = ["SimpChinese","English"]`（按系统语言自动选，不弹选择框）、`installerIcon`、`compression = lzma`、`shortDescription` / `longDescription`。
  - **刻意未改（等 D1–D3 决策）**：`productName`、`identifier`、`version`、`publisher`、`copyright`、`licenseFile`、签名项、`createUpdaterArtifacts`。
  - **实测：本机当前产不出安装器** —— `cargo build --release` 成功（5m09s，6.28 MB），但打包第一步需从 **GitHub release 资产**下载 NSIS 工具链，本机直连**三次全失败**（`Peer disconnected` / `fetch failed`；旁证：本机 cargo 走 rsproxy.cn 镜像）。已从 CLI 二进制**逐字提取**出两个文件的官方 URL + SHA1 与期望目录树，并给出三条出路（打包交给 CI / 本地按哈希预置 / `useLocalToolsDir`）。**用户决定：安装包暂不产出。**
  - 另有两条对「分发」尤其重要的风险已登记：**SmartScreen 拦截**（未签名必弹，是分发的头号杀手）与 **AV 误报**（全局鼠标钩子 + `SendInput` + 剪贴板 + UIA 的行为画像容易被启发式误判；缓解手段：签名优先、**只用鼠标钩子不挂键盘钩子**（现状已满足）、不做单文件自解压壳）。

### Fixed

- **release 构建的 7 处 `dead_code` + 1 处 `unused import`**（2026-09-15，R16）：`cargo build --release` 报 7 处 dead_code —— `inject::inspect_upload` / `inspect_conversation` / `ConvFp` / `LAST_CONV` / `VIRT_RATIO_HINT` / `log_conversation_delta`、`readback::begin_watch_diagnostic`。根因：这些诊断探针的唯一调用方 `lib.rs::setup_selftest` 带 `#[cfg(debug_assertions)]` ⇒ release 里必然不可达（CI 只跑 debug，所以一直没暴露）。
  - 修复：给这些项加**与调用方完全相同的 `#[cfg(debug_assertions)]`** —— **不用 `#[allow(dead_code)]` 掩盖**（本工程既定纪律）。随之 `inject.rs` 的 `use tauri::Manager` 在 release 变成未用导入，**一并 cfg 门控**。效果：release **零警告**，且不再把只服务排障的代码打进分发二进制（体积也更小）。
  - ★ 诊断**日志**（`diag`）不受影响 —— 它才是发给用户排障的主力手段，release 同样开启；仅**探针函数**不进 release。

- **R15-C1 登录态触达用户（未登录不再静默等 60s）**（2026-09-15）：`lib.rs::panel_state()` 早就算出了 `NEEDS_LOGIN`，但**只落诊断日志、全仓没有任何消费者** ⇒ 面板未登录时 `fill()` 找不到输入框、提问根本没发出去、回读每 250ms 采样失败直到 60s 超时降级，用户只看到「回答可能未完成（回读超时降级）」，**看不出真正原因是没登录**。
  - 修复：`readback.rs` 新增纯函数 `url_needs_login()`（与 `panel_state` 同口径：URL 含 `/sign_in`）+ `panel_needs_login(&panel)`；接入 `watch_loop` 两处 —— ① **采样失败分支**（快路径：首次失败即命中，节流与日志一致）⇒ 用户 **~1s 内**看到提示而不是 60s；② **超时分支**（兜底：登录页上也可能「容器能定位但永远等不到回答」）。
  - 提示卡口径：`complete: true`（★ 关键 —— 给 `false` 会额外标「回答可能未完成」，把**行动指引**说成**故障**）、`title=请先登录 DeepSeek`、`source=需要登录`；文案给出下一步与「只需一次」的预期。
  - ★ 放在**结果卡**而非浮层：不新增窗口，也不违反「用户反馈不做任何浮层提示」的既有硬约束。超时前的现场日志（`超时前尾窗` / `容器骨架`）仍在提示**之前**落盘，取证不受影响。
  - 单测 +1：`login_url_detection_matches_panel_state`（会话页 / 首页 / `about:blank` / 空串**均不得**误判 —— 误判会把一次正常提问直接掐掉并给出错误的行动建议）。
- **R15-C2 稳定性压测做成可重复命令**（2026-09-15）：`lib.rs::setup_stress`（`#[cfg(debug_assertions)]` 门控）读 `DSFLOAT_SELFTEST_STRESS=<每窗轮数>`，对 5 个常驻功能窗（panel/settings/popup/shot/result）各 show→hide N 轮并计数失败/缺窗，最后打印**判定行**；`DSFLOAT_SELFTEST_STRESS_EXIT=graceful` 改走 `app.exit(0)`（触发 `RunEvent::ExitRequested` + **真实 webview 析构**，正是历史上出 1411/1412 的路径），默认走生产路径 `std::process::exit(0)`（与托盘「退出」一致）。两条都跑才能区分「问题是析构引起的」还是「别处引起的」。
  - **实测结果（810 次 show/hide）**：3 轮打通 + **50 轮主压测（500 次）** + 20 轮 graceful 析构压测（200 次）+ 8 次冷启回归（80 次）⇒ **失败 0 · 缺窗 0 · 11/11 退出码 0 · `panic.log` 未新增 · 运行期无 1411/1412 · `exit -1` 零复现**。
  - **如实记录一处残留**：进程退出期仍会打印 WebView2 内部日志 `Failed to unregister class Chrome_WidgetWin_0. Error = 1412`（11 次中 4 次，**只在 teardown 阶段，不影响退出码**）⇒ 判据「无 1411/1412」字面未完全满足，但**实质目标（不崩溃）已满足**。
  - 结论见 `docs/m3-single-webview-plan.md` §8：**D-A/D-B 可拍板 —— 冻结 M3 全量形变、不投入方案 B**（触发条件已写明）。
  - ★ **压测必须独占运行**（单实例互斥体）：运行前先退出正在使用的实例。

### Fixed

- **R14-① 结果卡「数据未回流时高度过高」**（2026-09-15）：`result.rs::render` 的**首次显示**分支把「用工作区高度收窄」写成了**赋值**（`h = 工作区高 − 24px`）而非**收窄**（`h.min(...)`）⇒ 卡片首显高度与内容无关，恒为「近乎满屏」（1080p@1.25 → 逻辑 808，早已越过 `CARD_MAX_H = 620`）；再叠加「流式期间 `apply_size` 只长大不收缩」，**占位卡与整个流式过程**都顶着这个超高值，直到终态才一次收缩。
  - 修复：改为 `h = h.min(cap)`（与 `apply_size` 内同款口径）。
  - 并让**新会话允许收缩**：`allow_shrink = !view.streaming || view.turn == TurnMode::Fresh` —— `Fresh` 是硬复位（旧轮次已清空），若沿用「流式只长大」就会顶着上一问的高个子显示一张空卡。`Update`（同一轮流式增量）仍保持「只长大」以免抖动。
- **R14-② 截图识字回流「把用户自己的指令当回答」**（2026-09-15）：真机日志铁证 —— `[shot-ocr] **回读完成**（513 字符 / 45 次采样 / 11193ms）：「<div>…请把这张图片里的文字完整、准确地转录出来…」`。
  - 根因：**回答尚未开始时，尾窗末块就是「用户消息气泡」**（一串空 `<div>` 占位 + 我方提示词）。定位未命中「回显块的后一块」（它不存在）⇒ 走兜底路径取末块；而 `strip_own_prompt` 只剥**前导**提示词，**块首是标记**（`<div><div>…`）⇒ 剥不掉 ⇒ 整块被当回答交付。**卡片里显示的就是用户刚发出去的那句指令**（另一条日志则是把上一轮答案当本轮交付）。
  - 这是 R10（空壳 `<div></div>`）/ R12（瞬时状态「正在分析图片」）的**第三形态**：前两者内容是「空的占位」，本条**内容不为空、但那是提问不是回答**。
  - 修复：`readback.rs` 新增 `is_echo_only(text, prompt)` —— 去标签去空白后判「`[前置 UI 文案] + 我方提示词 + [装饰残留标点]`」，与 `is_decoration` / `is_markup_only` / `is_status_text` **并列接入两条返回路径**（只否决、不定位，最坏是继续等到超时并如实降级）。同时新增 `strip_decor`（剥离可能被包含进本块的页脚免责声明）与 `is_punct_only`。
  - ★ **两处被单测抓出的坑（都已修）**：① 判据第一版用「提示词之外总字符数 ≤ N」——会把**「回显 + 同块内真回答」**（`<div>请翻译：你好Hello</div>`，既有语义 `locate_answer_strips_combined_block`）一起拦掉；故改为要求「提示词**之后**不得再有正文」。② `strip_decor` 里顺手 `trim_matches` 标点会把**提示词自带的句尾「。」**削掉 ⇒ `c == p` 不再成立；标点残留必须交给**独立的** `is_punct_only` 容忍。单测 **101 → 107**。

### Added

- **截图全局热键 + 设置页录制控件（R13-S4）**（2026-09-15）：新增 `src-tauri/src/hotkey.rs` —— Win32 `RegisterHotKey` + **独立泵线程**（`hwnd=None` 时 `WM_HOTKEY` 投递到**调用线程**的消息队列，且 `UnregisterHotKey` **必须由注册它的线程**调用 ⇒ 注册/注销都收敛到同一泵线程）。默认 **`Ctrl+Alt+1`**（决策 D1），设置页可改/可关。
  - ★ **必带 `MOD_NOREPEAT`**：否则长按会连发 `WM_HOTKEY`、反复弹遮罩。
  - ★ **事务性改注册**：先注销旧键 → 试注册新键 → **失败则把旧键注册回去**（保证「改键失败反而把原本能用的键也弄丢」不会发生）。
  - ★ **失败降级不阻塞启动**：被其它程序占用（错误码 1409）时只落 `diag`，托盘 / 右键菜单入口照常可用。
  - 热键回调**只发信号**，截图流程在 worker 线程（同 `mousehook` 纪律）；退出时 `hotkey::shutdown()` 与 `mousehook::stop()` 并列。
  - 配置：`AppConfig.hotkey`（**缺字段 = 默认 `Ctrl+Alt+1`**，`None`/空串 = 关闭；为此**手写** `impl Default`，避免「全新安装」与「JSON 缺字段」得到两种结果）。命令：`set_hotkey`（**先注册成功再落盘**，返回实际生效 spec）、`hotkey_status`（实际生效值，与 `config_get().hotkey`＝期望值区分，注册失败时两者不一致并在设置页明示）。
  - 设置页新增「截图热键」录制框：点击 → 按组合键 → 立即生效；被占用时红框提示并回滚显示。单测 **97 → 101**（spec 解析：默认值 / 大小写·空格·顺序无关 / 非法输入 6 类 / 往返一致）。
- **「截图提问」S2 + S3：框选 → 抓取 → PNG → 挂到面板输入框**（2026-09-15）：S1 的可行性切片落地为完整链路（S1 真机 6/6 出口判据通过，其中 `fileInputCount=1` 使「页内 `DataTransfer` 注入」路线成立）。
  - **S2**（`screenshot.rs`）：`CreateDIBSection` + `BitBlt(SRCCOPY)` 抓选区（物理像素，`biHeight = -h` 保证 top-down）→ **BGRA→RGBA**（★ 第 4 字节显式写 255：BitBlt 下恒为 0，按 premultiplied 解释会**整幅全透明**）→ `png` crate 编码（**零新增下载**：0.17.16 已在 `Cargo.lock` 内）。体积闸门 `MAX_EDGE=4096` + `MAX_PNG_BYTES=4MiB`，超限走自研 2× 盒式降采样；另有 `all_black` 启发式（GDI 抓不到独占全屏 DirectX 时落告警）。
  - **S3**（`dsfloat_init.js`）：`shotPush`（**带 `seq` 顺序校验**，256 KiB/块）→ `shotCommit`（base64 → `File` → `DataTransfer` → `input[type=file].files` → 派发 `input` + `change`）。★ 分块是必须的（4 MiB PNG → 5.4 MiB base64，单条 `eval` 脚本过大），而乱序/重复会产生**损坏图片且不报错**。
  - **时序**：`commit` 只 `hide()` 遮罩 + 起 worker；worker `sleep(120ms)` 后再 BitBlt（遮罩从合成器移除**不是即时的**，否则会拍进图），且**无论成败都 `finish()`**（防划词挂起与 `CAPTURE_ACTIVE` 泄漏）；慢操作绝不放主线程（F9 同源）。
  - **决策 D2 保持**：只贴图**不发送**，故**不调用** `submit()` / `pressEnter()`。单测 **94 → 97**（base64 RFC4648 向量 / 降采样均值 / 降采样保底 / 全黑判定 / **PNG 编码回读校验**）。
- **「截图提问」R12 追加 + 「截图识字」（不做本地 OCR）**（2026-09-15）：
  - 截图提问贴图后**自动展开面板**（框选完的下一步就是打字提问）。★ **刻意不 `set_focus()`** —— 本项目记录过 `show()` 后 `set_focus()` 触发 WebView2 abort（宿主 exit -1）的坑。
  - **截图识字**：新增 `ShotMode { Ask, Ocr }`，两条入口共用同一套框选/抓取/贴图，**只在贴图之后分叉**。按用户决策**不做本地 OCR**，改为贴图后发送固定提示词让 DeepSeek 转录、结果进**结果卡**（依据 `docs/ocr-feasibility-2026-09-14.md` 实测：本地 OCR 只对「不可选中的文字」有独占价值，「读出图里的字」DeepSeek 视觉更强）。入口：右键菜单 + 托盘各加「截图识字」。
  - `wait_attachment`：`shotCommit` 只是触发本地预览 + 异步上传，故轮询 `hasAttachment()` 至多 3s 再发送（**超时不算失败**，只落日志）。
- **「结果回流」R6–R12：一串真机驱动的修复**（2026-09-15，详见 `deepseek-float/docs/result-reflow-design.md` §13.12–§13.16、§十四）：
  - **R6** 面板懒跳**前移到启动期预热**（首次划词不再等 WebView2 导航 + SPA 启动）；球窗加 `color:transparent` 兜底。
  - **R7 → R8 → R9** 回流排版三连修：由「HTML→Markdown→HTML 往返」改为**净化 HTML 直出**（页内白名单序列化 `answerHtml` + 卡片 `DOMParser` 二次净化重建节点），忠实保留 DeepSeek 全部排版（含表格）且**绝不执行远端脚本**；R9 修掉「同文件并行 Edit 静默丢改」导致的三处「旧调用没被替换」回归（六门全绿≠改动生效）。
  - **R10** 修「净化 HTML **空壳**骗过最短长度闸门」→ 间歇性空白卡：空块从 `""`（0 字符）变成 `"<div></div>"`（11 字符**标记**），新增 `hasVisibleHtml` / `is_markup_only`。
  - **R11** 修**长划词 60s 超时 / 无回流**（日志取证 `未接受会话容器（cand=2 echo=0）`）：块载荷改成 HTML 后，`compactText` 只去空白**不去标签** ⇒ 提示词被渲染成多节点时标签插在中间打断连续性 ⇒ `echo=0` ⇒ 候选容器全被拒。新增 `stripTags` / `compact_html`（去标签+去空白）统一回显判据（页内 + Rust 两侧）；结果卡引用条由**硬裁剪**改为溢出提示 + **点击展开**。
  - **R12** 修**截图识字回流不正确**：DeepSeek 送图后先渲染的瞬时状态「正在分析图片」在整段分析时间里**保持不变** ⇒ 骗过「稳定 + 静默」终态判定 ⇒ 被当答案交付。新增 `is_status_text`，与 `is_decoration` / `is_markup_only` 并列否决（**只否决不定位**）。
- **托盘诊断探针撤下 + 错误类型收口（R13-S5）**（2026-09-15）：
  - S1/R0/R1 期的三个排障探针（检测上传入口 / 检测会话容器 / 发送并回读一轮）从托盘移除 —— 其中「发送并回读一轮」**会真往用户会话里发一条测试消息**，长期挂在托盘上误触代价最高。底层探针函数**保留**，改由环境变量触发：`DSFLOAT_SELFTEST=<秒> DSFLOAT_SELFTEST_PROBE=upload|conversation`（只跑探针、不发消息）。同时移除 S1 遗留的**死命令** `inspect_upload`（自 S1 起无任何前端调用方）。
  - `lib.rs` 剩余 3 处 setup 子函数（`setup_ctx_menu` / `setup_tray` / `setup_ball_position`）的错误类型由 `Box<dyn std::error::Error>` 统一为 `AppResult` —— 兑现 `error.rs` 模块头与本文档此前的声明（此前代码与其矛盾）。
- **「结果回流」R5 结果卡变「迷你会话」：继续提问输入框 + 逐轮复制 + 追问回流 + 鲸鱼头像**（用户诉求原文：「结果卡片的继续提问改成和豆包一样的输入框，复制按钮放在每次回复的下面，继续提问结果也读取返回到结果卡片下面，结果卡片的图标改成悬浮球一样的鲸鱼 logo」）：
  - **★ 会话模型（本批的架构性改动）**：卡片内容改为一份 `Session`（`title` / `source` / `Vec<Turn>`），**唯一真相在 Rust**；每次更新带 `TurnMode` —— `Fresh` 开新会话（划词 / 识文字 / 设置页运行，**清空**后插入首轮）、`Follow` 追加一轮（卡片内追问）、`Update` 更新末轮（流式增量与终态交付）。`merge_into(&mut Session, &CardView)` 是**纯函数**，故「追问不换标题」「`Update` 不抹掉本轮问题」这类易错口径可直接单测。**页内只做渲染**，不再维护任何业务状态 —— 「复制到上一轮答案」这类「结果对但内容错」的静默缺陷从根上消失。
  - **「继续提问」输入框**：底部胶囊输入框（占位「继续提问」+ 圆形上箭头发送），Enter 发送 / Shift+Enter 换行，输入框自增高（≤4 行）。追问**不弹面板**（答案本来就该回到卡片里），回答回流成卡片的**下一轮**并同样流式长出。
  - **复制按钮下移到每一轮回复下面**（豆包同款）：每轮各自的动作行，复制的是**那一轮**的正文；生成中显示「生成中…」占位（高度与动作行一致，完成时卡片不跳）。命令口径随之调整：`result_copy(text)` **必须**带上是哪一轮的正文。
  - **头像换成鲸鱼 logo**：复用悬浮球（`poc/ball.html`）的同款 DeepSeek 品牌标记（Simple Icons 路径，`viewBox 0 0 24 24`，**逐字节相同**），白底 + 品牌蓝内描边，与悬浮球视觉一致。
  - **卡片高度改为页内实测**：渲染函数**返回**期望高度（`head/turns/composer` 的 `offsetHeight` 之和，读 `offsetHeight` 会强制同步布局），Rust 用它 `set_size` —— 多轮之后「字符数 × 行高」已经算不准。`estimate_height` **降级为兜底**（页内未就绪 / 测量超时 400ms 时使用，并在日志里明说「退回常量估算」）。尺寸策略：**流式中只长大**，**终态一次到位（可缩）**，且被工作区高度二次收窄；页内折叠正文 / 输入框自增高走新增的 `result_resize(height)`。
  - 命令：`result_close`（原 `result_action_close`）、`result_copy(text)`、`result_ask(text)`（文本来自输入框）、`result_drag_start`、`result_resize(height)`。单测 **76 → 82**。详见 `deepseek-float/docs/result-reflow-design.md` §13.11。
- **「结果回流」R4 交付方式分流：每条提示词可选「结果卡 / 面板」**：`Prompt` 新增 `show_card`（**默认 `true`** = 结果卡），设置页每行一个「改为面板 / 改为结果卡」按钮 + 状态徽标。
  - `show_card=true` → 划词后 **不弹面板**：只保证远端页已加载（新增 `lib.rs::ensure_panel_loaded` —— 面板窗口是 `about:blank` 起步、首次展示才懒跳 DeepSeek，走卡片的链路**必须**自己触发一次懒跳，否则注入报「没找到输入框」、回读报 `input=0 cand=0 echo=0`），注入后 `begin_watch` 把回答搬上结果卡；
  - `show_card=false` → **弹出面板**并聚焦，注入后**不起回读**（用户自己在面板里看，不出卡片）。
  ★ 默认值取 `true` 而非 `false`：旧 `prompts.json` 没有该字段，缺省 false 会让升级后的用户被**静默**改成「弹面板」。单测 68 → 76（含「缺字段反序列化默认 true」「显式关闭不被 sanitize 改回」）。详见 `deepseek-float/docs/result-reflow-design.md` §13.10。
- **「结果回流」R3 结果卡体验：豆包式卡片 + 可拖动 + 思考回流 + 流式输出**：
  - **卡片重做为豆包同款**：头部「头像 + 一句话标题 + 折叠/关闭双按钮」，其下是**引用原句条**（左侧竖线 + 浅底，最多 3 行），正文区**Markdown 极简渲染**（标题 / 有序·无序列表 / 引用 / 围栏代码块 / 分隔线 / `**粗体**`·`` `代码` ``·`*斜体*`），底部「复制 / 继续提问」+ 状态位。宽 400 逻辑像素、`body` 留 8px 让**投影不被窗口裁掉**，高 clamp 168–560 超出内部滚动。
  - **可拖动**：按住**头部**移动即可（★ 刻意不用 `data-tauri-drag-region` —— 它在 mousedown 阶段吞事件、会让头部按钮点不动，与悬浮球同型教训；改为「位移超 4px 才调 `result_drag_start`」，于是点按钮与拖窗口共存）。
  - **思考过程回流**：DeepSeek 把「折叠头 + 思考正文 + 回答」首尾相连塞在同一块里，页内 `answerOf()` 剥掉思考前缀只回传回答 —— R3 把它拆成 `splitBlock()`，**被剥掉的那一段一并回传**（快照新增 `thinking`，与 `texts` **逐项对齐**），Rust 侧 `locate_answer_and_thinking` 同块取回。卡片顶部是可折叠的「已思考」区（默认展开、限高 160px 内部滚动）。
  - **流式输出**：`watch_loop` 在**确认本轮已开始之后**（未开始推的是上一轮半截 = QA P0-2 那个洞），按 `STREAM_PUSH_INTERVAL_MS=400` 节流把增量推给卡片（首帧开卡、后续只换内容），页内显示闪烁光标 + 「生成中…」。**卡片一旦打开就不再改位置**（尊重用户拖动），只在「长高会顶出工作区底部」时上移。
  - **补上主链路闭环**：`popup::send_prompt` 此前**只注入不回读** —— 划词 → 点工具条 → 面板里回答了，却**永远等不到卡**（真机报障直接来源）。现注入与 `begin_watch` 成对出现，并带上 `title`（「这里是我的{提示词}」）与 `quote`（被划中的原句）。
  - 命令由 `result_action(action: String)` 拆为**一个动作一个命令**（`result_action_close` / `result_copy` / `result_ask` / `result_drag_start`）：拼错在编译期暴露，不必等真机「点了没反应」。单测 **64 → 68**。详见 `deepseek-float/docs/result-reflow-design.md` §13.9。
- **「结果回流」R2 结果卡**（把回读到的回答显示在锚点附近）：新增第 **6** 个常驻窗口 `result`（`src-tauri/src/result.rs` + `poc/result.html` + `vite.config.ts` 第 5 入口）。卡片为白底圆角（延续工具条的 `#2b5cff` 蓝），头部为来源标签 + 关闭，正文**可选中 / 可复制**、超长内部滚动，底部「复制」「发送到对话」。**交付点唯一**：`readback::deliver` 在**代次复检之后**调用 `result::show`（QA P1-4 —— 旧回读不会覆盖新卡片）；`begin_watch` 新增**锚点参数** `Option<AnchorRect>`（物理像素矩形：能力 A = 截图选区、能力 B = 划词光标点；诊断入口传 `None` → 退回「当前光标位置」兜底），`WatchOpts` / `WatchOutcome` 新增 `source`（卡片头部标签，与日志口径的 `tag` 分离）。定位**整段复用**从 `popup.rs` 抽出的共享纯函数 `place_near(prefer, anchor, size, work, gap)`（不复制粘贴；工具条原内联实现改为调用它，行为**逐字等价**）：锚点下方优先 → 越界**单轴翻转** → 夹取工作区。卡片宽固定 380 逻辑像素、高按字数估算（clamp 132–520，超出走内部滚动）。**三条硬约束**：① `show()` 后**不调用** `set_focus()`（R4/W-V7 不打断用户输入）；② 挂 `CloseRequested → prevent_close + hide` 守卫（同 `screenshot.rs`）；③ **页面存活看门狗** —— 显示后 500ms 若页内 `__dsfResultSet` 未定义（`dist/` 未随新入口重建 → 404）自动隐藏，不留死窗（W-V8）。卡片正文一律 `textContent`（远端回答不可信，**禁 `innerHTML`**）；超时降级（`complete=false`）在卡片上标注「回答可能未完成」+ 落 `diag` warn（W-V5 不静默）。新增 IPC 命令 `result_action`（`copy` / `ask` / `close`；按 S1 的 `shot_*` 先例注册并登记进 capabilities）。**单测 51 → 64**（新增 `place_near` 四边 + 极窄工作区 7 条、`truncate_payload` / `estimate_height` 6 条）。详见 `deepseek-float/docs/result-reflow-design.md` §13.8。
- **「结果回流」R1 回读桥**（读取「本轮回答」并判定「回答已写完」，**纯观测 + 纯函数**）：新增 `src-tauri/src/readback.rs`（4 个纯函数 `parse_snapshot` / `strip_own_prompt` / `locate_answer` / `decide_done` + `begin_watch` / `watch_loop` / `poll_once` / `deliver` / `begin_watch_diagnostic` + **24 条单测**）、页面内函数 `__DSFLOAT__.snapshotConversation()`、托盘菜单「诊断：发送并回读一轮（会发一条测试消息）」。**定位策略由 R0 真机结论决定**：DeepSeek 会话列表是**虚拟化**实现（§12.6，风险 R3），虚拟化只卸载**整条消息**、而新回答恒在列表底部必落在渲染窗口内 ⇒ 弃用「整段会话前缀差集」，改为读**尾窗块**并从尾窗定位本轮回答：取**最后一处**含我方提示词的块（= 本轮回显）→ 其后一块即回答（回显与回答同处一块时退化为「取末块 + 剥前导提示词」）。**★ 必须反向扫**：正向扫会被「上一轮同款提示词的回显」或「上一轮回答引用的提示词原文」抢先命中 → 把上一轮回答当本轮交付（独立 QA P0-1）。**「本轮已开始」只认正向证据**（新回显落盘 / 末块即回显 / 页内 `streaming`）——**不得**以「答案文本偏离基线」为启动信号，否则上一轮仍在流式时基线会自己移动、把上一轮**半截**当本轮（QA P0-2）。**终态判定**＝「页内未报告 `streaming`（一票否决，带 10s 安全阀）**且** 文本与上次采样逐字相同 **且** 已**连续 ≥3 次采样**稳定 **且** 距上次变化 ≥ **1200ms** **且** 长度 ≥ 1」；首采样一律不判终态（QA P1-1：800ms 与「一次慢采样」同量级，慢页面上 1 次稳定采样即满足静默 → 会把半截当完成）。**超时 60s 不得静默**（硬约束 G4）：落 `diag` warn 并降级交付「已读到的部分 + 可能未完成」，日志附「先看 `inject:` 确认发送是否成功」以区分发送失败与回读失败。**线程模型**（硬约束 G6）：轮询在短命 worker 线程，经 `run_on_main_thread` 把 `eval_with_callback` 投回主线程（主线程只 `tx.send`、回调内绝不 sleep），worker 用 `recv_timeout` 收值；每次 `begin_watch` 自增代次使在途旧回读作废，且 `deliver` **交付前复检代次**（QA P1-4）以免旧回读覆盖 R2 的新卡片。文本采集跳过 `button`/`[role=button]`/`svg`/`aria-hidden` 子树（否则「复制」「重新生成」等 UI 文案会污染结果）+ 长度闸门（单块 4k / 尾窗 8 块 / 仅遍历末尾 16 个子块 / 遍历提前收敛，风险 R7）+ 定位缓存加 10s TTL（QA P2-1）。为消除「远端页改版只修一处」的隐患，容器定位抽为**共用**函数 `locateConversation()`（R0 探针 `inspectConversation` 输出字段逐字不变，已逐行比对等价）。另修：诊断入口**不主动 `show()`/`set_focus()`**（改为沿用面板当前可见性）——否则 R11/W-V13「面板隐藏时能否照常流式生成并读回」无法用唯一入口验证（QA P1-3）；`Instant` 减法下溢 panic（开机 60s 内触发，QA P3-1）。详见 `deepseek-float/docs/result-reflow-design.md` §十三（含真机前 QA 的 2 P0 + 4 P1 + 3 P2 + 1 P3 记录）。
- **「结果回流」R0 探针**（go/no-go 闸门，**纯观测**）：新增页面内函数 `__DSFLOAT__.inspectConversation()` + `inject.rs::inspect_conversation()` + 托盘菜单「诊断：检测会话容器」。用途是廉价地回答「结果回流」方案唯一**高不确定性点** U1 的前置问题：**①** 能否在本页定位到承载整段会话的 DOM 容器（不依赖 class/id，沿用 `findInput` 的「多因子打分」范式）；**②** 该容器是否**虚拟化**（滚动卸载旧节点 → 后续「前后快照差集」的前缀会消失、delta 不再是新增后缀 → 方案形态须改写）。判据 `virtRatio`＝已渲染**条目数** / 应有条目数（由 `scrollHeight ÷ 条目高度中位数` 推算，并排除撑高用的 sizer；非虚拟化 ≈ 1；<<1 说明只渲染可视区）。另在 Rust 侧缓存上次 `pick` 指纹：**两次触发之间滚动面板**，第二次的日志即带一行 `Δ`（childCount/textLen/scrollTop 变化量），无需人工比对两行长 JSON。探针只读 DOM 与计算样式，不改动、不派发事件、不点击、不滚动。经**独立 QA 静态审计**后加固 8 处（含 2 个 P1：虚拟化判据改用「条目数」基准以避开 spacer 撑高型虚拟化的误判；遍历预算改为每滚动容器独立，避免首个滚动容器吃光配额导致会话列表选错）。详见 `deepseek-float/docs/result-reflow-design.md` §十二。
- **「结果回流」R0 探针 · 真机结论 + Δ 判定加固**（2026-09-14 晚）：真机读数（`d1-debug.log`）**判定 DeepSeek 会话列表为虚拟化实现** —— 滚动容器 class `ds-virtual-list ds-virtual-list--printable ds-scroll-area`，内层 `ds-virtual-list-visible-items` 被撑到 2495px（≈ `scrollHeight`）而容器仅 540px，`virtRatio=0.067`（2 渲染 / 30 应有）。**风险 R3 触发但不致命**：DeepSeek 新回答恒在列表底部、必落在渲染窗口内，故 R1 仍能读到本轮回答；被否的只是「**整段会话前缀差集**」这一条定位路线，**R1 改用「按块列表定位最后一条 AI 块 + 其文本长度连续 N 次采样不再增长即判回答结束」**（§七 R3 已预留的备选）。另**修掉探针自身的一次当场误报**：首版 `Δ` 分支只看计数，遇「会话很短、虚拟化窗口恰好覆盖全部内容 → 滚动时窗口不动 → 计数不变」即错判「非虚拟化」；现改为 ① `Δ` 先与 `virtRatio` 交叉核对（`< 0.5` 直接判虚拟化，**计数不变不构成反证**）；② 指纹加入**首/末块文本**以识别「窗口移动但条目数恰好相同」；③ JS 侧新增**下钻包装层**（输出 `hoppedFrom`）+ `childHT` 诊断字段，避免 `virtRatio` 算在 spacer / 包装层上。判据类代码必须与其辅助信号交叉核对 —— 这次错判不会崩溃，却会让 go/no-go 结论**反向**。详见 `deepseek-float/docs/result-reflow-design.md` §12.6。
- **「结果回流」R0 探针 · 二轮 QA 加固（判据方向纠偏）**：真机复测通过后，独立审计指出上一版**新引入的反向误判** —— 把 `virtRatio < 0.5` 提升成**单向决定性**判据，而该比值因「分子分母不同源」**系统性偏低**（`scrollHeight` 计入被当作 sizer 排除的长块与块间 margin/gap，`itemH` 不计），**非虚拟化容器也会跌破 0.5**（审计反例：10 块 = 8×40px + 2×300px → `scrollHeight=920`、`median=40` → `expectedItems=23` 而 `renderedItems=10` → **比值 0.435**）。修法：① 新增**结构性主判据** `pick.virtStruct` —— 检测「高 ≥ 0.9×`scrollHeight`、直接子节点 ≤8、子高之和 < 0.5×自身高」的 **spacer/撑高层**，属**结构事实**、不受块高分布影响，**单次快照即可定论**（首次触发即报结论）；② 常量更名 `VIRT_RATIO_HI` → `VIRT_RATIO_HINT`，语义降为「**倾向虚拟化（需复核）**」、不再单独定论；③ JS 下钻加**高跨度护栏**（`高 ≥ 0.5×scrollHeight`）防「下钻进单条超长消息内部」+ 新增 `hops` 深度；④ Δ 日志在 `textLen` 增长而 `childCount` 未变时提示「也可能是**流式回答正在增长**」；⑤ `pick` 缺失时**清空基线**（防跨 reload 比对）；⑥ `preview` 截断统一 36（与 `moved` 比较长度一致）；⑦ 末块改取 `lastTexts` **真末项**。详见 `deepseek-float/docs/result-reflow-design.md` §12.7（含「已核查不成立」清单）。**教训：判据修复必须经过以「反向误判」为靶心的独立审计 —— 只验证目标场景修好了是不够的。**
- **「截图提问」S1 可行性切片**（最小链路，未含抓图/编码/投递/热键）：新增第 5 个常驻窗口 `shot`（`src-tauri/src/screenshot.rs` + `poc/shot.html` + `vite.config.ts` 第 4 入口）—— **整屏透明遮罩** + 页内框选 + 尺寸提示 + Esc/右键取消。S1 只验证两件最不确定的事：**(a)** 整屏透明窗在 WebView2 上能否正常显示与框选；**(b)** DeepSeek 页面是否存在可注入的 `input[type=file]`（探针见下）。`shot_commit` 暂只把「CSS 坐标 / DPR / 视口尺寸」与「窗口缩放 / 物理换算」落 `d1-debug.log`，作为 DPI 换算真机核对依据；BitBlt 抓取与 PNG 编码在代码内标 `TODO(S2)`。为不打断既有子系统，遮罩显示期间**挂起两个子系统**：`mousehook`（否则框选松开会被当成划词触发、白弹工具条）与 `fullscreen`（否则遮罩覆盖整屏会被判成全屏而藏球）。入口：右键菜单「截图提问」+ 托盘「截图提问」；另加托盘「诊断：检测上传入口」（S1 探针，枚举候选 file input 落 `diag`）。热键与设置页录制控件仍属 S4。详见 `deepseek-float/docs/screenshot-qa-design.md` §十一。**2026-09-14 真机验证：6/6 出口判据全部通过**（`d1-debug.log` 实证；其中 DPI 换算链 `dpr=1.250` 恰等于 `win_scale`，`viewport 1536×864 × 1.25 = 1920×1080` 恰等于显示器物理尺寸，且每次框选期间 `trigger_selection` 命中 0 次、取消后划词立即恢复）。**未知项 (b) 成立** —— 页面存在 1 个常驻 `input[type=file]`（`fileInputCount=1`、`multiple=true`、`accept` 5889 字符含 13 种图片扩展名、`display:none` + `size=0x0`，**元素存在即可注入**）→ **S2/S3 投递路线确定为「页内 `DataTransfer` 注入」**，无需退回剪贴板 CF_DIB。
- **用户反馈改为只落诊断日志**：原后端 10 处 `app.emit("panel-status", …)`（`popup.rs` 9 + `inject.rs::notify` 1）**前端零监听者**（toast UI 原在已删除的 `src/` React 里）；曾临时新增第 5 个常驻窗口 `toast` 承载反馈，经真机反馈「不要弹窗显示」后**整体移除**，全部改为 `diag::log` / `diag::warn`（`d1-debug.log`）。即：**本项目不做任何浮层提示**。
- **单实例互斥**：新增 `src-tauri/src/single_instance.rs`，用 Win32 命名互斥体（`Local\com.deepseekfloat.desktop.singleton.mutex`）+ 命名事件实现，**零新增依赖**。二次启动不再装第二个全局鼠标钩子 / 第二个托盘，也不会并发写 `config.json` 与 Run 注册表项（此前第二实例的启动信标还会抹掉首实例的 `poc2-state.log`）。第二实例经 `SetEvent` 唤醒首实例显示悬浮球后自行退出。
- **统一错误类型**：新增 crate 级 `src-tauri/src/error.rs`（`AppError` + `AppResult<T>`），收敛此前的 `Box<dyn std::error::Error>` 与 `Result<_, String>` 两类散装错误；错误对象自带「上下文 + 底层原因（`source`）」链，附 3 条单测（`cargo test --lib` 22 → 25）。
- **绿色版分发产物**：完成首次 release 冷编译（`pnpm tauri build`，16m43s），产出 `src-tauri/target/release/deepseek-float.exe` —— **5.99 MB 自包含单文件**（前端资源已 `include_str!` 内嵌，无需 Rust/Node/pnpm 运行时），并复制到仓库 `release/deepseek-float.exe` 便于取用（该目录已 gitignore，不入库）。**换机只需拷这一个 exe 双击运行，不需要任何命令行**；注意别整个拷 `target/release/` —— 实测该目录 **1.7 GB / 3356 个文件**的编译中间产物（`deps` 1.3GB + `build` 0.24GB + `incremental` 0.2GB + `.pdb`），运行时全不需要，exe 仅占其中 0.3%。

### Changed

- `capabilities/default.json` 的 `windows` 加入 `result`（结果卡窗，**防御性**登记 —— R2 起它需要前端 `invoke` `result_action`）。`vite.config.ts` 新增第 5 入口 `result: 'poc/result.html'`（多入口由 4 页变 5 页；**新增入口后必须重建 `dist`**，否则卡片页 404 → 由 `result::arm_watchdog` 在 500ms 后自动隐藏）。
- `popup.rs` 把定位 / 翻边 clamp 抽成**共享纯函数** `place_near`（结果卡复用，取消复制粘贴）；`show_at_cursor` 改为调用它，**行为逐字等价**（光标点即「宽高为 0 的锚点矩形」）。
- `capabilities/default.json` 的 `windows` 加入 `shot`（截图遮罩窗，**防御性**登记）：`shot` 是首个「不在 `windows` 列表内、却需要前端 `invoke`」的窗口，而「应用自定义命令不受 ACL 门控」这一结论尚未在本工程实测（现有 3 个页面本就在列表内）。登记零额外权限，只是把该窗纳入 `core:default` 覆盖。
- **权限收敛（最小权限）**：`capabilities/default.json` 的 `windows` 移除远端页宿主 `panel`；`permissions` 由 12 项收敛为 `["core:default", "core:window:allow-start-dragging"]`。依据：Tauri v2 中经 `invoke_handler` 注册的**应用自定义命令默认允许被所有窗口调用、不受 ACL 门控**，故 9 个自定义命令无需任何 capability；唯一需要 ACL 的是 `ball.html` 拖动用的 `plugin:window|start_dragging`。
- 悬浮球窗口补 `"skipTaskbar": true`，消除任务栏残留条目。
- `windows` crate 特性补 `"Win32_Security"`：`CreateMutexW` / `CreateEventW` 在 0.61.3 里是 `#[cfg(feature = "Win32_Security")]`（签名含 `*const SECURITY_ATTRIBUTES`）；属同一 crate 的特性开关，**不新增第三方依赖**。
- `lib.rs` 中已改用 `AppResult<T>` 的 setup 子函数为 2/5（`setup_panel` / `setup_popup_and_mousehook`），其余 3 个（`setup_ctx_menu` / `setup_tray` / `setup_ball_position`）仍为 `Box<dyn std::error::Error>`，待后续收口；`prompts::save` 已改用 `AppResult<T>`；`config::update` 落盘/序列化失败不再静默，改为 `diag::warn` 告警。
- 清理「全局热键」历史残留：`setup_popup_and_hotkeys` 更名 `setup_popup_and_mousehook`，并同步 `lib.rs` / `popup.rs` / `mousehook.rs` / `uia.rs` / `capture.rs` 中指向已移除热键的过期注释。
- 4 个未被任何代码引用的 PoC 实验页（`acrylic-drag` / `ime-input` / `morph-transition` / `transparent-init`）从 `poc/` 迁移至 `docs/poc/` 归档，`poc/` 只保留在用的 `ball` / `popup` / `settings`。
- **文档治理（重新总结更新）**：`README.md` 新增「安装与换机部署（使用者）」章节（**A 绿色版 exe（无需联网）/ B NSIS 安装包（需联网）两条路径**、换机步骤、数据迁移、卸载残留），修正 `popup` 窗口高度 `56 → 42`；文档索引改为「现行（唯一事实源）/ 历史档案」两类。`deepseek-float/docs/architecture-summary.md` 对齐当前代码：补 `uia` / `config` / `autostart` / `fullscreen` / `error` 五个模块、修正工具条尺寸与发送链路、新增「工具条留白 = CSS 与估算常量的成对约束」决策。`docs/plans/` 下两份 2026-09-12 设计/实施计划加「历史档案」状态戳（技术栈已整体废弃，防误导）；`docs/poc-phase2-gate-report.md` 修正项目名并补入执行环境实测表。
- **工具条白色留白收紧**（真机反馈「白色外框又变大了」）：窗口高 `56→42`、按钮高 `40→30`、胶囊左右内边距 `8→5`、按钮内左右内边距 `12→9`、图标 `16→14`、图标-文字间距 `6→5`、圆角 `28/20→21/15`、胶囊描边 `1.5px→1px`；`MIN_W 200→150`（提示词较少时不再被强制撑宽）。`popup.rs` 的宽度估算常量（`button_px 46→37` / `COPY_PX 72→63` / `MORE_PX 36→30` / `BAR_PAD 16→10` / `WIDTH_MARGIN 10→8`）**成对同步**，保持「估算 = 实际渲染」，避免横向白边或文字裁切回归。
- **NSIS 安装包在本环境无法产出**（已实测并写入 README）：`pnpm tauri build --bundles nsis` 的 release 编译成功，但打包环节需从 GitHub 下载 NSIS 3.11（`tauri-apps/binary-releases`），被代理拦截（`CONNECT proxy failed: proxy server responded 502`，`HTTP_PROXY=127.0.0.1:11819`）；本机既无 `%LOCALAPPDATA%\tauri` 缓存也无系统 `makensis`。**降级方案即方案 A 绿色版 exe**（功能一致，仅少了开始菜单快捷方式与卸载项）。

### Fixed

- **结果卡的「超时降级提示」从不出现（静默降级，违反 W-V5）**：`note` 只在**整份渲染**里写，而超时降级发生在**增量**路径上；判据本身也是错的（流式过程中 `complete` 必然为 false，标了就是误导）。改为 `Turn::note()` **单点裁决**「已停流 **且** 未判终态」才标注，整份 / 增量两条渲染路径共用。R5 修。
- **用户手动关掉的结果卡会被后续推送自动弹回**：`show()` 见「不可见」就 `show()`，于是流式推送会把用户刚关掉的卡片抢回屏幕。改为新增 `RESULT_DISMISSED`：用户主动关闭后，后续推送**只并入会话、不再弹窗**（下次打开就能看到），只有 `Fresh`（新的划词）才清位重新弹。R5 修。
- **追问串轮**：上一轮仍在流式时追问，旧回读被 `WATCH_GEN` 作废（一问一答**有意**如此），但旧那一轮会永远停在「生成中…」。现 `ask()` 在追加追问骨架前，把仍在 `streaming` 的末轮明确标成 `complete=false` ⇒ 卡片如实显示「回答可能未完成」。R5 修。
- **结果卡取回的「回答 / 思考过程」不准（2026-09-15 真机报障；决定性取证 + 根因修复）**：
  症状：结果卡稳定地显示 **15 字符**「内容由 AI 生成，请仔细甄别。」（免责页脚），且被判 `complete=true`。
  取证（`d1-debug.log`）：`起始尾窗` 只有两块 —— `#0(2540)`（整段对话）+ `#1(15)`（页脚）；`container=ds-virtual-list … pass=1 hops=0`。骨架 dump 给出最后一块拼图：`ds-virtual-list-items` 的 `cc:1`（**只有一个子节点** `ds-virtual-list-visible-items`）。
  ★ 根因：下钻链在 `ds-virtual-list-items` 这层被 `db.childCount < 2` **拦停**（它是**单子包装层**）⇒ `pickEl` 停在外层 `ds-virtual-list`，其直接子节点是「整个可见消息列表」+「免责页脚」⇒ 「回显块的后一块」取到**页脚**；次要成因：下钻护栏只认「高 ≥ 0.5×scrollHeight」，而 `visible-items`（h=9050）< 19000×0.5。
  修法（三处）：① **单子包装层不是终点** —— `db.childCount < 2 && !(dom.children.length === 1)` 才 break，于是能穿过 `items → visible-items` 两层包装；② 下钻护栏除「高跨度」外接受**结构性证据** `spread = 子块 ≥2 && 最大子块文本 < 自身 90%`；③ Rust 新增 `is_decoration()` —— **装饰文案只用于否决、绝不用于定位**，命中即返回空串让回读继续等真回答（宁可超时告警，也不把页脚当回答）。
  ★ 另补一条**结构化「仍在思考」信号** `pending`：真机里思考阶段 `streaming=false`（页面上没有可识别的「停止生成」控件），于是「思考完毕 → 正文开始」这段停顿会被判成「写完」，把**思考正文当回答**交付。现 `splitBlock()` 命中折叠头却切不出回答时返回 `pending:true`，Rust 侧并入终态否决（`veto = streaming_veto || pending`），且 `pending` 期间卡片**只长思考过程、正文留空**。单测 **68 → 76**。详见 `deepseek-float/docs/result-reflow-design.md` §13.10。
- **面板「怎么点都唤不回来」（2026-09-14 真机报障「结果卡弹窗导致面板无法显示」）**：
  取证：最近一次会话里两次点击悬浮球**都**打印 `toggle-panel: 面板已收起`，即 `is_visible()` 恒为 `true` —— 而用户在屏幕上根本看不到面板。
  ★ 根因（判定口径）：`is_visible()` 底层是 `IsWindowVisible`，**已最小化的窗口同样返回 TRUE** ⇒ 面板一旦被最小化，判定就恒为「可见」→ 点球走进**收起**分支 → 永远唤不回。且 `ShowWindow(SW_SHOW)` 对最小化窗口**无效**（须先 `SW_RESTORE`）。
  修法（三处，互为纵深）：① `show_panel` 先 `unminimize()` 再 `show()`；② `toggle_panel` 判定改为 `is_visible() && !is_minimized()`；③ 新增 `ensure_panel_on_screen` —— 面板左上角已不在任何显示器内时（显示器拓扑/分辨率变化、`WM_DPICHANGED` 系统建议矩形改写）回收到主显示器工作区，覆盖「show 成功但用户仍看不见」这第二种成因。
  ★ 同时**给下次取证留线索**：`toggle-panel` 日志现在带 `pos=(x,y) size=WxH`，一眼可判是「最小化」还是「跑到屏幕外」。
- **`pnpm tauri dev` 启动后没多久「自动关闭」（真机报障，2026-09-14；根因已定案并根治）**：症状是 dev 进程树**无声消失** —— `d1-debug.log` 全量 3237 行 / 52 次进程启动里 **`run:` 行数为 0**（从未优雅退出），且**无 `panic.log`**、Application / System 事件日志无任何记录、`EBWebView\Crashpad` 近 24h 无 `.dmp`（WebView2 也没崩），而 WebView2 的 `Local Storage` / `Session Storage` 一直正常写到最后一刻 → 判定为**外部 `TerminateProcess` 类硬杀**，不是崩溃。死亡与「划词抓取链」**强耦合**：7 个会话的最后一行日志正是 `trigger_selection 进入（…）`，其余停在 `capture: 已投递 Ctrl+C（4/4 事件）` 之后。**根因**：划词用 `SendInput` 合成 **Ctrl+C**，而 dev 二进制是 **console 子系统**（`main.rs` 只在 release 设 `windows_subsystem="windows"`）⇒ 当前台是终端 / conhost / Windows Terminal / mintty 时，合成按键被控制台转成 **`CTRL_C_EVENT`**，默认处理器以 `STATUS_CONTROL_C_EXIT(0xc000013a)` 杀进程；此前的止血 `SetConsoleCtrlHandler(None, true)` **只保护本进程** —— 同一个控制台里的 `pnpm` / `node`(vite) / `tauri` CLI 没有这层保护，被同一次控制事件一起打断 ⇒ dev 整棵树消亡、应用随父进程被杀。**修复**：复制和弦由 **`Ctrl+C` 换成 `Ctrl+Insert`**（`capture::send_copy_chord`）—— `Ctrl+Insert` 是 Windows **控制台的「复制」标准键**（conhost / Windows Terminal / mintty 均内置），语义等价于复制但**不产生 SIGINT / CTRL_C_EVENT**，Chromium 系（浏览器 / Electron）与传统 Win32 编辑控件同样支持，故一次改动同时消除危险并保留「在终端里划词」场景；**刻意不做 Ctrl+C 回落**（回落会把事故路径原样引回来）。`lib.rs` 的 `SetConsoleCtrlHandler` 保留作**纵深防御**。**此前同类事故**（同一次真机报障的「读取失败并软件崩溃」）根因是单实例 **Secondary 分支静默 `exit(0)`**（早于诊断初始化、一行日志都不写 ⇒ 与真崩溃无法区分），已在该分支补 `diag::warn` 留痕。
- **「结果回流」R1 回读桥的真机缺陷（读取失败 + 「软件崩溃」）**：① **容器选错** —— 原判据会选中侧栏历史 / 输入区工具栏 / 欢迎页（真机曾回读到 **67 字符的「思考正文 + 回答」** 与 **23 字符的装饰页脚**「深度思考 智能搜索 内容由 AI 生成，请仔细甄别」，两次都判 `complete=true` ⇒ W-V3/W-V4 **假通过**）；现加 **B3 结构性硬过滤**：只接受**尾窗里确实含我方提示词**的候选（用结构事实排除干扰容器），取「宁可不交付（走超时告警，不静默）也绝不交付错内容」。② **思考正文污染回读** —— 真机上折叠头「已思考（用时 N 秒）」、思考正文与回答**首尾相连无分隔符**，纯文本切不开；新增 `answerOf()` 按 **DOM 分层**剥离（找「为整块文本前缀且严格更短」的最长元素；**找不到则原样返回，绝不截断回答**），并修正思考词表（补 **`已思考`** —— 旧词表不含它 ⇒ 剥离恒不触发，这才是「回读带上整段思考」的直接原因）。③ **探针不唯一** —— 诊断探针原为常量提示词，两轮答案相同时「回读 == 面板回答」这条判据会**自证失效**；改为 `诊断编号 {毫秒时间戳}：…`，使含提示词的块**只可能命中本轮**。另修：定位缓存阈值随命中遍次（短会话列表 `textLen≈80` 却按 120 校验 ⇒ **缓存恒不命中**、每 250ms 全量扫 DOM）；新增 `structDump`/`outline` 分层取证与 `Snapshot.container`/`dump` 字段、Rust 侧 `fmt_blocks` 尾窗压行（**起始 / 交付前 / 超时三处**均落盘）、`[boot] pid=` 首路取证、`DSFLOAT_SELFTEST_SHOW=1`（**面板从未展开就停在 `about:blank` ⇒ 自检恒报「未注入」，W-V13 在自动化下永远拿不到现场**）。**真机结果**：隐藏态（`panel_visible=Some(false)`）回读 == 「回读测试」，**W-V13 通过**；`cargo test --lib` 49 → **51 passed**。
- **悬浮球右侧「隐形吞点击区」**（真机反馈「悬浮球右侧确实有不可见区域」）：根因是 Tauri 在本工程下给窗口保留了 `WS_CAPTION`（`tauri.conf.json` 的 `decorations:false` 与 `is_decorated()==false` **都成立**，但 OS 样式位仍在），Windows 据此按 `SM_CXMINTRACK`（本机 **136**）强制最小宽度 —— 球请求 64×64 逻辑，高度 64 > `SM_CYMINTRACK`(39) 未被夹，**宽度 64 < 136 被夹成 136**，于是球窗真实逻辑尺寸变成 **136×64**，而 `poc/ball.html` 只画左侧 64px → **右侧多出 72px 宽的不可见区域吞掉鼠标点击**（其余 4 个窗口 440 / 474 / 448 / 整屏本就宽于 136，故只有球中招）。新增 `src-tauri/src/ballstyle.rs`：清 `WS_CAPTION|WS_SYSMENU|WS_MINIMIZEBOX|WS_MAXIMIZEBOX`（合计 `0x00CB0000`）→ `SetWindowPos(SWP_FRAMECHANGED)` → 重新 `set_size(64×64)`，并在 `run()` 的 setup 中于 **`setup_ball_position` 之前**调用（位置夹取依赖窗口尺寸）。实测 `136×64 → 64×64`；零新增依赖（`Win32_UI_WindowsAndMessaging` 已启用）。
- **划词发送只填入不发送**：两处根因 —— ① `dsfloat_init.js::isVisible()` 对**发送按钮**沿用了输入框的阈值（宽 ≥ 40px），而发送按钮多为 ~32×32 图标按钮 → 被直接过滤 → `scoredSenders` 为空 → 退化成 `no-send-button`；② 退回的 OS 级 `SendInput(VK_RETURN)` 依赖前台焦点，面板未真正激活时回车落空（且原先**先发回车、再弹 toast**，`SendInput` 异步投递，紧随的窗口 show 有改变前台窗口的风险）。现改为：按钮判据放宽到 ≥12×12 并扩大候选范围（`button` / `[role=button]` / `data-testid|class` 含 `send`）；提交后以「**输入框已清空**」复核是否真的发出，未发出则依次回退「页内合成回车（`pressEnter()`，不依赖焦点、不会外泄到别的程序）→（面板聚焦时）OS 级回车」，全程**不谎报成功**。

### Removed

- 临时新增的第 5 个常驻窗口 `toast`（`src-tauri/src/toast.rs` / `poc/toast.html` / `vite.config.ts` 第 4 入口）：真机反馈「不要弹窗显示」，按用户要求整体移除，常驻窗口回到 4 个。（`windows` crate 的 `Win32_Security` 特性**保留** —— 单实例互斥仍需要它。）
- 失效的 `.gitignore` 规则（vitest `*.timestamp-*.mjs`），以及 `.npmrc` / `pnpm-workspace.yaml` 中提及已删除 react / vitest / typescript 的过期注释。
- `capture.rs` / `prompts.rs` 中指向已删除文件（`src/lib/promptTemplate.ts`）与已删除类型（`PresetId` / `compose_prompt`）的过期说明（改写为历史说明）。
- 重复 / 过期文档：`overview.md`（立项调研概览，内容已被 `deliverables/software-company/deepseek-float/00-final-recommendation.md` 与 `README.md` 覆盖，且状态停留在「Phase 0 阻塞、Phase 1 完成」并引用已删除的 `src/lib/*.ts`）；`docs/plans/poc-results.md`（Phase 2 PoC 过程记录，其「门控未裁定」与最终裁定书 `docs/poc-phase2-gate-report.md` 的「5/5 通过」冲突，独有执行环境数据已并入后者）。

## [0.1.0] - 2026-09-14

首个可运行版本：Windows 桌面 DeepSeek 悬浮助手骨架。

### Added

- **悬浮球**：64×64 透明无边框置顶窗口，自由拖动、左键开面板、右键菜单。
- **DeepSeek 面板**：内嵌 `chat.deepseek.com`，路线 A 持久化登录（cookie 存于 WebView2 data dir，跨重启保留）；可缩放、可拖动、关闭即隐藏不销毁。
- **划词工具条**：全局鼠标钩子（`WH_MOUSE_LL`）检测拖动选中自动浮现；含 复制 / 解释 / 翻译 / 总结 / 问问DeepSeek / 自定义 / ⋯；动态宽度、贴光标、屏幕边缘翻边避让、点击外部自动隐藏。
- **统一提示词模型**：内置 4 项（可编辑 / 隐藏 / 恢复默认，不可删）+ 自定义项（可增删改隐藏，上限 20），持久化 `prompts.json`。
- **自动注入并发送**：三级注入面板输入框（原生 setter + input 事件 → `execCommand` → Ctrl+V 兜底）+ 读回校验 + 自动点发送（不自动回车）。
- **设置窗口**：划词预览、提示词管理、产品功能三开关。
- **系统托盘**：显示/隐藏球·面板、重新登录、退出；托盘图标与悬浮球一致的 DeepSeek 鲸鱼。
- **位置记忆**：启动恢复悬浮球上次位置并夹取回可见区；拖动去抖落盘；退出前 flush 最终位置。
- **开机自启**：写 HKCU Run 项；启动时与注册表真实状态对账（尊重用户在任务管理器里的禁用）。
- **全屏隐藏**：前台全屏时自动隐藏悬浮球，退出全屏恢复（仅恢复原本可见的球）。
- **提权划词**：可选优先使用 UI Automation TextPattern 直读选区，失败回退剪贴板法。
- **release 优化**：`[profile.release]` 开启 `opt-level="s" + lto + codegen-units=1 + strip`。

### Changed

- 前端由 React 工程收敛为原生 `poc/*.html` 三页（ball / popup / settings）。
- `withGlobalTauri` 收窄为 `false`，本地页面改用 `__TAURI_INTERNALS__` 构造最小 IPC 桥。
- 字符串事件（`toggle-panel` 等）改为类型化 `#[tauri::command]` + `invoke_handler`。
- 提示词增加内存缓存，消除高频读盘。
- `lib.rs::setup` 拆分辅助函数，降低单函数复杂度。

### Fixed

- **划词崩溃** `STATUS_CONTROL_C_EXIT (0xc000013a)`：合成 Ctrl+C 被前台终端转成 `CTRL_C_EVENT` → 进程忽略 Ctrl+C（`SetConsoleCtrlHandler`）。
- **NoNewCopy**：合成 Ctrl+C 前等待 Ctrl/Alt 松开，避免投递成 Ctrl+Alt+C。
- **点击悬浮球无反应**：弃用 `data-tauri-drag-region`（mousedown 阶段吞 click），改脚本手动 `startDragging` + 位移阈值判定。
- **面板创建失败 `0x8007139F`**：同一 WebView2 user-data 目录下多个 webview 的 browser args 必须一致，移除面板的 `--disable-gpu`。
- **windows 0.61 编译**：UI Automation 特性改为 `Win32_UI_Accessibility`；显示器 API 归 `Win32::Graphics::Gdi`；注册表函数返回裸 `WIN32_ERROR`；`Mutex<Instant>` 非 const 初始化改 `OnceLock`。
- **配置竞态**：`config::update` 改为单锁内原子读-改-写，避免位置保存与设置写入互相覆盖。
- **工具条偏宽**：恢复 `button padding` / `.bar gap|height`，使 Rust 宽度估算与前端一致。
- **开机自启失败回滚**：`config_set_autostart` 仅在注册表写入成功时落盘，设置页失败时回滚开关。

### Removed

- 全局热键（`Ctrl+Alt+D`）—— 改为纯鼠标划词触发。
- 遗留未使用的 React 前端与 `vitest` / `tsc` 工具链。

---

> 早期 PoC 阶段（PoC-1..5：透明 / 内嵌 / IME / 形变 / 毛玻璃）的逐次验证记录见 `deepseek-float/docs/` 与仓库根 `docs/`。
