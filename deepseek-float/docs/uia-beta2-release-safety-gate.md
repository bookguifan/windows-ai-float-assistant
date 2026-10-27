# Beta.2 发行候选安全门禁（2026-10-26）

状态：已实施并完成本地自动化；**等待定向原生验收，未合并、未发布**。
基线为 feature/uia-safe-clipboard-fallback 的 a4dee7d55a68f2439d75ab2a2057bbff3bf60f2d；本轮为未提交修改。
Draft PR：https://github.com/bookguifan/windows-ai-float-assistant/pull/1 。远端 CI 40322183555 成功只覆盖旧 head，不能代表本轮未推送改动通过远端 CI。

## 路径与兼容性

UIA 增强选项开启 → UIA 优先 → 在 UIA 确认安全目标但没有文本时尝试标准 Unicode Edit 原生选区读取。增强选项默认不变；关闭时不自动启用此读取。只在明确启用研究特性的非生产 Debug 构建中，原生读取失败后才可能继续原有授权 Ctrl+C 回退。

native_selection 绑定根窗口、PID、线程、焦点控件和存活进程句柄；每次读前/后检查请求代次、截止时间、当前前台和增强选项。复用 EM_GETSEL、WM_GETTEXTLENGTH、WM_GETTEXT，拒绝密码控件、非 Unicode、RichEdit、自绘控件和超长文本。两次独立读取必须得到完全相同的选区、控件和原始文本；每次 proof 预算 80ms、消息请求最多 20ms，外层仍受请求截止时间约束。不能将消息超时视为所有场景的绝对执行时间保证，也不能把两次读取称作 Windows 原子快照。

无剪贴板分支没有系统剪贴板或 SendInput 能力；保留原始换行/缩进/Unicode，只返回选区。临时整控件缓冲上限 32768 UTF-16 单元并在释放时清零；最终选区上限 20000 字符。标准 Edit 自定义复制转换与原始文本可能不同，不能宣称等价覆盖所有应用。失败安静拒绝，不要求用户手动粘贴。VS Code 默认 auto 兼容性按维护者要求暂缓；此方案不修复其提供者缺失。

## 后端隔离证明

experimental_copy_available 是编译期条件：Windows AND debug_assertions AND experimental-clipboard-fallback AND NOT custom-protocol。Release 恒关闭；启用生产 custom-protocol 时也恒关闭，即使显式添加实验特性。修改配置、导入备份、环境变量或调用 IPC 均不能改变编译期条件。手工改变源码/编译标志产生的是另一个未经本门禁批准的构建。

| 入口 | 强制限制与证据 |
| --- | --- |
| 旧配置及旧 IPC | automatic_copy_allowed 始终 false；配置反序列化/保存和备份合并、覆盖沿用已有清洗；旧启用请求被拒绝 |
| safe_copy_status | available 与 enabled 都受构建策略约束，即使内部 Consent 被测试强行 grant 也不能启用 |
| safe_copy_authorize | 后端 request_authorization 在访问窗口或弹系统确认框之前拒绝；前端复选框灰显只是反馈，不是安全边界 |
| 授权 epoch | 发行策略关闭时无有效 epoch；持久化设置和备份不能授权 |
| 调度与捕获入口 | runtime 分支及 capture_inner 都检查后端策略 |
| 副作用边界 | snapshot、copy_chord、restore 各自先检查策略；current 也包含中央策略；发行路径不读取旧剪贴板、不模拟复制、不清空恢复 |
| 主动复制 | set_clipboard_text 等用户主动复制未修改，不属于自动回退；真实原生功能待单独验收 |

测试覆盖所有授权/目标/请求布尔组合、伪造运行时授权、旧配置 true、前端恢复后请求 true、调用入口的结构约束。已有 Rust 库中的配置/备份/IPC 拒绝测试也重新执行。结构测试是防止入口漏接的补充，不替代编译期布尔策略或原生验收。

## 本轮自动化结果

先新增发行门禁测试，旧代码实际 2 PASS / 1 FAIL；原授权全真时会放行。实施后新门禁通过。

| 命令/目标 | 实际结果 |
| --- | --- |
| cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib --test selection_core --test copy_engine --test native_copy_proof --test uia_strategy --test release_selection_policy --test native_selection_policy | 368 PASS、0 FAIL、12 ignored（包含模块复用，不等于 368 个独立原生场景） |
| 默认批次分项 | lib 278/3 ignored；copy_engine 17；selection_core 20；uia_strategy 40；native_copy_proof 2/6 ignored；native_selection_policy 7/3 ignored；release_selection_policy 4 |
| Debug + experimental-clipboard-fallback 的 release_selection_policy | 4 PASS：研究构建仍须授权、有效目标与请求同时满足 |
| Release + custom-protocol,experimental-clipboard-fallback 的 release_selection_policy | 4 PASS：实验特性无法解锁生产构建 |
| pnpm test:unit | 376 PASS、0 FAIL；含 safe-copy UI 13 PASS，不另重复相加 |
| cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings | PASS |
| cargo fmt --manifest-path src-tauri/Cargo.toml --check | PASS |
| pnpm build / pnpm check / git diff --check | PASS |

初次前端检查因 dist 比源码旧而拒绝；按既有流程先 build 后 check 已通过。初次 Clippy 指出测试目标未使用新辅助函数，补充真实过期守卫测试后通过。历史原始 FAIL/BLOCKED 文档未改写。

## CI 与人工门禁

CI 已增加四个既有 Rust 集成目标及两个新策略目标；另外验证显式研究 Debug 策略和带实验特性的生产 Release 策略。保持冻结安装、现有前端/浏览器回归、严格 Clippy 与生产特性检查。

需要创建原生窗口的既有测试保留并标明 ignore 原因。CI/default cargo test 会显示 ignored，不算 PASS。子进程夹具入口继续只由父测试运行，不能盲目对全部目标运行 --include-ignored。显式原生测试命令须单独授权：

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test native_copy_proof cross_process_edit_selection_is_exact_without_clipboard -- --ignored --test-threads=1
cargo test --locked --manifest-path src-tauri/Cargo.toml --test native_copy_proof cross_process_hung_text_read_is_bounded_and_rejected -- --ignored --test-threads=1
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib native_copy_proof::tests:: -- --include-ignored --test-threads=1
```

本轮未启动助手、未执行真实 GUI/剪贴板操作、未执行 Playwright、本机截图/OCR、VM/Sandbox 或六站回归。新增 CI 配置未推送，远端结果待后续授权提交/推送后核验；未修改 GitHub 分支保护。

## R01–R03 状态分离

实验性开发路径：R01 剩余时序、R02 同文本因果来源、R03 多格式部分恢复仍未关闭，历史材料和测试原样保留，不可认定已修复。

发行构建：以强制后端策略使上述自动 Ctrl+C/恢复路径不可达。这是隔离危险路径，不是恢复事务已无损，也不是把默认 false 当作安全证明。标准 Unicode Edit 改用原生消息读取，不再依赖旧剪贴板格式；对不支持控件安全拒绝。任何未来打算解锁发行构建的变更都必须重新审查 R01–R03，不能复用本报告放行。

## 构建冻结与发布范围

本报告在构建前进入源码清单。独立测试产物和完整日志在本机 TEMP/beta2-release-isolation-20261026-final；BUILD-FREEZE.json 记录完整源码清单摘要、Git 基线及 dirty 状态、工具链、命令、EXE 路径/SHA256 与构建后逐文件一致性。SOURCE-FILES.csv 和 SOURCE-PATCH.diff 记录本轮未提交源码，不能把基线 SHA 单独当作构建来源。现有 SOURCE-PROVENANCE、Beta.1 版本/Tag/Release/附件未改。本轮 Cargo.toml 仅新增空实验特性，没有依赖升级或锁文件变化；发行版本号仍 0.1.0，尚未完成 Beta.2 版本冻结。

新 EXE 只用于后续隔离验收，未签名、不发布。新二进制的 PE 隐私审查、许可编译输入对应、最终版本/发布清单仍在发布前待办，不能直接复用 Beta.1 许可构建来源结论。

## 人工验收与放行条件

待单独授权：新 EXE 启动/退出；开关灰显且直接授权请求拒绝；合并/覆盖备份后仍不可解锁；UIA 路径与用户主动复制；标准 Edit 中英文、Emoji、CRLF/缩进精确读取；密码框、失焦/换窗口、过期及控件销毁拒绝；确认新路径没有模拟复制、剪贴板原内容及格式不变。此项使用合成内容，未知剪贴板不得读取或备份。

本次不要求重做危险路径的真实竞争实验来证明发行禁用；如再研究实验构建，需独立授权并保持其 NO-GO。无新原生验收证据前，Beta.2 发行候选保持 BLOCKED，内部 Beta 未整体放行。下一步仅安排新 EXE 的上述定向验收，再核查新 CI，最后独立决定合并与发布。未自动提交、推送、合并或发布。

## 2026-10-27 原生复测与开发合并决定（后续补记）

前文保留为当时的构建/验证记录。最新原生复测：N01 预览与撤销子项正常但入口仍需重试；N05 有主动复制成功日志但精确粘贴 NOT MATCHED，60/33 UTF-16 单位；WM_COPY 来源未确认。完整问题及 N03/N04/N07、R01–R03 未关闭状态见 [Known Issues](beta2-known-issues.md)。

维护者已允许在新 CI 通过、发行安全门禁不退化、问题完整披露时合并开发代码；不再把全部兼容性缺陷关闭作为开发合并前提。Beta.2 正式发布仍 NO-GO，内部 Beta 未整体放行。本轮只提交/推送功能分支和更新 Draft PR，不合并 main，不创建 Tag/Release。
