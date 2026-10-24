// 版本号一致性校验：三处版本源必须相同，否则发版时会出现
// 「安装包叫 0.1.0、exe 属性是 0.2.0、关于页又是别的」这类不可追溯的问题。
//
// 校验的四处（`Cargo.lock` 可选）：
//   1. package.json               → version      （前端 / npm）
//   2. src-tauri/tauri.conf.json  → version      （Tauri 打包产物名、安装包名）
//   3. src-tauri/Cargo.toml       → [package] version  （exe 版本资源）
//   4. src-tauri/Cargo.lock       → [[package]] name = "deepseek-float" 的 version（由 cargo 自动写）
//
// 用法：node scripts/check-version.mjs    （在 deepseek-float/ 下；退出码 0 = 一致）
//
// 注意：只**校验**不做同步。改版本号时三处一起改；将来若要「一键改版本」，
// 再把这里扩成 --write 模式（保持默认只读，避免手滑改坏分发物命名）。

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

// ★ 用 fileURLToPath 而非手拆 pathname：后者在 Windows 上会得到 `/D:/…`
//   且对 %20（路径含空格）不解码。脚本自身位置 → 仓库根（deepseek-float/）。
const root = path.resolve(fileURLToPath(new URL("..", import.meta.url)));

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, "utf8"));
}

/** 从 Cargo.toml 的 `[package]` 段取 version（不引入 TOML 依赖，够用即可）。 */
function cargoTomlVersion(p) {
  const lines = fs.readFileSync(p, "utf8").split(/\r?\n/);
  let inPackage = false;
  for (const raw of lines) {
    const line = raw.trim();
    if (line.startsWith("[")) {
      inPackage = line === "[package]";
      continue;
    }
    if (!inPackage) continue;
    const m = /^version\s*=\s*"([^"]+)"/.exec(line);
    if (m) return m[1];
  }
  return null;
}

/** 从 Cargo.lock 取本包 version（可能不存在：首次构建前 Cargo.lock 里还没有本包）。 */
function cargoLockVersion(p, name) {
  if (!fs.existsSync(p)) return { value: null, note: "文件不存在" };
  const text = fs.readFileSync(p, "utf8");
  // 逐个 [[package]] 块找 name 匹配的那块。
  for (const block of text.split(/\[\[package\]\]/).slice(1)) {
    const nm = /^\s*name\s*=\s*"([^"]+)"/m.exec(block);
    const vr = /^\s*version\s*=\s*"([^"]+)"/m.exec(block);
    if (nm && nm[1] === name) return { value: vr ? vr[1] : null, note: "" };
  }
  return { value: null, note: "Cargo.lock 内无本包（首次 cargo 构建前属正常）" };
}

const sources = [
  { label: "package.json", value: readJson(path.join(root, "package.json")).version },
  { label: "src-tauri/tauri.conf.json", value: readJson(path.join(root, "src-tauri/tauri.conf.json")).version },
  { label: "src-tauri/Cargo.toml [package]", value: cargoTomlVersion(path.join(root, "src-tauri/Cargo.toml")) },
];

const lock = cargoLockVersion(path.join(root, "src-tauri/Cargo.lock"), "deepseek-float");
if (lock.value) sources.push({ label: "src-tauri/Cargo.lock [[package]]", value: lock.value });

console.log("=== 版本号一致性校验 ===");
for (const s of sources) console.log(`  ${s.label.padEnd(36)} ${s.value ?? "(未取到)"}`);
if (lock.value === null && lock.note) console.log(`  提示：${lock.note}`);

const uniq = new Set(sources.map((s) => s.value));
if (uniq.has(null) || uniq.size !== 1) {
  console.error("");
  console.error("✗ 版本号不一致（或有一处缺失）。发版前必须三处（含 Cargo.toml）完全相同。");
  console.error("  改法：同时修改 package.json / src-tauri/tauri.conf.json / src-tauri/Cargo.toml，");
  console.error("        再跑一次 cargo check（会自动更新 Cargo.lock）。");
  process.exit(1);
}

console.log("");
console.log(`✓ 全部一致：${[...uniq][0]}`);
process.exit(0);
