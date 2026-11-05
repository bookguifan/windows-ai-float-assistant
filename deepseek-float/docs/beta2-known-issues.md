# Beta.2 已知问题与验证范围

更新：2026-11-05。已发布 [v0.1.0-beta.2 Pre-release](https://github.com/bookguifan/windows-ai-float-assistant/releases/tag/v0.1.0-beta.2)，源码 `a67fa0d1c84989e995d023867a98bff304e0e1e9`。Beta 不是稳定版，不承诺所有 Windows 软件兼容。

| 问题或限制 | 当前结论 |
| --- | --- |
| [Issue #2：偶发启动异常退出](https://github.com/bookguifan/windows-ai-float-assistant/issues/2) | 继续开放，根因未彻底定位；本次成功启动不代表已经修复。保留[原始调查](beta2-startup-crash-analysis.md)。 |
| 自动复制回退 | 默认关闭，须本次运行的原生风险确认。Ctrl+Insert 默认，Ctrl+C 可选；一次只发送所选一种，不自动换键。关闭主开关/回退或重启均撤销授权。 |
| 复杂格式剪贴板 | HTML、RTF、图片、文件等经授权后允许尝试复制，但不备份、不恢复，原内容可能被覆盖且无法找回。空或受支持文本仅尽力恢复；异步或第三方并发写入时可能拒绝采纳或跳过恢复。 |
| WPS、PDF 及其他自定义控件 | 本机 WPS Writer 的 Ctrl+Insert、WPS PDF 的 Ctrl+C 已验证；该 PDF 的 Ctrl+Insert 未取得有效选区。不能推广到所有版本、文档或软件。 |
| 偶发复制结果读取失败 | 最终 Release 的 HTML 样本首轮发生一次读取失败，独立干净样本复测通过。保留首轮失败，不声称已经根治。 |
| 多屏不同 DPI | 尚未实机验证；本机结果卡跟随新划词、拖动后流式更新/追问保持位置及屏幕边缘可见已验证。 |
| 浏览器、VS Code、Microsoft Word 等 | 未完成受控原生兼容矩阵，不宣称全部支持。 |
| AI 网站变化 | 发送、附件上传、回答回读依赖站点结构、登录状态及网络。本次只对已认可的 DeepSeek 账号和合成内容做了冒烟。 |

## 已发布候选的验证

最终 main CI 成功；普通 Release 已人工辅助验证启动/UI Ready、球窗与托盘、记事本 NativeEdit、WPS 双快捷键和纯文本恢复、授权后 HTML/合成图片复制及不恢复警告、结果卡、AI、自动 OCR、截图提问与正常退出。主进程及相关 WebView2 子进程均已结束。收藏和提示词未变，配置仅改变获准记忆的球位置。

这些结果只适用于已验收文件，不代替后续改动的测试，也不等于完整跨软件或无损剪贴板验收。历史 N01/N05、WM_COPY、R01–R03 等阶段记录通过[固定 Tag 索引](archive/beta2/README.md)追溯；不把旧 FAIL、UNKNOWN 或未测项改写为 PASS。

遇到可稳定复现的无法启动/退出、错误目标复制或严重数据破坏，请停止相关操作并通过 Issues 提供版本、目标软件、最小步骤和脱敏现象，不提交账号或私人剪贴板内容。
