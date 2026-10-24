# 第三方许可与资源说明（源码候选）

项目原有MIT声明见[LICENSE](LICENSE)，仅适用于有权按MIT提供的项目代码。第三方许可、版权和商标不被替换。

## 精确依赖与现有材料

[依赖清单](deepseek-float/docs/third-party-dependencies.csv)记录438个Cargo和115个npm锁定版本；[缺口清单](deepseek-float/docs/third-party-license-gaps.csv)是逐项核验任务，不是553个违规结论。324项定位许可/通知文件，229项仅有声明；元数据和校验值不代替全部分发义务验证。未升级依赖或改变锁文件。

[Windows许可材料](deepseek-float/distribution/windows/README.md)保留全部589份文件及原始结构：所选许可/版权头、WebView2 SDK LICENSE/NOTICE、Rust工具链许可和五份原MPL源码归档；588行校验表和580行来源记录可核对。这里是保守保留的准备材料，不表示所有组件进入最终EXE，也不表示源码首版必须随附Windows运行库。

## 源码与二进制分发的区别

本候选不带EXE、DLL、安装器、node_modules或Cargo依赖源码vendor目录；仅声明依赖不等同于将全部开发工具代码重新分发。五份MPL上游归档确实随材料提供，应保留其版权/文件许可及[源码获取说明](deepseek-float/distribution/windows/SOURCES.md)，不将这些文件改为MIT。项目没有添加依赖补丁或改写上游源码；未来若有Covered Software修改/生成代码须重新核对。

采用Apache-2.0分支的再分发内容需保留许可及适用NOTICE/修改说明；存在OR时可选择分支，不能把所有选项都当必须同时履行。MIT/BSD/Unicode等也需保留适用原通知。现有通知包中的原文不因本README被重写。参考[MPL 2.0](https://www.mozilla.org/en-US/MPL/2.0/)、[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0)及[MIT](https://opensource.org/license/mit)。

brotli-decompressor 5.0.3的根BSD正文、src/context.rs MIT头及保守MIT正文均保留；包中斜杠声明不能擅自解读成统一AND/OR。当前候选不vendor其实现，不能把尚未完成的文件级/最终链接映射冒称关闭。本次已核对精确源文件并保守保留相应许可及原通知；未来二进制分发仍须携带相应材料。

WebView2 SDK条款不能只用Rust绑定MIT许可替代；本候选不带SDK加载器二进制或Runtime。未来可执行包需另核实际加载器/SDK、MSVC/标准库、生成前端、MPL和NOTICE，不继承当前源码候选状态。

## 视觉资源与权利

维护者已明确确认项目原创代码/图标的合法公开分发权利和第三方素材遵约，见 [权利确认](RIGHTS_CONFIRMATION.md)。[资源清单](RESOURCE-RIGHTS.csv)保留原来源记录及逐项摘要；声明不是独立第三方溯源鉴定，不把已注明第三方/品牌来源的资源重新分类为原创，不授予额外商标权。系统字体仅 CSS 回退，不随仓库捆绑字体。

## 本次对应源码补充

五个精确 MPL 原包和正文保持在 deepseek-float/distribution/windows/；额外的生成输入映射及本次 selectors 生成文件见 [MPL 对应源码补充](deepseek-float/distribution/windows/mpl-correspondence/SOURCES.md)。brotli-decompressor 5.0.3 的精确原源码归档另在 sources/，文件级许可与声明保留。本仓库不包含 Windows EXE，不能把源码包中的许可准备材料当成可执行包已获发布批准。
