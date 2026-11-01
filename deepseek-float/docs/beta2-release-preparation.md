# Beta.2.0 开发收口与预发布准备（2026-11-01）

PR [#3](https://github.com/bookguifan/windows-ai-float-assistant/pull/3) 已合并；main 合并提交为 `05eaf6b37381a12fc5f676011b16a2ef1bed3f2c`，合并前最终源提交为 `3d2bc215268ef8e400fbd6dad142c22fc5f7db50`。后续本地分支为 `feature/beta2-final-polish`。开发合并与发布授权分开，**Beta.2.0 发布仍 NO-GO**。

本文为当前状态入口；带日期的旧报告继续保留各自当时的 FAIL、BLOCKED 与授权边界，不把历史结果改写为通过。

## 已合并修复与更新日志草稿

- 测试隔离根统一派生配置、收藏、提示词、诊断、日志、WebView2、备份和临时路径；无效参数直接拒绝，不回退正式数据。正式 Release 默认路径保持不变。
- 测试特性下延后 Tauri 配置窗口创建，显式绑定 WebView2 数据目录，避免自动创建 identifier 派生空目录。保留球窗首帧握手和超时兜底。
- 划词显示事务绑定会话/票据/代次；旧异步结果、旧解锁定时器不得覆盖新动作；设置偏好保存串行化并处理写盘失败。
- Release 自动 Ctrl+C 回退继续固定禁用，不能由旧配置、备份、IPC 或实验特性开启。主动复制保留。

## 完善分支：启动显形保留新用户选择

本地修复提交 `723105b9ad08bf8af328d675aa48f8c3650ce2d7`：后台等待首帧完成后原本无条件显形，可能覆盖等待或排队期间用户刚执行的隐藏。现在在主线程完成回调中读取现有手动显隐代次；代次变化则保留新选择，没有新操作时 Ready/三秒超时仍正常显形。仅修改 ballready.rs 与 windows.rs，不改变数据路径、依赖或 Release 剪贴板策略。

5 个新增测试直接调用生产 completion helper：Ready 与超时无新操作仍显形、两条路径发生 Show→Hide 不被覆盖、排队后代次变化不被旧回调覆盖。保持旧行为的生产 helper 上，RED 为 8 passed / 3 个预期失败（显形 1 次而预期 0）；修复后首帧子集 11 passed。该修复不解释 Issue #2 的最初业务 panic，也未取得新版本的原生 HWND 验证。

| 新分支实际执行 | 结果 |
|---|---|
| cargo test --locked --offline | 436 passed / 0 failed / 12 ignored，11 个结果目标，含 doc tests；各目标可能复用模块 |
| 隔离路径 / Tauri MockRuntime 集成 | 7 passed；不启动应用 |
| Debug / guarded Release 安全策略 | 各 4 passed；自动 Ctrl+C 继续禁用 |
| fmt / 普通与隔离特性 all-target strict Clippy | exit 0 |
| 普通 Release check / build（custom-protocol） | exit 0 |
| git diff --check / 源码稳定性 | 通过，测试/构建期间业务源码无变化 |
| 前端 / Playwright | 源码未变，复用精确 PR HEAD 的 392 项检查 / 22 测试；未声称重跑 |
| 本地独立代码复核 | 无阻断发现；未冒充 GitHub 第三方审批 |

新普通 Release 构建基线为 main `05eaf6b` 加上述修复的当时未提交差异，构建后将相同源码固化为 `723105b`；保留实际构建时的 dirty 标记、源码清单和补丁，不称它是先从干净提交重建的产物。

- EXE SHA256：`cb3a8290756c806995f091a8fbf4ccd7479704a9b945b22a2395a75e5e3b0fd9`。
- PDB SHA256：`3f7afd919d687fbe7eeac48a7f05d7b107c524866ca15ac6a8588938fa0a1478`；GUID/Age `9ab09db8-ba28-4bcc-ac20-5411574c1ee6 / 1`，与 EXE 匹配，仅本地保留。
- 配方：普通 Release、custom-protocol、opt-level=s、LTO、codegen-units=1、strip=true、CARGO_INCREMENTAL=0；无 beta2-isolation 特性。新产物启动次数 0，**不是可按旧隔离计划启动的测试候选**。
- 旧冻结隔离 EXE/PDB 摘要未变；没有覆盖旧候选。下次原生复测应先生成绑定新源码的新隔离候选，再另行取得一次明确启动授权。
- 基本产物隐私扫描：仍有构建机 Cargo registry 源路径前缀（178 次 ASCII 匹配）；CodeView 仅引用 PDB 文件名，未命中所查常见私钥/令牌模式。路径片段仅本地留证，不入库。该产物不分发；最终发行构建应受控 remap 构建机路径后重新检查，不以 strip 或这次模式扫描宣称完整隐私验收通过。

## 原生基础验收

本次仅启动冻结隔离候选一次。WebView2、球窗有效 HWND/可见性和明确 ball_ready 首帧握手通过。设置页由人工打开，AX 读取内容；人工将无害的全屏隐藏从 false 改为 true，磁盘重新读取确认保存，随后关闭设置并使用应用菜单退出。保留句柄和 Process.ExitCode 两路均为 0，受控子进程无残留，无对应 Application Error 1000 / WER 1001。正式 config/favorites/prompts 的 SHA256 和自启项不变，未新增 identifier Known Folder。

设置操作记为 MANUAL_PASS。截图/坐标输入工具不可用不等于产品设置功能失败；没有用 HWND 存在代替 UI Ready。观察器等待人工期间有一次超时，随后对同一 PID/启动时间/映像重取句柄，没有第二次启动。

该候选 EXE SHA256 为 `3db108b71f284c99a9e0945fea4e05f0824392c165432e06e46233c8c39f0aed`，来自已记录的 dirty-source 隔离构建，不能宣称它等同于干净的合并提交产物，也不覆盖后续生产代码修改的原生验证。

Huorong 图标缓存事件仍为 **UNATTRIBUTED_WRITE / ACCEPTED_NONBLOCKING_RISK**。维护者明确接受其作为个人开源项目开发合并的非阻断风险；原始全局事件流保留，没有确定候选写入者或外部写入者，不宣称严格取证隔离 PASS。当前实用契约为：测试候选及受控子进程持久化写入不得逃逸隔离根，正式业务数据和自启不得修改。发现明确危险写入仍须停止。

## 自动化证据基线

[CI #9](https://github.com/bookguifan/windows-ai-float-assistant/actions/runs/41256361017) 对精确 PR HEAD `3d2bc21` 成功：Rust 431 passed / 12 ignored；隔离 Debug/Release 各 7；普通 Release bootstrap 1；Debug/Release 安全策略各 4；前端 232 项自定义断言与 160 个 Node 测试；Playwright 22。普通 Release 构建/check、严格 Clippy、fmt 及前端检查成功。不同矩阵有重叠，不汇总成独立原生场景数。

合并触发的 [main CI #10](https://github.com/bookguifan/windows-ai-float-assistant/actions/runs/41273704896) 对应 `05eaf6b`，其最终状态单独记录。源码未改的前端/Playwright结果可复用；后续 Rust 修改须运行受影响回归及必要编译检查。ignored/native 未执行项不计 PASS。

## 剩余问题与最短验收计划

| 优先级 / 项目 | 当前事实 | 下一步 |
|---|---|---|
| P0/P1 启动 Issue #2 | OPEN；曾在首帧握手后异常退出，原业务 panic 栈 UNKNOWN；成功一次不能证明修复 | 下次另行授权时，用匹配符号保留首次异常线程栈，正常退出和子进程清理一起记录 |
| P1 A01 / N01 | 历史首次点击/生命周期 FAIL 保留，自动化状态机和旧计时器覆盖不等于原生 PASS | 合成文本，首次操作、计时自动隐藏、切前台后旧票据失效、旧计时器不隐藏新工具条、迟到结果不覆盖新预览；Alt+Tab 不要求立即隐藏 |
| P1 N05 | 模型有 Unicode/raw 文本与竞争保护覆盖，历史完整文本不匹配仍未闭环 | 先获明确合成剪贴板授权；中文/英文/多行逐 UTF-16 比较，图片/富文本失败不覆盖、无无限重试；不读取用户未知当前剪贴板 |
| P1 备份与恢复 | 临时文件事务和前端预览/取消有自动化覆盖；原生完整流程未验 | 只用临时配置/收藏，导出合法备份、预览、取消、合并/覆盖和故障回滚，不恢复真实数据 |
| P2 交互 | 本次设置保存/关闭和正常退出通过，其他拖动/全屏切换等范围未完整验 | 以可复现缺陷为准，不为完成清单重构 |
| 历史正式 config 改写 | 本次无新变化；历史写入者 UNKNOWN | 保留问题和现有证据，不恢复、不重置，不以本次成功关闭 |
| 发布映射/隐私/许可 | 已有 LICENSE、第三方 notices、资源权利与发行许可清单；最终包未冻结 | 对最终源码/配置打包一次，校验清单和隐私、记录 EXE/PDB/源码映射；诊断 PDB 不公开 |

本轮一次启动额度已用完。新代码构建后的原生验证不得借旧候选结果标 PASS，需下一轮明确授权。发布门禁可在维护者明确接受已知非关键问题后调整，不能默默把未测改为通过。

## 安装与升级说明草稿

目标发布标识沿用仓库约定的 `v0.1.0-beta.2`；“Beta.2.0”为本轮任务名称，不擅自把产品版本升级成 `2.0.0`。当前 package/Tauri/Cargo 包版本 `0.1.0` 与该预发布标识的关系须在最终发行说明明确记录。

1. 预发布包准备好后，先核对其 SHA256，完整解压到新程序目录，保留 LICENSE、第三方 notices 和资源许可材料。
2. 退出旧助手，避免单实例唤醒旧版本。保留原程序和用户配置；不要删除或清理 AppData/WebView2 来升级。
3. 如需迁移，先在设置页通过显式导出生成备份；备份不含网站登录态、设备自启和自动剪贴板授权。恢复先预览，出错不覆盖真实数据。正式升级前还须完成上表的原生备份验收。
4. Windows x64 需可用的 WebView2 Runtime 和 VC++ x64 Runtime。绿色包不代替系统运行库安装器。是否签名及系统兼容范围须在实际发行说明披露。
5. 网站登录、选区兼容与外部站点页面变化仍有已知限制。遇启动异常保留日志、记录版本和步骤，不反复清理数据重试。

当前没有新的可下载 Beta.2 包；README 既有 Beta.1 下载和校验信息继续对应 Beta.1。

## 发布检查清单

- [x] PR #3 合并，源码与原生证据范围有记录，已接受的非阻断风险已披露。
- [x] Release 自动 Ctrl+C 仍禁用；无依赖升级或正式配置重置。
- [ ] 完成主要原生功能门禁及启动 P1 的可接受处置。
- [ ] 从最终源码与配置生成候选，记录 dirty 状态、差异、构建命令、EXE/PDB SHA256 与 GUID/Age。
- [ ] 新候选正常启动/设置/退出及子进程清理，正式数据不变。
- [ ] 检查发行 ZIP 清单：无 PDB、日志、配置、WebView2 profile、密钥和本机私密路径；许可证材料齐备。
- [ ] 核对最终版本、更新日志、已知问题、升级说明和测试结果。
- [ ] 维护者另行授权 Tag / 资产上传 / 预发布。当前不发布。
