# Windows AI 悬浮助手 v0.1.0-beta.2

Windows x64 绿色版：解压完整 ZIP，运行 deepseek-float.exe。请保留同目录的许可证、通知及源码材料。

需要本机 WebView2 Runtime 和 Microsoft Visual C++ x64 Runtime；本包不捆绑这些运行时。
- WebView2：https://developer.microsoft.com/microsoft-edge/webview2/
- Visual C++：https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist

升级前正常退出旧实例。程序会使用当前用户的正式配置、收藏和 WebView2 登录数据；不要删除 AppData 来升级或排障。AI 请求会发送给所配置的服务商，请避免提交敏感内容。

本候选尚待合并后 main CI 和普通 Release 原生验收。版本变化及已知问题见 RELEASE_NOTES.md。Issue #2 保持开放：https://github.com/bookguifan/windows-ai-float-assistant/issues/2

HASH.txt 是 EXE SHA256；CHECKSUMS.csv 覆盖包内文件（不包含自身）；ZIP 外的 .sha256 用于检查完整下载包。摘要校验不等于发布者签名。

项目许可见 LICENSE；第三方许可及适用范围见 THIRD_PARTY_NOTICES.md、components.csv、external-components.csv。必要原文在 licenses/、native/、toolchain/，对应源码归档和获取说明在 sources/、SOURCES.md、mpl-correspondence/。这些材料应随程序保留。
