#!/usr/bin/env node
// T1 · JS 净化/启发式 单元测试（jsdom）。
//
// 设计原则（审计报告 §5 第1批 T1）：
//   1. 不改动生产代码 —— 直接把生产源码里「自包含的净化函数块」抽取出来在 jsdom 中执行并断言；
//   2. 既测权威行为，也做「镜像校验」：页面侧 SAFE_TAGS 与卡片侧 SAFE_HTML_TAGS 必须一致；
//   3. 安全相关 fixture 优先（XSS 向量、危险 URL、on* 事件、代码块换行铁律）。
//
// 运行：NODE_PATH=<workspace>/node_modules node tests/sanitize.test.mjs
// 依赖：jsdom（仅 dev，装在托管 workspace，不进项目 node_modules）。

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const DS_PATH = resolve(root, 'src-tauri/src/dsfloat_init.js');
const RES_PATH = resolve(root, 'poc/result.html');

let passed = 0;
let failed = 0;
const failures = [];
function assert(cond, msg) {
  if (cond) { passed++; }
  else { failed++; failures.push(msg); console.error('  ✗ ' + msg); }
}
function section(name) { console.log('\n# ' + name); }

// ---- 抽取生产源码里的自包含函数块 ----
function read(p) {
  try { return readFileSync(p, 'utf8'); }
  catch (e) { throw new Error(`读不到 ${p}：${e.message}`); }
}
function sliceBetween(src, startMarker, endMarker) {
  const s = src.indexOf(startMarker);
  if (s < 0) throw new Error(`未找到起始标记：${startMarker}`);
  const e = src.indexOf(endMarker, s + startMarker.length);
  if (e < 0) throw new Error(`未找到结束标记：${endMarker}`);
  return src.slice(s, e);
}

// ---- 加载 jsdom（CJS require 才尊重 NODE_PATH；ESM 动态 import 不查 NODE_PATH） ----
let JSDOM;
try {
  const { createRequire } = await import('node:module');
  const require = createRequire(import.meta.url);
  ({ JSDOM } = require('jsdom'));
} catch (e) {
  console.error('jsdom 未安装。请在托管 workspace 执行：');
  console.error('  npm install --prefix <workspace> jsdom');
  console.error('并运行：NODE_PATH=<workspace>/node_modules node tests/sanitize.test.mjs');
  process.exit(2); // 跳过（非失败）
}

const ds = read(DS_PATH);
const res = read(RES_PATH);

// 页面侧净化块：const SAFE_TAGS = { ... } → 止于 function hasVisibleHtml
// ★ 批次5′（2026-10-03）：`serializeHtml` 新增 `isSrOnly(node)` 跳过视觉隐藏的角色头（`sr-only`），
//   而 `isSrOnly` 定义在 `SAFE_TAGS` **之前** ⟹ 不在本抽取区间内 ⟹ 必须**前置拼接**其定义，
//   否则 `new Function` 求值时 `serializeHtml` 闭包引用 `isSrOnly` 会抛 `ReferenceError`。
const isSrOnlySrc = sliceBetween(ds, 'function isSrOnly(node) {', 'function blocksOf(el) {');
const dsBlock = sliceBetween(ds, 'const SAFE_TAGS = {', 'function hasVisibleHtml');
const pageFns = new Function(
  isSrOnlySrc + '\n' + dsBlock + '\nreturn { SAFE_TAGS, VOID_TAGS, escapeHtmlText, safeUrl, safeImgSrc, HLJS_CLASS_RE, LANG_CLASS_RE, codeLangClass, serializeCodeText, serializeHtml, isSrOnly };'
)();

// 卡片侧块：const SAFE_HTML_TAGS = { ... } → 止于 function renderSafeHtml（需 document，放进 window）
const resBlock = sliceBetween(res, 'const SAFE_HTML_TAGS = {', 'function renderSafeHtml');
const dom = new JSDOM('<!DOCTYPE html><html><body></body></html>', { runScripts: 'outside-only' });
const w = dom.window;
w.eval(resBlock + '\n; window.__SAN = { SAFE_HTML_TAGS, safeClass, cloneSafe };');
const cardFns = w.__SAN;

// splitBlock 依赖簇（P1/P2 性能记忆化测试）：从 norm 起、至 answerOf 注释止的一段自包含代码，
// 涵盖 readableText / SAFE_TAGS / serializeHtml / answerHtml / toMarkdown / hasAnswerElement /
// splitBlockRaw / splitBlock（记忆化）/ splitCacheStats。其间夹杂的 locateConversation 等函数
// 仅在被调用时触及 document/window，求值时不执行，安全。
const dsSplitSrc = sliceBetween(
  ds,
  'function norm(s) {',
  '// 只取回答（剥离思考前缀后的部分）'
);
const dsSplit = new Function(
  dsSplitSrc +
    '\nreturn { norm, readableText, SAFE_TAGS, escapeHtmlText, safeUrl, safeImgSrc,' +
    ' codeLangClass, serializeCodeText, serializeHtml, hasVisibleHtml, answerHtml,' +
    ' toMarkdown, tableToMarkdown, hasAnswerElement, splitBlock, splitBlockRaw,' +
    ' splitCacheStats, isSrOnly, getHits: () => SPLIT_CACHE_HITS, getMisses: () => SPLIT_CACHE_MISSES };'
)();

const D = w.document;
function el(tag, attrs, kids) {
  const n = D.createElement(tag);
  if (attrs) for (const k in attrs) n.setAttribute(k, attrs[k]);
  if (kids) for (const c of kids) n.appendChild(typeof c === 'string' ? D.createTextNode(c) : c);
  return n;
}

// ============ escapeHtmlText ============
section('escapeHtmlText（SEC2 单引号转义回归）');
{
  const out = pageFns.escapeHtmlText("a'b<c>\"&");
  assert(out.includes('&#39;'), `单引号应转义为 &#39;（实际：${out}）`);
  assert(out.includes('&lt;') && out.includes('&gt;'), `尖括号应转义（实际：${out}）`);
  assert(out.includes('&quot;'), `双引号应转义（实际：${out}）`);
  assert(out.includes('&amp;'), `& 应转义（实际：${out}）`);
  assert(pageFns.escapeHtmlText(null) === '', 'null 应转成空串');
  assert(pageFns.escapeHtmlText('') === '', '空串应仍是空串');
}

// ============ safeUrl / safeImgSrc（危险 URL 丢弃） ============
section('safeUrl / safeImgSrc（XSS 向量）');
{
  const su = pageFns.safeUrl;
  assert(su('https://x.com') === 'https://x.com', 'https 应放行');
  assert(su('http://x.com') === 'http://x.com', 'http 应放行');
  assert(su('mailto:a@b.com') === 'mailto:a@b.com', 'mailto 应放行');
  assert(su('#frag') === '#frag', '# 锚点应放行');
  assert(su('/rel/path') === '/rel/path', '相对路径应放行');
  assert(su('javascript:alert(1)') === null, 'javascript: 必须丢弃');
  assert(su('blob:https://x') === null, 'blob: 必须丢弃');
  assert(su('data:text/html,<script>') === null, 'data:text/html 必须丢弃');
  assert(su('  HTTPS://X.COM') === '  HTTPS://X.COM', 'scheme 大小写不归一并保留原串');

  const si = pageFns.safeImgSrc;
  assert(si('https://x.com/a.png') === 'https://x.com/a.png', 'https 图片应放行');
  assert(si('data:image/png;base64,AAA') === 'data:image/png;base64,AAA', 'data:image 应放行');
  assert(si('data:text/html,x') === null, 'data:text/html 图片必须丢弃');
  assert(si('javascript:alert(1)') === null, 'javascript: 图片必须丢弃');
}

// ============ codeLangClass（方案A 白名单） ============
section('codeLangClass（language-* 提取）');
{
  const cl = pageFns.codeLangClass;
  assert(cl('language-python') === 'language-python', '纯 language-* 应保留');
  assert(cl('hljs language-python') === 'language-python', '混排时应取 language-*');
  assert(cl('hljs-keyword') === '', '纯 hljs-* 无 language 应返回空');
  assert(cl('hljs-keyword onclick') === '', '含白名单外类名应保守返回空');
  assert(cl('') === '', '空串返回空');
}

// ============ serializeHtml（页面侧净化主干） ============
section('serializeHtml（页面侧：剥离 script/style/svg/button/on*）');
{
  const s = pageFns.serializeHtml;
  assert(s(el('script')) === '', '<script> 应整体剥离');
  assert(s(el('style')) === '', '<style> 应整体剥离');
  assert(s(el('svg')) === '', '<svg> 应整体剥离');
  assert(s(el('button', { onclick: 'x()' })) === '', '<button> 应整体剥离');

  // on* 事件：serializeHtml 仅对 PRE/A/IMG 拷贝属性，其余标签一律不拷属性 → on* 天然不落地
  const divOn = el('div', { onclick: 'evil()', class: 'x' }, ['hi']);
  const outDiv = s(divOn);
  assert(outDiv === '<div>hi</div>', `非白名单属性不应拷贝（实际：${outDiv}）`);

  // 危险 a href
  const aJs = el('a', { href: 'javascript:alert(1)' }, ['x']);
  assert(s(aJs) === '<a>x</a>', `javascript: 链接不应保留 href（实际：${s(aJs)}）`);
  const aHttps = el('a', { href: 'https://x.com' }, ['x']);
  const outA = s(aHttps);
  assert(outA === '<a href="https://x.com" rel="noopener noreferrer" target="_blank">x</a>',
    `https 链接应带 rel/target（实际：${outA}）`);
  const aRel = el('a', { href: '/rel' }, ['x']);
  assert(s(aRel) === '<a href="/rel" rel="noopener noreferrer" target="_blank">x</a>',
    `相对链接应保留（实际：${s(aRel)}）`);

  // 危险 img src
  const imgBad = el('img', { src: 'javascript:alert(1)' });
  assert(!s(imgBad).includes('javascript:'), `img 危险 src 不应保留（实际：${s(imgBad)}）`);
  const imgOk = el('img', { src: 'https://x.com/a.png', alt: 'pic' });
  assert(s(imgOk) === '<img src="https://x.com/a.png" alt="pic">', `img 安全 src/alt 应保留（实际：${s(imgOk)}）`);

  // 白名单外标签应 unwrap 保留子内容（不丢文本）
  const fig = el('nav', {}, [el('span', {}, ['t'])]);
  assert(s(fig) === '<span>t</span>', `白名单外标签应 unwrap（实际：${s(fig)}）`);

  // 代码块：换行铁律 + 高亮保留
  const preHi = el('pre', {}, [
    el('code', { class: 'language-python' }, [
      span('hljs-keyword', 'def'),
      D.createTextNode('\n'),
      D.createTextNode('return'),
    ]),
  ]);
  const outPre = s(preHi);
  assert(outPre.includes('hljs-keyword'), `高亮 span 应保留（实际：${outPre}）`);
  assert(outPre.includes('language-python'), `语言标签应保留（实际：${outPre}）`);
  assert(outPre.includes('\n'), `代码块必须保留换行（实际：${outPre}）`);

  // 代码块：无换行 → 回退 innerText + 清空 class（换行铁律优先于高亮）
  const preNoNl = el('pre', {}, [
    el('code', { class: 'language-python' }, [span('hljs-keyword', 'def')]),
  ]);
  const outPre2 = s(preNoNl);
  assert(!outPre2.includes('hljs-keyword'), `无换行时应清空高亮 class（实际：${outPre2}）`);
  assert(!outPre2.includes('language-python'), `无换行时应清空语言标签（实际：${outPre2}）`);
  assert(outPre2 === '<pre><code>def</code></pre>', `无换行回退应退化为纯文本（实际：${outPre2}）`);

  // ★ 批次5′（2026-10-03，真机 `#88/#89`）：视觉隐藏的角色头（Tailwind `sr-only`）必须剥离。
  //   ChatGPT 助手块 = `<h4 class="sr-only select-none">ChatGPT 说：</h4>` + 正文；
  //   旧实现只跳 `aria-hidden=true`/`role=button`，不认 `sr-only` ⟹ 交付内容首行出现「ChatGPT 说：」。
  const srH4 = el('h4', { class: 'sr-only select-none' }, ['ChatGPT 说：']);
  assert(s(srH4) === '', `sr-only 角色头应整体剥离（实际：${s(srH4)}）`);
  const srSpan = el('span', { class: 'sr-only' }, ['隐藏']);
  assert(s(srSpan) === '', `sr-only SPAN 应整体剥离（实际：${s(srSpan)}）`);
  const srVari = el('div', { class: 'foo sr-only-abs bar' }, ['x']);
  assert(s(srVari) === '', `sr-only-* token 亦应剥离（实际：${s(srVari)}）`);
  const srBlk = el('section', {}, [srH4, el('div', { class: 'text-base' }, ['真正的回答正文'])]);
  const outSr = s(srBlk);
  assert(!outSr.includes('ChatGPT 说'), `剥离角色头后不得残留文案（实际：${outSr}）`);
  assert(outSr.includes('真正的回答正文'), `剥离角色头不得伤及正文（实际：${outSr}）`);
  // 窄口径：无 sr-only 的普通 class 不得误伤
  assert(s(el('h4', { class: 'select-none' }, ['标题'])) === '<h4>标题</h4>',
    '无 sr-only 的 class 不得误剥离');
  // isSrOnly 判据单点
  assert(pageFns.isSrOnly(srH4) === true, 'isSrOnly 应识别 sr-only');
  assert(pageFns.isSrOnly(el('h4', { class: 'select-none' })) === false, 'isSrOnly 不应误判');
}
function span(cls, text) { return el('span', { class: cls }, [text]); }

// ============ safeClass（卡片侧） ============
section('safeClass（卡片侧：类名极窄白名单）');
{
  const sc = cardFns.safeClass;
  assert(sc('hljs-keyword') === 'hljs-keyword', 'hljs-* 应保留');
  assert(sc('language-python') === 'language-python', 'language-* 应保留');
  assert(sc('hljs-keyword bad') === 'hljs-keyword', `白名单外 token 应剥离（实际：${sc('hljs-keyword bad')}）`);
  assert(sc('onclick x') === null, `on* 等应返回 null（实际：${sc('onclick x')}）`);
  assert(sc('') === null, '空串返回 null');
  assert(sc('data:x hljs') === 'hljs', `仅保留白名单 token（实际：${sc('data:x hljs')}）`);
}

// ============ cloneSafe（卡片侧：DOM 二次净化） ============
section('cloneSafe（卡片侧：DOMParser 式二次净化）');
{
  const root = D.createElement('div');
  const src = el('div', {}, [
    el('script', {}, ['evil()']),
    el('a', { href: 'https://x.com' }, ['l']),
    el('span', { class: 'hljs-kw bad' }, ['t']),
    el('img', { src: 'javascript:alert(1)' }),
    el('p', {}, ['keep']),
  ]);
  cardFns.cloneSafe(src, root);

  const html = root.innerHTML;
  assert(!html.includes('<script'), `script 必须被剥离（实际：${html}）`);
  assert(!html.toLowerCase().includes('javascript:'), `javascript: 必须被剥离（实际：${html}）`);
  assert(root.querySelector('a') && root.querySelector('a').getAttribute('href') === 'https://x.com', 'a 的 https href 应保留');
  const aEl = root.querySelector('a');
  assert(aEl && aEl.getAttribute('target') === '_blank', 'a 应带 target=_blank');
  const sp = root.querySelector('span');
  assert(sp && sp.getAttribute('class') === 'hljs-kw', `span 仅保留白名单 class（实际：${sp && sp.getAttribute('class')}）`);
  assert(html.includes('keep'), '普通文本应保留');
}

// ============ 镜像校验：两端标签集一致 ============
section('镜像校验：SAFE_TAGS ≡ SAFE_HTML_TAGS');
{
  const pKeys = Object.keys(pageFns.SAFE_TAGS).sort();
  const cKeys = Object.keys(cardFns.SAFE_HTML_TAGS).sort();
  assert(JSON.stringify(pKeys) === JSON.stringify(cKeys),
    `页面侧与卡片侧标签集应一致：\n  页面 ${pKeys.join(',')}\n  卡片 ${cKeys.join(',')}`);
}

// ============ splitBlock 记忆化（P1/P2 性能） ============
section('splitBlock 记忆化（稳定块命中缓存、流式块必重算）');
{
  const sb = dsSplit.splitBlock;
  const getHits = dsSplit.getHits;
  const getMisses = dsSplit.getMisses;

  // 稳定块：内容不变 → 第二次调用必须命中缓存（同一对象引用，不重跑 serializeHtml/querySelectorAll）
  const b1 = el('div', {}, [el('p', {}, ['hello'])]);
  const r1 = sb(b1);
  assert(r1 && r1.ans === '<div><p>hello</p></div>', `首算应得净化 HTML（实际：${r1 && r1.ans}）`);
  const m0 = getMisses();
  const h0 = getHits();
  const r2 = sb(b1);
  assert(r2 === r1, '内容不变时应返回同一对象引用（命中缓存，零重算）');
  assert(getMisses() === m0 && getHits() === h0 + 1, '内容不变时应 +1 命中、不增 miss');

  // 流式块：文本变化 → 指纹变 → 必重算
  b1.innerHTML = '<p>world</p>';
  const r3 = sb(b1);
  assert(r3 !== r1, '改文本后应重算（非缓存对象）');
  assert(r3.ans === '<div><p>world</p></div>', `改文本后应得新 HTML（实际：${r3.ans}）`);
  assert(getMisses() === m0 + 1, '改文本后应 +1 miss');

  // 结构变化（新增子节点）→ 文本随之变 → 必重算；结构未变时则命中
  const b2 = el('div', {}, [el('p', {}, ['same'])]);
  sb(b2); // miss
  const beforeHits = getHits();
  const r2a = sb(b2); // 同一元素、文本不变 → 命中
  b2.appendChild(el('span', {}, ['extra'])); // 文本变为 'sameextra' → 必重算
  const r2b = sb(b2);
  assert(r2b !== r2a, '结构变化致文本变时应重算');
  assert(getHits() === beforeHits + 1, '结构未变时应 +1 命中');

  // 不同元素（即便内容相同）各自独立计算
  const b3 = el('div', {}, [el('p', {}, ['hello'])]);
  const r3b = sb(b3);
  assert(r3b !== r1, '不同元素即便内容相同也应独立计算（WeakMap 按元素键）');
}

// ============ 结果汇总 ============
console.log(`\n通过 ${passed} 项，失败 ${failed} 项。`);
if (failed) {
  console.error('失败项：\n - ' + failures.join('\n - '));
  process.exit(1);
}
console.log('全部通过。');
