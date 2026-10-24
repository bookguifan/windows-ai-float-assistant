# Windows AI 桌面悬浮助手

**Private 源码暂存仓库，尚未公开或发布 v0.1.0-beta.1。** 本仓库从审核源码创建全新根历史，不包含旧 Git 历史或历史 EXE。原创权利已由维护者明确确认；私密安全报告渠道尚未验收，EXE 与公开发布仍未放行。

## 功能与架构

Windows桌面悬浮球与网页面板，支持划词提示词、截图提问/识字入口、回答回流、多轮追问、收藏搜索与Markdown导出、配置与收藏备份恢复。AI由当前所选第三方网站提供，不能保证所有网站版本或所有应用兼容。

Tauri v2 + Rust + WebView2，前端为原生HTML/CSS/JavaScript，Vite参与构建。Win32提供窗口、鼠标钩子、UI Automation和剪贴板能力。远程panel不应获得本地Tauri命令权限。见[接口契约](deepseek-float/docs/frontend-rust-contract.md)和[备份恢复](deepseek-float/docs/backup-restore-qa.md)。

## 自动抓词与主动复制

- 首版公开构建**固定禁用自动复制回退**，不只是默认false。旧配置显式true在读取时归零，保存/备份导出规范为false；合并与覆盖恢复都不能启用；设置控件禁用，后端开启请求返回错误。
- 集中构建策略没有环境变量、命令行或Cargo feature解锁入口；底层自动捕获入口在读取剪贴板、绑定目标或投递按键之前拒绝，后续新增调用者也不能靠跳过UI恢复危险路径。
- UIA优先策略保留。自动抓词须开启“提权划词”并由目标提供TextPattern；目标不支持时可能没有工具条或即时原因。用户主动复制保留，仍按有效UIA选区的原文复制。
- 旧字段仍接受布尔类型以兼容，但旧值不再授权能力。字段类型错误仍拒绝，备份其他偏好、收藏和Markdown语义不变。

## 剪贴板安全边界

旧自动复制回退实现存在未完整验证的同进程竞争、精确临界点、延迟渲染及写入失败边界；历史FAIL和旧产物结论保留。这些自动回退路径在本公开构建不可触发，不声称算法本身已修复。用户主动复制按显式意图写剪贴板，不被自动回退策略拦截，也不把其成功等同其他全部剪贴板行为已通过。

## 环境

- Windows开发目标为x86_64-pc-windows-msvc；Windows10/11和不同DPI的覆盖不能从一次构建推出。
- Node.js按package.json要求为^24.15.0，pnpm固定9.15.9。
- 既有验证使用Rust1.98.1。Cargo.toml的rust-version=1.77.2不代表完整锁定依赖图的MSRV已通过验证；依赖有更高要求。
- 安装MSVC C++构建工具、Windows SDK和WebView2 Runtime。源码不捆绑Runtime，参考[Tauri Windows要求](https://v2.tauri.app/start/prerequisites/#windows)。全新/受限Windows账户兼容性仍需单独验收。

## 开发和本地构建

仓库地址：https://github.com/bookguifan/windows-ai-float-assistant （当前 Private，需被授予访问权限）。取得源码后：

```powershell
cd deepseek-float
pnpm --version                       # 9.15.9
pnpm install --frozen-lockfile
pnpm tauri dev
```

pnpm dev只启动前端；完整桌面测试使用pnpm tauri dev。Rust修改需要重编译，先从菜单退出旧实例，避免单实例互斥唤醒旧程序。不要使用日常AI账户或真实剪贴板做专项测试。

本地生产链接检查可复用已有脚本，**不交付、不签名**：

```powershell
node scripts/build-release.mjs --no-deliver
```

脚本先build前端，再cargo build --locked --release --features custom-protocol，并检查六页资源及pnpm check。未传--sign不会签名，--no-deliver不写release目录；可用独立CARGO_TARGET_DIR隔离本地构建输出。仅cargo build --release缺少custom-protocol会被拒绝。本地生成EXE不等于获准发布。

## 源码验证

```powershell
pnpm build
pnpm check
node tests/settings-workflows.test.mjs
node tests/backup-ui.test.mjs
node tests/business-logic.test.mjs
node tests/favorites-export.test.mjs
node tests/release-security.test.mjs
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --offline --locked --release --features custom-protocol --manifest-path src-tauri/Cargo.toml --lib config::tests:: -- --test-threads=1
cargo test --offline --locked --release --features custom-protocol --manifest-path src-tauri/Cargo.toml --lib backup::tests:: -- --test-threads=1
cargo test --offline --locked --release --features custom-protocol --manifest-path src-tauri/Cargo.toml --lib popup::tests::
cargo clippy --offline --locked --release --features custom-protocol --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

offline要求预先准备精确依赖缓存。另有全量pnpm test:unit和pnpm test:js；本轮不执行截图/OCR专项、六站回归或完整原生GUI验收。自动化通过不能替代Windows原生验收，历史二进制测试结论不自动转移到新产物。

## 隐私与第三方网站

划词/截图在相关发送操作中传给所选AI网站，请确认资料可发送。设置、提示词、收藏、登录态保存在本机；备份白名单不打包Cookie/WebView2目录。开发日志或显式诊断可能含正文、网址及设备信息，不要提交真实配置、收藏、导出、备份、个人截图或日志。本候选不带原Git历史、旧EXE/HASH、测试产物及用户数据。

## 首版交付计划（尚未发布）

计划提供公开源码、独立构建并验收的Windows EXE、最终SHA256校验文件，以及必要LICENSE、THIRD_PARTY_NOTICES、NOTICE、第三方许可正文和MPL对应源码获取说明。当前候选不携带EXE，不能把旧EXE放进来充当首版。已准备的新内部 EXE 的定向原生验收为 12 PASS；五个 MPL 对应源码与实际生成文件已补齐。当前仓库不提供 EXE/ZIP 下载，也不表示完整 Beta 已通过。

## 许可、权利与反馈

项目自有原创代码使用既有 [MIT](LICENSE)。维护者已确认项目原创代码和原创图标具有合法公开分发权利，不含未经授权的雇主、客户或其他贡献者成果；相关原创资源允许随源码和 Windows EXE 分发。第三方各自许可和商标权利不受替换，见 [权利确认](RIGHTS_CONFIRMATION.md)、[资源清单](RESOURCE-RIGHTS.csv) 和 [第三方说明](THIRD_PARTY_NOTICES.md)。

[SECURITY](SECURITY.md)准确记录当前私密渠道尚未启用、未验收的状态。不要在公开 Issue/PR 发布敏感漏洞细节；Private 仓库的普通 Issue 也不被本项目作为私密漏洞接收渠道。贡献要求见 [CONTRIBUTING](CONTRIBUTING.md)。当前仅获准 Private 源码暂存，不授权公开、Tag、Release 或 EXE 上传；CI 不等于完整原生或独立安全认证。
