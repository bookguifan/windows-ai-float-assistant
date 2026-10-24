#!/usr/bin/env node
/**
 * CSP ↔ 页面实际需求 一致性门禁（R19）
 *
 * ## 为什么需要它
 * `tauri.conf.json` 的 CSP 只在**构建产物**里生效（dev 下资源由 vite dev server 提供，
 * 不经过 Tauri 的资产协议 ⇒ 不下发 CSP 头）。所以「CSP 打死页面」这类故障
 * **在 dev 里永远复现不出来**，只能等打包后才发现。本脚本把 5 个本地页里
 * *实际用到的* 资源与 CSP 指令逐条对齐，把这些故障提前到 CI。
 *
 * ## 两个反直觉的 Tauri 机制（本脚本的核心依据，均已源码核实）
 * 1. 内联 `<script>` 由 tauri-codegen **自动补 sha256** 进 `script-src`
 *    （`tauri-codegen/src/context.rs:48` 选的是 `script:not(:empty)`）。
 *    ⇒ 内联脚本**不需要** `'unsafe-inline'`；且 hash 一旦存在，
 *      `script-src` 里的 `'unsafe-inline'` 会被浏览器**忽略**（写了是自欺）。
 * 2. 内联 `<style>` 块**不补 hash**，而是由 `tauri-utils/src/html2.rs:75`
 *    打 nonce 占位符，运行时替换为随机 nonce 并**显式创建** `style-src`
 *    （`tauri/src/manager/mod.rs:96`）。
 *    ⇒ `style-src` 里没有 `'unsafe-inline'`，于是 **`style="..."` 属性会被拦**
 *      （nonce 只覆盖元素，不覆盖属性）。要让属性生效必须单独写
 *      `style-src-attr 'unsafe-inline'`（只影响属性，比放宽 `style-src` 窄）。
 *
 * ## 扫描前会剥离注释
 * 注释里出现的 `eval(` / `fetch(` 是**说明文字**不是调用（`popup.html:150` 就有一条
 * 讲 `win.eval("__dsfResetFired()")` 的注释）。若不剥离，本脚本第一次运行就报假阳性，
 * 之后必然被当成噪音忽略 —— 所以剥离注释是**必要**的，不是洁癖。
 * 剥离时跟踪字符串字面量，避免把 `"https://…"` 里的 `//` 误当行注释切掉。
 *
 * ## 用法
 *   node scripts/check-csp.mjs      # 需先 `pnpm build`（读的是 dist/）
 * 退出码 0 = 全部一致；1 = 有冲突（逐条列出冲突点）。
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const CONF = path.join(root, "src-tauri", "tauri.conf.json");
const SRC_PAGES = path.join(root, "poc");
const DIST_PAGES = path.join(root, "dist", "poc");

const fails = [];
const notes = [];
const fail = (msg) => fails.push(msg);
const note = (msg) => notes.push(msg);

// ---------- 0. 注释剥离 ----------

// 判定 `/` 是「除号」还是「正则起始」的启发式（标准做法：看前一个有效字符/关键字）。
const REGEX_OK_CHARS = new Set([..."(,=:[!&|?{};+-*%~^<>"]);
const REGEX_OK_WORDS = new Set([
  "return", "typeof", "case", "in", "of", "new", "delete", "void",
  "instanceof", "do", "else", "yield", "await",
]);

/**
 * 剥掉 JS 注释，保留字符串/模板/正则字面量。
 *
 * ★ 必须识别**正则字面量**：`escapeHtml` 里的 `/[&<>"']/g` 含引号，
 *   若只跟踪引号，状态机会在正则处错位，之后所有 `//` 都不再被当成注释 ——
 *   于是 `popup.html:150` 那句讲 `win.eval(...)` 的**注释**会被误判成真实调用。
 */
function stripJsComments(js) {
  let out = "";
  let quote = null; // 当前字符串定界符（' " `）
  let prev = ""; // 上一个有效字符（非空白），用于正则/除号歧义判定
  let word = ""; // 上一个标识符（用于 return / typeof 等关键字）
  for (let i = 0; i < js.length; i += 1) {
    const c = js[i];
    const d = js[i + 1];
    if (quote) {
      out += c;
      if (c === "\\") {
        out += d ?? "";
        i += 1;
      } else if (c === quote) {
        quote = null;
      }
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      quote = c;
      out += c;
      prev = c;
      word = "";
      continue;
    }
    if (c === "/" && d === "/") {
      while (i < js.length && js[i] !== "\n") i += 1;
      out += "\n";
      continue;
    }
    if (c === "/" && d === "*") {
      i += 2;
      while (i < js.length && !(js[i] === "*" && js[i + 1] === "/")) i += 1;
      i += 1;
      out += " ";
      continue;
    }
    if (c === "/" && (prev === "" || REGEX_OK_CHARS.has(prev) || REGEX_OK_WORDS.has(word))) {
      // 正则字面量：吃到未转义的结束 `/`；`[...]` 字符类里的 `/` 不结束。
      out += c;
      let inClass = false;
      i += 1;
      for (; i < js.length; i += 1) {
        const e = js[i];
        out += e;
        if (e === "\\") {
          out += js[i + 1] ?? "";
          i += 1;
        } else if (e === "[") {
          inClass = true;
        } else if (e === "]") {
          inClass = false;
        } else if ((e === "/" && !inClass) || e === "\n") {
          break; // 换行仍未闭合 ⇒ 判定失误，止损（宁可少剥一行）
        }
      }
      while (i + 1 < js.length && /[a-z]/i.test(js[i + 1])) {
        out += js[i + 1];
        i += 1;
      }
      prev = "/"; // 后续 `/` 视作除号
      word = "";
      continue;
    }
    out += c;
    if (!/\s/.test(c)) {
      prev = c;
      word = /[A-Za-z0-9_$]/.test(c) ? word + c : "";
    }
  }
  return out;
}

/** 剥离器自测：它错了，后面所有结论都是错的 —— 所以它自己也得被测。 */
function selfTest() {
  const cases = [
    // 正则里的引号不得把状态机带偏：后面的行注释必须仍被剥掉
    [`const a = /[&<>"']/g; // eval(1)\nconst b = 1;`, (o) => !o.includes("eval(")],
    // 字符串里的 `//` 不得被当成注释
    [`const u = "https://x/y"; const z = 1;`, (o) => o.includes("https://x/y") && o.includes("const z = 1;")],
    // 模板字面量里的引号不污染状态
    ["const t = `a'b\"c`; // eval(2)\nconst c = 2;", (o) => !o.includes("eval(")],
    // 块注释
    [`/* eval(3) */ const d = 3;`, (o) => !o.includes("eval(") && o.includes("const d = 3;")],
    // 除号不得被当正则（否则会把后续代码当正则吃掉 → 漏报）
    [`const r = a / b; eval(4);`, (o) => o.includes("eval(4)")],
    // return 后的正则
    [`function f(){ return /x/g; } // eval(5)`, (o) => !o.includes("eval(")],
  ];
  for (const [input, ok] of cases) {
    if (!ok(stripJsComments(input))) {
      console.error(`❌ stripJsComments 自测失败：${JSON.stringify(input)}`);
      process.exit(1);
    }
  }
}
selfTest();

/** HTML 注释去掉 + `<script>` 体去 JS 注释；`<style>` 体原样保留（CSS 注释无 `//` 风险）。 */
function sanitize(html) {
  return html
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/(<script\b[^>]*>)([\s\S]*?)(<\/script>)/gi, (_m, a, body, c) => a + stripJsComments(body) + c);
}

// ---------- 1. 读 CSP ----------

const conf = JSON.parse(fs.readFileSync(CONF, "utf8"));
// `--csp="…"` 覆盖入口：用于**负向测试**与「改配置前先试算」，CI 不传该参数。
const overrideArg = process.argv.find((a) => a.startsWith("--csp="));
const csp = overrideArg ? overrideArg.slice("--csp=".length) : conf?.app?.security?.csp;
if (overrideArg) {
  console.log(`（--csp 覆盖：本次校验命令行传入的策略，不是 tauri.conf.json 里的那份）\n`);
}
if (!csp || typeof csp !== "string") {
  fail(`tauri.conf.json 的 app.security.csp 缺失或非字符串 —— 无 CSP 等于完全放开`);
}

/** "a b; c d" → Map{ a:[b], c:[d] }（与 tauri-utils config.rs:2458 的解析方式一致） */
function parseCsp(text) {
  const map = new Map();
  for (const chunk of text.split(";")) {
    const t = chunk.trim();
    if (!t) continue;
    const [name, ...sources] = t.split(/\s+/);
    map.set(name.toLowerCase(), sources);
  }
  return map;
}

const dirs = parseCsp(csp);
const src = (d) => dirs.get(d) ?? [];
/** `default-src` 兜底语义：指令缺失时按 default-src 取值。 */
const eff = (d) => (src(d).length ? src(d) : src("default-src"));
const allow = (d, v) => eff(d).includes(v);

// ---------- 2. 读页面（以 dist 为准：那才是被打包的那份） ----------

if (!fs.existsSync(DIST_PAGES)) {
  console.error(`❌ 找不到 ${path.relative(root, DIST_PAGES)} —— 请先跑 pnpm build 再校验。`);
  process.exit(1);
}

const names = fs.readdirSync(DIST_PAGES).filter((f) => f.endsWith(".html")).sort();

// dist 过期会让校验结果失真（校验的是旧页面）。
const newestSrc = Math.max(...fs.readdirSync(SRC_PAGES).map((f) => fs.statSync(path.join(SRC_PAGES, f)).mtimeMs));
const oldestDist = Math.min(...names.map((f) => fs.statSync(path.join(DIST_PAGES, f)).mtimeMs));
if (newestSrc > oldestDist) {
  fail(`dist/ 比 poc/ 旧（poc 有新改动未重新构建）—— 校验对象不是最新页面，先跑 pnpm build`);
}

/** 按页面抽出「CSP 相关事实」。代码级判定一律跑在 sanitize() 之后的文本上。 */
function facts(html) {
  const code = sanitize(html);

  // —— 计数类：用原始文本，才能与 Tauri 的「Tauri 会 hash 哪些 script」对齐 ——
  const scripts = [];
  const extScripts = [];
  const re = /<script\b([^>]*)>([\s\S]*?)<\/script>/gi;
  let m;
  while ((m = re.exec(html)) !== null) {
    const attrs = m[1] ?? "";
    const body = m[2] ?? "";
    const srcAttr = /\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)')/i.exec(attrs);
    if (srcAttr) extScripts.push(srcAttr[1] ?? srcAttr[2] ?? "");
    else if (body.trim()) scripts.push(body); // 空脚本 Tauri 不 hash，也不用管
  }

  const count = (re2, text = code) => (text.match(re2) ?? []).length;

  return {
    scripts,
    extScripts,
    links: code.match(/<link\b[^>]*>/gi) ?? [],
    // style="..." / style='...'（含 JS 字符串里拼出来的）
    styleAttrs: count(/\sstyle\s*=\s*(?:"[^"]*"|'[^']*')/g),
    // 内联事件处理器：受 script-src-attr 单独管辖
    inlineHandlers: count(/\son(?:click|input|change|key\w*|mouse\w*|dblclick|focus|blur)\s*=/gi),
    forms: count(/<form\b/gi),
    // 页面自发的网络出口（不写进 connect-src 就会静默失败）
    netCalls: count(/\bfetch\s*\(|new\s+XMLHttpRequest|new\s+WebSocket|new\s+EventSource/g),
    // eval 家族：script-src 无 'unsafe-eval' 时这些会直接抛错
    evalish: count(/\beval\s*\(|new\s+Function\s*\(|set(?:Timeout|Interval)\s*\(\s*["'`]/g),
    // CSS 里的外部引用（style-src 只放 'self' 时会被拦）
    cssExternal: count(/@import\b|url\(\s*["']?https?:/gi),
    // 是否可能渲染远端图片（result.html 的白名单正则含 https?:）
    mayRenderRemoteImg: /https\?:/.test(code) && /\bIMG\b/.test(code),
    usesDataImg: /data:image/.test(code),
  };
}

const all = names.map((f) => ({ name: f, ...facts(fs.readFileSync(path.join(DIST_PAGES, f), "utf8")) }));

// ---------- 3. 逐条对齐 ----------

// (a) 内联脚本由 Tauri 补 sha256 ⇒ 不该同时写 'unsafe-inline'（会被忽略、误导后人）
if (all.some((p) => p.scripts.length)) {
  const total = all.reduce((n, p) => n + p.scripts.length, 0);
  if (allow("script-src", "'unsafe-inline'")) {
    fail(`script-src 不该出现 'unsafe-inline'：${total} 段内联脚本已由 Tauri 自动补 sha256，` +
         `两者并存时浏览器会忽略 'unsafe-inline'，写它只会误导（见本文件模块头机制 1）`);
  }
  note(`${total} 段内联脚本 → 依赖 Tauri 自动补 sha256（无需 'unsafe-inline'）`);
}

// (b) style="..." 属性只受 style-src-attr 管辖，nonce 覆盖不到
const styleAttrPages = all.filter((p) => p.styleAttrs);
if (styleAttrPages.length) {
  const n = styleAttrPages.reduce((s, p) => s + p.styleAttrs, 0);
  if (!allow("style-src-attr", "'unsafe-inline'")) {
    fail(`${n} 处内联 style="..." 属性（${styleAttrPages.map((p) => p.name).join(", ")}）会被拦：` +
         `style-src 只有 'self'+nonce（nonce 不覆盖属性）⇒ 必须在 style-src-attr 放 'unsafe-inline'`);
  } else {
    note(`${n} 处 style="..." 属性 → 由 style-src-attr 'unsafe-inline' 放行（比放宽 style-src 窄）`);
  }
}

// (c) 内联事件处理器
if (all.some((p) => p.inlineHandlers) && !allow("script-src-attr", "'unsafe-inline'")) {
  fail(`存在内联事件处理器（onclick= 等）但 script-src-attr 未放行 'unsafe-inline'`);
}

// (d) 外部脚本 / 样式
const originOf = (u) => {
  try {
    return new URL(u).origin;
  } catch {
    return null;
  }
};
for (const p of all) {
  for (const s of p.extScripts) {
    const o = /^https?:/i.test(s) ? originOf(s) : null; // 相对路径由 'self' 覆盖
    if (o && !allow("script-src", o) && !allow("script-src", "https:")) {
      fail(`${p.name} 引用了外部脚本 ${s}，script-src 未允许其来源`);
    }
  }
  for (const l of p.links) {
    if (/\brel\s*=\s*["']?stylesheet/i.test(l) && /\bhref\s*=\s*["']https?:/i.test(l)) {
      const o = originOf(/\bhref\s*=\s*["']([^"']+)/i.exec(l)[1]);
      if (o && !allow("style-src", o) && !allow("style-src", "https:")) {
        fail(`${p.name} 引用了外部样式 ${l}，style-src 未允许其来源`);
      }
    }
  }
  if (p.cssExternal && !allow("style-src", "https:")) {
    fail(`${p.name} 的 CSS 含 @import / url(http…) 共 ${p.cssExternal} 处，style-src 未允许`);
  }
}

// (e) eval 家族
const evalCount = all.reduce((n, p) => n + p.evalish, 0);
if (evalCount && !allow("script-src", "'unsafe-eval'")) {
  fail(`页面含 eval / new Function / 字符串型 setTimeout 共 ${evalCount} 处，但 script-src 无 'unsafe-eval'`);
} else if (!evalCount) {
  note(`无 eval 家族调用 → 'unsafe-eval' 保持缺席（本次收窄的主要收益之一）`);
}

// (f) 图片来源
if (all.some((p) => p.mayRenderRemoteImg) && !allow("img-src", "https:")) {
  fail(`result.html 会渲染远端图片（答案里的 ![](https://…)），img-src 必须含 https:`);
}
if (all.some((p) => p.usesDataImg) && !allow("img-src", "data:")) {
  fail(`页面按 data:image/ 渲染图片，img-src 必须含 data:`);
}

// (g) 网络出口
const netCount = all.reduce((n, p) => n + p.netCalls, 0);
if (netCount && !allow("connect-src", "ipc:")) {
  fail(`页面有 ${netCount} 处 fetch/XHR/WebSocket，connect-src 未允许其来源`);
}
if (!allow("connect-src", "ipc:")) {
  fail(`connect-src 缺 ipc: —— Windows 上 IPC 走 ipc://localhost，漏了会导致**所有命令失效**`);
}
if (!allow("connect-src", "http://ipc.localhost")) {
  note(`connect-src 建议按 Tauri 官方文档同时保留 http://ipc.localhost（大 payload 走 fetch 分支时用）`);
}

// (h) 表单
if (all.some((p) => p.forms) && allow("form-action", "'none'")) {
  fail(`页面有 <form> 但 form-action 'none' 会拦掉提交`);
}

// (i) 硬化基线（防回退）
const flat = [...dirs].flatMap(([d, s]) => s.map((v) => `${d} ${v}`));
for (const bad of ["'unsafe-eval'", "https://*", "wss:", "http://localhost:*"]) {
  if (flat.some((x) => x.includes(bad))) {
    fail(`CSP 出现通配/危险源 ${bad} —— 本次收窄就是要去掉它；若确需，请在本脚本里补一条有依据的例外`);
  }
}
if (!(src("default-src").length === 1 && src("default-src")[0] === "'self'")) {
  fail(`default-src 应恰为 'self'（当前：${JSON.stringify(src("default-src"))}）`);
}
for (const d of ["object-src", "base-uri", "frame-src", "form-action"]) {
  if (!dirs.has(d)) fail(`缺硬化指令 ${d}（建议 object-src 'none' / base-uri 'self' / frame-src 'none' / form-action 'none'）`);
}

// ---------- 4. 输出 ----------

console.log(`CSP: ${csp}\n`);
console.log(`校验页面（${all.length}，源 dist/）：`);
for (const p of all) {
  console.log(
    `  ${p.name.padEnd(14)} 内联脚本 ${p.scripts.length}  内联样式属性 ${p.styleAttrs}  ` +
    `外链脚本 ${p.extScripts.length}  事件处理器 ${p.inlineHandlers}  ` +
    `网络出口 ${p.netCalls}  eval 家族 ${p.evalish}`
  );
}
console.log();
for (const n of notes) console.log(`  · ${n}`);
console.log();
if (fails.length) {
  for (const f of fails) console.error(`❌ ${f}`);
  console.error(`\n共 ${fails.length} 处 CSP 与页面需求不一致。`);
  process.exit(1);
}
console.log(`✅ CSP 与 ${all.length} 个页面实际需求一致（0 冲突）。`);
