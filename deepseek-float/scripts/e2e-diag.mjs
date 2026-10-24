// R17-P1 端到端验证：release 绿色版 exe 的诊断落盘开关（真机进程，非代码层推测）。
//
// 判据链：
//   A) 不设 DSFLOAT_DIAG      → d1-debug.log 零新增（也不新建）
//   B) DSFLOAT_DIAG=off       → 同上（显式关闭）
//   C) DSFLOAT_DIAG=1         → 有新增，且启动行报「档位=Meta」
//   D) DSFLOAT_DIAG=full      → 有新增，且启动行报「档位=Full」
//   E) DSFLOAT_DIAG=ture(拼错) → 有新增，且落到 Meta（宁开不错关）
//   F) 存在 diag.flag 标记文件 → 有新增，且落到 Meta（绿色版双击用户的入口）
//   G) 移除 diag.flag          → 回到零落盘（开关可逆）
//
// 说明：**正文脱敏**这一层由 `cargo test --release --lib diag` 覆盖 ——
//   release 编译下 `debug_assertions=false`，单测直接调用 `render_content(s, Level::Meta)`
//   与 `parse_level(raw, false)`，与线上同一份实现。本脚本只验「进程级开关是否真的生效」。
//
// 用法：node scripts/e2e-diag.mjs

import { spawnSync, spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// 相对脚本自身定位工作区根（scripts/ 的上级），避免硬编码绝对路径导致换机/换目录即失效。
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const EXE = path.join(ROOT, "src-tauri", "target", "release", "deepseek-float.exe");
const TASKKILL = "C:/Windows/System32/taskkill.exe";
const IMAGE = "deepseek-float.exe";
const LOG = path.join(process.env.LOCALAPPDATA, "com.deepseekfloat.desktop", "d1-debug.log");

const WAIT_MS = 7000; // 覆盖 WebView2 冷启 + setup 落首行

const results = [];
function check(name, ok, detail) {
  results.push({ name, ok, detail });
  console.log(`${ok ? "OK  " : "FAIL"} ${name}${detail ? " :: " + detail : ""}`);
}

function killAll() {
  const r = spawnSync(TASKKILL, ["/F", "/IM", IMAGE], { encoding: "utf8" });
  return (r.stdout || "") + (r.stderr || "");
}

function stat() {
  try {
    const s = fs.statSync(LOG);
    return { exists: true, size: s.size, mtimeMs: s.mtimeMs };
  } catch {
    return { exists: false, size: 0, mtimeMs: 0 };
  }
}

function readRange(from, to) {
  if (!fs.existsSync(LOG)) return "";
  const st = fs.statSync(LOG);
  const end = Math.min(to ?? st.size, st.size);
  if (end <= from) return "";
  const fd = fs.openSync(LOG, "r");
  const buf = Buffer.alloc(end - from);
  fs.readSync(fd, buf, 0, buf.length, from);
  fs.closeSync(fd);
  return buf.toString("utf8");
}

const busyWait = (ms) => {
  const t = Date.now();
  while (Date.now() - t < ms) {
    /* 忙等：Bash shim 半残、sleep 不可用，同步等待保证时序确定 */
  }
};

/** 启动 exe → 驻留 → 强杀 → 返回「本次新增区段」与增量字节数。 */
function runOnce(envValue) {
  killAll();
  busyWait(700);
  const before = stat();
  const env = { ...process.env };
  if (envValue === undefined) delete env.DSFLOAT_DIAG;
  else env.DSFLOAT_DIAG = envValue;

  const child = spawn(EXE, [], { env, detached: true, stdio: "ignore" });
  child.unref();
  busyWait(WAIT_MS);
  killAll();
  busyWait(500);
  const after = stat();
  const seg = readRange(before.size, after.size);
  return { before, after, delta: after.size - before.size, seg };
}

console.log("=== R17-P1 release 诊断落盘端到端验证 ===");
console.log("exe  :", EXE, "exists=", fs.existsSync(EXE), "size=", fs.existsSync(EXE) ? fs.statSync(EXE).size : "-");
console.log("log  :", LOG);
console.log("初始 :", JSON.stringify(stat()));
console.log("");

// A) 默认（Off）
{
  const r = runOnce(undefined);
  check("A 不设变量 → 零落盘", r.delta === 0, `delta=${r.delta}B exists=${r.after.exists}`);
}

// B) 显式 off
{
  const r = runOnce("off");
  check("B DSFLOAT_DIAG=off → 零落盘", r.delta === 0, `delta=${r.delta}B`);
}

// C) Meta
{
  const r = runOnce("1");
  check("C1 DSFLOAT_DIAG=1 → 有落盘", r.delta > 0, `delta=${r.delta}B`);
  check("C2 启动行报「档位=Meta」", r.seg.includes("档位=Meta"), firstDiagLine(r.seg));
}

// D) Full
{
  const r = runOnce("full");
  check("D1 DSFLOAT_DIAG=full → 有落盘", r.delta > 0, `delta=${r.delta}B`);
  check("D2 启动行报「档位=Full」", r.seg.includes("档位=Full"), firstDiagLine(r.seg));
}

// E) 拼错 → 保守 Meta
{
  const r = runOnce("ture");
  check("E1 拼错值 → 仍落盘（不静默关闭）", r.delta > 0, `delta=${r.delta}B`);
  check("E2 拼错值 → 落到 Meta", r.seg.includes("档位=Meta"), firstDiagLine(r.seg));
}

// F) 标记文件 diag.flag（绿色版双击用户的唯一入口）
{
  const flag = path.join(path.dirname(LOG), "diag.flag");
  fs.mkdirSync(path.dirname(LOG), { recursive: true });
  fs.writeFileSync(flag, "");
  const r = runOnce(undefined);
  check("F1 diag.flag 存在 → 有落盘", r.delta > 0, `delta=${r.delta}B`);
  check("F2 diag.flag → 落到 Meta", r.seg.includes("档位=Meta"), firstDiagLine(r.seg));

  // G) 删掉标记文件 → 回到零落盘（开关可逆）
  fs.rmSync(flag, { force: true });
  const g = runOnce(undefined);
  check("G 移除 diag.flag → 回到零落盘", g.delta === 0, `delta=${g.delta}B`);
}

function firstDiagLine(seg) {
  const m = seg.split(/\r?\n/).find((l) => l.includes("[diag] 档位"));
  return m ? m.trim().slice(0, 120) : "(未找到 [diag] 档位 行)";
}

console.log("");
const failed = results.filter((r) => !r.ok);
console.log(`=== 结果：${results.length - failed.length}/${results.length} 通过 ===`);
for (const f of failed) console.log("  ✗", f.name, "::", f.detail);
process.exit(failed.length ? 1 : 0);
