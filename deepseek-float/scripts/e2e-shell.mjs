/**
 * 外壳端到端门禁：**脱离 dev server** 拉起真实 release exe，用 CDP 直接查页面真相。
 *
 * 为什么必须有这道门禁（R20 真机事故）：
 *   ① `cargo build --release` 若没带 `--features custom-protocol`，产出的 exe 会以
 *      **dev 模式**运行 —— 窗口去加载 `build.devUrl`（localhost:5173）而**不内嵌 dist/**。
 *      脱离 dev server 打开就是整片空白（真机表现：悬浮球 / 工具条「显示出错」）。
 *      而当时所有自动化都只查日志（不依赖前端），于是 11 条断言全绿、问题照旧出厂。
 *   ② CSP 只在构建产物里生效（dev 下页面由 vite 提供，走不到 Tauri 资产协议），
 *      所以「CSP 把页面打死」这类问题在 dev 里**永远测不出来** —— 只能在这里测。
 *
 * ⇒ 本门禁的判定口径是「**渲染出的 DOM 事实**」，不是日志、不是配置文件。
 *
 * 用法：
 *   node scripts/e2e-shell.mjs                       # 默认测 target/release
 *   node scripts/e2e-shell.mjs <exe 路径>            # 测指定产物（如 release/deepseek-float.exe）
 * 前置条件：**不要**启动 vite dev server。
 * 退出码：0 = 全绿；1 = 有断言失败。
 */
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const ROOT = path.resolve(
  path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1")),
  "..",
);
const DEFAULT_EXE = path.join(ROOT, "src-tauri", "target", "release", "deepseek-float.exe");
const EXE = process.argv[2] ? path.resolve(process.argv[2]) : DEFAULT_EXE;
const PORT = 9444;
const DEV_HOST = "localhost:5173"; // devUrl —— 出现即说明产物是 dev 模式

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let pass = 0;
let fail = 0;
const check = (name, ok, detail = "") => {
  if (ok) {
    pass += 1;
    console.log(`  PASS  ${name}${detail ? `  ${detail}` : ""}`);
  } else {
    fail += 1;
    console.log(`  FAIL  ${name}${detail ? `  ${detail}` : ""}`);
  }
};

/** 极简 CDP 会话。 */
class Cdp {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    this.events = [];
    ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.id && this.pending.has(msg.id)) {
        this.pending.get(msg.id)(msg);
        this.pending.delete(msg.id);
      } else if (msg.method) {
        this.events.push(msg);
      }
    });
  }
  send(method, params = {}) {
    const id = ++this.id;
    return new Promise((resolve) => {
      this.pending.set(id, resolve);
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }
  async evalJson(expression) {
    const r = await this.send("Runtime.evaluate", { expression, returnByValue: true });
    const v = r.result?.result?.value;
    try {
      return JSON.parse(v);
    } catch {
      return { __raw: v };
    }
  }
  close() {
    try {
      this.ws.close();
    } catch {
      /* ignore */
    }
  }
}

async function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  await new Promise((res, rej) => {
    ws.addEventListener("open", res, { once: true });
    ws.addEventListener("error", rej, { once: true });
  });
  const cdp = new Cdp(ws);
  await cdp.send("Runtime.enable");
  return cdp;
}

const listTargets = async () => (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();

/** 悬浮球页：产物模式 + 首屏渲染真相。 */
const BALL_PROBE = `JSON.stringify({
  url: location.href,
  title: document.title,
  sheetRules: [...document.styleSheets].map(s => { try { return s.cssRules.length; } catch (e) { return -1; } }),
  bodyMargin: getComputedStyle(document.body).margin,
  bodyOverflow: getComputedStyle(document.body).overflow,
  ballExists: !!document.querySelector('.ball'),
  logoBox: (() => { const el = document.querySelector('.logo'); if (!el) return null;
    const b = el.getBoundingClientRect(); return { w: Math.round(b.width), h: Math.round(b.height) }; })(),
  scroll: { sw: document.documentElement.scrollWidth, cw: document.documentElement.clientWidth }
})`;

/**
 * 5 个本地页各自的「样式真的生效了吗」判据。
 * ★ 之所以逐页都要查：CSP 打死的是**某一条资源类型**，不同页面用的东西不同
 *   （settings 的 `style="…"` 属性 / shot 的透明遮罩 / result 的卡片圆角…），
 *   只查 ball 会漏。
 */
const PAGE_CHECKS = {
  popup: {
    label: "工具条 bar 用上 flex 布局",
    expr: `JSON.stringify({
      bar: (()=>{const e=document.querySelector('.bar');if(!e)return null;const s=getComputedStyle(e);
        return {display:s.display,h:Math.round(e.getBoundingClientRect().height)}})()
    })`,
    want: (j) => j.bar && j.bar.display === "flex" && j.bar.h > 20,
  },
  settings: {
    label: "设置页内联 style 属性生效（#editor-card 被 display:none 收起）",
    expr: `JSON.stringify({
      editorDisplay: (()=>{const e=document.getElementById('editor-card');return e?getComputedStyle(e).display:null})(),
      styledCount: [...document.querySelectorAll('*')].filter(e=>{const s=getComputedStyle(e);
        return s.borderRadius!=='0px' || s.boxShadow!=='none' ||
               (s.backgroundColor!=='rgba(0, 0, 0, 0)' && s.backgroundColor!=='transparent');}).length
    })`,
    // ★ `display:none` 只有 `style="…"` 属性能给 —— 这条同时验证了 style-src-attr
    want: (j) => j.editorDisplay === "none" && j.styledCount >= 15,
  },
  shot: {
    label: "截图遮罩层样式生效（#mask 半透明黑 + #hint 提示条）",
    expr: `JSON.stringify({
      maskBg: (()=>{const e=document.getElementById('mask');return e?getComputedStyle(e).backgroundColor:null})(),
      hintBg: (()=>{const e=document.getElementById('hint');return e?getComputedStyle(e).backgroundColor:null})()
    })`,
    want: (j) => j.maskBg === "rgba(0, 0, 0, 0.35)" && j.hintBg && j.hintBg !== "rgba(0, 0, 0, 0)",
  },
  result: {
    label: "结果卡卡片圆角/底色生效",
    expr: `JSON.stringify({
      card: (()=>{const e=document.querySelector('.card');if(!e)return null;const s=getComputedStyle(e);
        return {radius:s.borderRadius,bg:s.backgroundColor}})()
    })`,
    want: (j) => j.card && j.card.radius !== "0px" && j.card.bg !== "rgba(0, 0, 0, 0)",
  },
};

if (!fs.existsSync(EXE)) {
  console.log(`FAIL 找不到产物：${EXE}\n     先跑 \`cargo build --release --features custom-protocol\``);
  process.exit(1);
}

console.log(`=== e2e-shell：${EXE} (${fs.statSync(EXE).size} B) ===`);
console.log("前置条件：**不要**启动 vite dev server —— 本门禁专门验证产物能否独立运行。\n");

// ★ 启动前清残留：本应用有单实例互斥，残留进程会让新进程直接退出、调试端口永不出现
spawnSync("taskkill", ["/IM", "deepseek-float.exe", "/F", "/T"], { stdio: "ignore" });
await sleep(1500);

const proc = spawn(EXE, [], {
  env: {
    ...process.env,
    DSFLOAT_DIAG: "1", // 顺带留现场；断言不依赖它
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
  },
  stdio: "ignore",
});

let targets = [];
for (let i = 0; i < 40; i += 1) {
  await sleep(500);
  try {
    targets = await listTargets();
    if (targets.some((t) => (t.url || "").includes("ball.html"))) break;
  } catch {
    /* 端口未就绪 */
  }
}
// 悬浮球最早出现；面板预热与其余功能窗的 webview 更晚，再等一轮并刷新列表
await sleep(5000);
try {
  targets = await listTargets();
} catch {
  /* 保持旧列表 */
}

const ball = targets.find((t) => (t.url || "").includes("ball.html"));
check("A1 找到悬浮球页面 target", !!ball, ball ? ball.url : `targets=${targets.length}`);

if (ball) {
  // ★ A2：产物模式判定 —— 本次事故的核心判据
  check("A2 产物为生产模式（未指向 devUrl）", !ball.url.includes(DEV_HOST), `url=${ball.url}`);

  const cdp = await connect(ball.webSocketDebuggerUrl);
  await cdp.send("Log.enable");
  await sleep(2000);

  const s = await cdp.evalJson(BALL_PROBE);
  check("A3 悬浮球 DOM 已渲染", s.ballExists === true, `title=${JSON.stringify(s.title)}`);
  check(
    "A4 样式表已生效（非空规则）",
    (s.sheetRules || []).some((n) => n > 0),
    `rules=${JSON.stringify(s.sheetRules)}`,
  );
  check("A5 body 已去掉默认外边距", s.bodyMargin === "0px", `margin=${s.bodyMargin}`);
  check(
    "A6 无内容溢出（退化成默认样式的征兆）",
    s.scroll && s.scroll.sw === s.scroll.cw,
    `scrollWidth=${s.scroll?.sw} clientWidth=${s.scroll?.cw}`,
  );
  check(
    "A7 logo 为 CSS 指定尺寸 40×40",
    s.logoBox && Math.abs(s.logoBox.w - 40) <= 2 && Math.abs(s.logoBox.h - 40) <= 2,
    `logo=${JSON.stringify(s.logoBox)}`,
  );

  // A8：重载一次抓 CSP 违规（首屏 violation 在 attach 之前就发生了）
  cdp.events.length = 0;
  await cdp.send("Page.enable");
  await cdp.send("Page.reload", { ignoreCache: true });
  await sleep(4000);
  const logs = cdp.events
    .filter((e) => e.method === "Log.entryAdded" && e.params?.entry)
    .map((e) => e.params.entry);
  const violations = logs.filter((l) => /Content Security Policy|Refused to/i.test(l.text || ""));
  check(
    "A8 重载后无 CSP 违规",
    violations.length === 0,
    violations.length ? violations.map((v) => v.text).join(" | ") : `console 记录=${logs.length}`,
  );
  cdp.close();
}

// ---- B 组：其余 4 个本地页的样式生效性（CSP 打死某条资源类型时会在这里暴露）----
for (const [name, spec] of Object.entries(PAGE_CHECKS)) {
  const tgt = targets.find((t) => (t.url || "").includes(`poc/${name}.html`));
  if (!tgt) {
    check(`B ${name}：${spec.label}`, false, "找不到该页面 target");
    continue;
  }
  const cdp = await connect(tgt.webSocketDebuggerUrl);
  await sleep(800);
  const j = await cdp.evalJson(spec.expr);
  check(`B ${name}：${spec.label}`, spec.want(j), JSON.stringify(j));
  cdp.close();
}

proc.kill();
await sleep(600);
spawnSync("taskkill", ["/IM", "deepseek-float.exe", "/F", "/T"], { stdio: "ignore" });

console.log(`\n=== ${pass} passed / ${fail} failed ===`);
process.exit(fail === 0 ? 0 : 1);
