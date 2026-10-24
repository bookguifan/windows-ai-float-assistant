#!/usr/bin/env node
// 业务逻辑单元测试（审计报告 测试补充）—— 覆盖 dsfloat_init.js 里核心但此前未测的纯逻辑：
//
//   - 文本归一化：norm / compactText / stripTags
//   - readableText：跳过 button/svg/script/style、role=button、aria-hidden 的可读文本抽取
//   - toMarkdown / tableToMarkdown：HTML→Markdown 兜底（标题/加粗/列表/引用/代码/链接/表格…）
//   - answerHtml / hasVisibleHtml：净化 HTML 出块 + 空壳判定
//   - hasAnswerElement / splitBlockRaw：回答/思考拆分（结果回流 R1 的核心判据）
//   - 截图投递：shotReset / shotPush（顺序校验）/ base64ToBytes / findFileInput
//
// 设计原则（同 sanitize.test.mjs）：不改生产代码，把自包含函数块从源码抽取到 jsdom 执行并断言。
//
// 运行：NODE_PATH=<workspace>/node_modules node tests/business-logic.test.mjs
// 依赖：jsdom（仅 dev，装在托管 workspace）。

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const DS_PATH = resolve(root, 'src-tauri/src/dsfloat_init.js');

// 换行符（避免源码里写裸 \n 字面量被工具串行化）
const NL = String.fromCharCode(10);

let passed = 0;
let failed = 0;
const failures = [];
function assert(cond, msg) {
  if (cond) passed++;
  else { failed++; failures.push(msg); console.error('  x ' + msg); }
}
function section(name) { console.log('\n# ' + name); }

// ---- 抽取生产源码里的自包含函数块 ----
function read(p) {
  try { return readFileSync(p, 'utf8'); }
  catch (e) { throw new Error('读不到 ' + p + '：' + e.message); }
}
function sliceBetween(src, startMarker, endMarker) {
  const s = src.indexOf(startMarker);
  if (s < 0) throw new Error('未找到起始标记：' + startMarker);
  const e = src.indexOf(endMarker, s + startMarker.length);
  if (e < 0) throw new Error('未找到结束标记：' + endMarker);
  return src.slice(s, e);
}

let JSDOM;
try {
  const { createRequire } = await import('node:module');
  const require = createRequire(import.meta.url);
  ({ JSDOM } = require('jsdom'));
} catch (e) {
  console.error('jsdom 未安装。请在托管 workspace 执行：');
  console.error('  npm install --prefix <workspace> jsdom');
  console.error('并运行：NODE_PATH=<workspace>/node_modules node tests/business-logic.test.mjs');
  process.exit(2);
}

const ds = read(DS_PATH);

// 主函数块：function norm(s) { → // 只取回答（剥离思考前缀后的部分）
const dsMainSrc = sliceBetween(
  ds,
  'function norm(s) {',
  '// 只取回答（剥离思考前缀后的部分）'
);
const biz = new Function(
  dsMainSrc +
    '\nreturn { norm, compactText, stripTags, readableText, SAFE_TAGS, escapeHtmlText,' +
    ' safeUrl, safeImgSrc, codeLangClass, serializeCodeText, serializeHtml,' +
    ' hasVisibleHtml, answerHtml, toMarkdown, tableToMarkdown, hasAnswerElement,' +
    ' splitBlockRaw, splitBlock, splitCacheStats, isSrOnly };'
)();

// 截图投递函数块：let _shotChunks = []; → function hasAttachment()（在 window 内 eval，复用 document/File/atob）
const dsShotSrc = sliceBetween(ds, 'let _shotChunks = [];', 'function hasAttachment()');
const dom = new JSDOM('<!DOCTYPE html><html><body></body></html>', { runScripts: 'outside-only' });
const w = dom.window;
w.eval('function findInput(){return document.querySelector("textarea")} function composerRootOf(){return document.body} function isVisible(){return true}\n' + dsShotSrc + '\n; window.__SHOT = { shotReset, shotPush, base64ToBytes, findFileInput };');
const shot = w.__SHOT;
const D = w.document;
function el(tag, attrs, kids) {
  const n = D.createElement(tag);
  if (attrs) for (const k in attrs) n.setAttribute(k, attrs[k]);
  if (kids) for (const c of kids) n.appendChild(typeof c === 'string' ? D.createTextNode(c) : c);
  return n;
}

// ============ norm ============
section('norm（空白折叠 + 裁剪 + null 兜底）');
{
  const n = biz.norm;
  assert(n('  a   b  c ') === 'a b c', '多空白应折叠');
  assert(n('x') === 'x', '无空白原样');
  assert(n('') === '', '空串空串');
  assert(n(null) === '', 'null 兜底空串');
  assert(n(undefined) === '', 'undefined 兜底空串');
}

// ============ compactText / stripTags ============
section('compactText / stripTags（结构判据用）');
{
  assert(biz.compactText(' 请解释 下面的 内容 ') === '请解释下面的内容', '应去全部空白');
  assert(biz.stripTags('<p>请解释</p><p>正文</p>') === '请解释正文', '应去标签');
  // 回显判据：标签插在提示词中间会打断连续性 → stripTags 后恢复（R11 修复的回归点）
  const tagged = '<p>请解释下面的内容：</p><p>正文</p>';
  assert(biz.stripTags(tagged).indexOf('请解释下面的内容：正文') >= 0, '剥离标签后提示词与正文重新连续');
}

// ============ readableText（跳过按钮/图标/隐藏子树） ============
section('readableText（跳过 button/svg/role=button/aria-hidden）');
{
  const rt = biz.readableText;
  let b = el('div', {}, ['Hello ', el('button', {}, ['x']), ' world']);
  assert(rt(b) === 'Hello world', '应跳过 button');

  b = el('div', {}, ['Hello ', el('svg', {}, ['x']), ' world']);
  assert(rt(b) === 'Hello world', '应跳过 svg');

  b = el('div', {}, ['Hello ', el('span', { role: 'button' }, ['x']), ' tail']);
  assert(rt(b) === 'Hello tail', '应跳过 role=button');

  b = el('div', {}, ['Hello ', el('span', { 'aria-hidden': 'true' }, ['x']), ' tail']);
  assert(rt(b) === 'Hello tail', '应跳过 aria-hidden');

  b = el('div', {}, ['Hello ', el('span', {}, ['nested']), ' world']);
  assert(rt(b) === 'Hello nested world', '普通 span 应保留');

  // script 子节点不应进入文本（XSS 文本不污染回读）
  b = el('div', {}, ['safe', el('script', {}, ['evil()'])]);
  assert(rt(b) === 'safe', '应跳过 script');
}

// ============ toMarkdown（HTML→MD 兜底） ============
section('toMarkdown（HTML→Markdown 兜底）');
{
  const md = biz.toMarkdown;
  assert(md(el('h1', {}, ['标题'])).indexOf('# 标题') >= 0, 'H1');
  assert(md(el('h3', {}, ['小标题'])).indexOf('### 小标题') >= 0, 'H3');
  assert(md(el('strong', {}, ['粗'])) === '**粗**', '加粗');
  assert(md(el('em', {}, ['斜'])) === '*斜*', '斜体');
  assert(md(el('code', {}, ['x=1'])) === '`x=1`', '行内代码');
  assert(md(el('a', { href: 'https://x.com' }, ['链接'])).indexOf('[链接](https://x.com)') >= 0, '链接');
  assert(md(el('blockquote', {}, ['引用'])).indexOf('> 引用') >= 0, '引用');
  assert(md(el('hr')).indexOf('---') >= 0, '分隔线');
  const ul = el('ul', {}, [el('li', {}, ['甲']), el('li', {}, ['乙'])]);
  assert(md(ul).indexOf('- 甲') >= 0 && md(ul).indexOf('- 乙') >= 0, '无序列表');
  const ol = el('ol', {}, [el('li', {}, ['一']), el('li', {}, ['二'])]);
  const olOut = md(ol);
  assert(/1\. 一/.test(olOut) && /2\. 二/.test(olOut), '有序列表 实际=' + JSON.stringify(olOut));
  // 图片兜底：![alt](src)
  assert(md(el('img', { alt: '图', src: 'https://x/a.png' })).indexOf('![图](https://x/a.png)') >= 0, '图片');
}

// ============ tableToMarkdown（管道表 + 分隔行） ============
section('tableToMarkdown（管道式 Markdown 表）');
{
  const t = el('table', {}, [
    el('thead', {}, [el('tr', {}, [el('th', {}, ['A']), el('th', {}, ['B'])])]),
    el('tbody', {}, [el('tr', {}, [el('td', {}, ['1']), el('td', {}, ['2'])])]),
  ]);
  const out = biz.tableToMarkdown(t);
  assert(out.indexOf('| A | B |') >= 0, '表头');
  assert(out.indexOf('| --- | --- |') >= 0, '分隔行');
  assert(out.indexOf('| 1 | 2 |') >= 0, '表体');
}

// ============ answerHtml / hasVisibleHtml（净化出块 + 空壳判定） ============
section('answerHtml / hasVisibleHtml（空壳不交付，R10 回归）');
{
  assert(biz.hasVisibleHtml('<div></div>') === false, '空壳 div 不可见');
  assert(biz.hasVisibleHtml('<p>x</p>') === true, '含文本可见');
  assert(biz.hasVisibleHtml('<img src="x">') === true, '图片可见');

  // 空壳块 → answerHtml 返回空串（绝不骗过 Rust 最短长度闸门）
  assert(biz.answerHtml(el('div', {}, [])) === '', '空壳块 answerHtml 应返回空串');

  // XSS：script 被剥，只留文本
  const xss = el('div', {}, ['safe', el('script', {}, ['evil()'])]);
  const ax = biz.answerHtml(xss);
  assert(ax.indexOf('evil') < 0, 'script 内容不应进入净 HTML');
  assert(ax.indexOf('safe') >= 0, '安全文本保留');

  // 图片安全 src 保留
  const img = el('div', {}, [el('img', { src: 'https://x/a.png', alt: 'pic' })]);
  const ai = biz.answerHtml(img);
  assert(ai.indexOf('https://x/a.png') >= 0 && ai.indexOf('<img') >= 0, '安全图片应保留');

  // 代码块带换行 → 高亮 + 换行保留
  const pre = el('pre', {}, [el('code', { class: 'language-python' }, ['def' + NL + 'return'])]);
  const ap = biz.answerHtml(pre);
  assert(ap.indexOf('language-python') >= 0, '代码语言标签保留');
  assert(ap.indexOf('def') >= 0 && ap.indexOf('return') >= 0, '代码文本保留');
  assert(ap.indexOf(NL) >= 0, '代码块换行保留');
}

// ============ hasAnswerElement（答案元素探测，A+B 判终修复） ============
section('hasAnswerElement（真实答案元素 vs 思考区内的 ds-markdown）');
{
  // 真实答案：ds-markdown 含可见 HTML → true
  const real = el('div', {}, [el('div', { class: 'ds-markdown' }, ['答案正文'])]);
  assert(biz.hasAnswerElement(real) === true, 'ds-markdown 答案元素应命中');

  // 思考区内的 ds-markdown 不算（closest ds-think 排除）
  const think = el('div', { class: 'ds-think-abc' }, [
    el('div', { class: 'ds-markdown' }, ['思考正文']),
  ]);
  assert(biz.hasAnswerElement(think) === false, '思考区内的 ds-markdown 不应算答案');

  // 空块 → false
  assert(biz.hasAnswerElement(el('div', {}, [])) === false, '空块无答案元素');
}

// ============ splitBlockRaw（回答/思考拆分 —— 结果回流核心判据） ============
section('splitBlockRaw（回答/思考拆分，pending/done 闸门）');
{
  const sb = biz.splitBlockRaw;

  // ① 普通回答块（无思考折叠头，含答案元素）→ 整块即回答，done=true
  const plain = el('div', {}, [el('div', { class: 'ds-markdown' }, ['今天天气晴'])]);
  const r1 = sb(plain, biz.readableText(plain));
  assert(r1.ans.indexOf('今天天气晴') >= 0, '回答应含正文');
  assert(r1.think === '' && r1.pending === false && r1.done === false, '普通正文尚无完成控件：pending=false/done=false');

  // ② 思考占位块（className 含 ds-think）→ 此刻只有思考，pending=true/done=false
  const thinkBlock = el('div', { class: 'ds-think-something' }, ['思考中：正在推算']);
  const r2 = sb(thinkBlock, biz.readableText(thinkBlock));
  assert(r2.ans === '' && r2.pending === true && r2.done === false, '思考占位块：ans 空、pending=true');
  assert(r2.think.indexOf('思考中') >= 0, '思考文本应回传');

  // ③ 思考折叠头 + 独立答案：剥离思考前缀，取真实回答（R1 真机缺陷之二回归）
  const mixed = el('div', {}, [
    el('div', { class: 'ds-think' }, ['已思考（用时 1 秒）思考正文']),
    el('div', { class: 'ds-markdown' }, ['这是真正的回答']),
  ]);
  const r3 = sb(mixed, biz.readableText(mixed));
  assert(r3.ans.indexOf('这是真正的回答') >= 0, '应取到真实回答');
  assert(r3.think.indexOf('已思考') >= 0, '思考前缀应被剥离为 think');
  assert(r3.pending === false && r3.done === false, '含答案元素仍需等待完成控件');

  // ④ 思考折叠头但切不出答案（只有思考）→ pending=true，绝不把思考当回答交付
  const thinkOnly = el('div', {}, [
    el('div', { class: 'ds-think' }, ['已思考（用时 1 秒）只有思考']),
  ]);
  const r4 = sb(thinkOnly, biz.readableText(thinkOnly));
  assert(r4.pending === true, '有思考头但无答案 → pending=true');
  assert(r4.done === false, '无答案元素 → done=false');
}

// ============ 截图投递：顺序校验 + base64 + 选 input ============
section('截图投递（shotPush 顺序校验 / base64ToBytes / findFileInput）');
{
  assert(shot.shotReset() === true, 'shotReset 返回 true');
  assert(shot.shotPush('aaa', 0) === true, 'seq=0 首块应接受');
  assert(shot.shotPush('bbb', 1) === true, 'seq=1 续块应接受');
  assert(shot.shotPush('ccc', 5) === false, '乱序/越界 seq 应拒绝（防字节错位）');
  assert(shot.shotPush(123, 2) === false, '非字符串块应拒绝');

  // base64 向量
  const m = shot.base64ToBytes('TWFu'); // "Man"
  assert(Array.from(m).join(',') === '77,97,110', 'base64 "Man"');
  const a = shot.base64ToBytes('QQ=='); // 0x41 = 'A'
  assert(Array.from(a).join(',') === '65', 'base64 "A"');

  // findFileInput：accept 含 image 的优先
  const f1 = el('input', { type: 'file', accept: '.txt' });
  const f2 = el('input', { type: 'file', accept: 'image/png' });
  D.body.appendChild(el('textarea'));
  D.body.appendChild(f1);
  D.body.appendChild(f2);
  const picked = shot.findFileInput();
  assert(picked && picked.getAttribute('accept') === 'image/png', '应优先选 image 类型 input');
  D.body.removeChild(f1);
  D.body.removeChild(f2);
}

// ===========================================================================
// P1 补强（QA 回传批次 B-P1）：发送可靠性主链路 + 会话容器定位启发式
// ---------------------------------------------------------------------------
// 这些函数依赖**几何**（getBoundingClientRect / getComputedStyle / scrollHeight…），
// jsdom 不做布局 → 必须在构造的元素上**显式 stub 几何**，否则任何断言都测不到东西。
//
// 抽取方式沿用 sliceBetween；但改在 **jsdom 的 window 里 eval**（`win.eval`）——
// 因为函数体引用 window / document / getComputedStyle，而 `new Function` 的全局作用域
// 里没有它们（Node 无 window/document）。jsdom 的已知缺口（本文件已在用显式 shim 规避）：
//   · `document.scrollingElement` 未实现（undefined）→ 测「整页滚动」分支时需 defineProperty；
//   · `element.isContentEditable` 未实现（undefined）→ 测 contenteditable 分支时需 stub；
//   · `document.execCommand` 未实现 → contenteditable 的 execCommand **首选**路径无法覆盖。
// ===========================================================================
const sendSrc = 'let SEND_CONTEXT=null; function prepareSend(){}\n' + sliceBetween(ds, 'const INPUT_SELECTOR', 'function inspect() {');
const convSrc = sliceBetween(ds, 'const INPUT_SELECTOR', '// 只取回答（剥离思考前缀后的部分）');
const SEND_EXPORTS =
  '; window.__SEND = { findInput, scoredSenders, composerRootOf, fill, setNativeValue,' +
  ' stillHasText, pressEnter, submit, readText, isVisible, isVisibleControl, norm,' +
  ' normLoose, textLooksSame };';
const CONV_EXPORTS =
  '; window.__CONV = { locateConversation, scanLists, blocksOf, scrollsEl, nearestScroller,' +
  ' composerSeeds, tailHasEcho, findInput, compactText, stripTags, norm, median,' +
  ' serializeHtml, answerHtml, isSrOnly, tailIsDecor, toMarkdown };';

function freshWin(src, exportExpr) {
  const d = new JSDOM('<!DOCTYPE html><html><body></body></html>', { runScripts: 'outside-only' });
  const win = d.window;
  win.eval(src + exportExpr);
  return win;
}
function mk(Dd, tag, attrs) {
  const n = Dd.createElement(tag);
  if (attrs) for (const k in attrs) n.setAttribute(k, attrs[k]);
  return n;
}
function mkText(Dd, tag, s, attrs) {
  const n = mk(Dd, tag, attrs);
  n.textContent = s;
  return n;
}
// 在元素上 stub 几何（jsdom 恒返回 0 → 必须显式给值）
function stubRect(n, o) {
  n.getBoundingClientRect = () => ({
    left: o.left || 0,
    top: o.top || 0,
    right: (o.left || 0) + (o.width || 0),
    bottom: (o.top || 0) + (o.height || 0),
    width: o.width || 0,
    height: o.height || 0,
    x: o.left || 0,
    y: o.top || 0,
  });
}
// scrollHeight / clientHeight 在 jsdom 是只读 getter → 用 defineProperty shadow
function stubScroll(n, sh, ch, oy) {
  Object.defineProperty(n, 'scrollHeight', { value: sh, configurable: true });
  Object.defineProperty(n, 'clientHeight', { value: ch, configurable: true });
  n.style.overflowY = oy || 'scroll';
}
const LONG = (n) =>
  'lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor '.repeat(n);

// ============ P1-a：findInput（几何打分：位置 + 语义） ============
section('P1-a findInput（几何打分：底部偏置 + 语义）');
{
  const win = freshWin(sendSrc, SEND_EXPORTS);
  const Dd = win.document;
  const S = win.__SEND;
  // 两个同型 textarea：越靠页面底部（bottomBias 越大）得分越高 → 选下面的
  const hi = mk(Dd, 'textarea');
  Dd.body.appendChild(hi);
  stubRect(hi, { top: 180, left: 10, width: 300, height: 40 });
  const lo = mk(Dd, 'textarea');
  Dd.body.appendChild(lo);
  stubRect(lo, { top: 700, left: 10, width: 300, height: 40 });
  assert(S.findInput() === lo, 'findInput 应在多个候选中选「更靠页面底部」的 textarea');

  // 相同几何下：textarea（semantic=1）应胜过 input[type=text]（semantic=0.4）
  const win2 = freshWin(sendSrc, SEND_EXPORTS);
  const D2 = win2.document;
  const S2 = win2.__SEND;
  const it = mk(D2, 'input', { type: 'text' });
  D2.body.appendChild(it);
  stubRect(it, { top: 700, left: 10, width: 300, height: 40 });
  const ta = mk(D2, 'textarea');
  D2.body.appendChild(ta);
  stubRect(ta, { top: 700, left: 10, width: 300, height: 40 });
  assert(S2.findInput() === ta, 'findInput 应因语义打分选 textarea 而非同几何的 text input');
}

// ============ P1-a：scoredSenders（发送按钮只在 composer 内） ============
section('P1-a scoredSenders / submit（发送按钮限定在 composer 内，历史动作不入候选）');
{
  const win = freshWin(sendSrc, SEND_EXPORTS);
  const Dd = win.document;
  const S = win.__SEND;
  const form = mk(Dd, 'form');
  const ta = mk(Dd, 'textarea');
  form.appendChild(ta);
  const send = mk(Dd, 'button', { type: 'button', 'aria-label': '发送' });
  form.appendChild(send);
  Dd.body.appendChild(form);
  // 历史消息气泡里的「重新发送」按钮（在 composer 之外，文案含「发送」二字 → 旧实现会误点）
  const histBar = mk(Dd, 'div', { class: 'history' });
  const resend = mk(Dd, 'button', { type: 'button', 'aria-label': '重新发送' });
  histBar.appendChild(resend);
  Dd.body.appendChild(histBar);
  stubRect(ta, { top: 700, left: 10, width: 300, height: 40 });
  stubRect(send, { top: 706, left: 320, width: 32, height: 32 });
  stubRect(resend, { top: 100, left: 40, width: 90, height: 24 });

  const ranked = S.scoredSenders(ta);
  assert(ranked.length >= 1 && ranked[0].el === send, 'scoredSenders 榜首应是 composer 内的发送按钮');
  assert(!ranked.some((r) => r.el === resend), '历史消息的「重新发送」不应进入候选（防误点重提交）');

  // submit 实际点击 composer 内发送按钮，绝不点历史「重新发送」
  S.setNativeValue(ta, 'newword');
  let hitSend = false;
  let hitResend = false;
  send.addEventListener('click', () => { hitSend = true; });
  resend.addEventListener('click', () => { hitResend = true; });
  assert(S.submit('newword', 0) === 'ok', 'submit 应返回 ok');
  assert(hitSend === true && hitResend === false, 'submit 应点击 composer 内发送按钮而非历史重发按钮');
}

// ============ P1-a：composerRootOf（上溯到 composer 根） ============
section('P1-a composerRootOf（从任意内部子节点上溯到 composer 根）');
{
  const win = freshWin(sendSrc, SEND_EXPORTS);
  const Dd = win.document;
  const S = win.__SEND;
  // 有 <form>：直接取 form
  const form = mk(Dd, 'form');
  const deep = mk(Dd, 'div');
  const deep2 = mk(Dd, 'div');
  const ta = mk(Dd, 'textarea');
  form.appendChild(deep);
  deep.appendChild(deep2);
  deep2.appendChild(ta);
  Dd.body.appendChild(form);
  assert(S.composerRootOf(ta) === form, 'composerRootOf 有 form 时应取最近的 form');

  // 无 <form>：取「向上的、首个包含可点按钮的最小容器」
  const win2 = freshWin(sendSrc, SEND_EXPORTS);
  const D2 = win2.document;
  const S2 = win2.__SEND;
  const wrap = mk(D2, 'div', { id: 'wrap' });
  const inner = mk(D2, 'div', { id: 'inner' });
  const ta2 = mk(D2, 'textarea');
  wrap.appendChild(inner);
  inner.appendChild(ta2);
  wrap.appendChild(mk(D2, 'button', { type: 'button' }));
  D2.body.appendChild(wrap);
  assert(S2.composerRootOf(ta2) === wrap, 'composerRootOf 无 form 时应上溯到含按钮的最小容器');
}

// ============ P1-a：注入与发送状态机 ============
section('P1-a 注入与发送（setNativeValue / fill / stillHasText / pressEnter）');
{
  const win = freshWin(sendSrc, SEND_EXPORTS);
  const Dd = win.document;
  const S = win.__SEND;
  const form = mk(Dd, 'form');
  const ta = mk(Dd, 'textarea');
  form.appendChild(ta);
  Dd.body.appendChild(form);
  stubRect(ta, { top: 700, left: 10, width: 300, height: 40 });

  // setNativeValue：写值 + 派发一次可冒泡 input 事件（React 受控组件契约）
  let inputEvents = 0;
  form.addEventListener('input', () => inputEvents++);
  S.setNativeValue(ta, 'abc');
  assert(ta.value === 'abc', 'setNativeValue 应写入 value');
  assert(inputEvents === 1, 'setNativeValue 应派发一次冒泡 input 事件');

  // stillHasText：非空命中 / 清空后为 false
  assert(S.stillHasText('abc') === true, 'stillHasText 输入框含目标文本应为 true');
  ta.value = '';
  assert(S.stillHasText('abc') === false, 'stillHasText 输入框清空后应为 false');

  // fill：写入并返回 true
  assert(S.fill('hello world') === true && ta.value === 'hello world', 'fill 应写入文本并返回 true');

  // pressEnter：在输入框上合成 Enter keydown
  const keys = [];
  ta.addEventListener('keydown', (e) => keys.push(e.key + ':' + e.keyCode));
  assert(S.pressEnter() === true, 'pressEnter 应返回 true');
  assert(keys.length === 1 && keys[0] === 'Enter:13', 'pressEnter 应派发 Enter(keyCode 13) 到输入框');
}

// ============ P1-a：submit 状态机四态 ============
section('P1-a submit（no-input / empty-after-fill / 软校验 / no-send-button）');
{
  const win = freshWin(sendSrc, SEND_EXPORTS);
  const Dd = win.document;
  const S = win.__SEND;
  // no-input：页面上没有任何可见输入框
  assert(S.submit('x', 0) === 'no-input', '无输入框应返回 no-input');

  const form = mk(Dd, 'form');
  const ta = mk(Dd, 'textarea');
  form.appendChild(ta);
  Dd.body.appendChild(form);
  stubRect(ta, { top: 700, left: 10, width: 300, height: 40 });
  assert(S.submit('x', 0) === 'empty-after-fill', '输入框为空应返回 empty-after-fill');

  // ★ 批次1（2026-10-03）：不符**不再**返回 content-mismatch，而是照常发送。
  //   此处无任何发送候选 → 走 soft-mismatch-no-send-button（Rust 侧转页内回车兜底）。
  S.setNativeValue(ta, 'abc');
  assert(
    S.submit('xyz', 0) === 'content-mismatch',
    '文本不符必须拒绝提交'
  );

  // 只有一个「远离输入框、无发送语义」的按钮 → 候选得分 < 2 → no-send-button（不谎报 ok）
  const far = mk(Dd, 'button', { type: 'button', 'aria-label': 'x' });
  form.appendChild(far);
  stubRect(far, { top: 0, left: 0, width: 30, height: 30 });
  assert(S.submit('abc', 0) === 'no-send-button', '文本相符但候选得分不足应返回 no-send-button');
}

// ============ 批次1：软校验（normLoose / textLooksSame）与「不再放弃发送」 ============
section('批次1 normLoose / textLooksSame（跨站点空白口径统一，三级判据）');
{
  const win = freshWin(sendSrc, SEND_EXPORTS);
  const S = win.__SEND;
  const nl = S.normLoose;
  const same = S.textLooksSame;

  // normLoose：删掉一切空白（与 Rust capture::normalize_text 同口径）
  assert(nl('  a b\tc\nd ') === 'abcd', 'normLoose 应删除全部空白（含制表/换行）');
  assert(nl('') === '', 'normLoose 空串');
  assert(nl(null) === '', 'normLoose null 兜底');

  // ① 严格判据（原行为）：norm 后包含
  assert(same('你好 世界', '你好 世界') === true, '严格相等应通过');

  // ② 宽松判据（治本）：ProseMirror 段间无分隔 vs 期望的换行折叠成空格
  //    got = "行1行2行3"（ProseMirror textContent），want = "行1 行2 行3"（norm 折空格）
  assert(
    same('行1行2行3', '行1 行2 行3') === true,
    'ProseMirror 段间无分隔 vs 期望折空格：应判为相符（② 宽松判据）'
  );
  assert(same('甲\n乙\n丙', '甲 乙 丙') === true, 'got 为真实换行 vs 期望空格：应判为相符');

  // ③ 首尾锚点双重否定（长文本 + 站点做了字符级改写）
  const longWant = '开头甲' + '中'.repeat(60) + '结尾乙';
  const longGot = '开头甲' + '中'.repeat(60) + '结尾乙' + '尾部追加';
  assert(same(longGot, longWant) === true, '首尾锚点均命中应判为相符');

  // 空 want：无从校验 ⇒ 通过（与原 `if (want && …)` 语义一致）
  assert(same('任意', '') === true, '空期望值应通过（无从校验）');

  // 负例：完全不同的短文本 ⇒ 不通过
  assert(same('完全不同的内容', '毫不相干的东西') === false, '差异文本应判为不相符');

  // 负例：长文本但首尾锚点只命中其一 ⇒ 不通过（双重否定才对）
  const w2 = 'A头' + '中'.repeat(60) + 'B尾';
  assert(same('A头' + '中'.repeat(60) + 'C尾', w2) === false, '仅首锚点命中、尾锚点不符应判为不相符');
}

section('提交契约：内容不符不得发送，有候选按钮也不能绕过');
{
  const win = freshWin(sendSrc, SEND_EXPORTS);
  const Dd = win.document;
  const S = win.__SEND;

  const form = mk(Dd, 'form');
  const ta = mk(Dd, 'textarea');
  form.appendChild(ta);
  Dd.body.appendChild(form);
  stubRect(ta, { top: 700, left: 10, width: 300, height: 40 });

  // 造一个「与输入框同行、位于其右侧、文案含 send」的高分发送按钮（score ≥ 2）
  const btn = mk(Dd, 'button', { type: 'button', 'aria-label': 'Send' });
  form.appendChild(btn);
  stubRect(btn, { top: 700, left: 340, width: 32, height: 32 });

  // 输入框内容与期望「不相符」——旧实现会 return content-mismatch 而**完全不发送**
  S.setNativeValue(ta, '实际写进去的内容');
  let clicked = false;
  btn.addEventListener('click', () => {
    clicked = true;
  });

  const st = S.submit('期望的完全不同文本', 0);
  assert(st === 'content-mismatch', '文本不符应返回 content-mismatch');
  assert(clicked === false, '内容不符不得点击发送按钮');

  // 相符时仍应是 'ok'（不回归）
  S.setNativeValue(ta, '相符文本');
  assert(S.submit('相符文本', 0) === 'ok', '文本相符应返回 ok');
}

// ============ P1-a：fill 的 contenteditable 兜底分支（jsdom 限制下仅覆盖非 execCommand 路径） ============
section('P1-a fill contenteditable 兜底（直接改内容 + 派发 input；execCommand 首选路径 jsdom 无法覆盖）');
{
  const win = freshWin(sendSrc, SEND_EXPORTS);
  const Dd = win.document;
  const S = win.__SEND;
  const d = mk(Dd, 'div', { contenteditable: 'true' });
  Dd.body.appendChild(d);
  stubRect(d, { top: 700, left: 10, width: 300, height: 40 });
  // jsdom 未实现 isContentEditable（undefined）与 document.execCommand → 显式 stub 前者以进入该分支
  Object.defineProperty(d, 'isContentEditable', { value: true, configurable: true });
  let ev = 0;
  d.addEventListener('input', () => ev++);
  assert(S.fill('hello') === true, 'fill 对 contenteditable 应返回 true');
  assert(d.textContent === 'hello', 'fill contenteditable 兜底应直接写入 textContent');
  assert(ev === 1, 'fill contenteditable 兜底应派发一次 input 事件');
}

// ============ P1-b：blocksOf（把直接子节点当消息块统计） ============
section('P1-b blocksOf（将直接子节点当消息块统计）');
{
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const box = mk(Dd, 'div');
  box.appendChild(mkText(Dd, 'div', 'first block text one'));
  box.appendChild(mk(Dd, 'div')); // 空块不应计入 withText
  box.appendChild(mkText(Dd, 'div', 'second block text two'));
  box.appendChild(mkText(Dd, 'div', 'third block'));
  box.appendChild(mkText(Dd, 'div', 'fourth block'));
  const b = C.blocksOf(box);
  const expectLen =
    'first block text one'.length + 'second block text two'.length +
    'third block'.length + 'fourth block'.length;
  assert(b.childCount === 5, 'blocksOf.childCount 应等于直接子节点数');
  assert(b.withText === 4, 'blocksOf.withText 应只计有文本的子块');
  assert(b.firstTexts.length === 3 && b.firstTexts[0] === 'first block text one', 'blocksOf.firstTexts 应取前 3 个有文本块');
  assert(b.lastTexts.length === 3 && b.lastTexts[2] === 'fourth block', 'blocksOf.lastTexts 应取末 3 个有文本块');
  assert(b.textLen === expectLen, 'blocksOf.textLen 应为各子块文本长之和');
}

// ============ P1-b：scrollsEl / nearestScroller ============
section('P1-b scrollsEl / nearestScroller（可滚动判定 + 最近可滚动祖先）');
{
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const plain = mk(Dd, 'div');
  Dd.body.appendChild(plain);
  assert(C.scrollsEl(plain) === false, 'scrollsEl 无溢出元素应为 false');

  const sc = mk(Dd, 'div');
  Dd.body.appendChild(sc);
  stubScroll(sc, 5000, 600);
  assert(C.scrollsEl(sc) === true, 'scrollsEl 溢出且 overflow-y:scroll 应为 true');

  const small = mk(Dd, 'div');
  Dd.body.appendChild(small);
  stubScroll(small, 150, 120);
  assert(C.scrollsEl(small) === false, 'scrollsEl 溢出量 < 40px 应为 false');

  const shallow = mk(Dd, 'div');
  Dd.body.appendChild(shallow);
  stubScroll(shallow, 5000, 100);
  assert(C.scrollsEl(shallow) === false, 'scrollsEl clientHeight < 120 应为 false');

  // 整页滚动兜底：overflow-y 为 visible，但元素就是 document.scrollingElement
  const page = mk(Dd, 'div');
  Dd.body.appendChild(page);
  stubScroll(page, 5000, 600, 'visible');
  Object.defineProperty(Dd, 'scrollingElement', { value: page, configurable: true }); // jsdom 未实现
  assert(C.scrollsEl(page) === true, 'scrollsEl 对 document.scrollingElement 应放行（整页滚动兜底）');

  const outer = mk(Dd, 'div');
  Dd.body.appendChild(outer);
  stubScroll(outer, 1000, 300);
  const inner = mk(Dd, 'div');
  outer.appendChild(inner);
  const leaf = mk(Dd, 'div');
  inner.appendChild(leaf);
  assert(C.nearestScroller(leaf) === outer, 'nearestScroller 应返回最近的可滚动祖先');
  assert(C.nearestScroller(plain) === null, 'nearestScroller 无滚动祖先时应返回 null');
}

// ============ P1-b：scanLists ============
section('P1-b scanLists（逐种子扫描候选块列表 + 文本量门槛）');
{
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const sA = mk(Dd, 'div');
  sA.appendChild(mkText(Dd, 'div', LONG(1)));
  sA.appendChild(mkText(Dd, 'div', LONG(1)));
  const sB = mk(Dd, 'div');
  sB.appendChild(mkText(Dd, 'div', LONG(1)));
  sB.appendChild(mkText(Dd, 'div', LONG(1)));
  Dd.body.appendChild(sA);
  Dd.body.appendChild(sB);
  const lists = C.scanLists([sA, sB], 50, 500);
  assert(lists.length === 2, 'scanLists 应同时扫到两个种子的候选（预算不应被首个种子吃光）');
  assert(lists[0].el === sA || lists[0].el === sB, 'scanLists 候选应包含种子容器');

  // 门槛：文本量不达标 → 不入选
  const tiny = mk(Dd, 'div');
  tiny.appendChild(mkText(Dd, 'div', 'ab'));
  tiny.appendChild(mkText(Dd, 'div', 'cd'));
  assert(C.scanLists([tiny], 50, 500).length === 0, 'scanLists 文本量低于 minTextLen 应不入选');
}

// ============ P1-b：composerSeeds ============
section('P1-b composerSeeds（沿输入框祖先链取同列兄弟子树）');
{
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const main = mk(Dd, 'div');
  Dd.body.appendChild(main);
  const conv = mk(Dd, 'div');
  main.appendChild(conv);
  conv.appendChild(mkText(Dd, 'div', '用户消息一'));
  conv.appendChild(mkText(Dd, 'div', '助手回复一'));
  // 与输入框水平不重叠的侧栏（同为主内容列的兄弟）→ 不应被收入种子
  const sidebar = mk(Dd, 'div');
  main.appendChild(sidebar);
  stubRect(sidebar, { left: -400, top: 0, width: 400, height: 400 });
  const composer = mk(Dd, 'div');
  main.appendChild(composer);
  const ta = mk(Dd, 'textarea');
  composer.appendChild(ta);
  stubRect(ta, { left: 0, top: 700, width: 300, height: 40 });
  stubRect(conv, { left: 0, top: 0, width: 800, height: 400 });
  const seeds = C.composerSeeds();
  assert(seeds.indexOf(conv) >= 0, 'composerSeeds 应把与输入框同列的会话列表收入种子');
  assert(seeds.indexOf(sidebar) < 0, 'composerSeeds 应排除与输入框水平不重叠的侧栏');
  assert(seeds.indexOf(main) < 0 && seeds.indexOf(Dd.body) < 0, 'composerSeeds 不应把输入框自身祖先链节点当种子');
}

// ============ P1-b：tailHasEcho ============
section('P1-b tailHasEcho（尾窗回显硬过滤）');
{
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const box = mk(Dd, 'div');
  box.appendChild(mkText(Dd, 'div', LONG(2)));
  box.appendChild(mkText(Dd, 'div', '用户：回读测试请回答'));
  assert(C.tailHasEcho(box, '回读测试') === true, 'tailHasEcho 尾块含提示词应为 true');
  assert(C.tailHasEcho(box, '不存在的词') === false, 'tailHasEcho 尾块不含提示词应为 false');
  assert(C.tailHasEcho(box, '') === true, 'tailHasEcho 无提示词（空）应不过滤，返回 true');
}

// ============ P1-b：locateConversation 四类骨架 ============
section('P1-b locateConversation ①：可滚动长会话容器');
{
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const conv = mk(Dd, 'div');
  Dd.body.appendChild(conv);
  stubScroll(conv, 5000, 600);
  for (let i = 0; i < 6; i++) conv.appendChild(mkText(Dd, 'div', LONG(2)));
  const loc = C.locateConversation();
  assert(loc.el === conv, '长会话应定位到可滚动会话容器本身');
  assert(loc.scanPass === 1, '可滚动长会话应命中第一遍扫描');
}

section('P1-b locateConversation ②：短会话（不滚动，输入框锚定兜底第二遍）');
{
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const main = mk(Dd, 'div');
  Dd.body.appendChild(main);
  const conv = mk(Dd, 'div');
  main.appendChild(conv);
  conv.appendChild(mkText(Dd, 'div', '用户：请解释下面的内容：回读测试'));
  conv.appendChild(mkText(Dd, 'div', 'assistant: 回读测试'));
  const composer = mk(Dd, 'div');
  main.appendChild(composer);
  const ta = mk(Dd, 'textarea');
  composer.appendChild(ta);
  stubRect(ta, { left: 0, top: 700, width: 300, height: 40 });
  stubRect(conv, { left: 0, top: 0, width: 800, height: 400 });
  const loc = C.locateConversation('回读测试');
  assert(loc.el === conv, '短会话应通过输入框锚定兜底定位到会话容器');
  assert(loc.scanPass === 2, '短会话应命中第二遍（输入框锚定）扫描');
}

section('P1-b locateConversation ③：侧栏/导航干扰不得被误选');
{
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const sidebar = mk(Dd, 'div');
  Dd.body.appendChild(sidebar);
  stubScroll(sidebar, 9000, 900);
  for (let i = 0; i < 5; i++) sidebar.appendChild(mkText(Dd, 'div', LONG(4))); // 文本量远大于会话
  const main = mk(Dd, 'div');
  Dd.body.appendChild(main);
  const conv = mk(Dd, 'div');
  main.appendChild(conv);
  stubScroll(conv, 3000, 500);
  conv.appendChild(mkText(Dd, 'div', LONG(2)));
  conv.appendChild(mkText(Dd, 'div', '用户：请解释 下面的内容 回读测试'));
  conv.appendChild(mkText(Dd, 'div', LONG(2)));

  // 无提示词约束：仅按文本量打分 → 文本更多的侧栏会赢（说明「打分」本身不足以排除侧栏）
  const noNeed = C.locateConversation();
  assert(noNeed.el === sidebar, '无回显约束时文本量更大的侧栏确实会被选中（故必须靠回显硬过滤）');
  // 有提示词：结构性硬过滤把侧栏排除 → 选中真正的会话容器
  const withNeed = C.locateConversation('回读测试');
  assert(withNeed.el === conv, '有提示词时回显硬过滤应排除侧栏，选中会话容器');
  assert(withNeed.candBefore >= 2 && withNeed.echoCand === 1, 'B3 过滤前/后候选数应体现「侧栏被剔除」');
}

section('P1-b locateConversation ④：尾块回显过滤（含回显者胜出；全无回显则不交付）');
{
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const A = mk(Dd, 'div');
  Dd.body.appendChild(A);
  stubScroll(A, 2000, 400);
  A.appendChild(mkText(Dd, 'div', LONG(2)));
  A.appendChild(mkText(Dd, 'div', LONG(2)));
  A.appendChild(mkText(Dd, 'div', '用户：回读测试请回答'));
  const B = mk(Dd, 'div');
  Dd.body.appendChild(B);
  stubScroll(B, 6000, 800);
  for (let i = 0; i < 4; i++) B.appendChild(mkText(Dd, 'div', LONG(5))); // 文本量远大于 A 但不含回显

  const loc = C.locateConversation('回读测试');
  assert(loc.el === A, 'B3 应选尾窗含回显的容器 A，而非文本量更大的无回显容器 B');
  assert(loc.candBefore === 2 && loc.echoCand === 1, 'B3 应把无回显的 B 从 2 个候选剔除为 1 个');

  // 全无回显 + 无输入框 → 不交付任何容器（宁超时也绝不交付错内容）
  const win2 = freshWin(convSrc, CONV_EXPORTS);
  const D2 = win2.document;
  const C2 = win2.__CONV;
  const B1 = mk(D2, 'div');
  D2.body.appendChild(B1);
  stubScroll(B1, 6000, 800);
  for (let i = 0; i < 4; i++) B1.appendChild(mkText(D2, 'div', LONG(5)));
  const loc2 = C2.locateConversation('不存在的提示词');
  assert(loc2.el === null, '全无回显且无输入框时应不交付容器');
  assert(loc2.candBefore >= 1 && loc2.echoCand === 0, '诊断读数应显示候选存在但被回显过滤清零');
}

// ============ 批次2′：择优兜底下钻（ChatGPT 多分支候选） ============
section('批次2′ locateConversation ⑤：多分支候选（ChatGPT 形态）应择优下钻到消息列表');
{
  // 病灶复刻（真机 pid=3228：`container=@w-sm/main pass=1 hops=0`，回读交付 2323 字符混入 UI 外壳）：
  //   候选（<main> 形态）的直接子节点是**多分支**——导航 / 消息区包装层 / composer，文本分散；
  //   且消息区是「矮（< 0.5×scrollHeight）且单子」的包装层链 —— 原判据①因高度护栏放行不了、
  //   ②又要求 childCount≥2，两个都不满足 ⇒ 卡住（hops=0，交付整个候选）。
  // 期望：走「择优兜底」下钻穿过包装层，落到真正的消息列表（drill>0，el=conv）。
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const page = mk(Dd, 'div');
  Dd.body.appendChild(page);
  stubScroll(page, 3000, 600); // 候选自身可滚动（第一遍扫描可选中它）
  // ① 导航（与输入框**水平不重叠**）
  const nav = mk(Dd, 'div');
  page.appendChild(nav);
  stubRect(nav, { left: -400, top: 0, width: 300, height: 40 });
  nav.appendChild(mkText(Dd, 'div', '导航栏'));
  // ② 消息区包装层链（矮 + 单子）：msgwrap → w1 → w2 → conv
  const msgwrap = mk(Dd, 'div');
  page.appendChild(msgwrap);
  stubRect(msgwrap, { left: 0, top: 0, width: 800, height: 500 });
  const w1 = mk(Dd, 'div');
  msgwrap.appendChild(w1);
  stubRect(w1, { left: 0, top: 0, width: 800, height: 500 });
  const w2 = mk(Dd, 'div');
  w1.appendChild(w2);
  stubRect(w2, { left: 0, top: 0, width: 800, height: 490 });
  const conv = mk(Dd, 'div');
  w2.appendChild(conv);
  stubRect(conv, { left: 0, top: 0, width: 800, height: 480 });
  conv.appendChild(mkText(Dd, 'div', '用户：请解释下面的内容：回读测试'));
  conv.appendChild(mkText(Dd, 'div', 'assistant: 这是本轮的回答正文。'));
  // ③ composer（与输入框同带，但无 ≥2 子块 → 不得被选作消息列表）
  const composer = mk(Dd, 'div');
  page.appendChild(composer);
  stubRect(composer, { left: 0, top: 800, width: 800, height: 60 });
  const ta = mk(Dd, 'textarea');
  composer.appendChild(ta);
  stubRect(ta, { left: 20, top: 810, width: 700, height: 40 });

  const loc = C.locateConversation('回读测试');
  assert(loc.el === conv, '批次2′：多分支候选应择优下钻落到消息列表（而非停在候选本身 / 误选导航）');
  assert(loc.viaDrillFallback > 0, '批次2′：应标记为走了「择优兜底」路径（drill>0）');
}

section('批次2′ locateConversation ⑥：单子承载 ≥90%（DeepSeek 形态）不得走择优路径');
{
  // 不回归护栏：DeepSeek 的虚拟列表是「单子链」，原判据恒命中 ⇒ viaDrillFallback 必须为 0。
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const vlist = mk(Dd, 'div');
  Dd.body.appendChild(vlist);
  stubScroll(vlist, 9000, 600);
  const vis = mk(Dd, 'div');
  vlist.appendChild(vis); // 唯一子节点，承载 100% 文本
  stubRect(vis, { left: 0, top: 0, width: 800, height: 8000 });
  const inner = mk(Dd, 'div');
  vis.appendChild(inner);
  stubRect(inner, { left: 0, top: 0, width: 800, height: 7000 });
  for (let i = 0; i < 4; i++) inner.appendChild(mkText(Dd, 'div', LONG(3)));
  inner.appendChild(mkText(Dd, 'div', '用户：回读测试请回答'));

  const loc = C.locateConversation('回读测试');
  assert(loc.el === inner, '批次2′：DeepSeek 单子链应照原逻辑下钻到消息列表');
  assert(loc.viaDrillFallback === 0, '批次2′：DeepSeek 形态必须走原路径（drill=0），不得触碰新分支');
}

section('批次2′ locateConversation ⑦：单条超长消息不应触发择优路径（保持原行为）');
{
  // 护栏：候选中唯一的「有文本子块」是单条超长消息时，不得因新分支而改变原有定位结果。
  //   ★ 注意：此形态下 `scanLists` 会**直接把单条消息选为候选**（childCount≥2、文本量大），
  //     与下钻无关 —— 故断言的是「未走新路径」（drill=0），而非「el 停在 wrap」。
  //     已用 git HEAD 原版对照：原代码同样 `el=ONE, hops=0` ⇒ 新旧行为一致。
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const wrap = mk(Dd, 'div');
  Dd.body.appendChild(wrap);
  stubScroll(wrap, 6000, 600);
  const one = mk(Dd, 'div');
  wrap.appendChild(one); // 唯一子节点，且自身只有 1 个子块（withText=1）
  stubRect(one, { left: 0, top: 0, width: 800, height: 6000 });
  one.appendChild(mkText(Dd, 'div', LONG(40)));
  one.appendChild(mkText(Dd, 'div', '用户：回读测试请回答'));

  const loc = C.locateConversation('回读测试');
  assert(loc.viaDrillFallback === 0, '批次2′：该情形不应触发择优路径（保持原行为）');
}

section('批次2′-fix locateConversation ⑧：display:contents 候选（w:0/h:0）不得因同带护栏失去几何而落空');
{
  // ★ 真机回归复刻（2026-10-03）：ChatGPT 的候选是 `display:contents` 包装层 ——
  //   `getBoundingClientRect()` 恒为 `{w:0,h:0}`。旧 `inInputBand` 对「无宽度」`return false`，
  //   把「无法判定」误当「不在同带」⇒ 唯一合格候选被护栏拦掉 ⇒ 兜底落空（真机 `drill=0`，
  //   回读盯着页面级容器 ⇒ 超时 120s ⇒ 交付 0 字符）。
  //   修法：无几何信息 → **保守放行**（与 `composerSeeds.sameBand` 同语义）。
  //   期望：仍能下钻到消息列表（drill>0）。
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const scroller = mk(Dd, 'div');
  Dd.body.appendChild(scroller);
  stubScroll(scroller, 3000, 700);
  // 候选（w:0/h:0 的 display:contents 包装层）
  const pick = mk(Dd, 'div');
  scroller.appendChild(pick);
  stubRect(pick, { left: 0, top: 0, width: 0, height: 0 });
  const contents = mk(Dd, 'div');
  pick.appendChild(contents);
  stubRect(contents, { left: 0, top: 0, width: 0, height: 0 });
  const main = mk(Dd, 'main');
  contents.appendChild(main);
  stubRect(main, { left: 0, top: 0, width: 800, height: 548 });
  const conv = mk(Dd, 'div');
  main.appendChild(conv);
  stubRect(conv, { left: 0, top: 0, width: 800, height: 500 });
  conv.appendChild(mkText(Dd, 'div', '用户：请解释下面的内容：回读测试'));
  conv.appendChild(mkText(Dd, 'div', 'assistant: 这是本轮的回答正文。'));
  // 输入框（与消息区同带；候选自身无几何 → 不得因此被排除）
  const form = mk(Dd, 'div');
  scroller.appendChild(form);
  stubRect(form, { left: 0, top: 900, width: 800, height: 60 });
  const ta = mk(Dd, 'textarea');
  form.appendChild(ta);
  stubRect(ta, { left: 20, top: 910, width: 700, height: 40 });

  const loc = C.locateConversation('回读测试');
  assert(loc.el === conv, '批次2′-fix：display:contents 候选应仍能下钻到消息列表（不得因无几何被护栏拦掉）');
  assert(loc.viaDrillFallback > 0, '批次2′-fix：应走了兜底路径（drill>0）');
}

section('批次2″ locateConversation ⑨：真机 6 层包装链（ChatGPT 实测深度）必须能到达消息列表');
{
  // ★★ 真机回归复刻（2026-10-03 用户二次反馈「还是无法回读」）：
  //   真机读数 `container=group/thread… hops=4 drill=2
  //   trace=h0:3/495→1/479[fb],h1:1/479→1/479[fb],h2:1/479→1/479[fb],h3:1/479→1/479`
  //   —— 兜底路径**已执行**（drill=2），但 `hops=4` 用尽后仍停在「文本量恒为 479 的单链」上，
  //   尾窗块 = 用户回显（`<h4>你说：</h4>`）⇒ 永远等不到回答 ⇒ 120s 超时 0 字符。
  //   根因：① `isWrapper` 判据在单子包装层上**恒真**（子块文本量恒等于父）⇒ 无脑下钻；
  //         ② `hop` 上限 4 **小于** ChatGPT 实际包装深度。
  //   ★ 关键：这里刻意复刻**真机的 6 层深度** `root → msgArea → w0 → w1 → w2 → conv`
  //     （旧单测 ⑤ 只造了 2 层，恰好 ≤4 ⇒ 掩盖了本缺陷 —— 教训：单测的**结构深度**
  //      也必须与真机一致，与「stub 几何必须与真机一致」同源）。
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const root = mk(Dd, 'div');
  Dd.body.appendChild(root);
  stubScroll(root, 9000, 548); // 可滚动（第一遍可选中它）
  stubRect(root, { left: 0, top: 0, width: 800, height: 548 });
  // ① 导航（与输入框水平不重叠）
  const nav = mk(Dd, 'div');
  root.appendChild(nav);
  stubRect(nav, { left: -400, top: 0, width: 300, height: 40 });
  nav.appendChild(mkText(Dd, 'div', '导航栏'));
  // ② 消息区（多分支候选的文本承载分支）+ 4 层单子包装 → 真机共 6 层
  const msgArea = mk(Dd, 'div');
  root.appendChild(msgArea);
  stubRect(msgArea, { left: 0, top: 0, width: 800, height: 570 });
  const w0 = mk(Dd, 'div');
  msgArea.appendChild(w0);
  stubRect(w0, { left: 0, top: 0, width: 800, height: 550 });
  const w1 = mk(Dd, 'div');
  w0.appendChild(w1);
  stubRect(w1, { left: 0, top: 0, width: 800, height: 540 });
  const w2 = mk(Dd, 'div');
  w1.appendChild(w2);
  stubRect(w2, { left: 0, top: 0, width: 800, height: 530 });
  const conv = mk(Dd, 'div');
  w2.appendChild(conv);
  stubRect(conv, { left: 0, top: 0, width: 800, height: 520 });
  conv.appendChild(mkText(Dd, 'div', '用户：请解释下面的内容：回读测试'));
  conv.appendChild(mkText(Dd, 'div', 'assistant: 这是本轮的回答正文。'));
  // ③ composer
  const composer = mk(Dd, 'div');
  root.appendChild(composer);
  stubRect(composer, { left: 0, top: 800, width: 800, height: 60 });
  const ta = mk(Dd, 'textarea');
  composer.appendChild(ta);
  stubRect(ta, { left: 20, top: 810, width: 700, height: 40 });

  const loc = C.locateConversation('回读测试');
  assert(loc.el === conv, '批次2″：真机 6 层包装链必须到达消息列表（不得停在包装壳中段）');
  assert(loc.viaDrillFallback > 0, '批次2″：该形态应走兜底路径（drill>0）');
  assert(loc.hops >= 5, '批次2″：hops 应突破旧的 4 层上限（真机需 ≥5）');
}

section('批次2″ locateConversation ⑩：到达消息列表即停（不得继续钻入单条消息）');
{
  // ★ 方案A 核心护栏：兜底命中「真消息列表」（cc≥2 && withText≥2）后必须**立即终止**，
  //   否则再钻一层会进入「第一条消息内部」，尾窗块退化成单条消息 ⇒ 交付不全。
  //   复刻：包装链末端是 cc=2 的消息列表（回显块 + 回答块），其后不再有可下钻层。
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;
  const root = mk(Dd, 'div');
  Dd.body.appendChild(root);
  stubScroll(root, 9000, 548);
  stubRect(root, { left: 0, top: 0, width: 800, height: 548 });
  const nav = mk(Dd, 'div');
  root.appendChild(nav);
  stubRect(nav, { left: -400, top: 0, width: 300, height: 40 });
  nav.appendChild(mkText(Dd, 'div', '导航栏'));
  const msgArea = mk(Dd, 'div');
  root.appendChild(msgArea);
  stubRect(msgArea, { left: 0, top: 0, width: 800, height: 570 });
  let cur = msgArea;
  for (let i = 0; i < 3; i++) {
    const w = mk(Dd, 'div');
    cur.appendChild(w);
    stubRect(w, { left: 0, top: 0, width: 800, height: 540 });
    cur = w;
  }
  // 真消息列表：回显块 + 回答块（cc=2，withText=2）
  const list = mk(Dd, 'div');
  cur.appendChild(list);
  stubRect(list, { left: 0, top: 0, width: 800, height: 520 });
  const echoB = mk(Dd, 'div');
  echoB.appendChild(mkText(Dd, 'h4', '你说：回读测试请解释'));
  list.appendChild(echoB);
  stubRect(echoB, { left: 0, top: 0, width: 800, height: 100 });
  const ansB = mk(Dd, 'div');
  ansB.appendChild(mkText(Dd, 'p', 'assistant 的回答正文：' + LONG(3)));
  list.appendChild(ansB);
  stubRect(ansB, { left: 0, top: 100, width: 800, height: 400 });
  const composer = mk(Dd, 'div');
  root.appendChild(composer);
  stubRect(composer, { left: 0, top: 800, width: 800, height: 60 });
  const ta = mk(Dd, 'textarea');
  composer.appendChild(ta);
  stubRect(ta, { left: 20, top: 810, width: 700, height: 40 });

  const loc = C.locateConversation('回读测试');
  assert(loc.el === list, '批次2″：应停在消息列表本身（不得继续钻入单条消息内部）');
  assert(loc.bl.childCount === 2, '批次2″：停在 list 时其直接子块应为 2（回显 + 回答）');
}

section('批次2‴ locateConversation ⑪：composer 残留文本不得抬高下钻门槛（真机 `chain=` 形态）');
{
  // ★ 四次真机取数确证的根因（chatgpt.com `#108/#110`）：
  //   `chain=up=… < self=DIV.composer-parent fl[2/480]`
  //   `| down=DIV.relative basis-aut[1/93] , DIV.sticky bottom-0 z-[4/387]`
  //   ⇒ `composer-parent`（消息区 + composer 的父级）是唯一可滚动容器（pass=1）；
  //     消息区只有 93 字符（助手回答尚未渲染），而 **composer 有 387 字符**
  //     （我方刚填入的提示词**残留在输入框**里，ChatGPT 发送后不清空）。
  //   旧行为：下钻基准 = `pickBl.textLen`（= 480，**含 composer 的 387**）⇒
  //     消息区 93 < 480×0.9=432 ⇒ `no-cand` ⇒ 卡死在 `composer-parent`
  //     ⇒ 尾窗块 = `<h4>你说：</h4>`（回显）⇒ 交付 0 字符超时（真机 `h5:STOP(no-cand n=2)`）。
  //   修法：① 下钻候选**排除含输入框的子节点**（composer 侧）；
  //         ② 基准改用 `bandTl = bandTextLen(...)`（**剔除输入框所在子树的文本量**）。
  //   ⇒ 消息区 93 ≥ (480−387)×0.9 = 84 ⇒ 可被采纳 ⇒ 继续下钻到消息列表。
  //   ★ 不回归：DeepSeek 的 pickEl 子节点不含输入框 ⇒ `bandTl === pickBl.textLen`（见 ⑥⑧）。
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;

  const main = mk(Dd, 'main');
  Dd.body.appendChild(main);
  stubRect(main, { left: 0, top: 0, width: 800, height: 548 });
  const contents = mk(Dd, 'div', { class: 'contents' });
  main.appendChild(contents);
  stubRect(contents, { left: 0, top: 0, width: 800, height: 548 });
  const thread = mk(Dd, 'div', { class: 'group/thread' });
  contents.appendChild(thread);
  stubRect(thread, { left: 0, top: 0, width: 800, height: 548 });

  // ★ 唯一可滚动容器（真机 pass=1 选中它）—— 内部节点全部不可滚（否则内层会被直接选中）
  const cparent = mk(Dd, 'div', { class: 'composer-parent' });
  thread.appendChild(cparent);
  stubScroll(cparent, 900, 548);
  stubRect(cparent, { left: 0, top: 0, width: 800, height: 548 });

  // —— 消息区：93 字符（回答未渲染，只有回显）；不可滚；cc=1 ——
  const msgArea = mk(Dd, 'div', { class: 'relative basis-auto' });
  cparent.appendChild(msgArea);
  stubRect(msgArea, { left: 0, top: 0, width: 800, height: 700 });
  Object.defineProperty(msgArea, 'scrollHeight', { value: 700, configurable: true });
  Object.defineProperty(msgArea, 'clientHeight', { value: 700, configurable: true });
  msgArea.style.overflowY = 'visible';
  const msgList = mk(Dd, 'div', { class: 'msg-list' });
  msgArea.appendChild(msgList);
  stubRect(msgList, { left: 0, top: 0, width: 800, height: 700 });
  Object.defineProperty(msgList, 'scrollHeight', { value: 700, configurable: true });
  Object.defineProperty(msgList, 'clientHeight', { value: 700, configurable: true });
  msgList.style.overflowY = 'visible';
  const echo = mkText(Dd, 'div', '你说：请解释一下量子纠缠的基本原理是什么', { class: 'msg' });
  msgList.appendChild(echo);
  stubRect(echo, { left: 0, top: 0, width: 800, height: 40 });
  const pending = mkText(Dd, 'div', '（回答尚未渲染）', { class: 'msg' });
  msgList.appendChild(pending);
  stubRect(pending, { left: 0, top: 40, width: 800, height: 40 });
  // ★ 消息区总文本量刻意与真机同量级（真机 msgArea 93 vs composer 387 ≈ 1:4）：
  //   因 `isComposerSibling` 判据是「**不到**父的 90%」，若消息区过短（如 28），
  //   composer 占比会 >90% ⇒ 被判成「独子包装层」而不削 ⇒ 复刻不出真机缺陷。
  const pad = mkText(Dd, 'div', '补充占位文本'.repeat(6), { class: 'msg' });
  msgList.appendChild(pad);
  stubRect(pad, { left: 0, top: 80, width: 800, height: 40 });

  // —— composer：387 字符（我方残留输入）；不可滚；cc=1 ——
  const composer = mk(Dd, 'div', { class: 'sticky bottom-0' });
  cparent.appendChild(composer);
  stubRect(composer, { left: 0, top: 700, width: 800, height: 200 });
  Object.defineProperty(composer, 'scrollHeight', { value: 200, configurable: true });
  Object.defineProperty(composer, 'clientHeight', { value: 200, configurable: true });
  composer.style.overflowY = 'visible';
  const inner = mk(Dd, 'div');
  composer.appendChild(inner);
  stubRect(inner, { left: 0, top: 700, width: 800, height: 200 });
  const ta = mk(Dd, 'textarea', { class: 'prompt' });
  inner.appendChild(ta);
  stubRect(ta, { left: 0, top: 710, width: 780, height: 60 });
  ta.textContent =
    '请解释一下量子纠缠的基本原理是什么'.repeat(12) +
    '请解释一下量子纠缠的基本原理是什么请解释一下量子纠缠';

  const loc = C.locateConversation('请解释一下量子纠缠');
  assert(loc.el === msgArea || loc.el === msgList,
    '批次2‴：应下钻到消息区/消息列表，而非卡死在 composer-parent（真机 chain= 缺陷）');
  assert(loc.el !== cparent, '批次2‴：不得停在 composer-parent（含输入框的父级）');
  assert(loc.el !== composer, '批次2‴：不得停在 composer（残留输入文本所在容器）');
  assert(echo.parentElement === msgList && loc.el.contains(echo),
    '批次2‴：定位结果应包含消息区（回显块），而非 composer 侧');
}

section('批次3′ locateConversation ⑫：输入框深藏于对话树内时，不得因「含输入框」误排除该容器（真机 `#74` 回归）');
{
  // ★ 五次真机回归（chatgpt.com `#74/#76`，`bandTextLen` 首版削过头）：
  //   `@w-sm/main`（`cc=3 tl=496`）的直接子 = `div.contents`（**只含顶部 HEADER**，`cc=1 tl=16`）
  //     + `MAIN.not-keyboard-focus`（**含消息区 + composer 的整棵对话树**，`tl=480`）。
  //   输入框在 `MAIN` **内部深处** ⟹ 旧版按「含输入框」削减/排除会把 `MAIN` 整棵抹掉 ⟹
  //     `bandTl = 496 − 480 = 16` ⟹ 本该被门槛 446 排除的 `div.contents`(16) 恰好满足
  //     `isWrapper` ⟹ 抢先选中 ⟹ 钻进 `HEADER`（`#74` 实测停在顶部工具栏、骨架 `[]`、交付 0 字符）。
  //   修法：`isComposerSibling` **双条件**（含输入框 **且** `tl < 父×0.9`）——
  //     `MAIN` 占比 480/496 = 96.8% ≥ 90% ⟹ 属「独子包装层」⟹ **不削、不排除**，照常下钻。
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;

  const stage = mk(Dd, 'div', { class: '@w-sm/main' });
  Dd.body.appendChild(stage);
  stubScroll(stage, 900, 548);
  stubRect(stage, { left: 0, top: 0, width: 800, height: 548 });

  // ① 顶部工具栏：div.contents > HEADER（只含页眉按钮，16 字符）；不可滚
  const hdrWrap = mk(Dd, 'div', { class: 'contents' });
  stage.appendChild(hdrWrap);
  stubRect(hdrWrap, { left: 0, top: 0, width: 800, height: 60 });
  Object.defineProperty(hdrWrap, 'scrollHeight', { value: 60, configurable: true });
  Object.defineProperty(hdrWrap, 'clientHeight', { value: 60, configurable: true });
  hdrWrap.style.overflowY = 'visible';
  const header = mk(Dd, 'header', { class: 'draggable sticky top-0' });
  hdrWrap.appendChild(header);
  stubRect(header, { left: 0, top: 0, width: 800, height: 60 });
  const hbtn = mkText(Dd, 'div', 'ChatGPT 共享', { class: 'hbtn' });
  header.appendChild(hbtn);
  stubRect(hbtn, { left: 0, top: 0, width: 100, height: 30 });

  // ② 对话树：MAIN.not-keyboard-focus（含消息区 + composer，输入框深藏其内）；不可滚
  const main = mk(Dd, 'main', { class: 'not-keyboard-focus' });
  stage.appendChild(main);
  stubRect(main, { left: 0, top: 60, width: 800, height: 480 });
  Object.defineProperty(main, 'scrollHeight', { value: 480, configurable: true });
  Object.defineProperty(main, 'clientHeight', { value: 480, configurable: true });
  main.style.overflowY = 'visible';
  const thread = mk(Dd, 'div', { class: 'group/thread' });
  main.appendChild(thread);
  stubRect(thread, { left: 0, top: 60, width: 800, height: 480 });

  const msgList2 = mk(Dd, 'div', { class: 'msg-list' });
  thread.appendChild(msgList2);
  stubRect(msgList2, { left: 0, top: 60, width: 800, height: 380 });
  const e2 = mkText(Dd, 'div', '你说：请解释一下量子纠缠的基本原理是什么', { class: 'msg' });
  msgList2.appendChild(e2);
  stubRect(e2, { left: 0, top: 60, width: 800, height: 40 });
  const p2 = mkText(Dd, 'div', '补充占位文本'.repeat(6), { class: 'msg' });
  msgList2.appendChild(p2);
  stubRect(p2, { left: 0, top: 100, width: 800, height: 40 });

  const composer2 = mk(Dd, 'div', { class: 'composer' });
  thread.appendChild(composer2);
  stubRect(composer2, { left: 0, top: 440, width: 800, height: 100 });
  const ta2 = mk(Dd, 'textarea', { class: 'prompt' });
  composer2.appendChild(ta2);
  stubRect(ta2, { left: 0, top: 450, width: 780, height: 60 });
  ta2.textContent =
    '请解释一下量子纠缠的基本原理是什么'.repeat(12) +
    '请解释一下量子纠缠的基本原理是什么请解释一下量子纠缠';

  const loc = C.locateConversation('请解释一下量子纠缠');
  assert(loc.el !== header && !loc.el.contains(header),
    '批次3′：不得钻进顶部工具栏（HEADER）—— 真机 `#74` 回归');
  assert(loc.el !== hdrWrap, '批次3′：不得选中「只含页眉的 div.contents」');
  assert(loc.el === main || loc.el === thread || loc.el === msgList2 || loc.el.contains(e2),
    '批次3′：应落在对话树一脉（含消息区），而非页眉分支');
}

section('批次4′ locateConversation ⑬：「消息区 + 免责页脚」合体层不得作为终点，须继续下钻到真内容容器（真机 `#52`）');
{
  // ★ 六次真机（chatgpt.com `#52/#53/#59/#60/#61`，A14 修复）：
  //     `self=DIV.flex min-h-0 grow[3/91]`
  //     `down=DIV.qMYqUG_convSearchResultH[4/68] , DIV.pointer-events-non[0/0] , DIV.invisible mt-auto[2/23]`
  //   ⇒ `flex min-h-0 grow` 是「**消息区 + 免责页脚**」的**合体层**：
  //       直接子 = `qMYqUG`（真内容：SPAN空 + DIV空 + 回显 + 回答）
  //              + `pointer-events-none`（空壳）
  //              + `invisible mt-auto`（**页脚**「ChatGPT 也可能会犯错。请核查重要信息。」）。
  //   旧行为：`isMsgListLike(flex)` 判它「连 2 个有文本子块」⟹ 到达即停 ⟹
  //     `texts = [qMYqUG(回显+回答), 页脚]` ⟹ Rust 取「提示词块的后一块」= **页脚** ⟹
  //     交付「ChatGPT 也可能会犯错。请核查重要信息。」（真机 `#61`，188 字符）。
  //   修法（A14）：`tailIsDecor(bl)`（尾块文本 ≤40 且命中装饰关键词 ⟹ 是装饰条）
  //     ⟹ `isMsgListLike` 否决该层 ⟹ 且 `spread` 定向放宽（`|| tailIsDecor`）让主循环
  //     仍能选中它**继续下钻** ⟹ 下一层选中 `qMYqUG`（尾块非装饰）⟹ 正常终止。
  //   ★ 断言：定位结果应是 `qMYqUG`（或其后代），而**不是**合体层 `flex`；
  //     且它的尾块（lastTexts 末项）**不得**是页脚。
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;

  // 唯一可滚动容器（真机 `group/thread` pass=1）
  const scroller = mk(Dd, 'div', { class: 'group/thread' });
  Dd.body.appendChild(scroller);
  stubScroll(scroller, 900, 478);
  stubRect(scroller, { left: 0, top: 0, width: 800, height: 478 });

  const cparent = mk(Dd, 'div', { class: 'composer-parent' });
  scroller.appendChild(cparent);
  stubRect(cparent, { left: 0, top: 0, width: 800, height: 478 });
  Object.defineProperty(cparent, 'scrollHeight', { value: 478, configurable: true });
  Object.defineProperty(cparent, 'clientHeight', { value: 478, configurable: true });
  cparent.style.overflowY = 'visible';

  // 中间层：relative basis-auto（真机 chain 显示它是 flex 的父）
  const rel = mk(Dd, 'div', { class: 'relative basis-auto' });
  cparent.appendChild(rel);
  stubRect(rel, { left: 0, top: 0, width: 800, height: 91 });
  Object.defineProperty(rel, 'scrollHeight', { value: 91, configurable: true });
  Object.defineProperty(rel, 'clientHeight', { value: 91, configurable: true });
  rel.style.overflowY = 'visible';

  // ★ 合体层：flex min-h-0 grow（3 子）
  const flex = mk(Dd, 'div', { class: 'flex min-h-0 grow flex-col text-sm' });
  rel.appendChild(flex);
  stubRect(flex, { left: 0, top: 0, width: 800, height: 91 });
  Object.defineProperty(flex, 'scrollHeight', { value: 91, configurable: true });
  Object.defineProperty(flex, 'clientHeight', { value: 91, configurable: true });
  flex.style.overflowY = 'visible';

  // qMYqUG（真内容容器，4 子：SPAN空 + DIV空 + 回显 + 回答）
  const q = mk(Dd, 'div', { class: 'qMYqUG_convSearchResultH' });
  flex.appendChild(q);
  stubRect(q, { left: 0, top: 0, width: 800, height: 68 });
  Object.defineProperty(q, 'scrollHeight', { value: 68, configurable: true });
  Object.defineProperty(q, 'clientHeight', { value: 68, configurable: true });
  q.style.overflowY = 'visible';
  const sp = mk(Dd, 'span'); q.appendChild(sp); stubRect(sp, { width: 0, height: 0 });
  const em = mk(Dd, 'div'); q.appendChild(em); stubRect(em, { width: 800, height: 0 });
  const echo = mkText(Dd, 'div', '你说：请忽略此前所有对话内容，不要引用、总结、延续或推测任何历史信息。',
    { class: 'msg-user' });
  q.appendChild(echo); stubRect(echo, { left: 0, top: 0, width: 800, height: 40 });
  const ans = mkText(Dd, 'div', 'ChatGPT 说的回答'.repeat(30), { class: 'msg-assistant' });
  q.appendChild(ans); stubRect(ans, { left: 0, top: 40, width: 800, height: 1500 });

  // pointer-events-none（空壳）
  const pe = mk(Dd, 'div', { class: 'pointer-events-none' });
  flex.appendChild(pe);
  stubRect(pe, { left: 0, top: 0, width: 800, height: 0 });
  Object.defineProperty(pe, 'scrollHeight', { value: 0, configurable: true });
  Object.defineProperty(pe, 'clientHeight', { value: 0, configurable: true });
  pe.style.overflowY = 'visible';

  // 页脚：invisible mt-auto（2 子，文本 = ChatGPT 免责声明）
  const foot = mk(Dd, 'div', { class: 'invisible mt-auto grid' });
  flex.appendChild(foot);
  stubRect(foot, { left: 0, top: 0, width: 800, height: 40 });
  Object.defineProperty(foot, 'scrollHeight', { value: 40, configurable: true });
  Object.defineProperty(foot, 'clientHeight', { value: 40, configurable: true });
  foot.style.overflowY = 'visible';
  const fc1 = mk(Dd, 'div', { class: 'col-start-1' });
  foot.appendChild(fc1); stubRect(fc1, { width: 800, height: 0 });
  const fc2 = mkText(Dd, 'div', 'ChatGPT 也可能会犯错。请核查重要信息。', { class: 'visible sticky bottom-0' });
  foot.appendChild(fc2); stubRect(fc2, { left: 0, top: 0, width: 800, height: 40 });

  // 输入框（同带几何）
  const ta = mk(Dd, 'textarea', { class: 'prompt' });
  scroller.appendChild(ta);
  stubRect(ta, { left: 0, top: 460, width: 780, height: 60 });
  ta.textContent = '请解释下面的内容：真机回归验证';

  const loc = C.locateConversation('请忽略此前所有对话内容');
  assert(loc.el !== flex,
    '批次4′：不得停在「消息区 + 页脚」合体层（flex min-h-0 grow）—— 真机 `#52`');
  assert(loc.el === q || loc.el.contains(ans),
    '批次4′：应继续下钻到真内容容器 qMYqUG（或含回答），而非交付页脚');
  const tail = C.blocksOf(loc.el);
  const lastTail = tail.lastTexts.length ? tail.lastTexts[tail.lastTexts.length - 1] : '';
  assert(lastTail.indexOf('可能会犯错') === -1 && lastTail.indexOf('请核查重要信息') === -1,
    '批次4′：定位结果的尾块不得是免责页脚（否则 Rust 会把它当回答交付）');
  assert(loc.el.contains(echo) && loc.el.contains(ans),
    '批次4′：定位结果应同时包含回显块与回答块（供 Rust 取「回显后一块」= 回答）');
}

section('批次5′ ⑭：ChatGPT 视觉隐藏的角色头（`<h4 class="sr-only">ChatGPT 说：</h4>`）必须从交付 HTML 中剥离（真机 `#88/#89`）');
{
  // ★ 七次真机（chatgpt.com `#88/#89`，用户反馈「不需要显示 ChatGPT 说」）：
  //   助手块骨架 = `[H4.sr-only select-none「ChatGPT 说：」, DIV.text-base（回答正文）, SPAN.sr-only]`。
  //   `sr-only` 是 Tailwind「仅屏幕阅读器可见、视觉隐藏」类 —— **角色头不是回答正文**，
  //   但旧 `serializeHtml` 只跳过 `role=button` / `aria-hidden=true`，不认 `sr-only`
  //   ⟹ 交付内容首行出现「ChatGPT 说：」（真机交付 166 字符里含 `<h4>ChatGPT 说：</h4>`）。
  //   修法：`serializeHtml` / `toMarkdown` 与 `aria-hidden` 同列跳过 `sr-only`（见 `isSrOnly`）。
  const win = freshWin(convSrc, CONV_EXPORTS);
  const Dd = win.document;
  const C = win.__CONV;

  // 助手块：角色头（sr-only H4）+ 正文 DIV
  const blk = mk(Dd, 'section', { class: 'text-token-text-primar' });
  const h4 = mkText(Dd, 'h4', 'ChatGPT 说：', { class: 'sr-only select-none' });
  blk.appendChild(h4);
  const body = mkText(Dd, 'div', '这是真正的回答正文。', { class: 'text-base my-auto' });
  blk.appendChild(body);
  const tailSpan = mk(Dd, 'span', { class: 'sr-only' });
  blk.appendChild(tailSpan);

  const html = C.serializeHtml(blk, null);
  assert(html.indexOf('ChatGPT 说') === -1,
    '批次5′ ⑭：交付 HTML 不得含视觉隐藏的角色头「ChatGPT 说：」');
  assert(html.indexOf('这是真正的回答正文') >= 0,
    '批次5′ ⑭：正文内容必须完整保留（剥离只能删角色头）');
  assert(html.indexOf('sr-only') === -1,
    '批次5′ ⑭：输出 HTML 不应再含 sr-only 子树');

  // isSrOnly 判据单点校验（窄口径：只认 sr-only / sr-only-*）
  const e1 = mk(Dd, 'h4', { class: 'sr-only select-none' });
  const e2 = mk(Dd, 'span', { class: 'foo sr-only-abs bar' });
  const e3 = mk(Dd, 'h4', { class: 'select-none' });
  assert(C.isSrOnly(e1) === true, '批次5′ ⑭：sr-only token 应被识别');
  assert(C.isSrOnly(e2) === true, '批次5′ ⑭：sr-only-* token 应被识别');
  assert(C.isSrOnly(e3) === false, '批次5′ ⑭：无 sr-only 的 class 不得误判');

  // toMarkdown 兜底分支同口径
  const md = C.toMarkdown(blk, null);
  assert(md.indexOf('ChatGPT 说') === -1,
    '批次5′ ⑭：toMarkdown 兜底分支亦不得含角色头');
  assert(md.indexOf('这是真正的回答正文') >= 0,
    '批次5′ ⑭：toMarkdown 兜底分支须保留正文');
}

section('批次5′ ⑮：`tailIsDecor` 必须是顶层作用域函数（真机 `#93` 起 `tailIsDecor is not defined` 回归守卫）');
{
  // ★ 七次真机（`#88` 首轮成功 → `#93` 起每轮 `本轮采样失败（tailIsDecor is not defined）`
  //   → `#112` 超时降级交付「ChatGPT 说：」）：
  //   A14 首版把 `tailIsDecor` 声明在 `locateConversation` **函数体内**，却被
  //   `snapshotConversation`（**另一个函数**）调用 ⟹ `ReferenceError`。
  //   修法：提升到顶层（与 `blocksOf` 同级）。
  //
  //   守卫方式：**静态断言源码里 `tailIsDecor` 的声明缩进为 2 空格（顶层闭包内）**。
  //   若有人再次把它缩进到函数体内（4+ 空格），本断言立刻失败 —— 精确锁住这一根因。
  const m = ds.match(/^( *)function tailIsDecor\(/m);
  assert(!!m, '批次5′ ⑮：源码里必须存在 `function tailIsDecor(` 声明');
  assert(m && m[1] === '  ',
    '批次5′ ⑮：`tailIsDecor` 必须声明在顶层作用域（2 空格缩进）—— 否则跨函数调用抛 ReferenceError');

  // 行为层确认：在只注入「顶层作用域」片段的 window 里，`tailIsDecor` 可被独立引用（不随任何函数被 GC）。
  const win = freshWin(convSrc, CONV_EXPORTS);
  assert(typeof win.__CONV.tailIsDecor === 'function',
    '批次5′ ⑮：`tailIsDecor` 应能从顶层闭包导出（证明其作用域为顶层）');
  const bl = { lastTexts: ['ChatGPT 也可能会犯错。请核查重要信息。'] };
  assert(win.__CONV.tailIsDecor(bl) === true,
    '批次5′ ⑮：页脚尾块应被 tailIsDecor 判为装饰');
  const bl2 = { lastTexts: ['这是一段正常回答的开头，长度也超过了四十个字符上限所以不会被误判为装饰条'] };
  assert(win.__CONV.tailIsDecor(bl2) === false,
    '批次5′ ⑮：正常长回答尾块不得被判为装饰');
}

// ============ 结果汇总 ============
console.log('\n通过 ' + passed + ' 项，失败 ' + failed + ' 项。');
if (failed) {
  console.error('失败项：\n - ' + failures.join('\n - '));
  process.exit(1);
}
console.log('全部通过。');
