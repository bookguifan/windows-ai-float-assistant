// tests/global-teardown.mjs
// ---------------------------------------------------------------------------
// 回收 global-setup 起的静态服务：读取 PID 文件，按 PID `taskkill /T /F`（整棵进程树）。
// 只杀「本进程组自起」的那一个（PID 由 setup 落盘）；外部/复用服务不会被碰。
// ---------------------------------------------------------------------------
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const PID_FILE = path.join(os.tmpdir(), 'dsfloat-e2e-preview.pid');

export default async function globalTeardown() {
  let pid = NaN;
  try {
    pid = Number(fs.readFileSync(PID_FILE, 'utf8').trim());
  } catch {
    return; // 没有 PID 文件 → 本轮没起服务，无需回收
  }
  try {
    fs.rmSync(PID_FILE, { force: true });
  } catch {
    /* ignore */
  }
  if (!Number.isFinite(pid) || pid <= 0) return;

  if (process.platform === 'win32') {
    spawnSync('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' });
  } else {
    try {
      process.kill(-pid, 'SIGKILL');
    } catch {
      /* 已退出 */
    }
  }
}
