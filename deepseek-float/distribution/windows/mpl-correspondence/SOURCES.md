# MPL对应源码供应：当前禁用回退构建

本补充对应已审查的固定禁用回退实现及其内部构建；本仓库不分发 EXE。五个组件的精确完整源码在deepseek-float/distribution/windows/sources/的crate归档，原版权/MPL通知和正文在deepseek-float/distribution/windows/licenses/；无需邮箱、账户或费用，使用支持gzip/tar的工具读取，不需执行归档程序。COMPONENTS.csv记录精确版本、摘要和获取位置；COMPILER-INPUTS.csv只记录从本次编译dep-info实际观察到的输入，不把编译输入误称全部进入最终机器码。

每个包实际Cargo源缓存文件都与锁定归档逐字比对，无修改。所有可能保留的原始文件保守提供，而不依赖LTO裁剪证明排除。selectors的build.rs及属性词表在原归档，实际OUT_DIR的ascii_case_insensitive_html_attributes.rs另逐字随附；它由原生成器和锁定phf_codegen构建生成，修改时优先编辑build.rs及词表，再用Cargo.lock固定依赖重建。

cssparser-macros的可修改形式为原lib.rs的解析器、quote模板；实际输入来自归档中的cssparser src/macros.rs、tokenizer等，以及selectors parser.rs。直接依赖方dom_query的源码未检出这些宏调用。生成MAX_LENGTH常量和字节匹配表的模板/调用输入均完整提供；不以机器码反编译或额外不稳定编译器展开作为首选修改形式。代码生成器及输入是对应可修改源码的一部分，不把自动生成结果冒充人工原创MIT文件。

MPL-2.0原条款/声明继续适用于相关Covered Software，不由项目MIT替换。组合应用不因此自动全部改MPL；有其他许可证的生成器依赖按Cargo.lock与其原通知处理。本补充不删除原始通知，不授予商标权，不声称排除任何未观察到的第三方内容或提供独立法律认证。若后续修改/patch/生成输入或编译器来源改变，必须重新对应，不能直接复制本结论。

源码供应对本次所核对编译输入以保守全集覆盖处理；最终逐文件机器码保留范围仍未证明，不将其写成已测二进制裁剪结果。整个包是否可发布仍以总发布门禁为准，本补充不能使其他未完成权利或安全渠道自动READY。

许可正文：https://www.mozilla.org/en-US/MPL/2.0/ （尤其Source Code Form定义及第3.1–3.4节）。

路径说明：上述 deepseek-float/ 路径相对仓库根目录；generated/ 相对本补充目录。
