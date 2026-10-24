#!/usr/bin/env node
// 内联脚本语法门禁（R17-P2）。
//
// 为什么需要它：`poc/*.html` 的 JS 全部**内联**在页面里，没有 eslint / tsconfig /
// 打包期类型检查兜底。一个手滑的括号就能让某个页面「加载成功但所有按钮失效」——
// 而这类故障在真机上只表现为「点了没反应」，定位成本极高。
//
// 本脚本把每个内联 `<script>` 交给 Node 的 `vm.Script` **编译一次**（只编译、不执行），
// 语法错误立刻报错并以非 0 退出；页面渲染与 DOM 一律不碰。
//
// 用法：node scripts/check-inline-js.mjs [目录，默认 poc]
// 退出码：0 = 全部通过；1 = 存在语法错误或未找到任何页面。

import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import vm from "node:vm";

const dir = resolve(process.argv[2] ?? "poc");

/** 取出 HTML 里所有**无 src** 的 `<script>` 块（有 src 的由打包器/浏览器加载，不在此校验）。 */
function inlineScripts(html) {
  const out = [];
  const re = /<script\b([^>]*)>([\s\S]*?)<\/script>/gi;
  let m;
  while ((m = re.exec(html)) !== null) {
    const attrs = m[1] ?? "";
    const body = m[2] ?? "";
    if (/\bsrc\s*=/i.test(attrs)) continue; // 外链脚本由打包器负责
    const t = attrs.match(/\btype\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))/i);
    const type = (t ? (t[1] ?? t[2] ?? t[3]) : "").trim().toLowerCase();
    // 非 JS 的模板（例如 type="text/template"）跳过；空 type 视为 JS。
    if (type && type !== "text/javascript" && type !== "application/javascript" && type !== "module") {
      continue;
    }
    // 行号：用开标签位置换算，报错时能直接跳到源文件对应行。
    const line = html.slice(0, m.index).split("\n").length;
    out.push({ body, line });
  }
  return out;
}

let files;
try {
  files = readdirSync(dir).filter((f) => f.toLowerCase().endsWith(".html"));
} catch (e) {
  console.error(`✗ 读不到目录 ${dir}：${e.message}`);
  process.exit(1);
}
if (files.length === 0) {
  console.error(`✗ ${dir} 下没有任何 .html —— 路径不对？`);
  process.exit(1);
}

let total = 0;
let failed = 0;

for (const file of files.sort()) {
  const path = join(dir, file);
  const html = readFileSync(path, "utf8");
  const blocks = inlineScripts(html);
  for (const [i, block] of blocks.entries()) {
    total += 1;
    try {
      // 只编译不执行：能抓到语法错误；不做任何 DOM / 网络访问。
      new vm.Script(block.body, { filename: `${file}#inline${i + 1}` });
    } catch (e) {
      failed += 1;
      const at = block.line;
      console.error(`✗ ${file}（第 ${at} 行起的第 ${i + 1} 个内联脚本）:`);
      console.error(`  ${e.message}`);
    }
  }
  console.log(`· ${file}：${blocks.length} 段内联脚本`);
}

console.log(`\n内联脚本语法门禁：${total} 段，失败 ${failed} 段。`);
if (failed > 0) process.exit(1);
console.log("✓ 全部通过");
