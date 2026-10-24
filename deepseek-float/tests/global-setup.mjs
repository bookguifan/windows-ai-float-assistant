// tests/global-setup.mjs
// ---------------------------------------------------------------------------
// 为净化链 E2E 起一个静态服务：`vite preview` serve dist/，把 /poc/result.html 暴露给测试。
//
// ★ 为什么不用 Playwright 内置的 `webServer`：
//   本沙箱（Git Bash / Windows）下 Playwright 会以 shell 包裹的方式 spawn 静态服务，
//   关服时无法回收那个 vite 进程（杀进程组打不到孙进程），表现为「10 passed」打出后
//   runner 仍卡住不退（实测 `timeout 90` → rc=124）。改为自行以 **detached、无 shell**
//   的方式启动，并在 globalTeardown 里按 PID `taskkill /T` —— 进程可控、可回收。
//
// 复用规则：若已存在服务（人工起过 / 上一轮残留 / 指向外部 App）则不接管、不启动。
// ---------------------------------------------------------------------------
import { spawn } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const PORT = 4173;
const URL = `http://localhost:${PORT}/poc/result.html`;
const PID_FILE = path.join(os.tmpdir(), 'dsfloat-e2e-preview.pid');

async function reachable() {
  try {
    const ctrl = new AbortController();
    const t = setTimeout(() => ctrl.abort(), 1500);
    const r = await fetch(URL, { signal: ctrl.signal });
    clearTimeout(t);
    return !!r && (r.ok || r.status === 304);
  } catch {
    return false;
  }
}

export default async function globalSetup() {
  // 指向外部 App 或已有服务：不接管、不启动（不写 PID，teardown 也就不会误杀）。
  if (process.env.DSFLOAT_RESULT_URL) return;
  if (await reachable()) return;

  const child = spawn(
    process.execPath,
    ['node_modules/vite/bin/vite.js', 'preview', '--port', String(PORT), '--strictPort'],
    { cwd: ROOT, detached: true, stdio: 'ignore' },
  );
  child.unref();
  fs.writeFileSync(PID_FILE, String(child.pid), 'utf8');

  const deadline = Date.now() + 120000;
  while (Date.now() < deadline) {
    if (await reachable()) return;
    await sleep(400);
  }
  throw new Error(`vite preview 未在 120s 内就绪：${URL}`);
}
