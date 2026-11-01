# Beta.2.0 Windows 原生验收计划（Draft）

## 候选

- 源码：`feature/beta2-release-safety` / `2f2ab6b5cac9b8b650dd1a5c8fcd149c67bc249e`
- EXE：`deepseek-float.exe` / SHA256 `372196e9f6c2cdbc098768450126188b31ba519b50532c160e7d82aee77f5e01`
- PDB（仅诊断）：SHA256 `662d172e6e16a0dcb563e8fb6c4a647e93c0d91ecb3685ca1eda8c49479ec545` / GUID-Age `c57a5122-1d56-4fff-a6e1-883780a4c819 / 1`
- Build features：`custom-protocol`；不含 `beta2-isolation`、`experimental-clipboard-fallback`；Release 自动 Ctrl+C 回退关闭。
- 本计划尚未启动该 EXE，启动计数为 0。

## 启动隔离前置条件

普通 Release 使用正式 Tauri identifier，不能仅设置 `LOCALAPPDATA` 就声称测试数据隔离。先在独立 Windows 测试账户，或已安装并启用的 Windows Sandbox 内运行；确认配置、收藏、提示词、日志、WebView2 profile 和备份位置都归属于该测试环境。基线先保留在测试账户的独立证据目录，不接触维护者正式配置。候选不具备可用隔离环境时，不启动。

本机未发现 `WindowsSandbox.exe`，当前没有第二个已验证的隔离用户配置，因此本计划目前为 **BLOCKED**。不通过切换环境变量、重用旧 Known Folder 或创建新的 identifier 数据目录来绕过。

## 原生验收顺序

1. 验 EXE/PDB SHA256 与 GUID/Age，再记录测试配置下三份业务文件、自启项和原有用户数据基线。
2. 启动一次候选，记录 PID、时间、进程子树、WebView2 首个 HRESULT、实际数据路径、球窗 HWND/可见性和 `ball_ready`。
3. 验悬浮球拖动、显示/隐藏、设置页打开/保存/重读/关闭及应用正常退出；保存时只改测试账户配置。
4. 用测试文本和测试应用做 A01 合成选区，验计时隐藏、Alt+Tab 行为、旧票据失效、快速重复选择及异步旧结果拒绝。
5. 仅在用户另外授权后，保存测试专用剪贴板内容，再执行纯文本、中文/英文、多行、Unicode、HTML/RTF、图像、占用与发布失败恢复；核验成功、完整/部分恢复、失败格式及资源释放，最后还原测试基线。
6. 在测试账户临时目录做收藏导出、备份/取消、预览、合法恢复与非法文件拒绝；失败回滚后逐文件核验测试数据。
7. 正常退出，记录真实退出码、WebView2 子进程清理和应用事件；对比测试账户的正式文件基线及自启项。异常时保留首个失败阶段，不自动重启。

## 当前状态

- 本机候选启动：**NOT RUN**。本轮没有操作真实系统剪贴板、没有改写真实用户配置、没有执行原生备份恢复。
- 自动化/内存 facade 只证明相应测试边界，不代表桌面窗口、原生剪贴板或文件选择器验收通过。
- 取得隔离测试账户/Windows Sandbox 后，应以此处冻结的 EXE SHA 为唯一候选；若候选身份变更，重新生成本计划并另作启动授权。
- Issue #2 保持 OPEN；有限次成功启动不证明间歇性异常已修复。
