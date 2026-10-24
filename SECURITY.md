# 安全政策

## 当前支持与渠道状态

本仓库是 Private 源码暂存仓库，v0.1.0-beta.1 尚未发布。优先处理最新源码的问题，不承诺旧 EXE 自动更新或固定响应 SLA。

首选未来 GitHub Private Vulnerability Reporting，但目前未启用、未完成外部报送/通知/回复/权限验收；本项目尚无已指定并验证的替代安全邮箱。历史 Git 作者邮箱、Cargo 示例地址不是安全联系方式。创建本仓库不等于已经建立私密报告入口。

GitHub 将该报告能力面向 Public 仓库。待维护者另行批准公开并启用后，核实 Security and quality / Advisories 的 Report a vulnerability 入口及通知后，才更新本政策为可用。Private 阶段需要维护者提供并验证真实临时私密入口，不在此虚构联系方式。没有仓库访问权的人不能依赖本 Private 仓库的普通 Issue 联系维护者。

不要在 Issue、PR、讨论或附件发布未修复漏洞细节、凭据、真实剪贴板、Cookie、AI 对话或个人截图。渠道未建立前不要提交真实敏感报告；尚未提供或验证的渠道不视为可用。

## 建立渠道后的报送与处理

渠道建立并验证后，报告应含受影响提交/版本、Windows/WebView2 版本、最小合成复现、预期与实际行为、影响和脱敏证据。维护者私密接收、分级、复现、修复验证，并与报告者协调披露。实测需另外授权的外部账号提交明确的非真实漏洞合成报告，核对实际通知、回复及无授权访问被拒；管理员创建 draft advisory 不能代替外部报告测试。测试不申请 CVE 或发布安全公告。

## 已知边界

自动复制回退在此构建固定禁用，旧配置、备份、IPC、UI 无法解锁；UIA 与主动复制保留。不支持 UIA 的应用可能无法自动抓词。远程 AI 网页不可信，划词/截图发送受所选站点隐私政策约束；备份、收藏和导出可能含个人内容，不应上传真实用户资料。源码测试和限定原生检查不是独立安全认证。

[GitHub 官方配置说明](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository)
