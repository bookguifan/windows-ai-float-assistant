#!/usr/bin/env node
// 净化白名单一致性检查（M1）。
//
// 两端各维护一份净化白名单（dsfloat_init.js::SAFE_TAGS 与 result.html::SAFE_HTML_TAGS），
// 本脚本把它们从源码中抽取出来，与权威清单 scripts/sanitize-whitelist.json 做深比较，并校验
// 类名正则的「页面侧双正则」与「卡片侧合并正则」语义等价。任一 drift → 进程退出码 1。
//
// 运行：node scripts/check-sanitize-whitelist.mjs
// 纯 Node，无外部依赖（不依赖 jsdom）。

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const JSON_PATH = resolve(here, 'sanitize-whitelist.json');
const DS_PATH = resolve(root, 'src-tauri/src/dsfloat_init.js');
const RES_PATH = resolve(root, 'poc/result.html');

let failures = 0;
function fail(msg) {
  failures++;
  console.error('  ✗ ' + msg);
}
function ok(msg) {
  console.log('  ✓ ' + msg);
}

function read(p) {
  try {
    return readFileSync(p, 'utf8');
  } catch (e) {
    fail(`无法读取文件：${p}（${e.message}）`);
    return '';
  }
}

// 从 `const NAME = { K:1, K2:1, ... };` 抽取键名（大写标识符）。
function extractObjKeys(src, name) {
  const start = src.indexOf(`const ${name} = {`);
  if (start < 0) return null;
  const open = src.indexOf('{', start);
  const close = src.indexOf('}', open);
  if (close < 0) return null;
  const body = src.slice(open + 1, close);
  const keys = [];
  const re = /([A-Za-z0-9_]+)\s*:\s*\d+/g;
  let m;
  while ((m = re.exec(body)) !== null) keys.push(m[1]);
  return keys.sort();
}

// 抽取字面量正则：`const NAME = /.../`。
function extractRegex(src, name) {
  const re = new RegExp(`${name}\\s*=\\s*\\/([^/]*)\\/`);
  const m = src.match(re);
  return m ? m[1] : null;
}

// 抽取卡片侧 safeClass 内的合并正则字面量（`/^(hljs(...)|language-...)$/`）。
// 该字面量在源码中出现于注释与函数体，字符串完全一致，直接正则捕获即可。
function extractCardClassRegex(src) {
  const m = src.match(/\/\^\(\s*hljs\([^)]*\)\?\|language-[^)]*\)\$\//);
  return m ? m[0].slice(1, -1) : null;
}

console.log('净化白名单一致性检查（M1）');

const json = JSON.parse(read(JSON_PATH));
const ds = read(DS_PATH);
const res = read(RES_PATH);

if (!ds || !res) {
  console.error('源文件缺失，跳过。');
  process.exit(failures ? 1 : 0);
}

// 1) 标签集三方一致
const authoritative = [...json.tags].sort();
const dsKeys = extractObjKeys(ds, 'SAFE_TAGS');
const resKeys = extractObjKeys(res, 'SAFE_HTML_TAGS');

if (!dsKeys) fail('未在 dsfloat_init.js 找到 SAFE_TAGS');
else if (JSON.stringify(dsKeys) !== JSON.stringify(authoritative))
  fail(`dsfloat_init.js::SAFE_TAGS 与权威清单不符：\n    权威 ${authoritative.join(',')}\n    源码 ${dsKeys.join(',')}`);
else ok('dsfloat_init.js::SAFE_TAGS 与权威清单一致');

if (!resKeys) fail('未在 result.html 找到 SAFE_HTML_TAGS');
else if (JSON.stringify(resKeys) !== JSON.stringify(authoritative))
  fail(`result.html::SAFE_HTML_TAGS 与权威清单不符：\n    权威 ${authoritative.join(',')}\n    源码 ${resKeys.join(',')}`);
else ok('result.html::SAFE_HTML_TAGS 与权威清单一致');

if (dsKeys && resKeys && JSON.stringify(dsKeys) !== JSON.stringify(resKeys))
  fail(`两端标签集不一致：页面 ${dsKeys.join(',')} vs 卡片 ${resKeys.join(',')}`);
else if (dsKeys && resKeys) ok('页面侧与卡片侧标签集一致');

// 2) 类名正则：页面双正则 vs 卡片合并正则 语义等价（token 级）
const hljs = extractRegex(ds, 'HLJS_CLASS_RE');
const lang = extractRegex(ds, 'LANG_CLASS_RE');
const cardLit = extractCardClassRegex(res);

if (!hljs) fail('未在 dsfloat_init.js 找到 HLJS_CLASS_RE');
if (!lang) fail('未在 dsfloat_init.js 找到 LANG_CLASS_RE');
if (!cardLit) fail('未在 result.html::safeClass 找到合并类名正则');

if (hljs && lang && cardLit) {
  const reHljs = new RegExp(hljs);
  const reLang = new RegExp(lang);
  const reCard = new RegExp(cardLit);
  const combined = new RegExp(json.classRegexCombined);

  // 卡片字面量应与权威合并正则完全一致（字符串级，catch drift）。
  if (cardLit !== json.classRegexCombined)
    fail(`result.html 类名正则与权威清单不符：\n    权威 ${json.classRegexCombined}\n    源码 ${cardLit}`);
  else ok('result.html 类名正则与权威清单一致');

  const tokens = ['hljs', 'hljs-keyword', 'hljs-built_in', 'hljs-number', 'language-python',
    'language-c++', 'language-go', 'onclick', 'foo', 'javascript', 'data', 'bad', 'x-y'];
  let allEq = true;
  for (const t of tokens) {
    const pageMatch = reHljs.test(t) || reLang.test(t);
    const cardMatch = reCard.test(t);
    const combMatch = combined.test(t);
    if (pageMatch !== cardMatch || cardMatch !== combMatch) {
      allEq = false;
      fail(`token "${t}" 语义不一致：页面=${pageMatch} 卡片=${cardMatch} 合并=${combMatch}`);
    }
  }
  if (allEq) ok('页面双正则 / 卡片合并正则 / 权威合并正则 在 token 级语义等价');
}

// 3) URL scheme 提示（仅信息，不阻断）
console.log(`  · href 放行：${json.urlSchemes.href.join(' ')}；imgSrc 放行：${json.urlSchemes.imgSrc.join(' ')}`);

console.log(failures ? `\n失败 ${failures} 项。` : '\n全部一致。');
process.exit(failures ? 1 : 0);
