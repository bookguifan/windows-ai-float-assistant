# Beta.2.0 功能完善与预发布准备（2026-11-01）

PR [#3](https://github.com/bookguifan/windows-ai-float-assistant/pull/3) 已合并至 main，合并提交为 05eaf6b37381a12fc5f676011b16a2ef1bed3f2c。[main CI #10](https://github.com/bookguifan/windows-ai-float-assistant/actions/runs/41273704896) 已成功。当前开发分支为 feature/beta2-final-polish，本轮仅本地提交，不推送、不合并、不创建 Tag 或 Release。**Beta.2.0 发布继续 NO-GO。**

新候选代码基线为干净提交 **686258ef7b2e43b6b9ad72420ae0ed9d894b44f3**。后续本文件的文档提交不改变编译输入。本轮助手候选原生启动次数 **0**，真实系统剪贴板操作次数 **0**，原生文件选择器/备份恢复操作次数 **0**。

## 更新日志草稿

- 已合并隔离修复：统一派生配置、收藏、提示词、日志、诊断、WebView2、备份和临时目录；无效参数 fail closed。隔离构建延后配置窗口创建并显式绑定 WebView2 数据目录，避免 identifier 派生空目录。普通 Release 默认路径不变。
- **723105b**：主线程启动完成回调读取最新手动显隐代次；迟到的显形不覆盖用户主动隐藏。无新操作时 Ready/三秒超时仍正常显形。
- **a4b0272**：修复主动纯文本复制的 NULL owner。先准备 UTF-16 内存，再创建私有 message-only 窗口，使用有效 HWND 打开并发布；CloseClipboard 后销毁窗口。成功后内存交给系统，失败时释放自有资源，无无限重试。
- **686258e**：补强 A01 旧菜单请求失败、快速刷新与最新票据回归；补强备份文件读写、超限拒绝、失败清理、事务回滚及页面忙碌/取消/失败重试。
- Release 自动 Ctrl+C 回退继续固定禁用；旧配置、备份、IPC 和实验特性不能开启。没有升级依赖或修改锁文件。

主动复制缺陷依据 [Microsoft OpenClipboard 文档](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openclipboard)：NULL owner 后 EmptyClipboard 将 owner 置空，导致 SetClipboardData 失败。新测试将真实生产 writer 编译接入内存 Win32 facade；旧 writer 为 2 passed / 3 failed，修复后 5 passed。该证据是模拟 API 合约回归，未冒充原生复制成功。

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

## 下一次单次原生回归

必须另行取得明确授权，最多启动一次，不自动重试。启动前重核冻结身份、新根及祖先重解析点、正式三份文件备份/哈希、自启基线、冲突实例，以及 HWND/PID、UI Ready 和保留进程句柄退出码观察能力。截图不可用不单独阻断，不能把 HWND 存在当 UI Ready。

1. 严格使用本地 native-root-plan.json 中的 EXE、参数和唯一根；仅子进程启用 meta 诊断。
2. 记录 PID、启动时间、startup_id、首个 HRESULT、ball_ready 首帧握手、窗口身份、WebView2 子进程及实际数据目录。
3. 托盘可操作后尽快 Show→Hide，观察至少五秒。隐藏使用空区域逻辑，不能单凭 IsWindowVisible 判断。
4. **只有 ball_hide 后出现“手动显隐选择已更新，跳过启动显形”日志且保持隐藏，才能将本次迟到完成场景记 PASS。** 若启动已完成，只能记普通隐藏通过，竞态原生项为 NOT_EXERCISED/UNKNOWN。当前无延迟开关，不为取得 PASS 新增生产行为；确定性时序覆盖来自真实 helper 单测。
5. 同一实例重新显示后打开设置，确认可见/可交互；只在隔离配置中切换一次“全屏隐藏”，保存并重新读盘确认。工具不能操作时可保留人工记录为 MANUAL_PASS。
6. 正常关闭设置并通过应用退出，记录真实退出码、WebView2 子树清理及对应 Error 1000/WER1001；比较正式业务配置、自启和目录。
7. 0x8000FFFF、ball_ready 超时、无效 HWND、正式配置写入等立即停止后续验收；不重启，不把人工终止记正常退出。

上述计划不包含 A01、真实剪贴板或原生备份恢复授权。

## 已知问题与剩余发布门禁

| 项目 | 当前事实 / 最短下一步 |
|---|---|
| Issue #2 启动 P1 | 保持 OPEN；历史业务 panic 栈 UNKNOWN，单次成功不能证明间歇性问题已修复。新迟到显形修复仍待上述原生回归 |
| A01 / N01 | 自动化保护通过，历史原生首次点击/生命周期 FAIL 保留；另验合成选区、计时隐藏、失效票据、旧结果和连续选区 |
| N05 | 新主动复制修复只有模拟回归，历史完整文本不匹配/WM_COPY 原生证据未闭环；先另获合成剪贴板授权，再逐 UTF-16 核验中英文、多行、Unicode、忙碌与快速操作 |
| 剪贴板系统失败边界 | 分配/锁定/创建 owner/Open/Empty 失败及授权拒绝发生在破坏前，模拟测试保留原文本/HTML/RTF/DIB；**Empty 成功后 Set 再失败仍可能留空，不保证任意格式事务回滚** |
| 备份/恢复原生门禁 | 当前隔离构建有意禁用外部文件对话框，不静默放开。另行授权在无真实数据的独立 Windows 测试环境使用普通 Release，验合成收藏/设置/完整备份、取消、预览、合并/覆盖及非法文件；失败回滚保留真实临时事务故障注入证据 |
| 交互范围 | 页面测试覆盖设置、收藏、拖动等前端流程；托盘、全屏恢复、快速开关和拖动的真实桌面表现未在新候选验证，不为清单额外重构 |
| 历史正式 config 改写 | 写入者 UNKNOWN，本轮哈希未新增变化；不恢复、不重置，不以构建成功关闭历史问题 |
| Huorong 缓存事件 | 仍为 UNATTRIBUTED_WRITE / ACCEPTED_NONBLOCKING_RISK，不继续追查全部系统缓存。契约是候选及受控子进程持久写入不得逃逸隔离根，正式业务数据与自启不得修改 |
| 发行隐私 | 普通 Release EXE 有 178 处本机构建用户路径匹配，诊断候选有 176 处；所查私钥/GitHub token 模式无命中。扫描非完整安全证明，两者均不分发；最终构建应受控 remap 后复查 |
| 最终包与许可 | 依赖和锁文件不变，现有 LICENSE、第三方 notices、WebView2/Rust 许可与 MPL 源码材料保留；新发行 ZIP 尚未冻结审查，不能只交付 EXE |
| 最终版本映射 | 最终发行源码、普通 Release 配置、EXE SHA256、私有 PDB GUID/Age 和打包清单必须绑定；诊断测试特性不能混入发行版 |

旧候选 3db108b7… 的 WebView2/球窗/UI Ready、人工设置保存、正常退出及子进程清理曾通过，真实退出码 0，正式配置/自启未变化。该候选早于本分支启动和复制修复，不代替新候选结果；旧报告与原始事件流保留。

## 安装、升级与发布构建说明

目标发布标识沿用仓库约定 **v0.1.0-beta.2**；“Beta.2.0”是本轮任务名称，package/Tauri/Cargo 产品版本仍为 0.1.0，不擅自改为 2.0.0。当前没有新 Beta.2 下载包，既有 Beta.1 链接继续对应旧版本。

1. 普通发行构建必须先构建前端，再使用 custom-protocol，显式 CARGO_INCREMENTAL=0，**不得带 beta2-isolation 或测试 identifier**。本轮普通 Release check/build 已通过，诊断变体另行保留。
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
- [ ] 单独授权并完成新候选原生显示/隐藏/UI Ready/设置/退出/子进程与数据隔离回归。
- [ ] 对 Issue #2、A01、N05、原生备份等门禁取得真实结果或维护者明确的处置。
- [ ] 最终普通发行构建 remap/隐私、完整许可证包和 EXE/PDB/源码映射复核。
- [ ] 维护者另行授权推送、合并、Tag / 资产上传 / 预发布。本轮不发布。
