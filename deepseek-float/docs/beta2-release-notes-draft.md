# Windows AI 悬浮助手 v0.1.0-beta.2

候选发行说明，尚未正式发布。应用内部版本为 0.1.0。

## 变更

- 修复球窗 Ready 迟到覆盖手动隐藏状态，加强划词工具条异步会话隔离。
- 加强主动剪贴板写入失败后的尽力恢复及备份输入校验。Windows 多格式剪贴板恢复不保证原子性。
- 球窗附加诊断只在 beta2-isolation 测试模式启用，普通 Release 不注册诊断 IPC。
- 沿用单一 Windows CI 和打包入口；绿色版包含必要许可证、通知及对应源码。

## 使用和已知问题

解压完整 Windows x64 ZIP 后运行 deepseek-float.exe。需要本机 WebView2 Runtime 和 Visual C++ x64 Runtime。升级前退出旧实例并保留用户配置及收藏。程序可能使用已有登录状态；AI 问答发送给配置的服务商。

Issue #2（启动后偶发异常退出）继续开放：https://github.com/bookguifan/windows-ai-float-assistant/issues/2 。尚未确认根因，一次启动成功不代表彻底修复。

## 候选状态

PR #6、#7 各自 CI 已通过，本地组合进行了自动化验证。正式发布仍须合并后的 main CI 通过，并对最终普通 Release 完成启动、UI Ready、球窗、托盘、设置页、A01、截图/OCR、AI 问答及正常退出验收。目前未启动本候选，不能宣称原生功能已经通过。

高风险剪贴板、设置保存、备份恢复、自启修改仅使用专用数据并另行确认；不以破坏正式数据为代价验收。包未签名，SHA256 仅用于内容校验；EXE 摘要见 HASH.txt，ZIP 摘要见同名 .sha256 文件。
