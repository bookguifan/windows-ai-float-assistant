# 前端 ↔ Rust 接口契约表（`deepseek-float`）

> 维护约定：**任何新增 / 改名 / 改参的全局函数或 IPC 命令，必须同步更新本表**。
> 以当前 `lifecycle.rs::generate_handler!`、`ipc.rs` 和页面导出函数为准；本表描述接口，不代表所有 Windows 场景已完成原生验收。
>
> 评审依据：逐行审阅 `lib.rs` / `lifecycle.rs` / `ipc.rs` / `inject.rs` /
> `dsfloat_init.js` / `result.html`，以及 `capabilities/default.json`。

---

## 0. 总体边界

| 通道 | 方向 | 机制 | 信任模型 |
|---|---|---|---|
| 注入脚本全局函数 | Rust → 页面 | Rust `eval_with_callback` 调用 `window.__DSFLOAT__.*` | 项目未向远端声明 remote capability；敏感自定义命令仍需后端来源与授权校验，见 §4 |
| 结果卡全局函数 | Rust → 本地结果页（同步返回测量值） | 页面内 `window.__dsfResult*` 被 Rust `eval` 调用，返回测量高度 | 同单向边界 |
| IPC 命令 | 双工 | 页面 `invoke('cmd', args)` ↔ Rust `#[tauri::command]` | 命令注册与参数校验见 §4；敏感入口还校验来源、选区票据或原生授权 |

**关键约束**：动态字符串必须经 `serde_json::to_string` / `json!` 等结构化序列化后传入脚本，参见 `inject.rs`；不能把未经转义的选区、提示词或网址拼入 JS。

---

## 1. 窗口类型（创建及显隐以实现为准）

| 窗口 id | 用途 | 创建位置 | 备注 |
|---|---|---|---|
| `ball` | 悬浮球（64×64，**不透明窗 + 椭圆区域裁剪** / 置顶 / 可拖拽） | 普通构建由 `tauri.conf.json` 预建；隔离测试可延后创建 | 仅样式页 `poc/ball.html`（不注入 `dsfloat_init.js`）；显隐由 `panels::ball_show` / `panels::ball_hide` 套用 `ballstyle::apply_ball_shape` 区域。★ **启动期显形须经 `ball_ready` 握手（≤3s 兜底）**，握手期间维持**零面积区域**驱动首帧 |
| `panel` | 内嵌 DeepSeek 网页端（隐藏，懒导航） | `panels::setup_panel` | 注入 `dsfloat_init.js`；登录态持久化 |
| `popup` | 划词工具条（含 `__dsfSetPrompts` / `__dsfResetFired`） | `panels::setup_popup_and_mousehook` | 全局鼠标钩子 `WH_MOUSE_LL` 触发 |
| `settings` | 设置页 | `setup` | 收 `settings_save` / `settings_run` |
| `shot` | 截图遮罩（含 `__dsfShotReset`） | `screenshot::ensure` | `shot_commit` / `shot_cancel` 交付 |
| `result` | 结果卡（本地页面，Dev 由开发服务器提供） | `result::ensure` | 含 `__dsfResultSet/Stream/Reset` |
| `context-menu` | 自绘圆角右键菜单 | `context_menu::ensure` 在 setup 预建，右键只更新/显示 | 先 `context_menu_ready` 握手，后渲染显形 |

---

## 2. 注入脚本全局函数（`panel` 窗口：`window.__DSFLOAT__`）

> 定义于 `src-tauri/src/dsfloat_init.js`，由 Rust 侧 `eval` 调用。
> 不可枚举（`Object.defineProperty` 设为非 enumerable），但属性访问可达（Rust eval 走属性访问，不受影响）。

| 方法 | 入参 | 返回 | 用途 |
|---|---|---|---|
| `fill(text)` | 待填文本 | — | 把划词内容填入 DeepSeek 输入框 |
| `submit()` | — | — | 提交当前输入框 |
| `pressEnter()` | — | — | 合成回车（兜底提交路径） |
| `stillHasText()` | — | `bool` | 输入框是否仍有未提交文本（防重复发送） |
| `inspect()` | — | 诊断对象 | 页面侧自检读数 |
| `inspectUpload()` | — | 诊断对象 | 定位上传入口（探针） |
| `inspectConversation()` | — | 诊断对象 | 定位会话容器 + `splitCache` 命中率（性能核验） |
| `snapshotConversation(prompt, wantDump)` | 问题与诊断开关 | 快照对象 | 回读尾部问答及角色、请求绑定等证据 |
| `prepareSend(prompt, refresh)` | 问题、是否刷新基线 | bool | 保存本轮消息基线 |
| `sendEvidence(prompt)` | 当前问题 | sent / unknown | 结合本轮回显确认发送；unknown 不等于可安全重发 |
| `readText()` | — | 字符串 | 读取当前输入框文字 |
| `newChat()` | — | 见实现 | 尝试站点新会话动作，由 Rust 编排取消旧任务 |
| `shotReset()` | — | bool | 清空分片并记录旧附件基线 |
| `shotPush(chunk, seq)` | base64 分片与序号 | bool | 严格顺序接收，不接受重复或乱序 |
| `shotCommit(mime)` | MIME | bool | 将分片作为图片投递到当前上传入口；不等于上传成功 |
| `hasAttachment()` | — | bool | 确认新增可见附件且无活动上传，才能继续自动 OCR |

> 扩展方法（如 `serializeHtml` / `serializeCodeText` / `splitBlock`）为内部纯函数，
> **不**经 `window.__DSFLOAT__` 暴露，仅被上述方法在页内调用。
>
> 快照包含 `roles`（assistant/user/unknown）和 `requestBound`。站点语义路径当前返回 splitV=6，通用回退路径为 5；不能只用一个版本号证明本轮绑定或完整回答。注入目标仍为 panel 的初始化脚本，本地球页独立上报 ball_ready；属性不可枚举不构成对远端脚本的保密边界。

---

## 3. 结果卡全局函数（`result` 窗口）

定位由 Rust `result.rs` 管理：新划词消费本次选区锚点，流式更新与追问不重新消费光标位置，保留用户拖动位置及必要的工作区边缘避让。多屏不同 DPI 尚无实机通过声明。

> 定义于 `poc/result.html`，被 Rust `eval` 调用以驱动渲染并**回传测量高度**。

| 函数 | 入参 | 返回 | 用途 |
|---|---|---|---|
| `__dsfResultSet(payload)` | 渲染载荷 | `offsetHeight` | 整段结果渲染完成，回报期望高度 |
| `__dsfResultStream(payload)` | 流式片段 | `offsetHeight` | 流式更新，回报当前高度 |
| `__dsfResultReset()` | — | — | 清空当前结果卡会话 |

**结果卡 → Rust 的调用**：结果卡内用 `invoke('cmd', args)` 调 IPC（外链 `result_open_external`、
复制 `result_copy`、追问 `result_ask` 等）。点击委托挂在 `elThread` 上，拦截 `<a>`：
`http(s)` 走 `result_open_external`（系统浏览器），相对/`#` 链接 `preventDefault`（防卡片自导航）。

---

## 4. IPC 命令清单（定义于 `ipc.rs`，注册于 `lifecycle.rs`）

| 命令 | 所属窗口 | 用途 | 备注 |
|---|---|---|---|
| `toggle_panel` | ball/panel | 显隐面板 | — |
| `ball_contextmenu` | ball | 右键菜单动作 | — |
| `ball_ready` | ball | 悬浮球首帧就绪握手 → `ballready::signal_ball_ready()` | **幂等**；启动期 `windows::gate_ball_show` 等待它（**≤3s 兜底**）后才套椭圆区域显形 |
| `popup_action` | popup | `{ preset, selectionTicket }` | 来源须为可见本地 popup；票据须可解析且仍有效 |
| `popup_ready` | popup | `{ selectionTicket }` | 隐藏工具条已安装当前控件/票据后握手，再安排原生显示 |
| `settings_save` | settings | `{ prompts: Prompt[] }` | 成功返回规范化后的 `Prompt[]`；失败返回错误，前端保留最新修改。串行保存，只有最新快照成功才显示已保存 |
| `panel_sites_save` | settings | `{ sites: {label,url}[] }` | 保存自定义站点清单，成功返回规范化列表；最多20项、名称1至32字、地址去重；仅落盘成功后更新缓存，不导航 |
| `settings_run` | settings | `{ id, selectionTicket }` | 领取当前设置选区，拒绝过期动作 |
| `settings_ready` | settings | 无显式参数 → 当前选区快照或 null | 只由可见本地设置页读取 |
| `config_get` | 全局 | 读 `AppConfig` | `autostart` = 注册表真实值 **且** 本构建支持（dev 构建恒为 `false`） |
| `autostart_supported` | 全局 | 本构建是否支持开机自启 | 纯编译期判定：仅 release + `custom-protocol` 为 `true` |
| `config_set_autostart` | 全局 | 开关自启 | dev 构建下 `enable=true` 必定返回 `false`（拒绝登记）；`false` 始终删除 |
| `config_set_fullscreen` | 全局 | 开关全屏隐藏 | — |
| `config_set_privileged` | 设置页 | `{ enable }` → `bool` | 持久化智能划词主开关；关闭时撤销复制授权和旧工作 |
| `config_set_clipboard_fallback` | 旧兼容命令 | `{ enable: bool }` → `Result<(), String>` | true 拒绝，false 可持久化；不用于授予现行回退权限 |
| `safe_copy_status` | 设置页 | 无参数 → `State` | 返回主开关、enabled/available、mode、shortcut、last_error/last_notice、policy_version、session_only |
| `safe_copy_authorize` | 本地设置页 | `{ mode?, shortcut? }` → `State` | 原生确认后才授权；须聚焦可见本地设置页，主开关开启；缺省 compatibility / ctrl-insert |
| `safe_copy_revoke` | 设置页 | `{ mode? }` → `State` | 缺省 off；仅接受 off/safe，不能藉此启用 compatibility；撤销并失效旧选区 |
| `config_set_panel_url` | 全局 | 改面板 URL | 统一入口校验并取消旧工作流后原生导航；导航失败返回错误 |
| `set_hotkey` | 设置页 | `{ spec }` → 实际热键字符串 | 截图提问；空串关闭，注册或保存失败回滚 |
| `hotkey_status` | 设置页 | 无参数 → 实际热键或 null | 区别于 config 中期望的 hotkey |
| `set_ocr_hotkey` | 设置页 | `{ spec }` → 实际热键字符串 | 独立 OCR 热键；空串关闭，冲突保留旧键 |
| `ocr_hotkey_status` | 设置页 | 无参数 → 实际 OCR 热键或 null | 新安装/旧配置缺字段默认不注册 |
| `context_menu_ready` | context-menu | 无参数，页面就绪握手 | Rust 有待展示请求时渲染；未就绪不显示空窗口 |
| `context_menu_action` | context-menu | `{ generation, action }` | action 为 show_ball/hide_ball/show_panel/hide_panel/screenshot/ocr/settings/relogin/quit；仅当前代次消费一次 |
| `context_menu_switch_site` | context-menu | `{ generation, url }` | 校验当前菜单代次与网址；失败不消费代次，成功取消旧任务并切换 |
| `context_menu_close` | context-menu | `{ generation }` | 过期代次忽略；校验和窗口隐藏统一在UI线程 |
| `shot_commit` | shot | `{ captureId, revision, rect: { x, y, w, h, dpr, vw, vh } }` 生成预览 | 返回 `{captureId, revision, pngBase64, width, height}`；仅当前 Selecting 可提交。按客户区物理原点/缩放换算，此时不贴图或发送 |
| `shot_preview_ready` | shot | `{ captureId, revision, failed }` 图片加载握手 | 旧版本忽略；加载成功才允许确认，失败提供重选或取消 |
| `shot_confirm` | shot | `{ captureId, revision }` 确认预览 | 消费当前 PNG 一次；Ask 仅贴图，Ocr 贴图后发送识字提示词 |
| `shot_reselect` | shot | `{ captureId, revision }` 重选 | 返回下一 revision，保留 captureId、模式、几何及球恢复归属 |
| `shot_cancel` | shot | `{ captureId, revision }` 取消 | 立即失效并清除缓存；worker 清理完成前继续占用，拒绝旧回调 |
| `result_close` | result | 关闭结果卡 | — |
| `result_resume` | result | `{ request }` 恢复当前请求回读 | 仅重启读取，不重发；拒绝旧请求，返回 `ok`/`no-resume`/`panel-missing`/`busy` |
| `result_retry_send` | result | `{ request }` 重试未确认发送 | 仅当前失败请求可领取一次；完整prompt由Rust保留，不取截断卡片文本 |
| `result_copy` | result | 复制结果 | — |
| `result_ask` | result | 追问 | `TurnMode::Follow` 后 `begin_watch` Update |
| `result_drag_start` | result | 开始拖拽 | 手动阈值（非 `data-tauri-drag-region`） |
| `result_resize` | result | 重测高度 | — |
| `result_new_session` | result | 新会话 | 取消旧任务及恢复点、清空、原生导航；返回Result并反馈错误 |
| `result_favorite_save` | result | `{ revision, turn: { index, answerText, markdown } }` | 校验会话版本、轮次及原答案；有正文且非 streaming 才可收藏（允许超时部分回答）；问题、标题及来源取自 Rust Session，重复收藏幂等 |
| `result_export` | result | `{ revision, turns: [{ index, answerText, markdown }], whole }` | 单轮或整会话导出；整会话验证轮次数量、顺序和原答案；返回文件完整路径，不覆盖同名文件 |
| `favorites_list` | result | 无参数 | 返回本地收藏列表，读盘损坏或失败明确报错 |
| `favorites_delete` | result | `{ id }` | 删除对应收藏并持久化，失败反馈 |
| `favorites_export` | result | `{ id }` | 导出对应收藏，返回文件路径；已删除则报错 |
| `backup_export` | settings | 无参数 → 导出结果或 null | 原生保存对话框；取消无写入 |
| `backup_import_preview` | settings | 无参数 → 预览或 null | 原生打开对话框；读取校验后返回 ticket，不直接恢复 |
| `backup_import_cancel` | settings | `{ ticket }` | 失效待导入任务 |
| `backup_restore` | settings | `{ ticket, mode, confirmOverwrite }` → 恢复结果 | merge/overwrite；覆盖须二次确认；成功后撤销复制授权并刷新页面 |
| `result_open_exports_dir` | result | 无参数 | 打开应用实际数据目录下的 exports |
| `result_open_external` | result | 系统浏览器打开外链 | 仅放行 `http(s)`（纵深防御） |

> **权限说明**：`capabilities/default.json` 只向列出的本地窗口配置 core 权限，不声明远程 capability。自定义命令通过 generate_handler 注册；敏感入口的来源、参数、Session/Ticket、风险授权与事务校验仍须由后端保持，不能用“命令已注册”代替授权。
>
> 以下仅供测试诊断，不属于普通 Release 的可用命令：
>
> | 命令 | 编译条件 | 约束 |
> | --- | --- | --- |
> | `ball_diagnostic` | beta2-isolation | 球页、诊断开启、固定 stage 白名单 |
> | `popup_trace` | Debug 或 beta2-isolation | 本地工具条和合成诊断开关，固定阶段/动作 |

---

## 5. 其他窗口全局函数（popup / shot，定义于各自前端源）

| 函数 | 窗口 | 用途 |
|---|---|---|
| `__dsfSetPrompts` | popup | 注入提示词列表（Rust `popup.rs` 经 `serde_json` 转义填值） |
| `__dsfResetFired(ticket)` | popup | 更新当前选区票据并复位一次性动作锁 |
| `__dsfPrepareSelection(ticket, prompts)` | popup | 安装提示词和票据、收起菜单，再调用 popup_ready |
| `__dsfShowMore()` / `__dsfHideMore()` | popup | 同步展开/收起菜单的页面状态 |
| `__dsfMenuSet(view)` | context-menu | 接收 generation、ballVisible、panelVisible、hotkey、shotBusy、panelUrl、sites；更新文案与禁用态，旧代次忽略；返回是否渲染成功 |
| `__dsfShotReset(captureId, revision, mode)` | shot | 初始化新任务和固定 Ask/Ocr 模式，旧编号忽略；无编号时不能提交 |
| `__dsfShotReselect(captureId, revision)` | shot | 仅当前任务接受新的选区版本，清空旧预览并回到框选 |
| `__dsfShotInvalidate(captureId)` | shot | 新会话、换站点或取消时失效旧任务及异步图片回调 |

> 这些函数的完整签名见 `popup` / `shot` 的前端源（评审中已确认跨窗口 `eval` 均经
> `serde_json` / `json!` 转义，无注入面），本表只记录契约名。

---

## 6. 变更检查清单

新增 / 修改任一契约项时，确认：

1. Rust 侧：`lifecycle.rs::generate_handler!` 注册（命令）/ `inject.rs` 调用（全局函数）；
2. 前端侧：对应 `window.__*` 定义与返回语义；
3. 转义：任何把 Rust 字符串塞进 JS 的路径必须走 `serde_json::to_string`（`lit` / `js_string`）；
4. 文档：本表同步更新；
5. 测试：`tests/sanitize.test.mjs`（净化）或新增契约冒烟测试覆盖；
6. **改球窗显形路径后**验证：Ready 超时仍走兜底，但主线程执行前必须复核显隐代次，不覆盖等待期间用户的隐藏选择；保持原生无激活显形与窗口区域处理，避免重建标题栏；
7. 修改页内快照结构时同步评估 splitV 和 Rust 解析，检查 roles、requestBound 及发送证据；不能只凭版本号断定本轮结果有效。


当前 Beta.2：旧 clipboard_fallback_enabled 仍规范为 false，但普通 Release 包含独立的受保护自动复制能力；通过 safe_copy_* 查询及原生授权，不能读旧布尔字段判断开关是否启用。授权、mode 和复制快捷键属于本次运行状态，旧配置或备份不能授予权限。复杂格式经授权后可能覆盖且不备份/不恢复，详见[已知限制](beta2-known-issues.md)。旧首版策略保存在[已发布 Tag 的接口文档](https://github.com/bookguifan/windows-ai-float-assistant/blob/v0.1.0-beta.2/deepseek-float/docs/frontend-rust-contract.md)。
