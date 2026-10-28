# Beta.2 划词优化与开发合并准备（2026-10-28）

## 范围与基线

公开仓库 `bookguifan/windows-ai-float-assistant`，分支 `fix/beta2-native-acceptance`，HEAD `d7e7afd17b64a9989fa6cfa03f41ef6bbf0477e5`；本地与远端修复分支一致。main 本地和远端均为 `922479b1a30c070b572dfbd252cccaa543837449`。暂存区为空。

开始时已有 `beta2-known-issues.md` 的调查补充和未跟踪的 `beta2-startup-crash-analysis.md`，均保留。没有操作原私有仓库。本轮不提交、不推送、不合并，不启动原生助手，不读写系统剪贴板。

按维护者的新决策，启动崩溃调查暂缓；允许在完整披露、发行门禁不退化及新 SHA 远端 CI 通过后评审开发代码合并。Beta.2 发布仍 **NO-GO**。历史文档中的“暂不合并”等描述保留为当时状态，当前准入以本节及下方清单为准。

## N01：确认的附加问题与最小修复

前端 `fire()` 在 `open-more` IPC 已完成后仍固定等待 250ms 解锁。因此菜单显示后快速点击“设置”，会因 `fired` 为 true 被直接丢弃。新增测试在原前端真实失败：只有 `open-more`，没有派发 `open-settings`。

现改为 `open-more` Promise 完成后立即解锁，失败时允许明确重试；未完成时继续抑制重复派发。解锁仍绑定 actionEpoch，旧 IPC 不能解锁新票据。不改变原生 arm、Ticket、目标身份、PopupShow、350ms 显示事务及 8 秒 Session 校验；未扩大任何焦点豁免。

该问题只解释“已展开菜单后的快速后续点击被丢弃”，**不能证明它是全部首次 ⋯ 无响应的唯一原因**。已有显示阶段焦点/generation 竞态修复和全部专项测试保留，原生首次点击 FAIL 不改判。首次冷/热设置、旧前台事件及原生 arm 仍须新 EXE 验证。

## N05：首个差异节点仍缺历史证据

|节点|历史 60→33 样本|后续原生诊断样本|
|---|---|---|
|夹具文本总长度|期望 60 UTF-16|60 UTF-16|
|EM_GETSEL 真实起止|缺少可关联记录|2..60，真实选区 58 UTF-16|
|Native Edit 捕获|缺少同请求摘要|58，与选区摘要一致|
|Session / Ticket / 动作|缺少完整可关联摘要|58，摘要一致|
|主动写入 / 粘贴|实际 33；来源链未闭环|58，最终精确粘贴 PASS|

后续 58 样本 UTF-16LE SHA256 为 `63B67D257870FFCFEB5066EB093B35018A602D3793AFE850BF8EA5EE7C2539D6`，原生证据来自此前冻结版本，并非本轮新 EXE。实际选择范围不同不能直接按整框总长判定截断；但也不能倒推历史样本必然只选了 33。

当前源码审查：`selected()` 按 UTF-16 范围返回原文，trim 只用于拒绝无效空白；`read_stable` 返回原文；Session 分开保存 raw/normalized；Ticket 及 ToolbarAction 保留 raw；主动 copy 使用 session.raw；剪贴板写入 encode_utf16 加终止零，不做 trim 或换行替换。未发现可证明历史 60→33 的首个代码截断节点，因此不进行推测性复制修复。

已有纯链测试覆盖两种捕获方式、完整/部分范围、首尾空格、CRLF/LF、中文、Emoji、单次消费。补充 Native Edit 切片矩阵；这只证明纯转换函数，不能代替实际控件范围和系统剪贴板写入验收。历史 N05 FAIL 仍为 P1 未闭环；如后续观察到真实范围 60 而下游 33，立即停止并修复首个差异节点，不降级为 UI 问题。

历史 WM_COPY 来源继续 UNKNOWN。正常主动复制调用写入 API，并不向目标控件发送 WM_COPY；保留的自动 SendInput 入口仍受发行锁限制。最新样本 WM_COPY=0 不能证明此前两次消息归属。

## 设置与撤销保护

- 普通用户保留“智能划词增强”和“自动复制回退（实验性）”两项划词设置；其他常规设置不变。
- 新安装及配置缺少 `privileged_capture` 时默认 true；显式 false/true 均保留。JSON 字段名不变，无数据格式迁移。
- 智能路径优先 UIA，必要时读取标准 Unicode Edit；不使用系统剪贴板。不支持控件继续安全失败，不提示用户手动粘贴，不偷偷启用自动复制。
- 发行构建实验回退显示“待安全验收”，灰显关闭；旧版回退只移除 UI，字段、IPC 拒绝、测试及研究源码保留。
- 配置保存改用 checked commit，落盘失败返回 false，前端恢复显示状态。关闭智能划词后撤销 generation；快速重新开启不复活旧请求。
- UIA 读取前后复核当前权限、窗口、代次、期限；在途关闭会丢弃迟到结果。该纯编排函数先以失败关闭占位取得正向 RED，再实现前后检查；不将其描述为原生 COM 竞态已复现。
- 备份格式和合并/覆盖业务语义保持既有规则；旧备份省略智能字段仍按旧备份默认 false 解读，不能把备份当作新安装。显式智能值及发行回退 false 的落盘再加载已覆盖。
- 智能划词关闭且无研究构建授权时，生产 worker 不进入 UIA/原生 Edit；发行回退不能填补此路径。关闭后的原生窗口体验仍需 GUI 验收。

## 自动化与红绿证据

所有命令在公开仓库应用目录执行，日志及 JSON 命令/退出码保存于仓库外本轮独立证据目录，不提交日志或产物。

|项目|实际结果|
|---|---|
|菜单已完成后立即点设置|原前端 RED → GREEN|
|配置新安装/缺失默认开启|原默认 false 真实 RED → GREEN；显式选择、落盘与重载通过|
|设置保存返回 false|原 UI 不回滚 RED → GREEN|
|在途权限/代次撤销|纯编排 RED → GREEN；原生 GUI 未执行|
|Rust 全单元/集成/文档测试|404 PASS，12 ignored，0 FAIL；按目标汇总，存在模块重复编译，不宣称 404 个独立场景|
|PopupShow / 选择状态机|selection_core 42 PASS，含原显示事务负向测试|
|前端单测|390 PASS：两个自有断言集 71+161，node:test 158；并非 390 个独立 GUI 场景|
|Playwright 本地网页回归|22 PASS；现有净化测试不等于 N01/N05 原生或设置视觉验收|
|格式与严格 Clippy|退出 0|
|前端 build/check|退出 0|
|Debug / Release 策略|分别 4 PASS；Release 同时启用 custom-protocol 与 experimental-clipboard-fallback 仍锁定|
|Release custom-protocol 检查及编译链接|均退出 0；未启动 EXE|

本机 rustc/cargo 1.98.1、Node 24.14.0、pnpm 9.15.9、锁定 Tauri 2.11.5。Node 低于 package engines 的 ^24.15.0，pnpm 有环境告警；未升级依赖。未执行原生 GUI、剪贴板竞争、截图/OCR、六站网站回归或启动稳定性复测。

新增 Rust 测试在 --lib、selection_core、native_copy_proof 目标内，前端测试在现有 test:unit 内，均已被 CI 命令覆盖；无需为本轮新增 CI 目标。12 ignored 未执行，不计 PASS。

## 发行安全与开发合并门禁

现有 `release_policy`、`safe_copy`、`capture` 的准入规则未变。旧自动复制始终 false；实验路径仅显式研究特性的非 custom-protocol Debug 有效，Release 加实验特性仍不能启用。配置/备份/IPC 不能覆盖编译门禁；密码、窗口身份、旧代次、过期及单次消费负向覆盖保留。未把 R01–R03 改为已修复。

当前仅有 CI workflow：main/master push 与 pull_request 触发，contents:read，无 Tag/Release/附件上传步骤。main 合并不会自动发布 Beta.2。本轮未提交和推送，故不存在可供本轮核验的新提交远端 CI。

|开发合并条件|状态|
|---|---|
|本地自动化与构建|PASS；不代表原生启动或 N01/N05 通过|
|发行危险复制隔离|本地 Debug/Release 专项 PASS；准入源码未改|
|Known Issues / 原始 FAIL 保留|已保留，本轮原生验收 NOT_TESTED|
|新 SHA 对应远端 CI|PENDING：需单独提交/推送授权后验证|
|启动 P1 独立 GitHub Issue|PENDING：已有独立调查文档；本轮无 GitHub 写入授权，不伪造 Issue URL|
|PR 完整披露及维护者审阅|PENDING；本报告可作为说明底稿，不自动更新远端 PR|
|main 合并授权|未取得，不执行|

建议独立 Issue：**[P1] Windows 启动后偶发 fail-fast，Beta.2 发布阻断**。证据为既有 Application Error 1000、0xC0000409、RVA 0x4383EB，匹配符号 panic_with_hook；缺少原始业务栈，根因未知。后续一次隔离运行未复现不能关闭问题。完整记录见[启动调查](beta2-startup-crash-analysis.md)，暂缓进一步调试，不猜测修复。

## 下一轮最小原生矩阵（须单独授权）

1. 先核验新 EXE 摘要和独立配置，检查启动/菜单退出；发生崩溃即停止，不用单元测试代替稳定启动。
2. N01 冷/热设置首次 ⋯，以及菜单显示后快速单击设置；分别记录命中、arm、IPC、Ticket、ready、预览与焦点清理，不反复重试掩盖首次失败。
3. N05 先核对 EM_GETSEL 实际范围，再串联 capture→Session→Ticket→action→write→paste 的 UTF-16 长度/摘要；先纯单行，再首尾空格、多行/CRLF/LF/中文/Emoji。仅明确主动复制可改系统剪贴板，不读取此前未知内容。
4. N03 旧票据、8秒期限、替换、重复消费、目标销毁/切换、关闭智能划词及迟到 UIA 结果；旧事件不可清除新票据或复活预览。
5. N04 WPF UIA 和标准 Edit 全文保真，unsupported/password 安全拒绝；不能仅用工具条出现记精确内容 PASS。
6. N07 由隔离版本导出的有效合成备份请求回退 true，合并/覆盖后禁用，旧配置及直接 IPC 同样不能解锁；历史错误夹具和已有局部 PASS 均保留。
7. N08 菜单退出与记录的子进程清理。R01–R03 不执行；实验源码保留，但不进入本次发行验收。

**开发合并尚待远端 CI、独立 Issue 和审阅授权；Beta.2 发布 NO-GO。**


## 实际修改清单与候选冻结

生产代码：

- `poc/popup.html`：菜单 IPC 完成时按 epoch 解锁，避免固定 250ms 的后续点击丢失。
- `poc/settings.html`：两个划词设置、新提示、删除旧 UI 控件、保存拒绝回滚。
- `src-tauri/src/config.rs`：新安装/缺字段智能增强默认 true，保留显式值。
- `src-tauri/src/ipc.rs`：checked commit；关闭智能增强使旧代次失效。
- `src-tauri/src/selection.rs`、`selection_runtime.rs`：UIA 读取前后当前请求/设置复核。

测试：`src-tauri/src/backup.rs`、`native_copy_proof.rs` 只增加 cfg(test) 测试，`src-tauri/tests/selection_core.rs` 增加纯编排撤销测试；`tests/safe-copy-ui.test.mjs`、`settings-workflows.test.mjs`、`backup-ui.test.mjs` 覆盖菜单、迁移和发行设置。既有四项旧 UI 锁定测试逐项迁移到现有实验控件并保留伪造事件/迟到配置的负向断言，没有删除测试块；新增测试另核对旧 UI 控件不存在及后端旧命令不被调用。

文档：本报告及 `beta2-known-issues.md`；原有未跟踪启动调查文档逐字保留，SHA256 `A8834E2919E81317BBC48F8914F6A864267BBACBBED24667F345FD332918FD93`。未变更依赖锁文件、图标、发行准入代码、历史 EXE/HASH 或 CI 配置。

冻结目录名称：`beta2-selection-convergence-20261028`（本机 TEMP 外部证据目录，不入 Git）。

- 构建基线：HEAD d7e7afd，dirty 工作区，不宣称来自已提交或远端通过的新 SHA。
- 命令：`CARGO_INCREMENTAL=0 cargo build --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol --bin deepseek-float`（从应用目录），退出 0，链接耗时 3m09s。
- 特性：custom-protocol；未启用实验特性；x64，版本资源仍为 0.1.0，未做正式 Beta.2 版本冻结。
- 新 EXE：独立目录 `bin/deepseek-float.exe`，SHA256 **477BF2856EAE6E0226688D9F481CED195F38C5212954C990798ACBA241CD35C7**，NotSigned，未启动。
- 本地配套 PDB SHA256：`8A026A6E239E9774514A5DCF97629E61AF1B33F101D8096E8B57192D66B1AA29`；PE CodeView/PDB GUID 均为 `1cbe4332-df1a-4e54-8a14-09f8952a2b65`，Age 1。不得将符号、日志或转储作为发布附件。
- 静态模式检查有 177 个 ASCII 用户目录样式编译路径，不记录用户名原文；不能当作已完成隐私发布审查。该未验收候选只供本地定向复测，不能直接发布。
- `BUILD-SOURCE-SHA256.json` 为构建前 753 个源码文件清单，摘要 `335BEE9457580E35E1161861FB3825AB1BF3DDC6895E8F46AB9D7B24F0195154`；`BUILD-INPUTS-SHA256.json` 记录业务构建输入。报告收尾只改文档，最终全文件清单另存 `FINAL-SOURCE-SHA256.json`，不递归包含清单自身，最终复核必须保证编译输入摘要不变。
- 实际构建起止时间、工具输出与退出码见外部 `release-build.json`、`rust-results.json`、`frontend-results.json`、日志；匹配符号见 `PE-SYMBOLS.json`。新增/修改文件常见令牌、私钥、本机个人路径扫描未命中，不等于全面安全审计。

本轮新的 GUI、剪贴板和启动稳定性验收全部 NOT_TESTED。N01 附加前端缺陷修复与撤销保护已自动化验证；N05 历史首个差异仍证据不足；发布阻断没有因编译成功自动关闭。
