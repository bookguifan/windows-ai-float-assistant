# 前端 ↔ Rust 接口契约表（`deepseek-float`）

> 维护约定：**任何新增 / 改名 / 改参的全局函数或 IPC 命令，必须同步更新本表**。
> 这是《全栈架构评审报告》A1 项的落地：把散落在注释里的隐式契约收拢到单一清单，
> 降低"改了 Rust 忘了前端"或"改了前端忘了 Rust"的耦合风险。
>
> 评审依据：逐行审阅 `lib.rs` / `lifecycle.rs` / `ipc.rs` / `inject.rs` /
> `dsfloat_init.js` / `result.html`，以及 `capabilities/default.json`。

---

## 0. 总体边界

| 通道 | 方向 | 机制 | 信任模型 |
|---|---|---|---|
| 注入脚本全局函数 | Rust → 页面 | Rust `eval_with_callback` 调用 `window.__DSFLOAT__.*` | 单向：仅 Rust 驱动远端，远端**不能**回调 Tauri 命令（无 remote capability 即被拒） |
| 结果卡全局函数 | 页面 → Rust | 页面内 `window.__dsfResult*` 被 Rust `eval` 调用，返回测量高度 | 同单向边界 |
| IPC 命令 | 双工 | 页面 `invoke('cmd', args)` ↔ Rust `#[tauri::command]` | 自定义命令**默认放行**（见 §3），无需显式 capability |

**关键约束（务必遵守）**：所有动态 JS 构造走 `serde_json::to_string`（`inject.js` 的
`lit` / `js_string` 硬约束），**绝不手工字符串拼接**（grep 全量核对 18 处 `eval` 站点均符合）。

---

## 1. 窗口清单（7 个常驻窗口，只 show/hide 不销毁）

| 窗口 id | 用途 | 创建位置 | 备注 |
|---|---|---|---|
| `ball` | 悬浮球（64×64，**不透明窗 + 椭圆区域裁剪** / 置顶 / 可拖拽） | `tauri.conf.json`（config 窗口，先于 setup） | 仅样式页 `poc/ball.html`（不注入 `dsfloat_init.js`）；显隐由 `panels::ball_show` / `panels::ball_hide` 套用 `ballstyle::apply_ball_shape` 区域。★ **启动期显形须经 `ball_ready` 握手（≤3s 兜底）**，握手期间维持**零面积区域**驱动首帧 |
| `panel` | 内嵌 DeepSeek 网页端（隐藏，懒导航） | `panels::setup_panel` | 注入 `dsfloat_init.js`；登录态持久化 |
| `popup` | 划词工具条（含 `__dsfSetPrompts` / `__dsfResetFired`） | `panels::setup_popup_and_mousehook` | 全局鼠标钩子 `WH_MOUSE_LL` 触发 |
| `settings` | 设置页 | `setup` | 收 `settings_save` / `settings_run` |
| `shot` | 截图遮罩（含 `__dsfShotReset`） | `screenshot::ensure` | `shot_commit` / `shot_cancel` 交付 |
| `result` | 结果卡（本地 `custom-protocol` 页） | `result::ensure` | 含 `__dsfResultSet/Stream/Reset` |
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
| `snapshotConversation()` | — | 快照字符串 | 回读：序列化尾窗若干块的问答文本 |

> 扩展方法（如 `serializeHtml` / `serializeCodeText` / `splitBlock`）为内部纯函数，
> **不**经 `window.__DSFLOAT__` 暴露，仅被上述方法在页内调用。
>
> ★ **页内契约版本（#004）**：`snapshotConversation` 返回对象新增
> **`splitV: 5`**（契约版本号，由 `4` 升到 `5`）与 **`roles`**（块角色字段，与 `texts` / `thinking`
> 逐项对齐，取值 `assistant` / `user` / `unknown`）。
> · `splitV` 是**「新脚本是否在线」的判据**：日志出现 `splitV=5` 即证明新脚本已运行；
>   缺它 ⇒ **强制刷新面板页面 / 重建 WebView**（旧脚本会静默失效、修复看似无效）。
> · `roles` 供 Rust 侧**结构化拒绝回显**（只在正向识别 `user` 时拒绝交付）。
> · ★ **`dsfloat_init.js` 只注入 `panel` 窗**：`windows.rs:410` 是唯一的
>   `.initialization_script(include_str!("dsfloat_init.js"))`，**不注入球窗**
>   （`poc/ball.html` 自带独立内联脚本，经页内 `invoke('ball_ready')` 上报首帧）。

---

## 3. 结果卡全局函数（`result` 窗口）

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

## 4. IPC 命令清单（24 个 `#[tauri::command]`，定义于 `ipc.rs`）

| 命令 | 所属窗口 | 用途 | 备注 |
|---|---|---|---|
| `toggle_panel` | ball/panel | 显隐面板 | — |
| `ball_contextmenu` | ball | 右键菜单动作 | — |
| `ball_ready` | ball | 悬浮球首帧就绪握手 → `ballready::signal_ball_ready()` | **幂等**；启动期 `windows::gate_ball_show` 等待它（**≤3s 兜底**）后才套椭圆区域显形 |
| `popup_action` | popup | 工具条动作 | — |
| `settings_save` | settings | `{ prompts: Prompt[] }` | 成功返回规范化后的 `Prompt[]`；失败返回错误，前端保留最新修改。串行保存，只有最新快照成功才显示已保存 |
| `panel_sites_save` | settings | `{ sites: {label,url}[] }` | 保存自定义站点清单，成功返回规范化列表；最多20项、名称1至32字、地址去重；仅落盘成功后更新缓存，不导航 |
| `settings_run` | settings | 设置页运行动作 | — |
| `config_get` | 全局 | 读 `AppConfig` | `autostart` = 注册表真实值 **且** 本构建支持（dev 构建恒为 `false`） |
| `autostart_supported` | 全局 | 本构建是否支持开机自启 | 纯编译期判定：仅 release + `custom-protocol` 为 `true` |
| `config_set_autostart` | 全局 | 开关自启 | dev 构建下 `enable=true` 必定返回 `false`（拒绝登记）；`false` 始终删除 |
| `config_set_fullscreen` | 全局 | 开关全屏隐藏 | — |
| `config_set_privileged` | 全局 | 开关特权捕获 | URL 经 `is_valid_panel_url` 校验 + JSON 转义 |
| `config_set_clipboard_fallback` | 设置页 | `{ enable: bool }` → `Result<(), String>` | 实验性自动复制回退；默认/缺字段为false；使用try_update，写盘失败不改变缓存，页面回滚并提示；不控制UIA成功路径或主动复制 |
| `config_set_panel_url` | 全局 | 改面板 URL | 统一入口校验并取消旧工作流后原生导航；导航失败返回错误 |
| `set_hotkey` | 全局 | 改热键 | — |
| `hotkey_status` | 全局 | 热键泵状态 | — |
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
| `result_open_exports_dir` | result | 无参数 | 打开 `%LOCALAPPDATA%/com.deepseekfloat.desktop/exports` |
| `result_open_external` | result | 系统浏览器打开外链 | 仅放行 `http(s)`（纵深防御） |

> **权限说明**：自定义命令沿用本项目默认放行约定（`capabilities/default.json` 仅声明
> `core:default` 与窗口拖拽权限），**无需**为每个命令单独加 capability。新增命令直接
> 在 `lifecycle.rs::generate_handler!` 注册即可。

---

## 5. 其他窗口全局函数（popup / shot，定义于各自前端源）

| 函数 | 窗口 | 用途 |
|---|---|---|
| `__dsfSetPrompts` | popup | 注入提示词列表（Rust `popup.rs` 经 `serde_json` 转义填值） |
| `__dsfResetFired` | popup | 重置"已触发"标记 |
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
6. **改球窗显形路径后**必须验证两条：① 超时（`wait_ball_ready` 返回 `false`）时**照常显形**，
   绝不因超时把球藏起来；② 显形**未调 Tauri 的 `show()`**（会重建 `WS_CAPTION` 出边框，
   只能走原生 `ShowWindow(SW_SHOWNOACTIVATE)`）；
7. **改页内契约后**必须同步升 `splitV`（当前 `5`），并在日志里看到 `splitV=N` 才算新脚本生效；
   契约新增字段（如 `roles`）须同步更新本表 §2。


首版公开构建策略（2026-10-24）：config_get的clipboard_fallback_enabled规范为false；config_set_clipboard_fallback(true)返回明确错误，false仍可持久化。旧布尔字段兼容但无启用权，备份合并/覆盖后false，设置控件固定禁用；UIA与主动复制不受此能力约束。
