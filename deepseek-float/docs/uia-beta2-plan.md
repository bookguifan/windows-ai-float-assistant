# Beta.2 划词实施计划

> 初始计划历史说明：下文“不运行系统剪贴板原生测试”是本计划建立时的范围，后续用户已另行授权合成资料原生验收。最终支持范围、27文件清单、逐轮验证和未关闭风险以[uia-beta2-validation.md](uia-beta2-validation.md)及[R01–R03报告](uia-beta2-r123-validation.md)为准；本计划保留原文，不改写历史授权与测试结论。

目标：UIA 优先；旧自动回退保持禁用，新的单次复制事务必须经过本机原生确认授权。

约束：仅公开仓库 feature/uia-safe-clipboard-fallback 分支；不提交、推送、发布，不运行系统剪贴板原生测试。失败静默，手动输入只由用户主动打开。

1. 先运行现有前端与 Rust 专项，记录环境限制。
2. 先为纯状态模块 selection.rs 写测试：目标身份、代次、有效期、最新请求槽、原文拼接、授权策略、单次复制及恢复竞争。用 rustc --test 运行以避免真实 Win32 调用。
3. 实现 selection.rs；mousehook.rs 在触发时携带上下文，独立调度线程处理 Dismiss，单一 MTA worker 读取，有界最新请求槽及超时拒绝。
4. uia.rs 使用有界焦点/命中/父级候选，只读 GetSelection；密码和身份不明拒绝；COM 接口先释放再 CoUninitialize。
5. popup.rs 将 raw/normalized/target/generation/expiry 绑定同一会话，使用前复核。新增失败不弹窗。
6. safe_copy.rs 复用 capture.rs 的有界内存读取与身份保护思路；统一策略校验、格式白名单完整快照、一次 Ctrl+C、复制来源检查及持锁条件恢复。未知格式或来源拒绝，不恢复第三方新数据。
7. IPC 只能从可见聚焦的本地 settings 请求原生风险确认；取消或关闭撤销，不持久化授权；备份不含授权。旧 IPC 仍拒绝启用。
8. 设置页同步后端状态、准确命名 UIA 开关。保留现有面板的手动输入，不自动打开输入框。
9. 运行 Rust 单测、前端专项/全量、格式、严格 Clippy、build/check；复核 diff 和私有仓库未改变。记录未执行的原生兼容性验收。

残余边界：owner/PID/序列组合不是来源证明；同进程竞争、broker 和延迟渲染仍有局限。MTA 调度超时不会强制中断 COM；阻塞时不扩线程。原生确认只证明本次运行的用户同意，不防御管理员或同权限恶意进程。
