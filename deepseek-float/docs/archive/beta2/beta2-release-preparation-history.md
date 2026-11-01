# Beta.2.0 功能完善与预发布准备（2026-11-01）

## 当前发布冲刺状态（2026-11-01）

- 候选源码提交为 `2f2ab6b5cac9b8b650dd1a5c8fcd149c67bc249e`，构建时工作树干净，较当时远端 `main` (`5e43b8e8f7beec4ae364bb4e89786a79385bf8e0`) 有 3 个实现提交。候选构建后只补充发布/验收文档，没有改变编译输入。GitHub 版本记录核对时仅有 `v0.1.0-beta.1` tag / Release；本轮不创建 Beta.2 tag 或 Release，PR/CI 动态状态以 GitHub 为准。目标命名沿用 `v0.1.0-beta.2`；Cargo、npm 和 Tauri 产品版本继续为 `0.1.0`。
- 使用 Node `v24.22.1` 直接运行 Vite 与四项前端检查；pnpm `9.15.9` frozen/offline 安装及前端构建完成。pnpm 启动器报告系统 Node `v24.14.0` 的 engine warning，因此包管理器提示仍保留；不把 warning 说成版本符合。`cargo build --locked --release --features custom-protocol` 在全新 `%TEMP%` Cargo target 完成，未启用 `beta2-isolation`、`experimental-clipboard-fallback` 或自动 Ctrl+C 回退；脚本使用 `--no-deliver`，没有刷新旧 `release/` 文件。构建未签名。
- 新本地候选 EXE SHA256 `372196e9f6c2cdbc098768450126188b31ba519b50532c160e7d82aee77f5e01`，7,705,088 bytes。诊断 PDB SHA256 `662d172e6e16a0dcb563e8fb6c4a647e93c0d91ecb3685ca1eda8c49479ec545`，GUID/Age `c57a5122-1d56-4fff-a6e1-883780a4c819 / 1`；PDB 不进入候选 ZIP。EXE CodeView RSDS 与 PDB GUID/Age 匹配。对 EXE 做的本机用户名、仓库路径、Cargo 路径、Temp 路径、私钥和常见 token 字符串扫描未命中；这是指定模式静态扫描，不是完整隐私证明。
- WebView2 SDK 锁定版本 `0.38.2`；本机安装 Runtime `156.0.4314.58`，不随包捆绑。EXE 未启动；本机没有 Windows Sandbox，普通产品标识会落到维护者正式用户目录。原生设置保存/备份恢复测试在具备独立 Windows 测试账户或 Windows Sandbox 的环境前保持 BLOCKED，不用改 `LOCALAPPDATA` 假装隔离。
- `distribution/windows/CHECKSUMS.csv` 中 593 项本地材料均通过 SHA256 复核；材料目录共 594 个文件。材料自身明确说明最终 EXE 内容与组件许可映射仍未完成，因此它们作为保守随包准备材料保留，但许可证门禁仍未关闭。
- 已在本地准备未发布的草稿 ZIP 与清单；不创建 tag、GitHub Release 或上传资产。临时构建根保留 `candidate-identity.json`（SHA256 `105ab4cbcef3d702816acff71c4a94237448af52b9728a91328551f9721d6a1c`）和 `release-build.log`（SHA256 `032bbbdb9491a646412ce3399fc72b70ce31e1daaec5946164130196b3d6b1e3`）；它们不进入 Git 或 ZIP。候选身份与本轮原生验收边界见 `docs/beta2-native-acceptance-plan.md` 和 `docs/beta2-release-notes-draft.md`。

自动化测试可复用与本提交树匹配的既有证据：Rust 450 passed / 12 ignored，剪贴板 facade 11/11，前端 14 个单测脚本、Playwright 22 passed、Clippy、fmt、安全策略与 diff 检查通过。以上是之前在同一源码内容上的运行记录，本轮未重跑全套；本轮实际新执行的是前端 build/check、干净 Cargo Release build、候选身份/隐私扫描和第三方材料摘要复核。所有原生候选功能仍未验收。

**当前结论：Beta.2.0 NO-GO。** 真实剪贴板、A01 原生生命周期、Issue #2、原生备份/恢复和最终发行许可映射尚未完成。准备阶段的 ZIP 不是冻结发行候选。

历史阶段说明（PR #3 合并后、进入 Release-Safety 分支前）：PR #3 已合并至 main，合并提交为 `05eaf6b37381a12fc5f676011b16a2ef1bed3f2c`；该 main 的 CI #10 当时成功。旧阶段曾在 `feature/beta2-final-polish` 工作。当前分支、HEAD、PR/CI 与发布决定以本文顶部“当前发布冲刺状态”为准。Beta.2.0 发布继续 NO-GO。

新候选代码基线为干净提交 **686258ef7b2e43b6b9ad72420ae0ed9d894b44f3**。后续本文件的文档提交不改变编译输入。本次隔离候选原生启动 **1 次**；真实系统剪贴板操作 **0 次**，原生文件选择器/备份恢复操作 **0 次**。

## 更新日志草稿

- 已合并隔离修复：统一派生配置、收藏、提示词、日志、诊断、WebView2、备份和临时目录；无效参数 fail closed。隔离构建延后配置窗口创建并显式绑定 WebView2 数据目录，避免 identifier 派生空目录。普通 Release 默认路径不变。
- **723105b**：主线程启动完成回调读取最新手动显隐代次；迟到的显形不覆盖用户主动隐藏。无新操作时 Ready/三秒超时仍正常显形。
- **a4b0272**：修复主动纯文本复制的 NULL owner。先准备 UTF-16 内存，再创建私有 message-only 窗口，使用有效 HWND 打开并发布；CloseClipboard 后销毁窗口。成功后内存交给系统，失败时释放自有资源，无无限重试。
- 本地未提交的 **feature/beta2-release-safety** 修改：在 EmptyClipboard 前快照可安全克隆的全局内存格式；无法确认的私有/非全局格式、读取失败或超限时先拒绝。发布失败时恢复快照，并区分完整恢复与恢复不完整，诊断日志列出失败格式。没有更改锁文件，也未启用 Release 自动 Ctrl+C 回退。
- **686258e**：补强 A01 旧菜单请求失败、快速刷新与最新票据回归；补强备份文件读写、超限拒绝、失败清理、事务回滚及页面忙碌/取消/失败重试。
- Release 自动 Ctrl+C 回退继续固定禁用；旧配置、备份、IPC 和实验特性不能开启。没有升级依赖或修改锁文件。

主动复制缺陷依据 [Microsoft OpenClipboard 文档](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openclipboard)：NULL owner 后 EmptyClipboard 将 owner 置空，导致 SetClipboardData 失败。新测试将真实生产 writer 编译接入内存 Win32 facade；旧 writer 为 2 passed / 3 failed，修复后 5 passed。该证据是模拟 API 合约回归，未冒充原生复制成功。

## 剪贴板发布失败回滚（2026-11-01，本地未提交）

当前 HEAD 仍为合并提交 `5e43b8e8f7beec4ae364bb4e89786a79385bf8e0`；以下改动尚未提交、未推送，也未冻结为新验收候选。剪贴板源码已在 `target/release` 生成普通 Release EXE/PDB；产物未启动、未复制到 `release/`，隐私检查见下节。工作区变更包括剪贴板实现/测试、两种 Release 构建入口及本文档。

剪贴板会先在锁内枚举并复制受支持的 HGLOBAL 数据，最多 128 个格式、合计 64 MiB。HTML/RTF 等注册格式、GDI 对象格式（0x0300–0x03FF）和 DIB/DIBV5 内容可参与恢复；DIB 存在时跳过可由其重新合成的 Bitmap/Palette。Owner-display、应用私有格式（0x0200–0x02FF）、无法安全克隆的格式、读取错误和超限都在 EmptyClipboard 前拒绝。GDI 对象格式使用 `GMEM_MOVEABLE` 全局内存，Windows 会在剪贴板清空时自动释放；见 [Microsoft 剪贴板格式文档](https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats)。新文本发布失败后逐格式尽力恢复；完全恢复返回 `PublishFailedRestored`，部分失败返回 `RestoreIncomplete` 并记录失败格式。该流程尽量恢复剪贴板，但 Windows 不提供多格式原子事务；恢复 API 本身失败时仍可能有格式缺失，日志会明确标出，不能声称原内容已恢复。

本次在 Windows 上重新执行 `cargo test --locked --quiet`：**449 passed / 0 failed / 12 ignored**；剪贴板专用内存模拟为 **10 passed / 0 failed**。`cargo clippy --all-targets --locked -- -D warnings`、`cargo fmt --check`、`git diff --check` 和 `cargo check --release --locked --features custom-protocol` 均 exit 0。Release 检查未启用 `beta2-isolation`。这些自动化测试没有读写真实系统剪贴板；没有重跑前端或 Playwright，因为本次没有前端修改。普通 Release 链接构建及 EXE 路径扫描已在下节记录；仍未执行原生复制或失败恢复验收。

## Release 路径隐私复核（2026-11-01，本地未提交）

首次普通 Release 构建的 EXE/PDB 扫描发现 `%USERPROFILE%\.cargo\registry\src\...\anyhow-1.0.104\src\error.rs`，PDB 还包含工作区路径和 MSVC 临时目录。两个 Release 入口现在保留已有 `RUSTFLAGS`/`CARGO_ENCODED_RUSTFLAGS`，并追加把项目根映射为 `/src`、Cargo home 映射为 `/cargo` 的 `--remap-path-prefix` 参数。实现位置为 `scripts/build-release.mjs` 和 `release.ps1`；Rustc 路径映射的范围与限制见 [rustc 文档](https://doc.rust-lang.org/rustc/remap-source-paths.html)。

通过 `node scripts/build-release.mjs --no-frontend --no-check --no-deliver` 重新构建，命令 exit 0，Release 链接耗时约 2 分 39 秒；6 个嵌入 HTML 的新鲜度检查通过，跳过了前端重建、check 链、签名和交付复制。features 为 `custom-protocol`，没有启用 `beta2-isolation`。产物仅位于 `src-tauri/target/release`，没有启动，`release/` 交付目录未刷新。

系统 PATH 的 Node 为 `v24.14.0`，低于 package.json 的 `^24.15.0`；因此首次执行没有重建前端。随后使用已安装的 Codex Node `v24.22.1` 重跑同一 `--no-deliver` 入口，满足 engines 约束，Cargo 报告 Release 已是最新，6 个 HTML 新鲜度检查再次通过。未升级或修改系统 Node；完整前端发行链仍需在受支持 Node 下单独运行。

该构建输入是 HEAD `5e43b8e8f7beec4ae364bb4e89786a79385bf8e0` 加 7 个未提交文件的当前工作区；此本地产物不是冻结候选，也未执行原生验收。

| 本机校验产物 | SHA256 | 大小 |
|---|---|---:|
| `deepseek-float.exe` | `41e86fea1816842c27fdcb7d0cee4e6230c213dd769766e9fb69989f7540605c` | 7,705,088 bytes |
| `deepseek_float.pdb` | `a61e40dbaca877cab83083c3e04e47593c668b1047da12ef8013bb5da5159c9d` | 2,535,424 bytes |

对 EXE 的 ASCII/UTF-16 静态扫描未命中本机用户名、工作区、Cargo home 或 Temp 路径。PDB 仍含 `%TEMP%\lnk{...}.tmp`、构建工作目录及 MSVC `cvtres.exe` 路径；这些来自链接器记录，Rustc source remap 不会重写它们。PDB 只作本机诊断，不得放入公开仓库或发行包；本次未提取 PDB GUID/Age，也未将其作为候选身份。以上是针对指定路径的静态扫描，不构成完整二进制隐私证明。`--no-deliver` 的成功输出也已调整为明确说明交付目录未刷新。

## 本轮自动化结果

| 实际执行 | 结果与范围 |
|---|---|
| cargo test --locked --offline | **444 passed / 0 failed / 12 ignored**；不同目标会复用模块，ignored/native 未执行项不计通过 |
| 隔离路径 / Tauri MockRuntime 集成 | **7 passed**；仅控制台探针和 mock runtime，不启动应用 |
| Debug / Release 安全策略 | **各 4 passed**；Release 带实验特性也不能开启自动 Ctrl+C |
| cargo fmt --check | exit 0 |
| 普通与 beta2-isolation all-target strict Clippy | exit 0；首次 mock 公开 API 告警通过收紧到 crate 可见修正，未关闭 lint |
| 普通 Release cargo check / build | exit 0；features 仅 custom-protocol |
| 前端检查、Vite build | exit 0 |
| 前端单元 | **232 项自定义断言 + 164 个 Node 测试通过**，本轮实际重跑 |
| Playwright | **22 passed**，本轮实际重跑 |
| 独立代码复核 / git diff --check / 变更文件敏感模式扫描 | 无阻断发现；差异检查通过。范围内检查不等于完整安全证明 |
| 隔离诊断 Release / 静态路径探针 | exit 0；EXE/PDB 匹配；关键路径在探针根内，缺参 exit 78 |

A01 页面 27 项、备份 Rust 21 项 / 页面 11 项、剪贴板 facade 5 项均为上述矩阵的子集，不能再相加作为独立测试总数。备份测试操作真实临时文件和生产事务函数，不读写正式业务数据。

A01 未复现新的生产缺陷，生产生命周期逻辑未改。会话/票据/代次保护保持：前台切换使旧操作失效；Alt+Tab 不要求立即隐藏，仍由原计时器隐藏；旧定时器/异步结果不能覆盖新动作。新增页面测试证明旧“更多”失败不会解锁新请求，连续 20 次刷新仍复用同一工具条并只派发最新票据。原生窗口数量、八秒隐藏和跨应用选区仍待验。

备份未发现需要改写事务实现的新缺陷。新增覆盖导出文件 roundtrip、超限输入、目标替换失败清理、最后一份文件写入失败回滚、损坏 journal 拒绝并保留证据，以及取消导出后重试、恢复失败解锁控件且要求新预览。既有版本兼容、非法 URL/记录、合并/覆盖和崩溃恢复回归保留。

## 新隔离诊断候选

- 源码：686258ef7b2e43b6b9ad72420ae0ed9d894b44f3，构建开始工作区干净，编译输入 SHA256 清单前后相同。
- EXE SHA256：**2b14672f17e5e0ba077fd62cdd0a13de6511d9f23a1987332d886538d0033f08**。
- PDB SHA256：**eff5b910b8935a5a17ccd63e3b28ac42b5bb8a2226e3d48c69c873c410285a03**。
- PDB GUID/Age：**ea17fda2-2abc-43ea-9932-7637210f63d3 / 1**，与 EXE RSDS 匹配。
- features：custom-protocol,beta2-isolation；独立 identifier：com.deepseekfloat.beta2.isolated.polish202611010da96344。
- 配方：Windows x64 MSVC，opt-level=s、LTO、codegen-units=1、CARGO_INCREMENTAL=0、release debug=1、strip=none。诊断 PDB 只保留本地。
- Node **24.15.0**，Rust/Cargo **1.98.1**，本机 Windows **10.0.19045 x64**，WebView2 Runtime **156.0.4314.58**。构建成功不等于覆盖其他 Windows 版本或 ARM。
- 旧冻结候选未覆盖；新候选只用于隔离诊断，不是可分发资产。

本地证据目录以 float-beta2-final-polish- 加唯一标识命名，位于系统 Temp，未入库；保存候选身份、构建日志、编译输入清单、校验、测试日志、路径探针、单次启动计划和证据摘要。不将本机绝对个人路径或 PDB 提交公开仓库。

控制台探针实际检查三份 store、诊断日志、WebView2 绑定、配置窗口延后创建、单实例名称，以及自启/外部导出禁用保护。探针有自己的数据根；原生计划另保留一个此前不存在的唯一根，不能复用探针根。原生启动时仍须从实际日志和子进程命令行核对生效路径。

| 隔离根内相对目标 | 用途 |
|---|---|
| config/config.json、config/favorites.json、config/prompts.json | 三份业务数据 |
| config/.backup-restore-journal | 恢复事务日志 |
| logs/ | 插件应用日志 |
| diagnostics/ | d1-debug.log、poc2-state.log、panic.log、last_crash.json |
| webview2/ | 新 WebView2 用户数据 |
| backups/ | 测试备份路径；交互式导出/恢复仍禁用 |
| temp/ | 测试临时文件 |

构建和探针前后的正式三份业务文件 SHA256 一致，亦与已有基线一致；没有把该静态结果记作尚未执行的原生隔离验收。

## 单次原生回归计划

以下是本次授权执行的检查步骤；实测结果见下一节。启动前重核冻结身份、新根及祖先重解析点、正式三份文件备份/哈希、自启基线、冲突实例，以及 HWND/PID、UI Ready 和保留进程句柄退出码观察能力。截图不可用不单独阻断，不能把 HWND 存在当 UI Ready。

1. 严格使用本地 native-root-plan.json 中的 EXE、参数和唯一根；仅子进程启用 meta 诊断。
2. 记录 PID、启动时间、startup_id、首个 HRESULT、ball_ready 首帧握手、窗口身份、WebView2 子进程及实际数据目录。
3. 托盘可操作后尽快 Show→Hide，观察至少五秒。隐藏使用空区域逻辑，不能单凭 IsWindowVisible 判断。
4. **只有 ball_hide 后出现“手动显隐选择已更新，跳过启动显形”日志且保持隐藏，才能将本次迟到完成场景记 PASS。** 若启动已完成，只能记普通隐藏通过，竞态原生项为 NOT_EXERCISED/UNKNOWN。当前无延迟开关，不为取得 PASS 新增生产行为；确定性时序覆盖来自真实 helper 单测。
5. 同一实例重新显示后打开设置，确认可见/可交互；只在隔离配置中切换一次“全屏隐藏”，保存并重新读盘确认。工具不能操作时可保留人工记录为 MANUAL_PASS。
6. 正常关闭设置并通过应用退出，记录真实退出码、WebView2 子树清理及对应 Error 1000/WER1001；比较正式业务配置、自启和目录。
7. 0x8000FFFF、ball_ready 超时、无效 HWND、正式配置写入等立即停止后续验收；不重启，不把人工终止记正常退出。

上述计划不包含 A01、真实剪贴板或原生备份恢复授权。

## 本次原生回归结果（2026-11-01）

使用本地冻结候选按计划启动一次，无重试。候选源码基线 `686258ef7b2e43b6b9ad72420ae0ed9d894b44f3`；EXE SHA256 `2b14672f17e5e0ba077fd62cdd0a13de6511d9f23a1987332d886538d0033f08`；PDB SHA256 `eff5b910b8935a5a17ccd63e3b28ac42b5bb8a2226e3d48c69c873c410285a03`；PDB GUID/Age `ea17fda2-2abc-43ea-9932-7637210f63d3 / 1`。隔离启动参数和 WebView2 数据目录都指向本次新建的独立测试根。

| 项目 | 结果 |
|---|---|
| WebView2 / UI Ready | PASS。收到 `ball_ready: 收到悬浮球首帧就绪握手信号`；未生成启动失败记录。首个失败 HRESULT 未记录（无失败事件），不提供 HRESULT 数值。 |
| 球窗 / HWND | PASS。主进程 PID `27796`，球窗 HWND `790074` 有效且可见。 |
| 隐藏与主动恢复 | PASS。由维护者操作隐藏后保持约 11.05 秒，再主动恢复；Win32 区域生命周期从空区域变为椭圆区域。 |
| 迟到启动回调抑制 | NOT EXERCISED。启动显示在手动隐藏前已完成，日志没有本次竞态跳过标记；确定性自动化覆盖保留，不能把本次普通隐藏/显示写成迟到回调原生通过。 |
| 设置页 | MANUAL_PASS。设置窗 HWND `331318`，AX 内容可读取；维护者正常关闭。未改动设置。 |
| 正常退出 | PASS。维护者使用应用退出入口；Process 句柄与 Win32 `GetExitCodeProcess` 均记录退出码 `0`，不是人工终止。 |
| WebView2 子进程 | PASS。受监控的候选子进程均已退出，无残留。 |
| 正式数据 / 自启 | PASS。`config.json`、`favorites.json`、`prompts.json` 的前后 SHA256 相同；正式目录元数据差异 0，自启注册表基线相同。没有新增 identifier Known Folder。 |
| 崩溃事件 | PASS。查询范围内未发现本次应用对应的 Application Error 1000 / WER 1001。 |

启动时间 `2026-11-01T07:38:55Z`，退出时间 `2026-11-01T07:50:29Z`；`startup_id` 未从本次诊断记录中取得。WebView2 Runtime 为 `156.0.4314.58`。截图未重试，AX 与 Win32 生命周期证据用于本次观察。历史 Huorong 缓存事件仍记为 `UNATTRIBUTED_WRITE / ACCEPTED_NONBLOCKING_RISK`；全局事件归属没有因此被改写。Issue #2 保持 OPEN，一次成功不证明间歇性启动问题已消失。

证据保存在维护者本地隔离目录，未纳入仓库：候选身份、单次启动账本、UI Ready、AX 设置页读取、Win32 窗口/区域生命周期、进程退出码、子进程清单、正式配置前后哈希、自启与正式目录差异、Windows 事件查询及后测摘要。证据清单由本地保存；本仓库不含 EXE、PDB、用户配置或原始诊断日志。

本次没有运行真实剪贴板操作、A01 划词验收或原生备份/恢复。截图工具超时和历史全局文件事件不作为本次候选失败，但也不记为严格全局零写入证明。

## 已知问题与剩余发布门禁

| 项目 | 当前事实 / 最短下一步 |
|---|---|
| Issue #2 启动 P1 | 保持 OPEN；历史业务 panic 栈 UNKNOWN，单次成功不能证明间歇性问题已修复。新迟到显形修复仍待上述原生回归 |
| A01 / N01 | 自动化保护通过，历史原生首次点击/生命周期 FAIL 保留；另验合成选区、计时隐藏、失效票据、旧结果和连续选区 |
| N05 | 当前本地 writer 已覆盖发布失败后对文本/HTML/RTF/DIB 的模拟恢复；真实系统剪贴板读写、普通文本/中文/多行/Unicode、忙碌与快速操作仍未验收，另需原生测试授权 |
| 剪贴板系统失败边界 | 分配/锁定/创建 owner/Open/Empty、枚举/读取/快照错误、授权拒绝和不支持格式都在清空前失败。发布失败后尽力逐格式恢复。恢复本身失败可能留下缺失格式，代码以 `RestoreIncomplete` 和诊断日志明示，不能保证 Windows 多格式原子回滚 |
| 备份/恢复原生门禁 | 当前隔离构建有意禁用外部文件对话框，不静默放开。另行授权在无真实数据的独立 Windows 测试环境使用普通 Release，验合成收藏/设置/完整备份、取消、预览、合并/覆盖及非法文件；失败回滚保留真实临时事务故障注入证据 |
| 交互范围 | 页面测试覆盖设置、收藏、拖动等前端流程；托盘、全屏恢复、快速开关和拖动的真实桌面表现未在新候选验证，不为清单额外重构 |
| 历史正式 config 改写 | 写入者 UNKNOWN，本轮哈希未新增变化；不恢复、不重置，不以构建成功关闭历史问题 |
| Huorong 缓存事件 | 仍为 UNATTRIBUTED_WRITE / ACCEPTED_NONBLOCKING_RISK，不继续追查全部系统缓存。契约是候选及受控子进程持久写入不得逃逸隔离根，正式业务数据与自启不得修改 |
| 发行隐私 | 首次未 remap 的本机 Release 命中 Cargo 源路径；新增 remap 后本地 EXE 对用户名、工作区、Cargo home、Temp 的 ASCII/UTF-16 扫描无命中。PDB 仍含 linker cwd/Temp 记录且不分发；正式打包前仍需复扫最终 EXE/安装包 |
| 最终包与许可 | 依赖和锁文件不变，现有 LICENSE、第三方 notices、WebView2/Rust 许可与 MPL 源码材料保留；新发行 ZIP 尚未冻结审查，不能只交付 EXE |
| 最终版本映射 | 最终发行源码、普通 Release 配置、EXE SHA256、私有 PDB GUID/Age 和打包清单必须绑定；诊断测试特性不能混入发行版 |

旧候选 3db108b7… 的 WebView2/球窗/UI Ready、人工设置保存、正常退出及子进程清理曾通过，真实退出码 0，正式配置/自启未变化。该候选早于本分支启动和复制修复，不代替新候选结果；旧报告与原始事件流保留。

## 安装、升级与发布构建说明

目标发布标识沿用仓库约定 **v0.1.0-beta.2**；“Beta.2.0”是本轮任务名称，package/Tauri/Cargo 产品版本仍为 0.1.0，不擅自改为 2.0.0。当前没有新 Beta.2 下载包，既有 Beta.1 链接继续对应旧版本。

1. 普通发行构建必须先构建前端，再使用 custom-protocol、显式 CARGO_INCREMENTAL=0，并通过 Release 脚本映射源码路径，**不得带 beta2-isolation 或测试 identifier**。本轮普通 Release 链接及目标路径静态检查通过；诊断变体另行保留。
2. 最终包应包含完整 LICENSE、THIRD_PARTY_NOTICES、SOURCES、组件/来源/校验索引及相对许可目录；不得含 PDB、日志、用户配置、WebView2 profile、测试缓存或密钥。最终 EXE 和完整包分别计算 SHA256。
3. 核验哈希后完整解压到新程序目录，退出旧助手，保留旧程序、核心配置和收藏。不要删除 AppData 或清理 WebView2 来升级，也不要依赖测试目录覆盖实现正式迁移。
4. 迁移使用显式备份与恢复预览；备份不含网站登录态、设备自启或自动复制授权。原生备份验收未完成前，不建议在唯一一份真实数据上恢复。
5. 目标为 Windows x64，需要可用 WebView2 和 VC++ x64 Runtime；绿色包不代替系统运行库安装。当前验证环境见上，其他系统版本和 ARM 兼容性未证明，签名状态需最终披露。
6. 发布说明列出启动间歇性问题、跨应用选区兼容、外部站点变化和未闭环项；遇异常保留日志，不反复清理用户数据重试。

## 发布检查清单

- [x] PR #3 / main CI #10 成功，继续在本地功能分支完善。
- [x] 最小主动复制 owner 修复、针对性 A01/备份回归，全量自动化和普通 Release 构建通过。
- [x] 新隔离候选身份、干净代码提交、路径探针和单次原生计划准备完成，旧候选未覆盖。
- [x] Release 自动 Ctrl+C 仍禁用；正式业务文件哈希不变；本轮无真实剪贴板或原生恢复操作。
- [x] 剪贴板发布失败回滚与内存 facade 覆盖完成；本轮复核 450 passed / 12 ignored，剪贴板 facade 11 passed，严格 Clippy、fmt 与 Release build 通过。
- [x] 两个 Release 构建入口加入源码路径 remap；普通 Release EXE 静态隐私扫描未命中指定本机路径。PDB 仍含链接器临时路径，仅本地保留、不公开分发。
- [ ] 新剪贴板 writer 的 Windows 原生复制与失败恢复行为仍待授权验收；不操作用户当前剪贴板。
- [x] 新隔离候选单次原生启动、球窗/UI Ready、手动隐藏恢复、设置页读取/关闭、正常退出、子进程及正式数据/自启对比完成；迟到启动回调竞态本轮 NOT EXERCISED。
- [ ] 对 Issue #2、A01、N05、原生备份等门禁取得真实结果或维护者明确的处置。
- [ ] 最终发布包的完整许可证内容、EXE/PDB/源码版本映射和包级隐私复核；PDB 不进入公开资产。
- [ ] 推送当前功能分支、创建 PR 并等待最新 SHA CI；条件满足后按授权合并 main。Tag、资产上传和预发布仍需另行授权。

## Release-Safety 收口复核（2026-11-01）

本轮验证输入为 `feature/beta2-release-safety`，基线 HEAD `5e43b8e8f7beec4ae364bb4e89786a79385bf8e0` 加工作区修改。未启动 EXE，未读取或写入真实系统剪贴板，未访问正式配置；构建使用普通 Release 配置并以 `--no-deliver` 结束。

- 剪贴板：新增 GDI 格式范围边界回归，覆盖 `0x0300` 和 `0x03FF` 发布失败后的恢复。移除格式接受分支时该测试先按预期失败，恢复分支后通过。官方 Win32 格式契约将这段范围定义为可用可移动全局内存承载的数据；本测试仍是内存 facade，不是原生剪贴板验收。
- Rust：`cargo test --locked --offline` **450 passed / 0 failed / 12 ignored**；剪贴板集成目标 **11 passed / 0 failed**。首次无提权执行有 5 个收藏测试因沙箱拒绝写入仓库 `.test-artifacts` 而失败；获准后在相同代码下全量通过，未改业务代码。
- 静态检查：`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo fmt --check`、`git diff --check` 均 exit 0。Node `v24.22.1` 直接运行的 14 个前端单测脚本全部 exit 0；之前记录的 Playwright 22 PASS 可复用，本轮没有前端源码改动。
- 安全策略：Release 配置下执行 `cargo test --release --locked --offline --all-features --test release_selection_policy`，**4 passed / 0 failed**。普通 Release 构建只启用 `custom-protocol`，不含 `beta2-isolation` 或 `experimental-clipboard-fallback`；自动 Ctrl+C 回退仍禁用。
- Release：Node `v24.22.1` 执行 `node scripts/build-release.mjs --no-deliver`，前端构建、6 个嵌入 HTML 新鲜度检查、CSP/内联脚本/版本/净化白名单检查及 Rust Release 链接均 exit 0。npm 对 pnpm 配置项有提示警告，不影响退出结果。没有签名、复制发行文件或创建 `release/`。
- 本地 Release EXE：`deepseek-float/src-tauri/target/release/deepseek-float.exe`，SHA256 `72564eb196eb4750e35978d8c4051e10afb76b781c29e37f8ede3e1208c48b14`，7,705,088 bytes。对用户名、仓库目录、Cargo home 和 Temp 的 ASCII/UTF-16 路径扫描均无命中。该本机构建不是冻结候选，也没有启动。
- 本地 PDB：`deepseek-float/src-tauri/target/release/deepseek_float.pdb`，SHA256 `bd67929b7d50114891db49c98441e467f2ab861444ec37c3c2e322e87a5342a9`，仅在本地构建目录；没有跟踪 EXE/PDB，也未生成发行目录。不得公开分发 PDB。
- 许可材料：仓库包含 MIT `LICENSE`、`THIRD_PARTY_NOTICES.md` 和 Windows 发行材料。最终二进制与逐组件许可义务映射仍待发布包冻结后复核。

剪贴板原生测试计划在 `docs/windows-clipboard-native-test-plan.md`，只读验证脚本为 `scripts/windows-clipboard-acceptance.ps1`。脚本默认 `Plan` 不读取剪贴板；读操作需显式 `-AcknowledgeClipboardRead`，且脚本不包含剪贴板写 API。Native clipboard、A01 和原生文件选择器/恢复流程本轮均未执行；不完整恢复、真实格式保持和系统句柄生命周期仍属发布验收门禁。Issue #2 继续 OPEN。
