# Windows 发布许可材料

本目录只包含法律文本、通知、来源索引和五个上游 MPL 源码归档，没有 EXE、DLL、LIB、安装包或 HASH.txt。不能直接当作已批准的 Beta 包，也不能把历史 EXE 复制进来充当新构建。

已准备：项目 LICENSE、262 项的所选/保守许可正文和可用版权头、精确 WebView2 SDK LICENSE/NOTICE、当前 Rust 标准库许可、MPL 源码归档与获取说明。

入口：[第三方说明](THIRD_PARTY_NOTICES.md)、[组件表](components.csv)、[原始材料来源/摘要](files.csv)、[MPL 源码](SOURCES.md)、[额外组件](external-components.csv)、[完整目录校验表](CHECKSUMS.csv)。

未来经过验收的新构建应交付为：

```text
release/
  deepseek-float.exe       # 单独构建、验收、按需签名
  HASH.txt                # 对最终 EXE 计算，禁止填写示例或旧哈希
  LICENSE
  THIRD_PARTY_NOTICES.md
  SOURCES.md
  components.csv
  external-components.csv
  mpl-sources.csv
  files.csv
  CHECKSUMS.csv
  licenses/
  native/
  toolchain/
  sources/
```

保留该目录原有相对结构，将本材料与验收通过的新 EXE 一起组装；不得只拷贝 EXE 而省略必要通知。原文和源码包需要按字节保留，局部 .gitattributes 防止 Git 自动换行改写摘要。CHECKSUMS 不包含自身，也不包含尚不存在的 EXE/HASH；最终组装时需独立更新完整包校验表。

字节保留规则覆盖本材料目录的所有文件，包括参与摘要计算的自编说明和 JSON 元数据，避免 Git 换行转换使克隆后的校验表失效；不作用于应用业务目录。

29 个已核对的上游法律/包身份文件保留原始格式：9 个有行尾空格、2 个使用原始 CRLF、18 个有末尾空行；只对这些明确路径设置相应格式属性，不改写原文字节。规则不作用于业务代码或自编说明文档，原始来源和摘要仍逐文件校验。

在本目录运行以下 PowerShell 可复核材料摘要；它检查内容一致性，不是签名验证或构建来源证明：

```powershell
Import-Csv -LiteralPath ./CHECKSUMS.csv | ForEach-Object {
    $materialHash = (Get-FileHash -LiteralPath $_.path -Algorithm SHA256).Hash
    if ($materialHash -ne $_.sha256) { throw "材料摘要不匹配：$($_.path)" }
}
```

放行前必须记录精确源码/锁文件/编译器/目标/特性、工作树状态、补丁、构建输入与链接证据，核对实际二进制及生成/内嵌前端、原生运行库。LTO/strip 后仅凭字符串或候选图不能证明实际保留了哪个依赖。当前许可分支记录可供审核，最终内容变化必须重新生成材料；修改材料说明本身不是完成验收的证明。

保留 64 个构建/过程宏输入的材料是为覆盖可能的生成/复制内容，不表示把测试工具当作运行时组件。115 个 npm 锁定包不自动进入本目录；当前前端源码为内联 HTML/JS，但 Vite 生成/内嵌脚本和其他复制内容仍须按最终构建审查。Windows SDK/MSVC 及 Rust 标准库也不能仅由 Cargo.lock 代表。

本轮未建立最终 EXE 内容与材料的一一映射，brotli-decompressor 的斜杠/文件许可范围仍待核定，原生运行库和最终修改通知待复核。因此本目录尚未具备无条件随 Beta 发布的资格。
