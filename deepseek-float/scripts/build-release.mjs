// Single Windows x64 portable build entry: toolchain -> frontend/checks ->
// ordinary custom-protocol Release -> policy -> signing -> licenses/ZIP/SHA256.
// Full unit tests, formatting and Clippy run once in CI.
// Usage: pnpm release
// Optional: --no-frontend (checks freshness), --no-deliver, --sign,
//           --check-toolchain-only. Mandatory checks cannot be skipped.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { assembleRelease } from "./lib/release-package.mjs";
import {
  assertNodeEngineVersion,
  assertPnpmRuntime,
  nodeBoundEnvironment,
  pinnedPnpmVersion,
} from "./lib/release-toolchain.mjs";

// ★ 仓库根（deepseek-float/）：脚本自身在 scripts/ 下，向上一层。
const root = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const RUSTFLAG_SEPARATOR = String.fromCharCode(31);

/** Preserve explicit Rust flags while removing machine-specific source roots. */
function releaseRustflags() {
  let flags;
  if (process.env.CARGO_ENCODED_RUSTFLAGS !== undefined) {
    flags = process.env.CARGO_ENCODED_RUSTFLAGS
      .split(RUSTFLAG_SEPARATOR)
      .filter(Boolean);
  } else {
    const configured = process.env.RUSTFLAGS || process.env.CARGO_BUILD_RUSTFLAGS || "";
    flags = configured.trim().split(/\s+/).filter(Boolean);
  }

  const cargoHome = path.resolve(
    process.env.CARGO_HOME || path.join(process.env.HOME || os.homedir(), ".cargo"),
  );
  for (const [prefix, replacement] of [
    [root, "/src"],
    [cargoHome, "/cargo"],
  ]) {
    for (const variant of new Set([prefix, prefix.replaceAll("\\", "/")])) {
      flags.push(`--remap-path-prefix=${variant}=${replacement}`);
    }
  }
  return flags.join(RUSTFLAG_SEPARATOR);
}

// Windows 上 pnpm 是命令垫片，node 的 execFile 无法直接 exec ⇒ 经 shell 解析。
// 其它平台保持直接 exec（shell 反而可能改变参数解析行为）。
const WIN = process.platform === "win32";
const packageJson = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
assertNodeEngineVersion(process.version, packageJson.engines?.node || "");
const pm = "pnpm";
const expectedPnpmVersion = pinnedPnpmVersion(packageJson.packageManager || "");
const releaseEnvironment = nodeBoundEnvironment(process.env, process.execPath, process.platform);

function captureOutput(cmd, args) {
  return execFileSync(cmd, args, {
    encoding: "utf8",
    cwd: root,
    shell: WIN,
    env: releaseEnvironment,
  }).trim();
}

const actualPnpmVersion = captureOutput(pm, ["--version"]);
const pnpmNodeExecutable = captureOutput(pm, ["exec", "node", "-p", "process.execPath"]);
const pnpmNodeVersion = captureOutput(pm, ["exec", "node", "-p", "process.version"]);
assertPnpmRuntime({
  actualPnpmVersion,
  expectedPnpmVersion,
  actualNodeExecutable: pnpmNodeExecutable,
  expectedNodeExecutable: process.execPath,
  actualNodeVersion: pnpmNodeVersion,
  expectedNodeVersion: process.version,
  platform: process.platform,
});
console.log(`✓ Release toolchain: Node ${process.version}; pnpm ${actualPnpmVersion}; pnpm Node ${pnpmNodeVersion}`);

if (process.argv.includes("--check-toolchain-only")) {
  console.log("✓ Release toolchain check complete; build steps were not run.");
  process.exit(0);
}

const noFrontend = process.argv.includes("--no-frontend");
if (process.argv.includes("--no-check")) {
  throw new Error("Release checks cannot be skipped; use pnpm build for frontend development.");
}
const doSign = process.argv.includes("--sign");
const noDeliver = process.argv.includes("--no-deliver");

/**
 * 运行一条命令，失败即中止（stdio 透传，便于看到真实输出）。
 *
 * @param {string} cmd 可执行名（Windows 上 pnpm 等命令 shim 由 `shell` 解析）。
 * @param {string[]} args 参数数组。
 * @param {Record<string, string>} [extraEnv] 追加/覆盖的环境变量（仅作用于本次调用；默认 `{}`）。
 */
function run(cmd, args, extraEnv = {}) {
  console.log(`\n$ ${cmd} ${args.join(" ")}`);
  execFileSync(cmd, args, {
    stdio: "inherit",
    cwd: root,
    shell: WIN,
    env: { ...releaseEnvironment, ...extraEnv },
  });
}

// Validate the lockfile against the exact package-manager version before building.
run(pm, ["install", "--frozen-lockfile"]);

// ① 前端构建
if (!noFrontend) {
  run(pm, ["build"]); // vite build → dist/poc/*
} else {
  console.log("\n（跳过前端 build：--no-frontend）");
}

// Fail before Rust compilation if frontend/version/security configuration is invalid.
run(pm, ["run", "check"]);
if (process.env.TAURI_CONFIG) {
  throw new Error("Ordinary Release requires the tracked Tauri config; unset TAURI_CONFIG.");
}
const releaseFeatures = captureOutput("cargo", ["tree", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--depth", "0", "--prefix", "none", "--format", "{f}", "--features", "custom-protocol"]);
if (releaseFeatures !== "custom-protocol") throw new Error(`Unexpected Release features: ${releaseFeatures}`);
const releaseBuildEnvironment = {
  CARGO_INCREMENTAL: "0",
  CARGO_ENCODED_RUSTFLAGS: releaseRustflags(),
};

// ⑤ Verify the ordinary Release gate with its actual feature set. Tests never
// operate the system clipboard; runtime consent and native identity remain required.
run("cargo", [
  "test",
  "--locked",
  "--manifest-path",
  "src-tauri/Cargo.toml",
  "--release",
  "--features",
  "custom-protocol",
  "--test",
  "release_selection_policy",
], releaseBuildEnvironment);

// ② Rust release 构建。★ 必开 custom-protocol，否则 exe 白屏；build.rs 也会在漏开时直接 panic。
//
//   ★ CARGO_INCREMENTAL=0：**强制关闭增量编译**。原因：用户全局 `~/.cargo/config.toml` 的
//     `[build] incremental = true`（本意加速 dev）会**泄漏到 release**，且**压过** manifest 的
//     `[profile.release] incremental = false`（实测 cargo 1.98.1：`[build]` 优先于 `[profile.*]`）。
//     故用**环境变量**在本次调用强制关掉 —— 只作用于 release、不影响用户 dev 增量，且与
//     `release.ps1` 仅包装本脚本，发布配方只在此处定义。
//     效果：产物确定（去掉「增量缓存状态」这一非确定源）+ 消除 incremental lock 告警。
run(
  "cargo",
  [
    "build",
    "--locked",
    "--release",
    "--features",
    "custom-protocol",
    "--manifest-path",
    "src-tauri/Cargo.toml",
  ],
  releaseBuildEnvironment,
);

// ③ 新鲜度校验：`poc/` 下**全部** `*.html` 都必须有 `dist/poc/` 对应产物、且不旧于源。
//
// ★ 覆盖范围 = `poc/` 下的全部 `*.html`（顶层，与 vite 的 `dist/poc/` 产出一一配对）。
//
// ★ 历史教训（为什么不能只比 `result.html` 一对）：早期此处只对比 `poc/result.html` ↔
//   `dist/poc/result.html`。#7 改的是 `poc/ball.html` —— 当时若 `dist/poc/ball.html` 尚旧，
//   这道门禁**照样绿灯**，会把「不含修复的旧球页」交付出去，而下游会据一个不含修复的构建
//   去做「验证」，在一个幽灵上打转。故此处改为**全量配对**：任一**缺失或陈旧**即失败，
//   并**一次性列出全部问题文件**（不是只报第一个）。
const srcDir = path.join(root, "poc");
const dstDir = path.join(root, "dist", "poc");
if (!fs.existsSync(srcDir)) {
  console.error(`\n✗ 源目录缺失：${path.relative(root, srcDir)}`);
  process.exit(1);
}
// 取 poc/ 下顶层全部 *.html（非递归 —— 与 vite 构建产出结构一致）。
const srcHtmls = fs
  .readdirSync(srcDir, { withFileTypes: true })
  .filter((e) => e.isFile() && e.name.toLowerCase().endsWith(".html"))
  .map((e) => e.name);
if (srcHtmls.length === 0) {
  console.error(`\n✗ 源目录无 *.html：${path.relative(root, srcDir)}`);
  process.exit(1);
}
const missing = []; // dist/poc/ 缺对应产物
const stale = []; // dist/poc/ 产品比源旧
for (const name of srcHtmls) {
  const s = path.join(srcDir, name);
  const d = path.join(dstDir, name);
  if (!fs.existsSync(d)) {
    missing.push(name);
    continue;
  }
  if (fs.statSync(d).mtimeMs < fs.statSync(s).mtimeMs) {
    stale.push(name);
  }
}
if (missing.length > 0 || stale.length > 0) {
  console.error("");
  if (missing.length > 0) {
    console.error(`✗ 打包产物缺失 ${missing.length} 个（前端 build 未产出对应文件）：`);
    for (const n of missing) console.error(`    - dist/poc/${n}`);
  }
  if (stale.length > 0) {
    console.error(`✗ 打包产物陈旧 ${stale.length} 个（源 poc/ 已更新但未重新构建）：`);
    for (const n of stale) console.error(`    - dist/poc/${n} ← 比 poc/${n} 旧`);
  }
  console.error("  请先跑前端 build（或去掉 --no-frontend），再重新执行本脚本。");
  process.exit(1);
}
console.log(`✓ dist/poc/ 新鲜度 OK（${srcHtmls.length} 个 *.html 均有对应产物且不旧于 poc/ 源，已含最新前端改动）`);

// ⑥（E3）可选：给 exe 做代码签名；签名不保证消除 SmartScreen 提示。
//   Windows：Authenticode（signtool.exe 或 osslsigncode）。需证书，通过环境变量配置，缺失则警告跳过。
//   macOS：codesign（需开发者证书 + 钥匙串访问权限）。
//   Linux：无统一签名体系，跳过（仅提示）。
//   本步**非致命**：配置缺失时只警告、不中止，避免没有证书时阻断本地构建。
if (doSign) {
  signArtifacts();
} else {
  console.log("\n（跳过签名：未传 --sign；分发前建议 `node scripts/build-release.mjs --sign`）");
}

// ⑦ 交付：复制构建产物到 `release/` 并刷新 `release/HASH.txt`。
//   ★ 放在签名（⑥）之后：交付的应是**最终**（含签名）产物，其 SHA256 才与分发文件一致。
if (noDeliver) {
  console.log("\n（跳过交付复制：--no-deliver；release/ 未刷新）");
} else {
  deliverArtifacts();
}

console.log(
  noDeliver
    ? "\n✓ Release 构建完成（未刷新 release/ 交付目录）。"
    : "\n✓ 发布构建完成（普通 Windows x64 绿色版 EXE、许可证、ZIP 和 SHA256 均在 release/）。",
);
process.exit(0);

// ---- E3：代码签名（可选，非致命）----

/** 跨平台定位构建产物 exe / 二进制。 */
function releaseBinary() {
  const base = path.join(root, "src-tauri", "target", "release");
  if (os.platform() === "win32") return path.join(base, "deepseek-float.exe");
  return path.join(base, "deepseek-float"); // macOS / Linux
}

// Assemble verified legal materials and the final (possibly signed) binary.
function deliverArtifacts() {
  const bin = releaseBinary();
  if (!fs.existsSync(bin)) {
    throw new Error(`未找到构建产物 ${path.relative(root, bin)}（cargo build 未产出？）`);
  }
  const result = assembleRelease({
    binary: bin,
    materials: path.join(root, "distribution", "windows"),
    output: path.join(root, "release"),
    releaseNotes: path.join(root, "docs", "beta2-release-notes-draft.md"),
  });
  console.log(`✓ EXE SHA256: ${result.exeHash}`);
  console.log(`✓ ZIP: ${path.relative(root, result.zip)} (${result.files} files)`);
  console.log(`✓ ZIP SHA256: ${result.zipHash}`);
}

/** 给发布产物签名（失败仅警告，不中止构建）。 */
function signArtifacts() {
  const bin = releaseBinary();
  if (!fs.existsSync(bin)) {
    console.warn(`\n⚠ 签名跳过：未找到构建产物 ${path.relative(root, bin)}（先完成 cargo build）`);
    return;
  }
  const platform = os.platform();
  try {
    if (platform === "win32") {
      // Windows Authenticode：优先 signtool（需装 Windows SDK），回退 osslsigncode。
      const cert = process.env.DSFLOAT_SIGN_CERT; // 证书路径（PFX）
      const pwd = process.env.DSFLOAT_SIGN_PWD || "";
      const sha1 = process.env.DSFLOAT_SIGN_SHA1; // 可选：证书 SHA1 指纹（用于 signtool /sha1）
      if (!cert && !sha1) {
        console.warn("\n⚠ 签名跳过：未配置 DSFLOAT_SIGN_CERT（PFX 路径）或 DSFLOAT_SIGN_SHA1。");
        console.warn("  示例：set DSFLOAT_SIGN_CERT=my.pfx && set DSFLOAT_SIGN_PWD=*** && node scripts/build-release.mjs --sign");
        return;
      }
      // 优先用显式指定的工具，否则默认 signtool。
      const tool = process.env.DSFLOAT_SIGN_TOOL || "signtool";
      const args = ["sign", "/fd", "SHA256", "/tr", "http://timestamp.digicert.com", "/td", "SHA256"];
      if (sha1) args.push("/sha1", sha1);
      else args.push("/f", cert, "/p", pwd);
      args.push(bin);
      // 不打印命令参数、工具输出或异常 message：它们都可能包含 PFX 密码。
      console.log("\n正在执行 Windows 代码签名（参数及工具输出不写入日志）");
      execFileSync(tool, args, { stdio: "pipe" });
      console.log("✓ Windows 签名完成（Authenticode + RFC3161 时间戳）");
    } else if (platform === "darwin") {
      const id = process.env.DSFLOAT_SIGN_IDENTITY || "-"; // 默认 ad-hoc（仅本地可用）
      console.log(`\n$ codesign --force --sign ${id} ${path.relative(root, bin)}`);
      execFileSync("codesign", ["--force", "--sign", id, bin], { stdio: "inherit" });
      console.log("✓ macOS 签名完成");
    } else {
      console.warn("\n⚠ Linux 无统一代码签名体系，跳过（建议走 Flatpak/AppImage 的分发签名）。");
    }
  } catch {
    console.warn("\n⚠ 签名失败（非致命，产物不可视为已签名）；请在本机检查签名工具与证书配置。为保护凭据，不输出工具参数或错误详情。");
  }
}
