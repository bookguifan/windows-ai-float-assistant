# Beta.2 UIA 与自动复制回退验证报告

日期：2026-10-24。开发分支：`feature/uia-safe-clipboard-fallback`。
基线与当前 HEAD：`9862487eb12b18a9bd8c85fc12e8299edf23b9ee`。本轮修改未提交、未推送。

## 2026-10-26 提交前最终摘要

本摘要描述当前工作区最终状态；后文初始方案、各轮测试数字、FAIL与BLOCKED是按时间保留的历史记录，不代表最终通过数量或当前控件范围。HEAD仍为`9862487eb12b18a9bd8c85fc12e8299edf23b9ee`，分支`feature/uia-safe-clipboard-fallback`，尚未提交/推送。

### 最终范围与安全边界

- 共27个候选文件：13个已跟踪修改、14个新增；不是初始23个文件。新增`native_copy_proof.rs`、对应集成测试及R01–R03报告，并包含后续`settings.rs`的选区移交保护。
- UIA优先、用户主动复制保留。实验性自动Ctrl+C默认关闭、仅本进程原生确认授权；旧配置、备份及旧入口不能授予该权限，恢复备份后撤销授权。
- 自动回退最终还必须取得独立选区证明，因此目前只支持非密码的标准Unicode Edit。RichEdit、ANSI及未验证控件在发送Ctrl+C之前拒绝；`native_focus`的粗筛允许列表不等于最终能力范围。
- 复制前绑定精确控件和UTF-16选区；发键前、读取后重新取证。空内容或与非空证明不符的内容直接拒绝，不恢复旧快照。后文初始“空复制先恢复”描述已被该后续策略替代。
- HWND/PID/thread/owner/sequence及文本匹配均不能证明所有写入因果；恢复前持锁复核可防止已观察到的外部新数据被旧快照覆盖，但逐格式SetClipboardData不是整体事务。
- 27文件的定向差异复核未新增确认缺陷；已有R02来源歧义、R03恢复失败风险继续保留，不能将此结果写成独立安全认证或完整兼容性验收。

### 本次收尾重新执行的检查

| 检查 | 真实结果 |
| --- | --- |
| `pnpm test:unit` | **375 PASS、0 FAIL**：两组自定义断言71+161，12组Node测试合计143；Node汇总无取消/跳过。本命令含模拟截图/UI测试，不操作原生截图或OCR。 |
| `pnpm check` | 退出码0。 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 退出码0。 |
| `cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | 退出码0。 |
| `git diff --check`及14个新增文本文件行尾空白检查 | 通过；未修改Git索引。正式暂存后仍须执行`git diff --cached --check`并审查暂存差异。 |

没有重复全部Rust测试、Playwright、EXE构建或原生验收。此前同一构建源码快照的Rust357 PASS、独立控件6 PASS/1个子进程入口ignored（其中4项为模块复用）、严格检查及构建证据继续有效，明确不作为本次新执行结果。已用743文件构建清单核对业务源码/测试未变，仅追加文档；内部EXE摘要仍为`C876E00220BA3DCAD4D96E30FA92AD40E1104A8863BDE85EB6953D8F5B7B478F`。

### 复核范围

| 文件组 | 复核重点 |
| --- | --- |
| `mousehook.rs`、`selection_runtime.rs`、`windows.rs`、`lifecycle.rs`、`lib.rs` | 目标/代次绑定、有限队列、专用worker、退出及前台事件钩子卸载；持续阻塞COM不会被强制取消或自动重建。 |
| `uia.rs`、`uia_strategy.rs` | 命中/焦点与有界父级路径，密码、PID、根窗口及最终复核；诊断不记录选区正文。 |
| `safe_copy.rs`、`copy_engine.rs`、`native_copy_proof.rs`、`selection.rs`、`capture.rs`、`release_policy.rs` | 独立原生授权、一次发送、有限文本快照、选区证明、持锁读取与恢复比较；保留来源与恢复事务边界。 |
| `popup.rs`、`settings.rs`、`ipc.rs`、`poc/settings.html` | 单次会话、设置页票据、失焦/超时/旧版本清理、备份恢复撤销授权及迟到界面状态。 |
| 4个Rust集成测试、`tests/safe-copy-ui.test.mjs`、`package.json` | 行为与安全拒绝断言、原生控件夹具边界、前端命令注册；错误传播测试不能冒充真实恢复无损。 |
| README及3份开发文档 | 实验分支说明、最终支持范围、历史证据与发布门禁一致性。 |

27文件均为UTF-8文本，无EXE/ZIP、测试运行日志或缓存；有限格式扫描未命中GitHub令牌、AWS访问键或私钥头。这不是对任意未知凭据的完整安全保证。文档相对文件链接已核对，未访问网络验证外部链接。

### 提交、推送与发布分别判断

- 可整理为本开发分支的普通研发提交，保留已知风险、默认关闭和NO-GO说明；不需要为保存研发进度宣称R01–R03已经关闭。核心源码与对应测试建议一并提交，避免拆出不能编译或缺少保护测试的中间版本。
- 本轮不暂存、不提交、不fetch或推送；远端最新状态未核验。获准后需明确暂存这27文件、检查完整暂存差异及空白、创建提交，再fetch确认远端没有冲突，仅普通推送开发分支；远端CI需单独验证。
- **合并main及发布仍NO-GO**：R01只覆盖一次“外部写入后跳过恢复”；R02只覆盖同owner不同文本，尚缺同内容因果与精确临界点；R03仍缺写入失败后的整体数据保全；工具条观察与隔离日志不一致尚未解释。VS Code默认auto兼容由用户决定暂缓，保留已知限制。

## 初始实现结论（历史记录）

已实现有界 UIA 选区读取、会话隔离和需要本机原生确认的新自动复制入口。自动失败保持静默，不弹出要求手动粘贴的提示，也不自动打开输入框或发送残留剪贴板内容。用户主动打开现有面板仍可正常输入。

自动化验证通过不等于真实 Windows 应用兼容性验收通过。目前不建议直接合并或发布 Beta.2。新回退默认关闭，仅本次进程授权有效；它降低已知风险，但不能保证任意并发情况下剪贴板绝不受损。

## 审查证据与实现

原链路缺少完整选区目标身份，UIA 同步等待影响事件处理，焦点元素单一路径限制兼容性，原文缓存不能仅凭文本前缀证明会话归属。本轮将这些责任拆为可模拟验证的状态、遍历策略与单次事务，保留既有业务入口。

| 范围 | 当前实现与证据 |
| --- | --- |
| 事件目标 | [mousehook.rs](../src-tauri/src/mousehook.rs) 捕获 HWND/PID/线程、释放位置、时间和代次；前台切换失效旧代次；新增全局前台事件钩子配对卸载 |
| 调度 | [selection_runtime.rs](../src-tauri/src/selection_runtime.rs) 单 worker、一个可替换待处理槽、独立关闭处理；控制通道有界为 16 |
| UIA | [uia.rs](../src-tauri/src/uia.rs)、[uia_strategy.rs](../src-tauri/src/uia_strategy.rs) 焦点、命中及限定父级；PID/根窗口/密码检查；只读实际选区，不以整篇文档替代 |
| 原文 | [selection.rs](../src-tauri/src/selection.rs)、[popup.rs](../src-tauri/src/popup.rs) 同一 Session 保存原文、归一化文本、来源及有效期；使用前复核；保留换行缩进和不同范围的相同文本 |
| 授权 | [safe_copy.rs](../src-tauri/src/safe_copy.rs) 本机原生风险确认默认取消，只允许可见且聚焦的本地设置页请求；并发提示去重；撤销使在途授权代次失效 |
| 入口 | [release_policy.rs](../src-tauri/src/release_policy.rs) 旧自动回退保持恒定禁用；新入口独立检查授权、目标和事务有效性 |
| 备份 | [ipc.rs](../src-tauri/src/ipc.rs) 成功恢复后撤销授权并使选区失效；授权不属于配置或备份字段，旧 true、环境变量或普通配置不能授予权限 |
| 界面 | [settings.html](../poc/settings.html) 更名“UIA 划词增强”；旧入口仍灰显；新开关显示本次运行授权与实验性风险；取消、错误、重复点击、迟到状态均处理 |

UIA 为专用 MTA 工作线程创建和释放 COM 对象，先释放接口再反初始化；未跨线程传递这些接口。UIA 预算 350ms，父级深度 5、候选最多 12、选区最多 32、文本最多 20,000 字符。请求有效期 1.5 秒，展示会话有效期 8 秒。预算属于协作检查，不是强制中断 COM 的承诺。监督线程使过期结果作废并清理缓存，阻塞时不无限增加线程。

日志仅新增状态、耗时等元数据，不新增选区正文、剪贴板正文或 AI 内容日志。

## 初始自动复制事务（后续变更见顶部最终摘要）

1. UIA 成功不复制；仅安全目标的空选区/不支持结果才有资格进入已授权回退。
2. 验证原前台窗口、进程存活、线程、代次、有效期及聚焦控件；当前仅接受可核对的原生 Edit/RichEdit 非密码控件，拒绝终端与未知虚拟控件。
3. 仅空剪贴板或完整已知文本格式集合可快照。接受格式 1/7/13/16，非空时必须有 Unicode 文本，最多 4 格式、总量 1MiB；复制前预分配恢复内存。图片、HTML/RTF、文件、自定义或不稳定格式直接拒绝，不实施复制。
4. 单事务锁；最多发送一次 Ctrl+C，检查 SendInput 结果；复制等待上限 350ms，不盲重试。修饰键冲突时拒绝。
5. 核对 owner 的根窗口、PID、线程、序列号和目标上下文；读前后持剪贴板锁复核。可观察的外部写入、未知来源或失效上下文均不采纳结果、不强行恢复。
6. 恢复前持同一剪贴板锁核对精确 stamp，再转移已准备的已知格式。恢复报错不会返回成功。可信但空的复制结果先尝试条件恢复，然后拒绝作为选区。

[copy_engine.rs](../src-tauri/src/copy_engine.rs) 是原生适配器和合成测试共用的顺序控制；[capture.rs](../src-tauri/src/capture.rs) 仅向新入口提供已有的有界 Unicode 读取 helper。旧核心竞争处理和旧禁用策略未改为开放。

授权只保存在进程内存中，重启需重新确认；不新增存储系统或授权配置迁移。原生确认不能防御具有同等权限、能操作设置或注入程序的本机恶意软件。

## 初始修改范围（后续已扩展为27文件）

修改 12 个已有文件：根 `README.md`；应用 `package.json`、`poc/settings.html`；Rust `capture.rs`、`ipc.rs`、`lib.rs`、`lifecycle.rs`、`mousehook.rs`、`popup.rs`、`release_policy.rs`、`uia.rs`、`windows.rs`。

新增 11 个文件：Rust `selection.rs`、`selection_runtime.rs`、`uia_strategy.rs`、`copy_engine.rs`、`safe_copy.rs`；Rust 集成测试 `selection_core.rs`、`copy_engine.rs`、`uia_strategy.rs`；前端 `tests/safe-copy-ui.test.mjs`；本报告和 `uia-beta2-plan.md`。

无依赖、锁文件、图标、AI 发送回读、截图/OCR 或发布产物修改。原私有仓库仅作只读参考。原公开根快照清单仍描述 Beta.1 冻结内容，未将其伪装为本开发工作区的新摘要。

## 自动化结果

本次在 Windows 使用 Node 24.22.1、pnpm 9.15.9、Rust 1.98.1。命令从应用目录运行。日志保存在系统临时目录，未加入仓库。

| 实际命令 | 结果 |
| --- | --- |
| `pnpm install --frozen-lockfile` | 通过，锁文件未变 |
| `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib --test selection_core --test copy_engine --test uia_strategy` | 309 PASS：库 276、选区状态 11、复制事务 12、UIA 策略 10；0 FAIL |
| `pnpm test:unit` | 370 PASS，0 FAIL，含新设置授权测试 7 项 |
| `pnpm build` | 通过 |
| `pnpm check` | 通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 通过 |
| `cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | 通过 |
| `cargo check --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol` | 通过，仅生产特性编译检查，不是发布 EXE 构建或原生验收 |
| `git diff --check` | 通过 |

测试先暴露缺失的状态/事务实现，再验证修复；可信空结果未恢复快照的断言曾失败，调整恢复与拒绝顺序后通过。旧禁用文案断言曾失败，恢复准确文案而未削弱断言。前端 dist 过期检查曾失败，先构建后检查通过，未关闭校验。

新增测试覆盖焦点/命中/父级选择、密码与跨窗口拒绝、超时后结果失效、队列上限、原文保真、授权撤销与迟到响应、格式拒绝、单次复制、外部 owner/读写竞争及恢复报错。UIA 策略和复制事务使用合成 Provider/Fake；不把模拟结果当作真实 COM、SendInput 或剪贴板原生测试。COM 生命周期主要通过代码复核和编译检查确认，未进行原生阻塞注入。

Playwright 本轮未运行；未启动新 EXE、未调用真实剪贴板读写、未执行原生 GUI、六站或截图/OCR 测试。当前自动化无未解决失败，但以下原生风险仍未验收。

## 兼容性差异与未验证事项

| 对照 | 静态机制差异，不代表实测成功率 |
| --- | --- |
| 私有参考旧逻辑 | 自动复制思路存在，但不能继承其兼容率或旧验收结果；未运行私有程序 |
| 公开 Beta.1 | 旧复制回退固定禁用；既有 UIA 读取范围有限 |
| 本开发分支 | UIA 有界扩大候选；新回退显式授权、仅本次运行且控件/格式限制更严格；UIA 失败保持静默 |

Chrome、Edge、VS Code、Word、PDF、普通 Windows 编辑框、富文本编辑器：本轮均为 **NOT_TESTED**。未测私有/公开/新分支的同环境对比、成功率、平均/P95 延迟、CPU 或句柄增长。不能承诺达到私有版兼容率。严格的 owner/控件检查会拒绝一部分 broker、虚拟编辑器及富文本应用，属于已知功能降级。

## 残余风险与合并门禁

- **P1 / 未验证**：同进程、同线程、同根窗口的其他写入者可能满足同一来源检查；owner/PID/序列组合不是复制来源的绝对证明。跨进程 E01 目前只有共享算法的模拟回归通过，新原生路径未复测。
- **P1 / API 非事务性**：多个格式的恢复不是原子操作。`EmptyClipboard` 成功后某次 `SetClipboardData` 失败可能留下空或部分剪贴板；预分配只能减少、不能消除失败。必须在明确风险授权下进行定向验证，不能宣称不会丢失数据。
- **P2 / 未验证**：COM 和延迟渲染原生调用可能超出预算且无法强制中断；迟到返回会被拒绝，但工作线程可能持续不可用。此时不弹粘贴提示，不增加替代线程，用户仍可自行使用现有面板。
- **P2 / 未验证**：原生风险对话框、焦点身份、撤销时机、设置页调用边界、真实恢复 API 的失败行为和工具条主动复制需要合成环境验收；模拟测试不能替代。

最小下一阶段：另获授权后，在合成桌面验证本机授权/拒绝/撤销/重启/备份恢复不授予；UIA 与工具条主动复制；原生 Edit/RichEdit 单次复制、未知格式拒绝、目标切换与 E01 跨进程竞争；再执行必要应用小样本矩阵。对恢复失败与来源不确定风险作明确发布决策，不能仅凭单元测试放行。

当前状态：实现完成、自动化通过、原生兼容性与剪贴板风险未验证，**Beta.2 合并/发布暂不建议**。本轮无 commit、push、Tag、Release、EXE/ZIP 发布或私有仓库修改。


## 2026-10-24 原生失败后的局部修复

本节为后续修复记录，不覆盖上面的初次自动化记录或既有原生 FAIL。此前原生批次中 Chrome、Edge、VS Code、PDF、标准 Win32 等无工具条，不能沿用初次自动化结果宣布兼容。

### 已确认根因与修改

- `uia.rs::checked` 原先只检查祖先 depth0..6。只读合成 Chrome 元数据中，合法 Text/Document 节点到 depth8 才有目标 HWND；因此同 PID、非密码节点也被误判为目标变化。这证明特定真实树结构的误拒绝，不证明此前每次失败均属同因。Edge/VS Code 的同因尚待新 EXE 复测。
- 将窗口归属判断提取到 `uia_strategy.rs::verify_ownership`，生产 COM 适配器与测试共用。归属链独立限制为 depth0..32；文本候选搜索仍为原有最多12节点、父级最多5层。逐节点检查有效期、PID、密码属性、精确根 HWND/PID，身份比较失败、循环和超过上限均拒绝。保留350ms协作预算；同步 COM 调用本身仍无法强制中断。
- `read_with` 原先焦点获取或某个候选模式读取错误会终止所有路径。现在必须先验证鼠标命中目标；仅焦点获取不可用允许使用已验证的命中节点，已验证候选的模式读取内部错误允许继续其他候选。密码、跨窗口、目标变化和超时仍全局拒绝；无法验证命中节点时仍保守失败。不会无条件忽略所有校验错误。
- 返回文本前再次验证候选及命中目标。若所有路径无文本且曾出现模式读取错误，`safe_target=false`，不因该错误启动自动复制。
- 增加元数据状态：`UIA_ELEMENT_UNAVAILABLE`、`UIA_WINDOW_UNVERIFIED`、`UIA_ANCESTRY_LIMIT`、`UIA_ANCESTRY_CYCLE`；不记录 Name、Value、选区或剪贴板正文。

本次相对于开始修改时，仅改变 `src-tauri/src/uia.rs`、`src-tauri/src/uia_strategy.rs`、`src-tauri/tests/uia_strategy.rs` 和本报告。未改剪贴板事务、授权、允许控件范围、UI 或发送回读逻辑。之前开发工作区的其他修改保留。

### 回归与本次真实验证

先增加候选回归，旧实现10 PASS/3 FAIL；提取归属算法但保持原6层限制后11 PASS/5 FAIL，其中第8层合法根返回 `UIA_ANCESTRY_LIMIT`。修复后16 PASS，再加入安全边界回归后 UIA 专项23 PASS。相比原10项新增13项，覆盖第8层、32/33层边界、循环、密码祖先、错误根/PID、身份比较失败、遍历中请求变化、焦点不可用、模式读取错误和不授予自动复制资格。原跨窗口、密码和迟到结果断言保留。

以下命令从应用目录实际重新执行，全部退出码0：

| 命令 | 本次结果 |
| --- | --- |
| `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib --test selection_core --test copy_engine --test uia_strategy` | 322 PASS，0 FAIL：276库、11状态、12复制、23 UIA |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | PASS |
| `cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS |
| `cargo check --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol` | PASS，生产特性检查 |
| `pnpm test:unit` | 370 PASS，0 FAIL |
| `pnpm build` | PASS |
| `pnpm check` | PASS |

执行环境：Windows x64，Rust/Cargo1.98.1、Node24.22.1、pnpm9.15.9。详细日志保存在本地临时证据目录，不进入仓库。不把共享算法模拟测试等同于 Windows 原生验收。

### 未解决与定向复测边界

- 标准 Win32 合成 Edit 实测没有向本客户端提供 TextPattern，虽然 ValuePattern 可用；本修复不读取整框内容冒充选区，纯 UIA 无工具条仍可能存在。已授权的原生 Edit/RichEdit 回退是另一条受限路径，未放宽至浏览器/终端。
- 新代码尚未经过 Chrome/Edge/VS Code 拖选原生复测；应使用新隔离 EXE、合成资料，先关回退核对 UIA，再核对密码/跨窗口/过期拒绝。不要将旧 EXE 的任何 PASS 套用到新 EXE。
- 先前 C04 图片夹具 Clipboard busy、首次助手异常退出原因未定，以及未执行的 R01–R03 均保留。此次没有重测系统剪贴板、没有启动 GUI/全局钩子、没有截图/OCR/六站测试。

Beta.2 仍为 NO-GO，不能仅凭本次自动化宣布可合并或发布。本次不提交、不推送、不修改原私有仓库。


## 2026-10-25 候选公平调度与继续审核

本节接续10月24日修复及原生复测，保留此前所有FAIL。本次新修改仅涉及 uia.rs、uia_strategy.rs、selection_runtime.rs、tests/uia_strategy.rs 和本报告，未改剪贴板核心、UI、图标、AI发送/回读或旧发布产物。

### 已修复

1. **P1：父级分支抢先阻断有效命中候选。** 原循环处理focus后立即进入其parent并进行归属验证，之后才读hit。真实策略的纯模型证明：focus父级慢350虚拟ms或返回局部不可用时，已有有效文本的hit读取0次。现改为逐层先读整个候选frontier，再扩展父级，初始focus/hit不会被父级查询抢先消耗预算。局部未验证父级直接放弃，不读其内容；其他分支仍需独立证明归属。密码、目标变化、超时等硬失败仍整体拒绝。
2. **P1风险：同一节点重复完整归属验证。** focus/hit相同时原成功路径5次完整validate。现在复用同一次请求中精确节点的归属证明，读取前调用原生适配器与测试共用的recheck_ownership，实时核对请求、PID、密码属性及可取得的本机根句柄；返回文本前仍完整verify_ownership，若候选与hit不同还单独完整复核hit。相同节点正常路径减为2次完整归属验证加1次实时属性复核。不跨请求缓存，不删除最终保护，不修改350ms预算。共享ControlViewWalker，避免每次validate重新创建walker。
3. **P2：内部读取错误被后续空选区状态覆盖。** 保留候选错误优先级，所有候选无文本且有局部提供者错误时，返回该错误且safe_target=false，不能误报空选区或授权自动复制。
4. **P2：无法将日志关联到目标和阶段。** 增加请求代次、target_pid、控制/worker排队耗时和UIA阶段统计。单次trace在事务结束后输出，不在每次COM属性读取时写日志。calls与stage_ms数组顺序为focus、hit、validate、recheck、parent、pattern；还包含最后阶段、错误、候选角色及深度。pre_strategy_ms是请求创建至策略开始的总前置时间，包含COM初始化等，不冒充纯排队耗时；runtime的queue_ms单独记录。无Name、Value、选区、剪贴板、真实标题或绝对用户路径日志。

### 本次真实自动化

先补6个回归，旧实现23 PASS/6 FAIL；修复后29 PASS，再补6个保护回归后UIA专项35 PASS。合成预算不是Windows真实耗时。新增保护覆盖初次验证后密码/PID/根句柄/请求变化、读取后密码改变必须由最终完整校验拒绝、受保护父级不得作为可跳过的局部错误。

| 实际命令（应用目录） | 本次结果 |
| --- | --- |
| `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib --test selection_core --test copy_engine --test uia_strategy` | 334 PASS：276库、11状态、12复制、35 UIA；0 FAIL |
| `pnpm test:unit` | 370 PASS，0 FAIL |
| `pnpm build`、`pnpm check` | PASS |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | PASS |
| `cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS |
| `cargo check --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol` | PASS |

日志留在本地TEMP，不加入仓库。本轮没有重新执行原生GUI、系统剪贴板或Playwright；不把自动化当作原生通过。原生通过和失败来自上一测试EXE，其SHA256为F5BC3F3555BDA94AF427D5BC4D9357611DEAEA8D02B0882449B997CC72BE317F，不能套用到本次修改后的新产物。

### 新增明确问题与未解决风险

- **P1 / 代码确认，尚未修复：设置页“运行提示词”无法保留本次选区。** popup.rs::on_action的open-settings分支注释声称保留pending text，但实际调用hide→dismiss→clear_pending_text，将其清空；run_prompt依赖take_pending_text，必然拿不到该记录。即使仅删除clear，Session::usable也要求当前前台仍为原目标，设置窗口属于本进程会导致target=None，所以不能用简单保留字符串或取消跨窗口检查修复。应另做明确用户动作触发的有界选区移交，保留授权、过期和清理规则。本轮不为修复此功能而放宽会话保护。
- **P2 / 静态风险，未原生复现：迟到的前台事件可能使新请求失效。** mousehook::foreground_changed忽略事件目标/时间，无条件invalidate。需要明确事件时序的定向复现；不能取消前台失效机制掩盖它。
- **P2 / 代码行为确认、是否实际触发待确认：控制通道满时静默丢事件。** mousehook::emit忽略try_send错误，满16条时可能丢Selection/Dismiss；新mousedown的代次保护仍可阻止旧内容动作，但用户看不到原因。若补计数，应在hook中只用原子计数、在worker中汇总，不能在低级钩子里写日志。
- **P2 / 静态竞态，未原生复现：会话通过校验到实际窗口显示间仍可变更前台。** accept_session先校验再存储并show，最终动作仍会重新检查会话，因此不能推断误发；可能短暂显示过期工具条。后续需以主线程时序证据决定是否修复。
- **P1 / 仍待原生定位：VS Code无工具条。** 已修复确认的调度/重复工作，但此前失败没有阶段与目标绑定，不能宣称本次已解决VS Code提供者兼容。应使用新EXE在合成文本中只复现此项，根据新增trace区分取元素、归属、模式或超时。
- **PDF、R01–R03和恢复失败等原风险保留。** PDF两条45/46字符不匹配及不能确认末尾句号选中记录未变；R01–R03未执行。同步COM仍可能阻塞且无法硬中断，单worker有界退化保留。

下一步优先：新EXE的VS Code定向复测与必要否定用例，然后单独设计设置页选区移交；不重复全部人工矩阵、不扩大自动复制白名单、不要求用户手动粘贴来掩盖UIA失败。Beta.2保持NO-GO；本轮不提交、推送、合并或发布。


## 2026-10-25 问题真实性复核与设置移交修复

本节接续前一节，不删改此前“尚未修复”的审查历史与原生FAIL。

### 真伪与证据分级

| 前次问题 | 本次核定 | 处置 |
| --- | --- | --- |
| 从工具条进入设置后无法运行原选区 | **代码确认的功能缺陷**。原open-settings→hide→dismiss清空pending；原run_prompt只取pending；普通Session对设置前台也应拒绝 | 已按独立、显式、一次性移交修复；未放宽普通Session跨窗规则 |
| 满队列静默丢事件 | **确定的队列行为，非已复现的日常用户故障**。同型sync_channel16机制实验确认第17次try_send为Full，emit忽略该错误 | 保留有界、不阻塞钩子的设计；不贸然改为无界或阻塞发送。是否应采用latest覆盖/丢弃计数须针对负载取证 |
| 迟到前台事件使新请求失效 | **静态风险、未原生复现**；不是确认发生的故障 | 保持待确认，不取消foreground invalidate保护 |
| 校验后再显示时前台变化 | **静态时序窗口、未原生复现**；最终动作仍有会话校验，不能推定误发 | 保持待确认，不声称已解决 |

前一轮真实策略模型已复现父级阻断和重复校验预算问题，并有回归修复；它们是真实控制流问题，但仍不是VS Code某次失败的唯一根因证明。VS Code、PDF以及R01–R03原门禁不因本节自动化通过而放行。

### 最小修复机制

- 只在工具条显式open-settings（或更多菜单无溢出时的设置动作）中消费当时有效Session。普通托盘/菜单打开设置不自动继承旧选区。
- 新增纯内存SettingsSelection；移交前继续要求原窗口/PID/线程、代次、原会话期限有效，拒绝空、过短、超过20000字符和无效目标窗口。之后绑定设置窗口HWND，有效60秒，一次性消费。原始Unicode、Markdown、换行和缩进不做规范化替换；页面预览仍最多800字符。
- 普通工具条Session::usable保持原窗口和代次检查，不把“设置窗口也算原目标”作为修复。
- settings_run使用Tauri注入的真实调用窗口，复用现有可信本地settings URL/可见/聚焦验证，并核对精确快照凭据。其他窗口、缺失/过期/旧凭据不能运行，也不能消费新快照。安全性不依赖前端禁用按钮。
- 前端按单调版本忽略迟到旧预览、旧撤销；运行请求携带所见快照凭据。避免页面显示旧内容而后端隐式使用最新内容。
- 关闭/销毁、已聚焦后的失焦、普通重新打开、新有效选区、备份恢复、退出都会撤销；60秒清理由既有supervisor复用完成，不另建线程。撤销/消费/过期清除预览并禁用运行；保存未完成时仍保留原禁用规则。
- 一次性快照和凭据没有serde持久化，不进入配置、收藏、备份或日志；重启重新初始化为空。未改剪贴板读取/恢复、自动回退允许范围、AI站点发送/回读策略。

### 回归与真实测试结果

先添加新Rust状态API的测试，初次因缺少SettingsSelection编译失败；这是“新API缺失”的RED，**不是旧原生流程断言失败**。旧功能缺陷本身以真实调用链确认。普通Session跨窗口拒绝的原规则保留并新增回归。

前端真实DOM测试先出现7 PASS/2 FAIL（缺少清理入口/无选区仍可运行）；中间曾8 PASS/1 FAIL，定位到renderList中的保存状态覆盖了禁用条件，已合并规则。加入精确快照隔离测试后曾10 PASS/2 FAIL（旧预览覆盖、新请求无凭据），修复后该专项12 PASS。失败日志保留，不删除断言。

最终在Windows重新执行：

| 命令（应用目录） | 结果 |
| --- | --- |
| `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib --test selection_core --test copy_engine --test uia_strategy` | 343 PASS：库276、状态20、复制12、UIA35；0 FAIL |
| `pnpm test:unit` | 375 PASS，0 FAIL |
| `pnpm build`、`pnpm check` | PASS |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | PASS |
| `cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS |
| `cargo check --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol` | PASS |

新增Rust9项覆盖普通会话不得跨窗、移交保真、无效来源拒绝、目标绑定/超时、清理/替换、空/超量数据拒绝、旧凭据不消费新数据、撤销版本不复用；前端新增5项覆盖失效按钮、普通打开不继承、迟到预览/撤销隔离、请求携带精确凭据、恢复清理。

这不是Windows GUI原生验收。本轮未启动助手、未访问系统剪贴板、未执行六站/截图/OCR/竞争测试，也未重复整套人工矩阵。后续只需独立配置验证“划词→工具条设置→预览→运行一次”、关闭/失焦/过期撤销及必要否定用例，再用带阶段日志的新EXE定位VS Code。

### 修改边界

实际修改10个已有开发文件：src-tauri/src/selection.rs、settings.rs、popup.rs、ipc.rs、selection_runtime.rs；safe_copy.rs仅将现有trusted检查改为crate内可复用可见性；poc/settings.html；src-tauri/tests/selection_core.rs；tests/safe-copy-ui.test.mjs；本报告。

删除不再有调用方的peek_pending_text，仅替换设置运行获取选区的入口；提示词拼接与发送/回读机制不改。无依赖、锁文件、图标、历史EXE/HASH或用户数据格式变更。原私有仓库保持只读，未commit/push/merge/release。Beta.2仍NO-GO，原生交互和原有风险待验。


## 2026-10-26 VS Code 归属拒绝细分与隔离 A/B 对照

### 本轮结论

上一轮 PID31576 的5次首次 validate 返回 UIA_TARGET_CHANGED 是真实记录，但无法据此断定进程校验过严。本轮独立 VS Code PID35580 没有复现统一首次拒绝：进程和窗口根均验证通过，auto 模式日志出现模式不支持、空选区、350ms超时，另有一次代次变化导致请求取消（前台仍匹配）。这些是本轮测试时间段内的请求元数据，不等同于用户精确执行次数或成功率。

仅将隔离 VS Code 的 editor.accessibilitySupport 从默认 auto 改为 on 后，用户确认工具条出现；请求78对应 UIA_SUCCESS / SESSION_READY，185ms，进程和窗口根仍严格匹配。该对照支持提供者可访问模式影响选区暴露；没有证明所有 auto 失败均由同一原因造成。未执行正文精确复制比对，因此本项只记工具条出现 PASS。原默认模式 FAIL 保留，默认兼容性未修复，Beta.2 仍 NO-GO。

### 实际最小修改

- uia_strategy.rs：增加只读拒绝观察接口，区分 request-invalid / deadline / process-mismatch / root-mismatch / protected-control，并记录失败深度；返回状态、接受条件和失败关闭语义不变。
- uia.rs：记录上述原因、观察PID、代次/前台与窗口根匹配布尔值；不记录窗口标题、Name、Value、正文、剪贴板内容或用户路径。
- tests/uia_strategy.rs：新增5项诊断与失败关闭回归，包含祖先深度和最终复核失效；旧保护测试保持不变。
- 本报告：如实记录新旧失败、A/B范围、源码与诊断EXE差异及未关闭问题。

### 证据与验证

新增诊断测试先出现2项预期断言失败（无拒绝原因），实现后38项通过；复核中另增加2项边界测试，先捕获诊断深度/末次漏记，再修正后40项通过。最终同一源码重跑 Rust：276库测试、12 copy_engine、20 selection_core、40 uia_strategy，共348 PASS。严格Clippy、格式、custom-protocol release check通过。前端build/check本轮通过；未改前端，未重复运行前端全量单测。不执行六站、截图/OCR或剪贴板竞争测试。

本轮原生诊断EXE SHA256为 CEDD7CD8CA2AD9E8856AC87A0153FD63483A0604504207BC4F6F57A6E290C4B1，冻结740文件源码快照，清单摘要7f3ee318427d8157054c0b1b8b81906a78015718f971e8a4466e6545d9af2133。后续仅修正了诊断深度/末次拒绝记录及测试、补充报告；未改变安全条件。该EXE因此不等同于最终文档和诊断修正后的完整工作区，不能冒称最终工作区原生验收全部通过。

诊断助手PID38904正常退出（退出码0），独立VSCode已退出；隔离配置恢复原字节摘要4d8722b47b9704969ac18e5d674b3b737f5b629f71a5491f520004f898804efe。日常VSCode配置、原私有仓库均未修改。未提交、推送或发布。

### 保留的问题和后续最小方案

1. auto模式默认兼容仍FAIL；不能以静默修改用户编辑器配置、允许任意子进程、整框Value读取或扩大自动Ctrl+C范围规避。
2. 上一轮首次归属拒绝未在本轮复现具体原因，保留待取证；诊断已能区分后续实际失败分支。
3. 本轮代次变化时的安全取消是已观察行为，未证明其由迟到前台事件错误触发，不擅自删除代次隔离。
4. 若继续实现无感兼容，应单独验证VSCode提供者能否通过官方UIA/MSAA接口在保持精确选区和归属约束时暴露必要模式；在取得证据前不实施猜测性读取或全局屏幕阅读器设置。
5. PDF、R01–R03及完整Beta验收原有阻断保持不变。

官方模式说明：https://code.visualstudio.com/docs/configure/accessibility/accessibility 。这是模式语义证据，不替代本机原生测试。原始元数据、红绿日志、构建记录与A/B证据仅保留本地TEMP/uia-beta2-ownership-20261026，不提交测试产物。


## 2026-10-26 R01–R03 定向复核与研究（未放行）

用户暂缓VS Code默认兼容，转为处理R01–R03。当前复制引擎专项重新运行12 PASS，格式与diff检查通过；另直接引用生产纯策略/引擎的安全复现为2 PASS、2 FAIL，同根另一个owner和完全相同owner的竞争文本可被采纳。原生独立夹具两次遭遇Clipboard busy，第二次锁持有者为RadeonSoftware.exe；未进入竞争断言，不能记为R01原生PASS。原产品EXE未启动。

用户明确选择保留实验性回退、继续研发来源证明与无损恢复，Beta.2保持阻断。本地抛弃型原型6项通过：包含自建原生Edit选区读取、密码框拒绝、复制前后文本比对和内存恢复模型；这些研究通过不是产品端到端验收。相同内容竞争来源及多格式恢复的整体无损仍未证明；未修改生产代码。

详细证据、适配器边界、历史失败与下一步条件见[R01–R03定向报告](uia-beta2-r123-validation.md)。R01原生BLOCKED、R02策略复现FAIL、R03数据保全BLOCKED，旧失败/未验收记录保留。未提交、推送或发布。


### R01–R03 后续代码收敛（同日）

后续已接入标准Unicode Edit选区证明，R02两个不同竞争文本的断言由12 PASS/2 FAIL转为17项复制测试全通过。库/状态/UIA/复制专项本次共357 PASS；独立控件跨进程与阻塞测试6 PASS（含4项模块复用）、1个仅供父测试启动的入口ignored；前端相关12 PASS。严格Clippy、格式、build/check与生产Release构建通过。新EXE SHA256：C876E00220BA3DCAD4D96E30FA92AD40E1104A8863BDE85EB6953D8F5B7B478F，未启动验收。

R01重试仍被Radeon剪贴板占用阻塞；R02相同内容来源和精确临界点、R03多格式恢复无损仍未解决。RichEdit/ANSI等未获证明的控件安全拒绝自动复制，UIA与主动复制保留。详见[R01–R03报告后续执行节](uia-beta2-r123-validation.md)。不覆盖前节历史失败与研究记录，Beta.2仍NO-GO。


### 新EXE原生定向验收结果（2026-10-26，后续执行）

同一SHA256为C876E00220BA3DCAD4D96E30FA92AD40E1104A8863BDE85EB6953D8F5B7B478F的EXE现已完成合成资料定向测试：WPF精确UIA/主动复制、标准Edit授权回退恢复、Edit捕获内容精确检查三项PASS；R02同进程同owner不同文本竞争一次有效注入后被COPY_SELECTION_MISMATCH拒绝，新文本保留，记局部PASS。先前注入BLOCKED和后续独立请求的工具条记录均保留，不混算。

R01夹具时序无效，BLOCKED；R02同内容来源/精确临界点、R03整体无损恢复仍BLOCKED。授权取消未单独确认，本轮R03故障注入未执行。用户正常退出后助手退出码0，主进程、12个已记录WebView2进程与夹具无残留。没有追加生产修改或重复全量测试。详见[R01–R03原生收尾](uia-beta2-r123-validation.md)。Beta.2仍NO-GO，未提交、推送、合并或发布。


### R01异步夹具后续验收（2026-10-26）

V2夹具静态检查和原生自检通过后，使用同一内部EXE、新隔离配置完成一次产品测试：跨进程竞争写入序列1047后，助手request91返回COPY_RESTORE_SKIPPED，竞争文本保留。仅“外部写入后跳过恢复”场景局部PASS，不代表R01所有时序覆盖。用户报告工具条出现，而隔离日志没有SESSION_READY或popup show，保留为待核对差异，不判定误抓或通过。助手退出码0，12个已记录WebView2进程及夹具父子进程无残留。生产代码未变、旧BLOCKED/FAIL保留。R02剩余边界、R03仍未关闭，Beta.2继续NO-GO。详见[R01–R03定向报告](uia-beta2-r123-validation.md)。未提交、推送或发布。
