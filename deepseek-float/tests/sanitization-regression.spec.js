// sanitization-regression.spec.js
// ---------------------------------------------------------------------------
// deepseek-float 净化链回归测试（审计第九节 9.2 第 1 项）
//
// 目标：验证「不可信回答文本 → 结果卡渲染」链路对 XSS / 危险 scheme 的净化能力。
// 这是真实安全边界的端到端回归：任何能被结果卡渲染成可执行 handler 或
// 危险链接的 payload，都视为净化失败。
//
// 运行方式（二选一）：
//   A) 针对本地外壳页（无需编译 Rust）：
//        cd deepseek-float && npx vite preview   # 或任意静态服务器把 poc/ 暴露出来
//        # 然后把下方 BASE_URL 指向该地址，例如 http://localhost:4173/poc/result.html
//        npx playwright test tests/sanitization-regression.spec.js
//
//   B) 针对运行中的 App（真实 Tauri 环境）：
//        npm run tauri dev
//        # 把 BASE_URL 指向 App 暴露的结果卡（或保留本文件默认，走 A）
//        npx playwright test tests/sanitization-regression.spec.js
//
// 依赖：npm i -D @playwright/test  （首次需 npx playwright install chromium）
// ---------------------------------------------------------------------------

import { test, expect } from '@playwright/test';

const BASE_URL = process.env.DSFLOAT_RESULT_URL
  || 'http://localhost:4173/poc/result.html'; // 默认：vite preview 下的本地外壳页（vite 产物在 /poc/）

// 恶意 payload 矩阵（来自审计报告净化回归用例）。
// 每个用例把内容塞进「回答文本」这一不可信字段，断言渲染后：
//   - 不出现任何 on* 事件处理器
//   - 不出现 <script> / <iframe> / <object> / <embed>
//   - 链接 href/src 不含 javascript: / 非图片 data:
//   - 危险内容被降级为纯文本或丢弃（而非执行）
const PAYLOADS = [
  { name: 'script-tag',        html: '<script>window.__pwned=1</script>hello' },
  { name: 'img-onerror',       html: '<img src=x onerror="window.__pwned=1">' },
  { name: 'anchor-onmouseover',html: '<a href="https://ok.example">x</a><a href="https://x.example" onmouseover="window.__pwned=1">y</a>' },
  { name: 'js-href',           html: '<a href="javascript:window.__pwned=1">click</a>' },
  { name: 'md-js-link',        html: '[click me](javascript:window.__pwned=1)' },
  { name: 'data-img',          html: '<img src="data:text/html,<script>window.__pwned=1</script>">' },
  { name: 'svg-onload',        html: '<svg/onload="window.__pwned=1">' },
  { name: 'quote-attr-break',  html: '<a href="https://x.example" onmouseover="window.__pwned=1">z</a>' },
  { name: 'plain-text',        html: '正常回答，无标记' },
];

// 在页面内把 payload 喂给结果卡，并取回渲染后的安全 DOM 快照。
// 优先走真实 IPC 桥 window.__dsfResultSet；若不可用则回退到单元函数。
async function renderInCard(page, html) {
  return await page.evaluate((raw) => {
    const out = { used: null, html: null, handlers: [], badHrefs: [], badSrcs: [] };

    // 路径 1：真实集成边界（Rust 经 eval 调用 window.__dsfResultSet）
    //   ★ 真实契约（poc/result.html:1419 → renderFull:1310 → buildTurn:1219）：
    //     __dsfResultSet({ turns: [{ text: <净化后 HTML> }] })；
    //     正文落进 `#turns .turn .md`（buildTurn 内 renderSafeHtml(md, turn.text)）。
    if (typeof window.__dsfResultSet === 'function') {
      out.used = '__dsfResultSet';
      try {
        window.__dsfResultSet({ turns: [{ text: raw }], reset: true });
      } catch (e) { /* 渲染异常也应被捕获，不应导致可执行内容残留 */ }
      const ans = document.querySelector('#turns .turn .md');
      if (ans) out.html = ans.innerHTML;
    }
    // 路径 2：单元回退（result.html 内联脚本顶层声明的 renderSafeHtml/cloneSafe）
    // 签名是 renderSafeHtml(root, html) —— 必须传入一个宿主节点作挂载点。
    if (out.html == null && typeof window.renderSafeHtml === 'function') {
      out.used = 'renderSafeHtml';
      const host = document.createElement('div');
      try {
        window.renderSafeHtml(host, raw);
      } catch (e) { /* 渲染异常不应残留可执行内容 */ }
      out.html = host.innerHTML;
    }

    if (out.html == null) return out; // 桥缺失：交由测试外层跳过/告警

    // 统一的安全断言提取
    const tmp = document.createElement('div');
    tmp.innerHTML = out.html;
    tmp.querySelectorAll('*').forEach((el) => {
      for (const a of Array.from(el.attributes)) {
        if (/^on/i.test(a.name)) out.handlers.push(a.name + '=' + a.value);
      }
      const tag = el.tagName.toLowerCase();
      if (tag === 'script' || tag === 'iframe' || tag === 'object' || tag === 'embed') {
        out.handlers.push('forbidden-tag:' + tag);
      }
      const href = el.getAttribute && el.getAttribute('href');
      const src = el.getAttribute && el.getAttribute('src');
      if (href && /^\s*javascript:/i.test(href)) out.badHrefs.push(href);
      if (src && /^\s*data:/i.test(src) && !/^data:image\//i.test(src)) out.badSrcs.push(src);
    });
    return out;
  }, html);
}

const hasBridge = async (page) => {
  return await page.evaluate(() =>
    typeof window.__dsfResultSet === 'function' ||
    typeof window.renderSafeHtml === 'function');
};

test.describe('结果卡净化链回归', () => {
  test.beforeEach(async ({ page }) => {
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
    // 给内联脚本一点初始化时间
    await page.waitForTimeout(300);
    const ok = await hasBridge(page);
    if (!ok) {
      // 桥未暴露：记录但不在每个用例里重复失败，统一跳过并提示。
      test.skip(true, '结果卡渲染桥（__dsfResultSet / renderSafeHtml）未在当前页面暴露，请按文件头说明运行 vite preview 或 tauri dev');
    }
  });

  for (const p of PAYLOADS) {
    test(`净化：${p.name}`, async ({ page }) => {
      const r = await renderInCard(page, p.html);
      // 若桥缺失（理论上 beforeEach 已 skip），防护性断言
      if (r.html == null) {
        test.skip(true, '渲染桥未暴露');
        return;
      }
      expect(r.handlers, `出现事件处理器/禁用的标签: ${JSON.stringify(r.handlers)}`)
        .toEqual([]);
      expect(r.badHrefs, `出现 javascript: 链接: ${JSON.stringify(r.badHrefs)}`)
        .toEqual([]);
      expect(r.badSrcs, `出现非图片 data: 资源: ${JSON.stringify(r.badSrcs)}`)
        .toEqual([]);
    });
  }

  test('降级：危险链接不应保留可点击的 javascript: 语义', async ({ page }) => {
    const r = await renderInCard(page, '<a href="javascript:window.__pwned=1">x</a>');
    if (r.html == null) { test.skip(true, '渲染桥未暴露'); return; }
    // 即便文本 "x" 被保留，也不应存在可触发的 javascript: 链接
    expect(r.badHrefs).toEqual([]);
    expect(r.handlers).toEqual([]);
  });
});
