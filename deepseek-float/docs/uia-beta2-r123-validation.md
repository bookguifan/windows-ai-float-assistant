# Beta.2 R01–R03 定向审查与受控测试（2026-10-26）

## 范围与基线

- 用户暂缓 VS Code 默认 auto 模式兼容，转为处理 R01–R03，并确认可使用合成剪贴板资料。
- 公开开发副本分支 feature/uia-safe-clipboard-fallback，HEAD 9862487eb12b18a9bd8c85fc12e8299edf23b9ee。开始时740个源码文件，存在既有Beta.2未提交修改，暂存区为空。
- 原私有仓库只读；不提交、推送、合并或发布，不进行截图/OCR、六站或VM测试。
- 本轮尚未修改生产代码、启动新助手或构建新产品EXE。独立native_probe.exe不是产品EXE。

## 结论（不能视为三项已关闭）

|项目|状态|证据与边界|
|---|---|---|
|R01 跨进程竞争|BLOCKED（原生）；部分状态规则PASS|直接引用生产引擎的foreign-PID拒绝测试通过。两次Win32独立夹具均在模拟复制阶段遇Clipboard busy，未进入三个竞争断言。第二次锁占用元数据为RadeonSoftware.exe PID15960；未终止或更改该进程。没有产品EXE原生PASS。|
|R02 同进程竞争|FAIL（生产纯策略/引擎复现）；原生NOT_TESTED|同根另一个子窗口或相同owner/PID/thread、不同序列号，可被source_allowed接受。复现适配器返回合成竞争文本，run_copy随后恢复快照并返回该文本，两项要求拒绝的断言真实失败。适配器指定了竞争时序；未证明某一日常应用已发生该问题。|
|R03 恢复失败|状态错误传播PASS；数据完整性BLOCKED|引擎收到restore错误后不返回文本。但safe_copy.rs先EmptyClipboard再逐格式SetClipboardData，中途失败时旧数据已清空或部分恢复。现有测试只让Fake.restore返回错误，没有验证真实内容保全；本轮尚未执行真实产品故障注入。|

## 测试结果

- 当前开发副本：cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --test copy_engine：12 PASS / 0 FAIL。
- cargo fmt --manifest-path src-tauri/Cargo.toml --check、git diff --check：退出码0。
- TEMP/policy_repro.rs（直接path引用当前selection.rs、copy_engine.rs）：2 PASS / 2 FAIL。失败为R02两项安全要求，保留policy-red.log，不删除断言或将失败改写为通过。
- TEMP/native_probe.rs：rustc独立编译通过。首轮锁占用导致panic，清理路径还有二次断言；修正夹具清理后第二轮仍Clipboard busy，0 PASS / 1测试失败，实际验收分类BLOCKED。未进入跨进程写入断言、未执行R02或R03原生组。首轮终端输出已说明异常；r01-attempt2.log保留第二轮原始输出。
- native_probe使用真实Win32剪贴板API和当前纯引擎，但Win32适配器是局部测试实现，没有产品的授权/焦点/SendInput路径，不可作为产品端到端验收。夹具中的restore故障注入用例尚未运行。

## 源码证据与安全边界

1. selection.rs:198 source_allowed检查根窗口、owner非空、PID、线程和序列变化，没有验证实际复制因果关系。即使收紧为精确owner，也无法区分完全相同owner的并发写入；PID/线程/窗口/sequence组合不是来源证明。
2. safe_copy.rs:334 read在持锁时前后比较stamp；347 restore在同一OpenClipboard锁内比较后清空。这能拒绝已观察到的不同stamp，防止外部程序在比较和清空之间正常OpenClipboard写入，但不能解决在首次采样前已经形成“看似合法”的竞争stamp。
3. safe_copy.rs:362–367的多次SetClipboardData没有整体回滚事务。预分配HGLOBAL降低分配失败窗口，但不能保证系统写入永不失败。
4. copy_engine.rs:15错误传播是必要条件，不足以证明未抓错内容或未损失原数据。旧12项全绿不否定本次缺口。
5. 本轮并未复现产品误发送；纯引擎可能返回竞争文本属于潜在错误抓词风险，不把它扩大表述为已发生真实资料泄露。

## 最小处置选项

- 安全封闭方案：在集中后端能力门禁阻止自动Ctrl+C路径，UIA优先和用户主动复制保留，设置/备份/直接IPC不能绕过。此方案能隔离该危险路径，但属于自动回退的功能降级，不能称作“保留自动回退并修复全部竞争”。应由用户选择，不能悄悄降级或以默认关闭代替后端封闭。
- 保留自动回退方案：需要新增可独立验证的选区来源证据，以及可恢复的数据保全设计。仅增加固定延迟、更多PID判断、无界重试或延长锁时间均不能解决；本轮不做猜测性大改。可另评估严格限定原生Edit/RichEdit的非剪贴板选区读取，但不是本轮已完成的修复。
- 单独收紧同根不同owner只能修复R02一个子场景；同owner歧义与R03仍阻断。不能将这种局部修改作为R01–R03整体关闭。

## 后续原生验收

等待用户自行解除Radeon面板剪贴板占用，再进行受控R01时序验证；不结束日常进程、不无限重试。产品级验收需最新冻结源码的独立EXE、合成原生Edit/RichEdit夹具、真实授权及请求有效性检查。R02一旦出现错误抓词或覆盖即停止本组。R03需明确区分外部占用导致拒绝、恢复前失败、首个/中间格式写入失败；不能用测试适配器故意返回错误当作真实产品无损恢复PASS。

原历史FAIL、原有BLOCKED/NOT_TESTED均保留。Beta.2仍NO-GO。

## 用户选择与第二步研究结果

用户明确选择“保留实验性回退，继续研发来源证明与无损恢复；Beta.2保持阻断”，因此本轮不封闭或删除现有实验性能力。

按照brainstorming技能的可行性探索路径，本次source_proof_spike.rs仅为TEMP抛弃型原型，不接入产品，输出是可行性结论：6 PASS / 0 FAIL（source-proof-spike.log）。

- 两项原生API探针只操作夹具自己创建的隐藏Edit窗口：EM_GETSEL / WM_GETTEXTLENGTH / WM_GETTEXT，通过SendMessageTimeout限时读取；Emoji、中文、CRLF、缩进精确保真；密码框和空选区拒绝。没有读取其他窗口或系统剪贴板。
- 三项引擎适配器探针：复制前绑定文本与复制后不同则在restore前拒绝；相同Unicode文本保真；**相同内容竞争仍不可区分**。最后一项的PASS表示正确展示这一局限，不是R02安全通过。
- 一项内存模型：保留所有原字节，模拟写入失败后仍可用；不同事务stamp禁止重试。它没有调用产品restore，没有验证原生分配/写入失败，也没有实现隐私清理、寿命限制、重启保全或UI恢复，因此不是R03无损恢复通过。

### 后续可落地范围与未解决条件

来源证明候选需要绑定精确focus HWND、PID/线程、根窗口、请求代次、grant epoch；复制前后读取同一控件的范围与正文快照，确认稳定并逐UTF-16/Unicode校验，复制后文本不符则拒绝读取结果与恢复。只对已验证的标准Edit接口评估；RichEdit位置、CRLF语义、跨进程消息编组、长文本、阻塞与控件销毁须另证，不扩大生产允许范围。探针在同进程自建窗口通过，不能代替跨进程和真实SendInput验收。

此候选能减少错误文本采纳，但仍不能消除“另一个写入者写入完全相同文本/格式”的因果歧义。若最终要求不覆盖任何并发操作，必须考虑不借用系统剪贴板的选区读取路径，或取得目标进程协作；不能声称owner与内容比对已充分证明来源。

恢复保全候选需要在首次清空前保留独立可恢复字节、分配好所有句柄，失败时保留未转移资源并只在明确属于本事务的剪贴板版本下有限重试；检测到外部写入不重试。还须明确内存敏感数据寿命与退出清理，不写日志、不静默写磁盘。Windows剪贴板没有多格式事务；程序崩溃或系统持续拒绝写入时，单纯内存副本不能保证恢复。需要先明确产品承诺和风险边界，不能先将风险写成已修复。

本轮不做不充分的生产补丁，不将6项研究结果并入原12项产品测试或原生验收PASS总数。AMD占用未解除确认前不继续真实剪贴板组；VS Code默认兼容继续暂缓。

## 官方接口依据

- [OpenClipboard](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openclipboard)：打开期间阻止其他应用修改；已被别的窗口打开时失败。
- [GetClipboardOwner](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardowner)：owner由EmptyClipboard建立，不能单独当作复制来源证明。
- [SetClipboardData](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setclipboarddata)：逐格式写入可失败，成功后句柄所有权移交系统；没有声明多格式整体事务。


## 后续执行：选区证明接入与隔离构建（2026-10-26）

用户要求继续下一步。锁元数据查询短时空闲后，R01原夹具只重试一次，仍在模拟复制时被RadeonSoftware.exe PID15960占用，未进入三个竞争断言；记录r01-next-step.log。没有终止或修改该进程，没有将该结果记为PASS。

### 本次实际代码修改

1. copy_engine.rs：增加独立SelectionProof（精确控件、UTF-16起止位置、原始选区文本）。快照前取证、发键前复核、复制读取后逐字匹配并再次复核。缺失/无效证明不复制；选区变化或文本不符时不恢复、不返回。旧grant、目标身份、代次、单次提交、owner/sequence、读写持锁检查保留。
2. native_copy_proof.rs：只接受Unicode标准Edit且非密码框，使用系统EM_GETSEL/WM_GETTEXT接口限时读取。单次总预算80ms、单条消息最多20ms；控件正文最多32768个UTF-16单位，选区最多20000字符。范围、长度、样式和绑定上下文前后复核。仅临时读取有界控件正文，返回精确选区；临时正文缓冲区退出时清零，不写日志或磁盘。该读取不同于整框内容回传，未将未选中的正文作为抓词结果。
3. safe_copy.rs、lib.rs：将独立证明接到现有NativeIo；未改截图/OCR、UIA优先、用户主动复制、grant持久化或AI站点逻辑。
4. tests/copy_engine.rs：两个同owner/同根竞争文本回归；新增无证明、发送前选区变化、读取后选区变化。原空复制测试改为拒绝与非空证明不符的内容且不恢复，原因是不能把空的未知写入当成可信目标复制。保留了失败关闭，未删安全断言以求通过。
5. tests/native_copy_proof.rs：独立隐藏合成Edit跨进程测试、受控阻塞和正常退出；不安装钩子、不用SendInput、不读写系统剪贴板。另包含同模块的4项控件/UTF-16测试复用。

### 兼容性与未解决边界

- UIA不变；实验性回退仍需真实会话授权。新的证明约束只支持标准Unicode Edit。RichEdit、ANSI或索引语义未验证的控件会在模拟复制之前安全拒绝；不新增提示用户手动粘贴的流程，不放宽生产允许范围。
- R02两个“竞争内容不同于预先选区”的安全断言已从旧逻辑真实失败转为通过。不是整个R02关闭：另一个写入者写入完全相同内容、以及最后校验与SendInput的临界窗口仍不能凭文本/owner证明因果。
- R03的EmptyClipboard后逐格式写入路径本次未改，仍未证明整体无损。不得把来源证明作为恢复事务。
- R01没有新助手EXE端到端PASS。旧本地独立夹具是较早适配器版本；这次重试不代表新证明代码的产品验收。

### 红绿和最终验证

- 首先只加入两个R02断言：12 PASS、2 FAIL，实际返回SYNTHETIC COMPETITOR TEXT；r02-product-red.log保留。
- 接入证明后：cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib --test copy_engine --test selection_core --test uia_strategy：280+17+20+40 = **357 PASS**，0 FAIL（rust-final.log）。
- cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --test native_copy_proof -- --test-threads=1 --nocapture：**6 PASS、1 ignored**。6项含4项库测试复用、2项独立跨进程控件测试；ignored是只由父测试显式启动的子进程入口，两个父测试均实际启动它，不能额外算成2项独立业务验收。
- node --test tests/safe-copy-ui.test.mjs：**12 PASS**；未重复全量前端375项。
- 严格Clippy（--all-targets -- -D warnings）、cargo fmt --check、custom-protocol release check、pnpm build、pnpm check、git diff --check：通过。
- 辅助修改脚本一度因默认本地编码解码UTF-8失败而中止；从预先保存的原始字节重建测试文件、显式UTF-8后继续。最终相关Unicode测试通过，未保留乱码或不相关文本改动。

### 新内部产物

EXE位于本地TEMP/uia-beta2-r123-20261026/bin-next/deepseek-float.exe。未启动、未发布，不能套用旧EXE原生通过记录。

EXE SHA256：C876E00220BA3DCAD4D96E30FA92AD40E1104A8863BDE85EB6953D8F5B7B478F

构建冻结源码文件数：743。源码清单SHA256：1e109d95b83eb9c0ec57bb6d8922a6d8568625e6d41f64b3d584711ba70726df。使用cargo build --offline --locked --release --features custom-protocol --manifest-path src-tauri/Cargo.toml --bin deepseek-float。构建后仅补充本段与总验证报告，业务源码/测试与构建快照一致；完整工作区文档摘要因本段新增而变化。

### 下一步最小研发与验收

1. 用户自行解除Radeon剪贴板占用后，针对新EXE重建带证明的标准Edit合成夹具，完成授权、单次复制、三种R01时序、源文本不符拒绝和退出。不能通过适配器增加重试来冒充生产行为。
2. 同内容竞争仍需要真实来源证明；本次已证明可直接从原生Edit取得精确选区，后续可评估在该有限范围直接返回原生选区、避免借用系统剪贴板。它改变回退机制，需明确产品选择，不在本轮悄悄替换实验性Ctrl+C。
3. R03需独立设计恢复副本寿命、分配/逐格式写入失败、外部写入时禁止重试、退出/崩溃边界。内存副本不是系统剪贴板原子事务，不能承诺系统持续拒绝写入时仍自动恢复。

最终：R01 BLOCKED；R02部分修复但仍BLOCKED；R03 BLOCKED。Beta.2保持NO-GO，未提交、推送、合并或发布，原私有仓库保持只读。


## 2026-10-26 新EXE原生定向验收收尾

本节是前述“未启动验收”构建记录之后的实际执行，不追溯修改此前状态。使用同一内部EXE，SHA256为C876E00220BA3DCAD4D96E30FA92AD40E1104A8863BDE85EB6953D8F5B7B478F。独立配置、本地合成页面与合成控件；本轮只改验收文档，未修改生产代码，不读取或备份测试前未知剪贴板内容。

| 项目 | 结果 | 实际证据与边界 |
| --- | --- | --- |
| WPF UIA选区与主动复制 | PASS | 用户确认精确匹配；wpf-results.jsonl记录match=true、45字符。 |
| 授权标准Edit回退及原文本恢复 | PASS | edit-results.jsonl记录单次COPY_OBSERVED后RESTORE_PASS；助手记录COPY_SUCCESS及Clipboard会话。 |
| 标准Edit捕获内容精确检查 | PASS | 新一轮单次复制/恢复后，主动复制粘贴得到NATIVE_PASTE_PASS；用户确认精确内容。 |
| 取消授权保持关闭 | NOT_CONFIRMED | 用户未单独确认取消结果，不用随后的成功替代这项确认。 |
| R01跨进程竞争 | BLOCKED | 原夹具在WM_COPY内等待子进程，父进程先报告注入阻塞，子进程后来写入；时序无效，不能证明产品通过或失败。本轮停止该组，不重试。 |
| R02同进程同owner、竞争文本不同 | PASS（局部） | 新夹具11:08:10.304 UTC记录单次复制，11:08:10.306记录RACE_INJECTED，序列956；11:08:11.019记录RACE_PRESERVED、序列仍956。助手对应request=305、target_pid=36908，93ms后COPY_SELECTION_MISMATCH，无该请求的SESSION_READY；用户确认RACE PASS。 |
| R02完整风险 | BLOCKED | 同内容竞争及精确临界时序仍未证明。先前RACE_INJECTION_BLOCKED保留；随后copies=2/3/4与工具条属于后续请求，不能混入上述局部PASS或推断单请求重复提交。 |
| R03恢复失败及整体无损 | NOT_TESTED / 发布BLOCKED | 本轮未注入恢复写入失败；现有逐格式写入不构成整体事务，相关风险未关闭。 |
| 退出及进程残留 | PASS | 用户通过菜单退出；exit.json记录PID40956退出码0，2026-10-26 11:10:30 UTC；夹具11:10:31记录CLOSED。只读CIM核对主进程、12个已记录WebView2进程及夹具无残留，未终止进程。 |

证据保存在本机TEMP下uia-beta2-r123-20261026/native-acceptance-01，包括原始JSONL、元数据日志、r02-correlated-result.json与exit.json，不将夹具、测试EXE、运行日志或合成剪贴板正文加入Git。以上时间为证据中的UTC，北京时间加8小时。没有新增全量自动化测试，没有六站、截图/OCR、VM/Sandbox测试。

结论：基础划词/授权回退的三个定向检查以及一个R02局部竞争场景通过；并不等于R01–R03关闭。Beta.2继续NO-GO，未提交、推送、合并或发布。下一步先在不等待被测窗口消息线程的条件下验证R01夹具自身可可靠注入，再单独设计R03逐格式写入失败与外部新写入保护；同内容来源证明仍单独保留，不放宽生产校验。


## 2026-10-26 R01 异步夹具自检与局部原生验收

旧夹具在WM_COPY回调内等待子进程，存在消息线程相互等待风险；旧BLOCKED记录保留。本次仅在TEMP创建V2夹具，改为回调返回后的BeginInvoke触发，定时器单独读取写入回执；子进程有120秒寿命上限，写入只尝试一次，不使用无限重试或终止进程。三项静态契约检查在旧版本为1 PASS/2 FAIL，新版本3 PASS；它们只是结构检查。随后实际夹具自检单次复制、异步跨进程写入、内容保留通过，退出码0且父子进程无残留；不计作产品R01通过。

用户重新准备合成剪贴板后，复用同一EXE（SHA256 C876E00220BA3DCAD4D96E30FA92AD40E1104A8863BDE85EB6953D8F5B7B478F）、新隔离配置，开启UIA增强并由用户授权回退。V2实际记录：19:40:15.549（北京时间）单次COPY_OBSERVED，初始序列1037；复制后序列1042；子进程19:40:15.560写入成功、序列1047。助手request=91、target_pid=23888于19:40:15.564记录COPY_RESTORE_SKIPPED。夹具19:40:16.325核对竞争文本仍保留、序列1047。故“外部新写入后不恢复旧快照”这一个时序断言记为PASS，不能扩展为读前、读中、恢复临界点全部覆盖或R01整体关闭。

用户同时报告出现工具条，但本隔离日志没有任何COPY_SUCCESS、SESSION_READY或popup show。此差异保留为UI观察待核对，不认定工具条验收通过，也不在缺少文本证据时推断误抓或误发送。本轮没有重复该用例，没有运行真实AI网站。

用户关闭功能并通过菜单退出。exit.json记录PID4928于19:42:18正常退出、退出码0；V2于19:42:20记录CLOSED。只读CIM核对12个已记录WebView2进程、主进程与夹具及其写入进程无残留。日志证据位于本机TEMP/uia-beta2-r123-20261026/native-r01-v2；夹具自检证据位于同级r01-fixture-v2。测试EXE、夹具与运行日志均不加入Git。

本轮无生产代码变更，未重新构建助手、未访问测试前未知剪贴板内容、未执行截图/OCR或六站回归。R01一个时序局部PASS；R02同内容来源与精确临界点未关闭；R03整体恢复失败保护仍BLOCKED。Beta.2仍NO-GO。下一步先保留工具条观察差异，再针对R03以故障注入验证恢复副本寿命、逐格式失败与外部新写入保护，不能把返回错误当成已恢复原数据。
