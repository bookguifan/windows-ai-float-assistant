# Beta.2 合并后缺陷收敛（2026-10-27）

## 基线与边界

公开仓库 main / origin/main 均为 922479b1a30c070b572dfbd252cccaa543837449；PR #1 的 merge_commit_sha 与之相同。原分支 HEAD bc62713，工作区及暂存区干净；本地 main 从旧 9862487 快进后，创建 fix/beta2-native-acceptance。没有 reset、改写历史、提交或推送。本轮只修改公开仓库；原私有仓库没有写入。未启动助手、原生窗口或系统剪贴板测试。

## 问题矩阵

|编号|当前证据和影响|优先级/状态|
|---|---|---|
|N05|期望60、粘贴33个UTF-16单位，摘要匹配A去掉首尾空格的第一行；尚无原生区间/各阶段摘要|P1，原生FAIL保留，原因证据不足|
|N01|A/B有预览时人工确认完整；首次⋯需重新划词重试；失败offer没有对应action|P2，原生FAIL保留，首击丢失节点未确认|
|N03|失焦清空/禁用局部通过，旧票据/精确超时/替换/迟到/生命周期未全部原生核对|P1验证，PARTIAL|
|N04|WPF工具条出现，独立逐字符内容核对未完成|P2，NOT COMPLETE|
|N07|备份夹具缺内置explain，被正常拒绝，未进入合法合并/覆盖|P1验证，BLOCKED（夹具）|
|WM_COPY|完整夹具日志累计10条，来源没有标识，不能从STOP_AUTO_COPY名称推断调用者|UNKNOWN；若确认为未授权复制，立即停止相关组并列P1/按数据影响升级|
|R01–R03|实验性路径历史风险不变|发行构建禁止该路径；历史未宣布修复|

## 读取的证据与缺口

已读取仓库 uia-beta2-release-safety-gate.md、uia-beta2-n01-n05-fix.md、Known Issues、uia-beta2-validation.md、R01–R03记录，以及合并PR #1状态。两个本地原生报告 BETA2_NATIVE_ACCEPTANCE.md 和 N01_N05_NATIVE_RETEST_FINAL.md 均存在，已读取；旧夹具 Retest.cs、synthetic-expected.json 和脱敏d1-debug.log也存在。没有修改原始证据或将原日志加入Git。

旧EXE：3B14E14F654590F6D7AF953EB14AB945872932F597729E545ADBCE78EA3ECE1E。
最近原生复测EXE：258D2D91583DF20B8FC805D7DE38E6DF18C6489B322A274D61FAC1924038A3D2。
本轮未启动或重新验收这两个EXE，不能把旧结论当作本轮新构建的原生结果。

N05 request/ticket 424、427、443均记录NativeEdit offer、action accepted和复制成功，但没有EM_GETSEL区间或文本长度/摘要。最后的粘贴结果与哪一次实际拖选范围相同，不能从日志倒推。夹具只记录WM_GETTEXT查询、复制次数和粘贴摘要，没有记录原生选区区间。

源码审查：native_copy_proof::selected按UTF-16 start..end取片段；read_stable要求两次相同；Session::new保留raw、另建normalized；ToolbarAction携带原Session；popup复制使用raw；capture写入函数对raw逐UTF-16编码加终止符。未发现这条路径将raw trim或取首行。纯回归证明区间0..60返回完整A，区间2..35合法返回33单位的第一行；这仅证明两种情况都可由现有代码产生，**不证明旧测试实际选中了哪一个区间**。

所以首个差异节点当前仍UNKNOWN：不能认定情况A（误选）、B（捕获/Session变化）或C（写入变化）。本轮不做猜测性的trim/换行/API替换。

N01失败offer如170/191没有action，后续173/194有action accepted和settings-show。旧记录缺少原生命中、按钮名、frontend click/dispatch和拒绝原因，因此无法区分没命中、前端锁、无票据、IPC拒绝或焦点代次撤销。没有证据支持延长8秒Session或取消身份验证。

## 已确认并最小修复的问题

前端fire的250/400ms解锁回调未绑定票据/动作代次。A点击后产生计时器，B替换A并点击后，A迟到回调会解除B的锁，导致额外的B IPC派发。

RED：old toolbar unlock timer cannot re-enable a replacement ticket，在旧代码下失败，实际3次派发、预期2次。GREEN：增加actionEpoch；每次新票据/动作递增，只有当前epoch可解锁。没有改变后端授权、消费、过期或8秒寿命。该修复是可证实的前端重复派发缺陷，**不声称它已解释N01首击或修复N05内容不匹配**。

诊断的Promise完成事件也绑定派发时的票据，避免旧IPC完成日志错误归给新票据。这个诊断一致性测试与产品原生验收分开。

## 最小合成诊断

必须显式设置 DSFLOAT_SYNTHETIC_SELECTION_TRACE=synthetic-only，且现有诊断开启（下轮使用 DSFLOAT_DIAG=meta）；普通诊断/full档单独开启不能启动它。默认不记录新增内容摘要；明文永不写入新增诊断。

摘要进一步限定为代码中四段固定合成夹具文本及其实际连续子选区。非白名单匹配选区即使诊断开关开启也不记录正文指纹（units/SHA）；native-range仍仅记录数值区间，不含正文。不扩大白名单来记录真实资料。使用Windows CNG SHA256，编码为UTF-16LE、不含末尾NUL，与夹具摘要定义一致；不新增第三方依赖，仅开启现有windows crate Cryptography特性，锁文件不变。CNG失败不影响选区动作，也不伪造摘要。

链路记录：native-range(start/end/control_units) → native-read（两次稳定读取）或uia-read → session-created → session-offer → ticket-action → action-text → clipboard-write-input/success。request/session/generation/ticket和HWND关联。write-success只表示写入API返回成功，**不是读取剪贴板逐字符验证**。

N01记录：native-down（真实捕获时间、命中root/坐标、inside/own/armed/ticket/generation）→ frontend-click/busy/no-ticket/dispatch → ipc-action → ticket-check（missing-native-arm/ticket-replaced/native-click-expired/source-dead/session-stale/foreground-changed）→ settings-handoff-input/prepared → settings-preview/settings-show/ready → clear/expire。原生点击回调只try_send至32项有界队列，日志在独立线程写，队列满丢诊断而不阻塞钩子、不改变生产事件。前端诊断IPC只接受本地可见popup和固定阶段/动作值，不接收选区正文。

## WM_COPY 来源审查

主动复制popup/result使用SetClipboardData，不向目标发送WM_COPY。旧capture的Ctrl+Insert路径先受永远关闭的legacy gate拦截；safe_copy的Ctrl+C受编译策略、授权和事务守卫拦截，Release/custom-protocol不可进入。native_selection没有SendInput/剪贴板调用。未发现直接发送WM_COPY的生产入口。

这不能确定历史10条WM_COPY的实际来源。用户键盘、目标应用默认编辑行为或其他组件均不能仅凭消息名排除；SendInput injected标志、owner PID或HWND也不能单独当作完整来源证明。本轮未做键盘钩子、注入或竞争实测，结论保持UNKNOWN。下轮记录合成夹具WM_KEYDOWN/WM_COPY/EM_GETSEL时间及明确人工动作，与助手日志关联；不能将夹具状态名称当作自动复制证据。

## 修改范围

- popup.html：跨票据计时器守卫；合成诊断事件。
- selection_trace.rs（新增）：固定合成数据、SHA256及有界点击元数据；lib.rs声明。
- native_copy_proof.rs/native_selection.rs：成功非密码读取后的只读observer，不改变范围或验证。
- selection_runtime.rs/popup.rs/selection.rs/settings.rs：长度/摘要、拒绝原因及移交关联；没有放宽消费或过期。
- mousehook.rs：异步有界诊断投递和退出信号，无阻塞I/O。
- ipc.rs/lifecycle.rs：受本地来源约束的诊断命令注册。
- Cargo.toml：现有windows依赖增加系统CNG特性；无新crate/锁文件修改。
- selection_core.rs/native_copy_proof内纯测试/safe-copy-ui.test.mjs：实际区间与raw保真、旧票据和解锁隔离、诊断隐私/关联。
- 本报告与Known Issues后续补记。

## 本次自动化验证

执行环境：Windows NT 10.0.19045.0 / x86_64-pc-windows-msvc，rustc 1.98.1、cargo 1.98.1、Node v24.22.1、pnpm 9.15.9；锁定 tauri 2.11.5。所有Cargo命令带--offline --locked（fmt除外），环境CARGO_INCREMENTAL=0。pnpm脚本实际通过现有Node和pnpm 9.15.9入口执行，没有安装/升级依赖。

|实际命令（在deepseek-float下）|本次结果|
|---|---|
|pnpm build|退出0|
|pnpm check|退出0，CSP/内联语法/版本/净化白名单全部通过|
|pnpm test:unit|384 PASS，0 FAIL；其中safe-copy-ui 21 PASS|
|pnpm test:js|Playwright Chromium 22 PASS，0 FAIL；只运行现有本地页面测试|
|cargo fmt --manifest-path src-tauri/Cargo.toml --check|退出0|
|cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib --test selection_core --test copy_engine --test native_copy_proof --test uia_strategy --test release_selection_policy --test native_selection_policy|387 PASS，0 FAIL，12 ignored|
|cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings|退出0|
|cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --features experimental-clipboard-fallback --test release_selection_policy|4 PASS，0 FAIL|
|cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol,experimental-clipboard-fallback --test release_selection_policy|4 PASS，0 FAIL；Release门禁不可被实验特性解锁|
|cargo check --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol|退出0|
|cargo build --offline --locked --manifest-path src-tauri/Cargo.toml --release --features custom-protocol --bin deepseek-float|退出0，实际完成链接；没有运行EXE|
|git diff --check / git diff --cached --check|退出0，暂存区为空|

387为各Cargo目标的执行数量之和，包含模块复用，并非387个独立原生场景；12个ignored仍未执行，不计为PASS。新测试位于现有--lib、selection_core、native_copy_proof/native_selection_policy及safe-copy-ui.test.mjs执行范围，CI已有目标覆盖；未修改workflow或触发远端CI。

首次Rust编译因新增诊断借用使collect的String类型推断不明确而失败；已添加显式类型标注，随后重新执行完整Rust测试/Clippy/策略/构建并通过。首次失败日志保留。RED回归另使用HEAD原始popup与新测试在临时目录重现：3次派发!=2次，退出1；修复后同一断言通过。新诊断的合成SHA256、隐私开关及非夹具拒绝测试通过，不等于原生现场诊断已验收。

本轮变更扫描：新增内容的私钥、GitHub令牌、AWS访问键、本机绝对路径及原私有仓库URL模式均0命中；新增/更新文档的相对链接0缺失。此扫描不是完整安全认证。17个修改/新增文件，未加入EXE、日志或夹具；capture.rs、safe_copy.rs、release_policy.rs、两份锁文件及历史证据未修改。没有暂存、提交或推送。

日志、实际命令JSON、RED记录和源码摘要留在本机临时目录beta2-postmerge-validation-20261027与beta2-postmerge-rust-final-20261027，不加入Git。最终报告补齐后重新生成SOURCE-SHA256.json，清单自身在仓库外，避免自引用；构建/测试后仅本报告文字变化。诊断可能增加合成复测延迟，不以本次结果宣称性能收益。

### 本轮构建记录（仅供后续单独授权的原生复测）

HEAD仍为922479b1a30c070b572dfbd252cccaa543837449，工作分支fix/beta2-native-acceptance，dirty源码；不能声称产物仅对应未修改的main提交。完整751文件工作树摘要和构建来源JSON保存在上述临时目录。独立EXE位于该临时目录的bin/deepseek-float.exe。

EXE SHA256：740F9F372E60C387FAA41F0F4500846CC575C1ADDB743C522909F3AC4BF776AF。

没有覆盖旧验收EXE或Beta.1产物；本轮没有启动该EXE、读取系统剪贴板、签名或原生验收。它不是可发布包，旧EXE的原生PASS不可套用。

## 下一轮最小原生矩阵（待单独授权）

|项目|方法与断言|证据/停止条件|
|---|---|---|
|N05区间|合成Edit夹具在处理完鼠标抬起后记录实际EM_GETSEL、固定源文本单位数和选区摘要；分别全选0..60、明确子选区2..35，不用用户目视推断范围|对照native-range、read、Session、Ticket、write各阶段；首个差异定位为A/B/C；不要把工具条出现或API成功算内容正确|
|N05主动复制|另行取得覆盖剪贴板授权；仅本项人工点复制，粘贴到合成检查框逐UTF-16比较中文、Emoji、首尾空格、CRLF/LF和缩进|NOT MATCHED、错误内容或未授权复制立即停止；不读取原有未知剪贴板|
|N01|冷启动及已加载设置分别首次点⋯；保留完整native-down→frontend→IPC→Ticket→settings事件；同一轮不重复划词掩盖首次失败|没命中/没dispatch/被哪条拒绝分别记录；每次新选区独立票据；禁止延长寿命或放宽身份|
|N03|重复消费、8秒边界、1秒点击资格、B替换A后迟到A动作/清理、切窗/销毁/迟到初始化|旧动作不得复制/清除B，失焦不得复活；自动模型成功不能记原生PASS|
|N04|WPF固定整行逐UTF-16核对，经独立预览/主动复制授权检查|UIA路径和内容两项分别记录；无工具条或尾标点未选中保留实际结果|
|N07|从隔离测试版导出含完整内置prompt的合法备份，只更改请求回退字段为true；分别合并/覆盖；检查后端available/enabled与直接授权/IPC返回|不改产品备份校验；不使用真实用户备份；任何解锁立即停止|
|WM_COPY|仅合成控件记录消息和明确人工动作时间；核对发行路径不可达，必要实验另行授权|来源不能证明则UNKNOWN，不继续竞争注入|
|N02/N06/N08|合成密码拒绝且不查询正文；发行入口旧配置/备份/IPC/特性不能解锁；正常退出并检查已记录进程|密码读取/自动复制/残留异常立即停止相关组|

下轮需先复核本轮冻结的dirty源码清单、工具链、构建命令和新隔离EXE摘要，再取得原生授权。不得复用旧EXE的PASS作为本次修改原生PASS。本轮只做构建/自动化，不启动新EXE。

## 结论分类

- 已修复且自动化验证：旧前端解锁计时器跨票据干扰。
- 新增可验证诊断：仅合成来源的完整数据链及点击链；尚未取得新原生证据。
- 已通过原生验收：本轮无新项；历史子项原样保留。
- 仍失败：N01/N05历史原生结果。
- 尚未验证：N03剩余时序、N04精确内容、N07合法恢复、诊断在原生现场的完整性。
- 证据不足：N05首个差异节点、N01首次点击的唯一根因、WM_COPY来源。

Beta.2发布仍NO-GO；不提交、不推送、不合并、不创建Tag/Release。发行安全隔离与实验性研究历史结论分别记录。


## 原生诊断补验（2026-10-27，开发提交记录）

使用冻结测试 EXE SHA256 `740F9F372E60C387FAA41F0F4500846CC575C1ADDB743C522909F3AC4BF776AF`。该 EXE 早于本提交的紧凑工具条样式变化，不能将其结果宣称为当前工作区重新构建产物的原生通过。

- N05 局部 PASS：实际原生范围 2..60，共 58 个 UTF-16 单位，捕获、Session、票据消费、主动复制写入与最终粘贴的长度及 UTF-16LE SHA256 一致；夹具 WM_COPY 计数为 0。完整 0..60 及历史 60→33 不匹配的唯一原因尚未闭环，历史 FAIL 保留。
- N01 仍 FAIL：存在成功移交及完整预览的样本，但相邻新票据在工具条显示前已撤销，点击得到 no-ticket；本次 WPF 首次点击同样失败。文本读取成功不等于操作入口成功。
- 显示阶段原因审查：offer 后才 show；未交互时 `allows_popup_focus` 为 false，显示引起的前台事件可递增代次，随后 supervisor 撤销票据。当前纯状态机独立模型 2 PASS，证明该路径可达；原生日志确认 offer→撤销→show→no-ticket 边界，尚无逐次前台事件归因，不能写成已修复或完整根因证明。本提交未实施显示事务修复。
- N04 读取局部 PASS：固定 WPF 整行 54 个 UTF-16 单位，夹具、UIA、Session 摘要一致；设置入口因票据失效失败。中文、Emoji、换行及首尾空格的完整 UIA 矩阵仍未完成。前批夹具不可见的 BLOCKED 保留，后续在交互环境重新启动后才取得本项读取证据。
- N07 本轮合并/覆盖恢复 PASS：合成备份请求回退 true，两种恢复后用户确认开关仍禁用，磁盘配置均为 false。最初合成备份含小数秒被时间解析器拒绝，原始错误保留；另生成仅修正 created_at 的测试文件，解析函数 2 PASS，未放宽生产校验。不宣称本轮重新执行了所有 IPC 或编译组合的原生验证。
- N03 仍 PARTIAL：普通设置入口没有继承旧票据；旧票据重放、超时、替换与重复消费完整原生矩阵未完成。模型覆盖与 GUI 结果分开记录。
- 历史 WM_COPY 来源仍 UNKNOWN；本轮目标夹具未命中不能核定以前消息来源。R01–R03 风险及失败记录保留；发行自动 Ctrl+C 禁用策略未更改。
- 后续补验助手退出码 0，已记录 WebView2 和合成夹具均退出。没有提交 EXE、夹具、原始日志或本机路径。

Beta.2 发布仍 NO-GO。下一步先针对显示阶段撤销补证与最小修复，再冻结新 EXE 复测入口及剩余原生矩阵；本次推送仅固化开发代码，不合并、不发布。


## 修复分支推送前复核（2026-10-27）

本次授权仅提交并推送 `fix/beta2-native-acceptance`，不直接更新 main，不合并或发布 Beta.2。上述历史阶段中的“未提交/未推送”描述保留为当时记录，本节说明当前开发固化操作。

本次重新执行：pnpm build、pnpm check、pnpm test:unit（384 PASS）、pnpm test:js（Chromium 22 PASS）；cargo fmt --check；cargo test --offline --locked --lib 及 selection_core/copy_engine/native_copy_proof/uia_strategy/release_selection_policy/native_selection_policy（合计387 PASS、12 ignored）；严格 clippy --all-targets -- -D warnings；Debug experimental-clipboard-fallback 策略专项4 PASS；Release custom-protocol,experimental-clipboard-fallback 策略专项4 PASS；Release custom-protocol cargo check。全部命令退出0，ignored未执行，不计通过。该次没有重启原生助手。

环境差异：本机 Node 24.14.0、pnpm 9.15.9，Node低于package.json声明的^24.15.0，pnpm输出engine警告但上述命令均实际完成；CI使用Node24基线，远端新SHA结果仍需另行核对。不能将本地通过当作远端CI完成。

17个待提交文件的常见令牌、私钥、本机个人路径和原私有仓库URL扫描未命中；该扫描不构成完整安全认证。未纳入测试EXE、ZIP、原始日志或夹具。workflow没有发布步骤，当前只对main/master push和pull_request触发；单独推送这个尚无PR的修复分支不保证触发CI。

Beta.2仍NO-GO，N01显示阶段竞态未修复，完整原生矩阵未关闭；本次提交是带已知问题的开发快照。
