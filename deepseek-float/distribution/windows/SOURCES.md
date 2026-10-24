# MPL 源码获取说明

本目录是未来 Windows 构建的许可材料准备包，不对应现存历史 EXE。以下五个**未经本项目修改的上游发布归档**已经随材料一起提供；归档 SHA256 与当前 Cargo.lock 一致，包含原源码、包元数据及原始文件通知。未删除包中原始署名。

接收者可直接取得下面的本地文件，无需维护者联系方式或额外收费；也列出精确公共下载地址作为备用。`.crate` 是 gzip/tar 源码归档，可用支持该格式的工具查看，不需要运行归档中的程序。

| 包/版本 | 本地源码 | SHA256 | 备用源 |
| --- | --- | --- | --- |
| cssparser 0.36.0 | [sources/cssparser-0.36.0.crate](sources/cssparser-0.36.0.crate) | dae61cf9c0abb83bd659dab65b7e4e38d8236824c85f0f804f173567bda257d2 | [精确归档](https://static.crates.io/crates/cssparser/cssparser-0.36.0.crate) |
| cssparser-macros 0.6.1 | [sources/cssparser-macros-0.6.1.crate](sources/cssparser-macros-0.6.1.crate) | 13b588ba4ac1a99f7f2964d24b3d896ddc6bf847ee3855dbd4366f058cfcd331 | [精确归档](https://static.crates.io/crates/cssparser-macros/cssparser-macros-0.6.1.crate) |
| dtoa-short 0.3.5 | [sources/dtoa-short-0.3.5.crate](sources/dtoa-short-0.3.5.crate) | cd1511a7b6a56299bd043a9c167a6d2bfb37bf84a6dfceaba651168adfb43c87 | [精确归档](https://static.crates.io/crates/dtoa-short/dtoa-short-0.3.5.crate) |
| option-ext 0.2.0 | [sources/option-ext-0.2.0.crate](sources/option-ext-0.2.0.crate) | 04744f49eae99ab78e0d5c0b603ab218f515ea8cfe5a456d7629ad883a3b6e7d | [精确归档](https://static.crates.io/crates/option-ext/option-ext-0.2.0.crate) |
| selectors 0.36.1 | [sources/selectors-0.36.1.crate](sources/selectors-0.36.1.crate) | c5d9c0c92a92d33f08817311cf3f2c29a3538a8240e94a6a3c622ce652d7e00c | [精确归档](https://static.crates.io/crates/selectors/selectors-0.36.1.crate) |

许可为 MPL-2.0；各包的原通知及正文在 `licenses/<包名-版本>/`。selectors 0.36.1 的原 lib.rs 明确适用 MPL-2.0，包内没有独立正文，故另提供 Mozilla 官方的同一许可版本标准正文，未伪称它原本位于该归档。

分发含 Covered Software 的可执行形式时，需要按 [MPL 3.1–3.4](https://www.mozilla.org/en-US/MPL/2.0/) 提供对应源码并告知获取方式、保留通知；不是要求整个组合应用改为 MPL。源码归档包含构建/过程宏 cssparser-macros，不能因它是过程宏就跳过生成代码的许可核对。

**最终构建门禁**：本轮没有构建、链接或证明最终 EXE 使用的源码。若实际构建使用本地修改、补丁、替代来源或含 MPL 的生成文件，发布者必须增加相应的可修改源码/补丁、构建输入与修改说明，并重新校验；不能仍把这里的原始归档当作已涵盖修改。确认最终包包含的 MPL 组件和接收者可获得的源码完全对应后，才可以将本说明随新 EXE 交付。没有把“未发现 patch 配置”当作所有编译输入未经修改的证明。
