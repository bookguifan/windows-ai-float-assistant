import { defineConfig } from 'vite'

// 多入口：桌面壳的五个运行时页面（悬浮球 / 划词工具条 / 设置 / 截图遮罩 / 结果卡）。
// 不再依赖 React —— src/ 已作为遗留死代码移除，仅保留 poc/*.html。
// 相对路径由 Rollup/Vite 以项目根（默认 root = process.cwd()）为基准解析。
export default defineConfig({
  // ★ #7：显式绑定 **IPv4 回环** `127.0.0.1`。
  //   Vite 的 `host` 默认是 `'localhost'`，实测在 Windows 上只监听 `[::1]`（IPv6 回环），
  //   而 Tauri 的 `devUrl`（http://localhost:5173）在某些环境下无法连上；显式绑定 IPv4
  //   回环地址令二者可达 —— 这是**防御性**设置，保留。
  //   ⚠️ **归因更正（2026-09-26）**：此前此处把「真机球空白」归因于上述 IPv6 问题，
  //      **该归因不成立**（已证伪）。dev「启动时悬浮球白色」的真因是「26 GB 的
  //      `src-tauri/target` 被 Vite 冷启动全根递归扫描」，修法见下方 `server.watch` 处注释。
  //      两件事请勿混淆：IPv4 绑定是防御性的，白球是扫描阻塞。
  //   ⚠️ 刻意**不**绑定 `true` / `'0.0.0.0'`（那会暴露到局域网）—— 仅本机回环，安全不变。
  //   `port` 显式写死为与 `tauri.conf.json:devUrl` 一致的 5173，避免歧义。
  server: {
    host: '127.0.0.1',
    port: 5173,
    // ★ 2026-09-26（dev「启动时悬浮球白色」根因）：把 Rust 构建产物目录排除出 Vite 的扫描范围。
    //
    //   ## 症状
    //   `pnpm tauri dev` 启动后，球窗在 +3s 兜底显形时**页面还没画好**，用户看到「白色悬浮球」
    //   （约 15~47s 后才变成正常的鲸鱼 logo 球）。release 版不受影响（走自定义协议，不经 Vite）。
    //
    //   ## 证因（2026-09-26 实测，探针直接挂在 Vite 中间件上计时）
    //   · 请求**到达** Vite 只需毫秒级，但**首个** `/poc/ball.html` 的响应被扣住：
    //       不加本 ignore：first-request = 46894ms（复测 46628ms / 29247ms / 20471ms，波动大）
    //       加上本 ignore：first-request = 12ms
    //     且五个页面（ball/popup/settings/shot/result）的响应总是**在同一瞬间**一起返回
    //     —— 典型的一次性全局阻塞。
    //   · 阻塞源 = 项目根内的 `src-tauri/target/`：实测 **26 GB / 29192 个文件** 的 Rust 构建产物。
    //     冷启动时 Vite 对项目根做一次性递归遍历/扫描（`src-tauri` 不属于任何前端入口），
    //     把首个页面请求的 I/O 排在数万个目录/文件项之后 ⇒ 首屏被拖到十几~几十秒。
    //   · 注意：`server.watch: null`（关掉文件监听）**并不能**消除该阻塞（实测仍 46628ms），
    //     故这里用的是 `watch.ignored`（Vite 用它决定递归扫描要跳过哪些路径），而非关监听。
    //
    //   ## 为什么安全
    //   `src-tauri/**` 里没有任何前端资源（`poc/*.html` 全在项目根下），Vite 无需监听；
    //   Rust 侧热重载由 `cargo`/`tauri dev` 负责，不依赖 Vite 的文件监听。
    //   顺带把 `node_modules` / `.git` 显式写回，避免自定义 `ignored` 时丢掉默认忽略项。
    watch: {
      ignored: ['**/src-tauri/**', '**/node_modules/**', '**/.git/**'],
    },
  },
  build: {
    rollupOptions: {
      input: {
        ball: 'poc/ball.html',
        popup: 'poc/popup.html',
        settings: 'poc/settings.html',
        shot: 'poc/shot.html',
        // R2 结果卡：新增入口后**必须重建 dist**，否则卡片页 404 →
        // `result::arm_watchdog` 会在 500ms 后自动隐藏（该看门狗的行为即 W-V8 的验证法）。
        result: 'poc/result.html',
        menu: 'poc/menu.html',
      },
    },
  },
})
