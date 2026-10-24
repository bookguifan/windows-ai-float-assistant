// playwright.config.js
// ---------------------------------------------------------------------------
// 净化链回归测试的 Runner 配置（与 tests/sanitization-regression.spec.js 配套）。
//
// 职责：自动拉起 `vite preview`（serve dist/），把 poc/result.html 暴露给测试，
// 无需人工启动静态服务器。CI 用、本地也能用：
//   pnpm test:js            # 依赖 @playwright/test + chromium（见 CI 作业）
//   DSFLOAT_RESULT_URL=...  pnpm test:js   # 也可指向运行中的真实 App
// ---------------------------------------------------------------------------
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  // 只收 Playwright 规格（*.spec.js）。否则默认 testMatch 会把 tests/*.test.mjs
  // （node 单元测试：sanitize.test.mjs / business-logic.test.mjs）也当测试文件加载执行
  // —— 它们自带 jsdom 实例、并非 Playwright 用例，会重复执行并污染输出。
  testMatch: '**/*.spec.js',
  timeout: 30000,
  expect: { timeout: 5000 },
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:4173',
    headless: true,
    // 忽略系统/环境代理：本沙箱设了 HTTP_PROXY，经代理访问 localhost:4173 会卡死。
    // 加此标志后 `pnpm test:js` 无需再手工 unset 代理环境变量。
    launchOptions: { args: ['--no-proxy-server'] },
  },
  // 静态服务（dist/ → 结果卡）由 globalSetup 自起、globalTeardown 按 PID 回收。
  // ★ 刻意不用 Playwright 内置 `webServer`：本沙箱（Git Bash / Windows）下它 spawn 的
  //   vite 进程无法被回收（shell 包裹 → 杀进程组打不到孙进程），表现为「10 passed」
  //   打出后 runner 仍卡住不退（实测 90s 超时 rc=124）。自管进程可确定性回收。
  globalSetup: './tests/global-setup.mjs',
  globalTeardown: './tests/global-teardown.mjs',
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
