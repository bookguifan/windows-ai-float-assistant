/*
 * dsfloat_init.js —— 注入到 panel（chat.deepseek.com）的初始化脚本。
 *
 * 由 lib.rs 用 `.initialization_script(include_str!("dsfloat_init.js"))` 注入，
 * 每次页面加载都会执行，因此在 SPA 内部跳转后依然可用。
 *
 * 设计约束（架构师两条源码级结论）：
 *  1) 远端页里注入的脚本**不允许**回调 Tauri 命令（无 remote 声明的 capability 一律被 ACL 拒绝），
 *     所以本文件只做 DOM 操作，不 invoke、不 listen，也不主动发送。
 *     「发什么、发不发」全部由 Rust 侧决定（Rust eval 不受 ACL 限制）。
 *  2) 输入框定位**不依赖 class/id**（DeepSeek 前端会改名），只用可见性 + 语义 + 位置打分。
 *
 * 对外只暴露 window.__DSFLOAT__，供 Rust eval 调用：
 *   __DSFLOAT__.fill(text)      → 把文本写入输入框（三级注入），返回 true/false
 *   __DSFLOAT__.submit(expect, wantIdx) → 校验已填入内容 + 点击 rank=`wantIdx` 的发送按钮，返回状态字符串
 *   __DSFLOAT__.pressEnter()    → 页内合成一次 Enter（发送兜底：不依赖前台焦点/不会外泄到别的窗口）
 *   __DSFLOAT__.stillHasText(t) → 复核输入框是否仍包含 t（发送成功后输入框应被清空）
 *   __DSFLOAT__.inspect()       → 返回输入框与候选发送按钮的诊断信息（供真机核对）
 *   __DSFLOAT__.inspectUpload() → 返回候选「图片上传入口」(input[type=file]) 与上传类按钮锚点
 *                                 （「截图提问」S1 诊断探针，纯观测）
 *   __DSFLOAT__.inspectConversation() → 返回候选「会话容器 / 消息块列表」及虚拟化比 virtRatio
 *                                 （「结果回流」R0 诊断探针，纯观测）
 *   __DSFLOAT__.snapshotConversation(prompt) → 读取会话**尾窗块文本**（「结果回流」R1 回读桥的数据源，纯观测）
 *                                  `prompt` = 我方刚发出的提示词：页内据此做**结构性硬过滤**（只接受
 *                                  尾窗里确实含该提示词的容器），排除侧栏/输入区/欢迎页（真机缺陷 2026-09-14）
 *                                  `wantDump` = 是否附带容器骨架诊断（默认否；每轮都算会拖慢页面）
 *                                 定位策略见文件内注释（R0 真机结论：列表虚拟化 → 只读底部渲染窗口）
 *
 * submit() 返回：'ok' | 'already-submitted' | 'no-input' | 'empty-after-fill'
 *                | 'content-mismatch' | 'no-send-button'
 * 完整校验后单次点击；sendEvidence 只在本轮问题回显成立时确认已发送。
 */
(function () {
  if (window.__DSFLOAT__) return;

  const INPUT_SELECTOR = 'textarea, [contenteditable="true"], [contenteditable=""], input[type="text"]';
  const SEND_WORDS = /(send|发送|提交|submit)/i;

  function norm(s) {
    return String(s == null ? '' : s).replace(/\s+/g, ' ').trim();
  }

  // ★ 批次1（2026-10-03）：**删掉一切空白**后再比较 —— 「跨站点文本同源」判据。
  //
  //   为什么必须存在：`norm` 把空白折叠成**单个空格**，这隐含假设「写作端的换行 == 读回端的空白」。
  //   该假设**只在 DeepSeek 成立**。ChatGPT 输入框是 ProseMirror（`#prompt-textarea.ProseMirror`），
  //   段落以 `<p>` 承载，`textContent` 取回是「行1行2」（**段间无任何分隔**）；而 `norm(expected)`
  //   是「行1 行2」（换行被折成空格）⇒ `got.indexOf(want) === -1` **恒成立** ⇒ `submit()` 在
  //   `content-mismatch` 处直接放弃发送（真机 `#88`，2026-10-03）。
  //
  //   与 Rust 侧 `capture::normalize_text` **同口径**（那边的 `compactText` 亦然，见本文件 `:512`）。
  //   只用于**校验**，绝不参与任何写入/提交逻辑 ⇒ 纯判据，零副作用。
  function normLoose(s) {
    return String(s == null ? '' : s).replace(/\s+/g, '');
  }

  // 文本是否「实质相符」—— 三级判据，从严到宽（批次1 的核心）。
  //
  //   ① 严格：`norm` 后的包含关系（原行为，DeepSeek 上一直工作）。
  //   ② 宽松：`normLoose` 后的包含关系（治本 —— 与站点换行语义无关）。
  //   ③ 首尾片段：取期望文本（紧凑后）的**首 24 / 尾 24 字符**，两者**都**须命中。
  //      `normLoose` 包含也可能因**站点做了规范化改写**（如 ProseMirror 吞掉某些字符、
  //      或富文本把 `**bold**` 转成真加粗导致标记符消失）而失败；此级用「首尾锚点」兜底，
  //      要求**双重否定**（首尾都命中）才放行 —— 单片段命中不足以证明「发的是这段内容」。
  //   ★ 全部失败 ⇒ 返回 false ⇒ 仍按 mismatch 处理（保守，不误发）。
  function textLooksSame(got, want) {
    if (!want) return true; // 无期望值 ⇒ 无从校验（与原 `if (want && …)` 语义一致）
    const g = String(got == null ? '' : got);
    const w = String(want);
    if (g.indexOf(w) !== -1) return true; // ① 严格

    const gl = normLoose(g);
    const wl = normLoose(w);
    if (wl && gl.indexOf(wl) !== -1) return true; // ② 宽松（治本）
    if (!wl) return true;

    // ③ 首尾锚点双重否定（仅当文本足够长，否则与 ①② 等价，无意义）
    if (wl.length < 32) return false;
    const head = wl.slice(0, 24);
    const tail = wl.slice(-24);
    return gl.indexOf(head) !== -1 && gl.indexOf(tail) !== -1;
  }

  function isVisible(el) {
    return isVisibleWith(el, 40, 12);
  }

  // ★ 发送按钮的可见性判据必须比输入框宽松：DeepSeek 等前端的发送按钮是 ~32×32 的图标按钮，
  //   用 40px 的宽度门槛会被直接过滤掉 → scoredSenders 空 → 退化成「找不到发送按钮」→ 只能靠回车。
  //   这正是真机反馈「只填入不发送」的根因之一。
  function isVisibleControl(el) {
    return isVisibleWith(el, 12, 12);
  }

  function isVisibleWith(el, minW, minH) {
    if (!el || !el.getBoundingClientRect) return false;
    const r = el.getBoundingClientRect();
    if (r.width < minW || r.height < minH) return false;
    const st = window.getComputedStyle(el);
    if (!st) return false;
    if (st.display === 'none' || st.visibility === 'hidden' || st.opacity === '0') return false;
    return true;
  }

  // 打分（不依赖 class/id）：越靠页面底部越像输入框；面积越大越像；语义上 textarea/contenteditable 更可信。
  function findInput() {
    const vh = window.innerHeight || document.documentElement.clientHeight || 1;
    let best = null;
    let bestScore = -Infinity;
    for (const el of document.querySelectorAll(INPUT_SELECTOR)) {
      if (!isVisible(el) || el.disabled || el.readOnly || el.getAttribute('aria-disabled') === 'true') continue;
      const r = el.getBoundingClientRect();
      const bottomBias = r.bottom / vh; // 0..1
      const area = Math.min(1, (r.width * r.height) / 40000);
      const semantic = el.tagName === 'TEXTAREA' || el.isContentEditable ? 1 : 0.4;
      const score = bottomBias * 3 + area + semantic;
      if (score > bestScore) {
        bestScore = score;
        best = el;
      }
    }
    return best;
  }

  function readText(el) {
    if (!el) return '';
    if (!el.isContentEditable) return el.value || '';
    // Slate 占位文字属于 DOM 装饰，不是用户草稿。
    if (el.hasAttribute('data-slate-editor') || el.hasAttribute('data-lexical-editor')) {
      const copy = el.cloneNode(true);
      copy.querySelectorAll('[data-slate-placeholder], [contenteditable="false"]').forEach(n => n.remove());
      return copy.textContent || '';
    }
    return el.textContent || '';
  }

  function setNativeValue(el, text) {
    // React 受控组件：必须走原型上的原生 setter，否则 React 内部 state 不会被更新
    const proto = Object.getPrototypeOf(el);
    const desc = proto && Object.getOwnPropertyDescriptor(proto, 'value');
    if (desc && desc.set) desc.set.call(el, text);
    else el.value = text;
    el.dispatchEvent(new Event('input', { bubbles: true }));
  }

  function selectAll(el) {
    const sel = window.getSelection();
    if (!sel) return false;
    const range = document.createRange();
    range.selectNodeContents(el);
    sel.removeAllRanges();
    sel.addRange(range);
    return true;
  }

  // 原生 setter 或编辑器 insertText；失败返回 false，由 Rust 在截止时间内重试填入。
  const FILL_STAGES = new WeakMap();
  function fill(text) {
    const el = findInput();
    if (!el) return false;
    const needsFocus = el.isContentEditable && (el.hasAttribute('data-lexical-editor') || el.hasAttribute('data-slate-editor')) && document.activeElement !== el;
    try {
      el.focus();
    } catch (_) {}
    // 让受控富文本编辑器的 focus/selectionchange 状态先完成，再由有界重试填入。
    if (needsFocus) return false;

    if (el.isContentEditable) {
      if (el.hasAttribute('data-slate-editor') || el.hasAttribute('data-lexical-editor')) {
        // Slate / Lexical 的模型选区在 selectionchange 后更新；不能在同一拍插入，否则会追加到旧草稿。
        if (normLoose(readText(el)) === normLoose(text)) return true;
        const request = window.__dsfloatRequest || 0;
        let stage = FILL_STAGES.get(el);
        if (!stage || stage.request !== request || stage.text !== text) {
          selectAll(el);
          FILL_STAGES.set(el, {request: request, text: text, applied: false});
          return false;
        }
        if (!stage.applied) {
          stage.applied = true;
          el.dispatchEvent(new InputEvent('beforeinput', {bubbles: true, cancelable: true,
            composed: true, data: text, inputType: 'insertText'}));
        }
        // 等待编辑器提交模型变化；下一次有界采样核对完整正文后才允许发送。
        return false;
      }
      try {
        // Lexical 会在 selectionchange 后重建 Range；其原生 selectAll 命令能保持编辑器选择。
        if (!((el.hasAttribute('data-lexical-editor') || el.hasAttribute('data-slate-editor')) && document.execCommand && document.execCommand('selectAll'))) selectAll(el);
        if (document.execCommand && document.execCommand('insertText', false, text)) {
          if (normLoose(readText(el)) === normLoose(text)) return true;
        }
      } catch (_) {}
      // 受控编辑器不能直接改 DOM，否则显示内容会被内部旧 state 覆盖。
      if (el.hasAttribute('data-lexical-editor') || el.hasAttribute('data-slate-editor')) return false;
      // 普通 contenteditable 的兼容回退
      el.textContent = text;
      el.dispatchEvent(new InputEvent('input', { bubbles: true, data: text, inputType: 'insertText' }));
      return normLoose(readText(el)) === normLoose(text);
    }

    // textarea / input
    setNativeValue(el, text);
    if (normLoose(readText(el)) === normLoose(text)) return true;

    // 兜底：select-all + insertText（部分富文本输入框只认这条路）
    try {
      el.setSelectionRange(0, (el.value || '').length);
      if (document.execCommand && document.execCommand('insertText', false, text)) {
        if (normLoose(readText(el)) === normLoose(text)) return true;
      }
    } catch (_) {}
    return normLoose(readText(el)) === normLoose(text);
  }

  // 找到「输入框所在的 composer 容器」：只在该容器内寻找发送按钮。
  // ★ 关键修复（2026-09-20）：此前候选范围 = 整个页面，会把上一轮消息气泡上的
  //   「重新发送 / 重新生成」等动作按钮也当发送候选 —— 其中用户消息的「重新发送」文案含「发送」二字，
  //   被 SEND_WORDS 判 +5 分，极易被误点：点了会把**上一题重新提交**，而当前输入框里的新内容
  //   却没发出（这正是真机「内容进了输入框却没发送 + 上一题被重复触发」的根因）。
  //   发送按钮必然与输入框同处一个 composer 容器（form / 外层 wrapper），历史消息气泡在更上层、
  //   不在其中 ⇒ 把搜索范围收敛到 composer 即可彻底排除历史消息动作按钮。
  //   优先用 <form>；退而求其次取「输入框向上的、首个包含可点按钮的最小容器」（消息气泡在更高层，
  //   不会落进来；即便整页被一个 <form> 包住，命中的也是更靠下的 composer 包装层）。
  function composerRootOf(input) {
    // 实测新版 ChatGPT 与 Kimi 的编辑器边界；不能停在只含上传按钮的更内层。
    if (location.hostname === 'chatgpt.com') {
      const root = input.closest('[data-composer-body]');
      if (root) return root;
    }
    if (location.hostname === 'www.kimi.com' || location.hostname === 'kimi.com') {
      const root = input.closest('[data-testid="chat-editor"]');
      if (root) return root;
    }
    if (location.hostname === 'chatglm.cn') {
      const root = input.closest('#search-input-box');
      if (root) return root;
    }
    if (location.hostname === 'www.qianwen.com') {
      const root = input.closest('[data-chat-input-shell]');
      if (root) return root;
    }
    const form = input.closest('form');
    if (form) return form;
    let el = input.parentElement;
    for (let i = 0; i < 6 && el; i++, el = el.parentElement) {
      if (el.querySelector('button, [role="button"]')) return el;
    }
    return input.parentElement || input;
  }

  // 候选发送按钮打分：label 命中权重最高；与输入框同一行 + 位于其右侧加分；禁用的一律不点。
  // 候选范围限定在 composer 容器内（见 composerRootOf），不再扫全页。
  function scoredSenders(input) {
    const root = composerRootOf(input);
    const ir = input.getBoundingClientRect();
    const out = [];
    // 候选范围放宽：原生 button / role=button / data-testid 或 class 含 send 的元素
    // （图标按钮常常没有 aria-label，也不一定是 <button>）。搜索范围限定在 composer 容器内。
    const SEL = 'button, [role="button"], [data-testid*="send" i], [class*="send" i], img.enter_icon';
    for (const b of root.querySelectorAll(SEL)) {
      if (b.namespaceURI !== 'http://www.w3.org/1999/xhtml' || !isVisibleControl(b)) continue;
      if (b.disabled || b.getAttribute('aria-disabled') === 'true') continue;
      const label = [b.getAttribute('aria-label'), b.getAttribute('title'), b.textContent].filter(Boolean).join(' ');
      const identity = label + ' ' + (b.getAttribute('data-testid') || '') + ' ' + (b.className || '');
      // DeepSeek 2026-10 实测：无 aria-label 的圆形主按钮，图标为向上发送箭头。
      // 域名、按钮样式、角色和箭头路径必须同时命中；不能退回纯位置猜测。
      const deepseekSend = location.hostname === 'chat.deepseek.com' &&
        b.matches('[role="button"].ds-button--primary.ds-button--circle') &&
        Array.from(b.querySelectorAll('svg path')).some(p => (p.getAttribute('d') || '').includes('V15.0417H7V3.95579'));
      const glmSend = location.hostname === 'chatglm.cn' && b.matches('#search-input-box .enter-icon-container > img.enter_icon') && !b.closest('.disabled, .is-disabled');
      if ((!SEND_WORDS.test(identity) && !deepseekSend && !glmSend) || /(upload|attach|stop|上传|附件|停止)/i.test(label)) continue;
      const r = b.getBoundingClientRect();
      let score = 0;
      if (SEND_WORDS.test(identity) || deepseekSend || glmSend) score += 5;
      if (Math.abs(r.top + r.height / 2 - (ir.top + ir.height / 2)) < 48) score += 2; // 同一行
      if (r.left >= ir.left) score += 1; // 在输入框右侧
      out.push({ el: b, score, label: norm(label).slice(0, 40) });
    }
    out.sort(function (a, b) {
      return b.score - a.score;
    });
    return out;
  }

  function submit(expected, wantIdx) {
    const input = findInput();
    if (!input) return 'no-input';

    const got = norm(readText(input));
    if (!got) return 'empty-after-fill';

    // 只接受完整文本的空白归一化等价；不允许仅首尾片段相符就提交。
    const matched = normLoose(got) === normLoose(expected);
    if (!matched) return 'content-mismatch';
    if (SEND_CONTEXT && SEND_CONTEXT.submitted) return 'already-submitted';

    // ★ 点击前先聚焦输入框：让 DeepSeek 的富文本编辑器处于激活态，提高「点发送按钮」被受理的概率。
    try { input.focus(); } catch (_) {}

    const ranked = scoredSenders(input);
    // 调用方仅点击一次最可信候选；无可靠候选时保留草稿，由用户核对。
    const pick = ranked[wantIdx || 0];
    if (!pick || pick.score < 2) {
      return 'no-send-button';
    }

    // ★ 诊断：记下实际点到的按钮文案，供 Rust 侧落 d1-debug.log 核对是否点错按钮
    //   （2026-09-20：曾误点历史消息的「重新发送」按钮，导致上一题被重复提交）。
    try { window.__DSFLOAT__._diagSendLabel = pick.label; } catch (_) {}
    // ★ 批次1：把校验结论一并留给 Rust 侧核对（纯观测，失败不影响发送）。
    try { window.__DSFLOAT__._diagFillMatched = matched ? 1 : 0; } catch (_) {}
    prepareSend(expected, true);
    if (SEND_CONTEXT) SEND_CONTEXT.submitted = true;
    // ChatGLM 真机监听器：发送绑定在父容器 mousedown，click 仅为 tooltip。
    // 只派发该站点的一种提交事件，不能再补 click / Enter。
    if (location.hostname === 'chatglm.cn' && pick.el.matches('#search-input-box .enter-icon-container > img.enter_icon')) {
      pick.el.parentElement.dispatchEvent(new MouseEvent('mousedown', {bubbles: true, cancelable: true, button: 0}));
    } else pick.el.click();
    return 'ok';
  }

  // 页内合成一次 Enter：不依赖「前台窗口/焦点」，因此绝不会把按键漏到用户正在用的别的程序，
  // 也避开了 OS 级 SendInput 与 WebView2 内部焦点不同步的问题（后者是真机「没真正发送」的可疑原因）。
  function pressEnter() {
    const el = findInput();
    if (!el) return false;
    try {
      el.focus();
    } catch (_) {}
    const opts = {
      key: 'Enter',
      code: 'Enter',
      keyCode: 13,
      which: 13,
      bubbles: true,
      cancelable: true,
      composed: true,
    };
    try {
      el.dispatchEvent(new KeyboardEvent('keydown', opts));
      el.dispatchEvent(new KeyboardEvent('keypress', opts));
      el.dispatchEvent(new KeyboardEvent('keyup', opts));
      return true;
    } catch (_) {
      return false;
    }
  }

  // 发送是否真的发生：以「输入框已不再包含待发文本」为判据（发送成功后输入框会被清空）。
  // expect 为空时只判断「输入框是否已空」。
  function stillHasText(expect) {
    const el = findInput();
    const got = norm(readText(el));
    if (!got) return false;
    const want = norm(expect);
    return want ? got.indexOf(want) !== -1 : true;
  }

  // 真机核对用：把找到的输入框与候选按钮列出来
  function inspect() {
    const input = findInput();
    return {
      hasInput: !!input,
      inputTag: input ? input.tagName : null,
      inputEditable: input ? !!input.isContentEditable : false,
      inputText: input ? norm(readText(input)).slice(0, 80) : '',
      senders: input
        ? scoredSenders(input).slice(0, 6).map(function (s) {
            return { score: s.score, label: s.label };
          })
        : [],
    };
  }

  // 「截图提问」S1 诊断探针：枚举页面上候选的「图片上传入口」。
  // 纯观测 —— 只读 DOM，不改动、不派发事件、不点击。用于判定 S1 验证项 (b)：
  // 若存在可注入的 input[type=file]，首选投递路线（页内 DataTransfer 注入）成立。
  function inspectUpload() {
    try {
      const inputs = [];
      const files = document.querySelectorAll('input[type="file"]');
      for (let i = 0; i < files.length; i++) {
        const el = files[i];
        const r = el.getBoundingClientRect ? el.getBoundingClientRect() : { width: 0, height: 0 };
        const st = window.getComputedStyle(el);
        inputs.push({
          i: i,
          accept: el.getAttribute('accept') || '',
          multiple: !!el.multiple,
          hiddenAttr: !!el.hidden,
          size: Math.round(r.width) + 'x' + Math.round(r.height),
          display: st ? st.display : '',
          visibility: st ? st.visibility : '',
          cls: String(el.className || '').slice(0, 60),
        });
      }
      // 顺带找「上传 / 附件 / 图片」类按钮：部分前端不预置 file input，
      // 而是点击按钮时才动态创建 → 该按钮即备用投递锚点。
      const words = /(upload|attach|image|photo|picture|图片|上传|附件|相册|文件)/i;
      const buttons = [];
      const cands = document.querySelectorAll('button, [role="button"], [aria-label], [title]');
      for (let j = 0; j < cands.length && buttons.length < 8; j++) {
        const b = cands[j];
        const label = norm(
          [b.getAttribute('aria-label'), b.getAttribute('title'), b.textContent]
            .filter(Boolean)
            .join(' ')
        );
        if (label && words.test(label) && isVisibleControl(b)) {
          buttons.push(label.slice(0, 40));
        }
      }
      return {
        fileInputCount: inputs.length,
        fileInputs: inputs,
        uploadLikeButtons: buttons,
        url: location.href,
        viewport: (window.innerWidth || 0) + 'x' + (window.innerHeight || 0),
        dpr: window.devicePixelRatio || 1,
      };
    } catch (e) {
      return { error: String((e && e.message) || e) };
    }
  }

  // ---------------------------------------------------------------------------
  // 「结果回流」R0 诊断探针（设计文档 §三 U1 / §十 路线图）—— 纯观测
  //
  // 这是本方案的 **go/no-go 闸门**，要一次回答两件事：
  //   ① 能否在本页定位到承载整段会话的容器（不依赖 class/id，与 findInput 同范式）；
  //   ② 该容器是否**虚拟化** —— 若滚动会卸载旧节点，R1 的「前后快照差集」就会失真
  //      （前缀消失 → delta 不再是「新增后缀」），方案形态须改写（设计文档 §七 R3）。
  //
  // 判据 `virtRatio`（**条目数**基准，非高度基准）：
  //     expectedItems = scrollHeight / medianItemHeight    // 若全部条目都在 DOM，条目数应 ≈ 此值
  //     virtRatio     = renderedItems / expectedItems
  //   非虚拟化 ≈ 1；<< 1 → 只渲染了可视区 → 虚拟化。
  //   ★ 为何不用「已渲染高度之和 / scrollHeight」：react-window 式虚拟化会把内层 div 用 spacer
  //     **显式撑到内容总高**，于是子高之和 ≈ scrollHeight → 高度基准会**假报「非虚拟化」**。
  //     条目数基准对 spacer 免疫（spacer 会被下面的 sizer 口径排除）。
  // 另：Rust 侧会缓存上次 `pick` 的指纹，**两次触发之间滚动面板**即可从日志的 Δ 行
  //     直接读出「滚动是否导致 DOM 变化」，无需人工比对。
  //
  // 纯观测：只读 DOM / 计算样式，不改动、不派发事件、不点击、不滚动。
  // ---------------------------------------------------------------------------
  const CONV_SCROLL_EPS = 40; // scrollHeight 超出 clientHeight 多少才算「可滚动」
  const CONV_BLOCK_MIN_H = 16; // 计入「条目」的最小块高（滤掉装饰性节点）
  const CONV_MIN_TEXTLEN = 120; // 候选块列表的最小总文本长（滤掉空壳容器）
  // ★ 第二遍（输入框锚定）的放宽下限 —— 理由见 locateConversation 的 B2)：短会话/新会话的
  //   整段文本可能只有几十字符（探针「请只回复这四个字：回读测试」+ 回答「回读测试」≈ 9 字符），
  //   用 120 会把新会话整体滤掉 → 回读恒报「未定位到会话容器」（真机缺陷 2026-09-14）。
  const CONV_MIN_TEXTLEN_RELAXED = 20;
  const CONV_SIZER_MAX = 0.6; // 块高 > 0.6×clientHeight 即视为「撑高用的 sizer」，不算条目

  // class 片段仅供人眼核对，**不参与任何逻辑**（DeepSeek 前端会改名）。
  function elCls(el) {
    const raw = el.className;
    const s = raw && raw.baseVal !== undefined ? raw.baseVal : raw; // SVGAnimatedString 兜底
    return String(s || '').slice(0, 60);
  }

  function elSize(el) {
    const r = el.getBoundingClientRect ? el.getBoundingClientRect() : { width: 0, height: 0 };
    return Math.round(r.width) + 'x' + Math.round(r.height);
  }

  function median(nums) {
    if (!nums.length) return 0;
    const a = nums.slice().sort(function (x, y) {
      return x - y;
    });
    const m = Math.floor(a.length / 2);
    return a.length % 2 ? a[m] : Math.round((a[m - 1] + a[m]) / 2);
  }

  // ★★ 2026-10-03 六次真机（方案 A14 · JS 侧）：**「尾块是装饰条 ⟹ 本层不是消息列表」**。
  //
  //   真机证据（chatgpt.com · `#52/#53/#59/#60/#61`）：
  //     `self=DIV.flex min-h-0 grow[3/91]`
  //     `down=DIV.qMYqUG_convSearchResultH[4/68] , DIV.pointer-events-non[0/0] , DIV.invisible mt-auto[2/23]`
  //     ⇒ **`flex min-h-0 grow` 是「消息区 + 免责页脚」的**合体层**：
  //        第 1 子 = `qMYqUG`（真内容：SPAN空 + DIV空 + 回显 + 回答），
  //        第 3 子 = `invisible mt-auto`（**页脚**「ChatGPT 也可能会犯错。请核查重要信息。」）。
  //     `#60` 交付前骨架确认 `texts = [qMYqUG(1796), 页脚(188)]` ⟹
  //     Rust 取「含提示词块的后一块」= `texts[1]` = **页脚** ⟹ `#61` 交付 188 字符页脚。
  //
  //   为什么必须在此判定（而不是只靠 Rust 侧否决，方案 B）：
  //     单靠 B 否决页脚 ⟹ `locate_answer_and_thinking` 返回空（回答在 `texts[0]` 里取不到）⟹ 超时。
  //     **必须在 JS 侧把「合体层」降级为「中转层」**，继续下钻到真内容容器 `qMYqUG`，
  //     才能让 `texts` 变成 `[回显, 回答]` ⟹ Rust 取到**回答**。
  //
  //   判据（三项式，极保守 —— **不匹配就不否决**，与修改前行为一致 ⟹ 不回归）：
  //     ① 尾块文本（`bl.lastTexts` 末项 = 最后一个**有文本**直接子块的头 36 字符）**长度 ≤ 40**；
  //     ② 命中装饰关键词清单 `CONV_DECOR_HINTS`；
  //     ③ `lastTexts` 非空。
  //   ★ 无需改 `blocksOf` —— `lastTexts` 已由它提供（末项即尾块文本头）。
  //   ★ 关键词与 Rust 侧 `analyze.rs::DECOR_HINTS` **口径保持一致**（两处同步维护）。
  //
  //   ★★ 2026-10-03 六次真机复验修复（`tailIsDecor is not defined`，真机 `#93~#111`）：
  //     **必须声明在顶层作用域**（与 `blocksOf` 同级），因为 `tailIsDecor` 被**两个不同函数**
  //     调用：`locateConversation`（下钻判据）与 `snapshotConversation`（缓存校验）。
  //     首版误声明在 `locateConversation` **函数体内** ⟹ `snapshotConversation` 调用时
  //     抛 `ReferenceError` ⟹ 首轮定位成功（`#88` 选中 `qMYqUG`）后，**后续每轮轮询都失败**
  //     ⟹ 持续 60s「本轮采样失败」⟹ 超时降级交付「ChatGPT 说：」（真机 `#112`）。
  const CONV_DECOR_HINTS = [
    '可能会犯错', // ChatGPT：「ChatGPT 也可能会犯错。请核查重要信息。」
    '请核查重要信息',
    '内容由AI生成', // DeepSeek：「内容由 AI 生成，请仔细甄别」（去空格后匹配）
    '内容由 AI 生成',
    '请仔细甄别',
    '内容仅供参考',
  ];
  const CONV_DECOR_MAX_CHARS = 40; // 尾块文本头（36 字符）内即可容纳以上文案
  function tailIsDecor(bl) {
    if (!bl || !bl.lastTexts || !bl.lastTexts.length) return false;
    const lt = bl.lastTexts[bl.lastTexts.length - 1] || '';
    if (!lt || lt.length > CONV_DECOR_MAX_CHARS) return false;
    const compact = lt.replace(/\s+/g, '');
    for (let di = 0; di < CONV_DECOR_HINTS.length; di++) {
      if (compact.indexOf(CONV_DECOR_HINTS[di].replace(/\s+/g, '')) >= 0) return true;
    }
    return false;
  }

  // ★★ 2026-10-03 七次真机（问题②）：**「视觉隐藏」的角色头不是正文，序列化时必须剥离**。
  //
  //   真机证据（chatgpt.com · `#88/#89`；见 `d1-debug.log`）：
  //     助手块骨架 = `<SECTION.text-token-text-primar>[H4.sr-only select-none「ChatGPT 说：」,
  //                  DIV.text-base my-auto mx-a（回答正文）, SPAN.sr-only]`
  //     回显块骨架 = `[H4.sr-only select-none「你说：」, DIV.text-base（提示词）, SPAN.sr-only]`
  //   ⇒ **`<h4 class="sr-only select-none">` 是 ChatGPT 的「角色头」**（屏幕阅读器专用，
  //      视觉上被 `sr-only` 隐藏）。它不是回答内容，但 `serializeHtml` 此前只跳过
  //      `role=button` / `aria-hidden=true`，**不认 `sr-only` 类** ⟹ 角色头混进交付内容
  //      ⟹ 用户看到卡片首行是「ChatGPT 说：」（用户明确要求不显示）。
  //
  //   判据（**极窄**）：class token 含 `sr-only` / `sr-only-*`（Tailwind 通用视觉隐藏约定）。
  //     ★ 刻意**只**匹配 `sr-only` —— 不扩大到 `visually-hidden` 等自造类名：
  //       ChatGPT 只用了 `sr-only`（真机全量日志 `sr-only` 仅 6 处，均为该角色头 / 空 SPAN），
  //       收窄可确保「非 ChatGPT 站点零影响」。
  //     ★ 语义正确性：`sr-only` 的定义就是「仅屏幕阅读器可见、视觉隐藏」，剥离它是**修正**
  //       而不是「丢内容」；它永远不会承载模型回答的可见正文。
  function isSrOnly(node) {
    const cls = node.getAttribute ? node.getAttribute('class') : null;
    if (!cls || typeof cls !== 'string') return false;
    const tokens = cls.split(/\s+/);
    for (let i = 0; i < tokens.length; i++) {
      if (tokens[i] === 'sr-only' || tokens[i].indexOf('sr-only-') === 0) return true;
    }
    return false;
  }

  // 把一个容器的**直接子节点**当作「消息块」统计（不依赖 class/id）。
  // `itemHeights` 供虚拟化判定用（见下方「条目数基准」）；空文本块不进 first/lastTexts（纯噪声）。
  function blocksOf(el) {
    const kids = el.children || [];
    let textLen = 0;
    let withText = 0;
    let maxTextLen = 0;
    const itemHeights = [];
    const firstTexts = [];
    const lastTexts = [];
    for (let i = 0; i < kids.length; i++) {
      const c = kids[i];
      const h = c.getBoundingClientRect ? c.getBoundingClientRect().height : 0;
      if (h >= CONV_BLOCK_MIN_H) itemHeights.push(Math.round(h));
      const t = norm((c.textContent || '').slice(0, 2000)); // 先截断再归一化：避免对超大子树跑全量正则
      textLen += t.length;
      if (t.length > maxTextLen) maxTextLen = t.length;
      if (!t) continue;
      withText++;
      if (firstTexts.length < 3) firstTexts.push(t.slice(0, 36));
      lastTexts.push(t.slice(0, 36));
    }
    if (lastTexts.length > 3) lastTexts.splice(0, lastTexts.length - 3);
    return {
      childCount: kids.length,
      withText: withText,
      itemHeights: itemHeights,
      textLen: textLen,
      maxTextLen: maxTextLen,
      firstTexts: firstTexts,
      lastTexts: lastTexts,
    };
  }

  // 是否「可滚动」。★ 必须显式接受 `document.scrollingElement`：整页滚动时它的
  // `overflow-y` 是 `visible`，只按 overflow 判定会漏掉，导致祖先回溯失败、virtRatio 恒 null。
  function scrollsEl(el) {
    if (!el || !el.scrollHeight) return false;
    const sh = el.scrollHeight || 0;
    const ch = el.clientHeight || 0;
    if (sh - ch < CONV_SCROLL_EPS || ch < 120) return false;
    if (el === document.scrollingElement) return true;
    const st = window.getComputedStyle(el);
    const oy = st ? st.overflowY : '';
    return oy === 'auto' || oy === 'scroll' || oy === 'overlay';
  }

  // 自身或最近的可滚动祖先（整页滚动亦由 scrollsEl 统一处理，故不会漏）。
  function nearestScroller(el) {
    let sc = el;
    while (sc && !scrollsEl(sc)) sc = sc.parentElement;
    return sc;
  }

  // ---------------------------------------------------------------------------
  // 会话容器定位 —— R0 探针 `inspectConversation` 与 R1 回读 `snapshotConversation` **共用**
  //
  // ★ 共用一份实现是刻意的：容器定位是「远端页改版」风险（方案 §七 R10）的唯一收敛点；
  //   若两处各写一份，改版时必然只修一处 → 回读会静默读错容器。
  //
  // 不依赖 class/id（DeepSeek 前端会改名），只用「可滚动 + 文本量大 + 块数多」多因子打分，
  // 与 findInput / scoredSenders 同范式。
  //
  // 块列表扫描：从给定种子出发向下 2 层，收集「直接子节点 ≥2 且文本量达标」的候选容器，
  // 并按「文本量 + 块数」打分降序返回。
  // ★ 预算**每个种子独立**（QA P1-2）：曾用全局计数 → 首个种子即吃光预算，后续一个都遍历不到
  //   （侧栏、代码块滚动区会抢在前面）。
  function scanLists(seeds, minTextLen, budget) {
    const lists = [];
    let totalExamined = 0;
    for (let s = 0; s < seeds.length && totalExamined < budget; s++) {
      const queue = [{ el: seeds[s], d: 0 }];
      let local = 0;
      while (queue.length && local < 40 && totalExamined < budget) {
        const cur = queue.shift();
        local++;
        totalExamined++;
        const el = cur.el;
        if (el !== document.body && el !== document.documentElement) {
          const bl = blocksOf(el);
          if (bl.childCount >= 2 && bl.textLen >= minTextLen) {
            lists.push({ el: el, bl: bl });
          }
        }
        if (cur.d < 2) {
          const ks = el.children || [];
          for (let i = 0; i < ks.length && i < 20; i++) {
            queue.push({ el: ks[i], d: cur.d + 1 });
          }
        }
      }
    }
    lists.sort(function (a, b) {
      return b.bl.textLen + b.bl.childCount * 40 - (a.bl.textLen + a.bl.childCount * 40);
    });
    return lists;
  }

  // ★ 输入框锚定种子（R1 真机修复）：沿输入框的祖先链上溯 10 层，收集每层上「非本链」的兄弟子树。
  //   消息列表与输入框同处**主内容列** → 必在该集合中；且这条结构关系与「是否滚动」「DOM 多深」
  //   都无关，故不受 locateConversation 第一遍那两个门槛（可滚动 / 文本量 ≥120）影响。
  //   ★ 同列过滤：只收与输入框**水平重叠 ≥50%** 的兄弟 —— 侧栏在主内容列左侧、水平几乎不重叠，
  //     否则侧栏历史列表文本更多会赢下打分，导致回读读到侧栏（换成「静默读错容器」，更危险）。
  //   隐藏态可用性：`findInput()` 走 `isVisible()`（几何判定），面板 hide 只影响 OS 层可见性、
  //     不影响 WebView2 内部布局 —— 真机日志实证隐藏态下注入链路照样取得到输入框。
  function composerSeeds() {
    const seeds = [];
    const seen = [];
    const inp = findInput();
    const ir = inp && inp.getBoundingClientRect ? inp.getBoundingClientRect() : null;
    function sameBand(el) {
      if (!ir || !ir.width) return true; // 无几何信息 → 保守放行（不因缺数据而漏掉列表）
      const r = el.getBoundingClientRect ? el.getBoundingClientRect() : null;
      if (!r || !r.width) return false;
      const ov = Math.min(r.right, ir.right) - Math.max(r.left, ir.left);
      return ov >= Math.min(r.width, ir.width) * 0.5;
    }
    if (inp) {
      let node = inp;
      for (let up = 0; up < 10 && node; up++) {
        const p = node.parentElement;
        if (!p) break;
        const ks = p.children || [];
        for (let i = 0; i < ks.length; i++) {
          const c = ks[i];
          if (c === node || !c || c.nodeType !== 1) continue;
          if (!sameBand(c)) continue;
          if (seen.indexOf(c) >= 0) continue;
          seen.push(c);
          seeds.push(c);
        }
        node = p;
      }
    }
    return seeds.slice(0, 40);
  }

  // 返回 { el, bl, sc, sh, ch, hops, bestEl, scrollables, blockLists, scanPass, candBefore, echoCand }。
  //
  // `need`（非空时）= 我方提示词（紧凑归一）：只接受**尾窗含它**的候选（B3 结构性硬过滤）。
  // 未找到时 el === null；`scanPass` = 命中的遍次（0 = 两遍都没找到）；
  // `candBefore` / `echoCand` = B3 过滤**前 / 后**的候选数（诊断用，见 snapshotConversation 的 error）。
  // ---------------------------------------------------------------------------
  // 紧凑归一：去掉**一切空白**。只用于「容器是否含我方提示词」这一**结构判据** ——
  // 与 Rust 侧 `capture::normalize_text` 同口径（空白差异不影响判定），比 `norm` 更严（连空格都不留）。
  function compactText(s) {
    return String(s == null ? '' : s).replace(/\s+/g, '');
  }

  // ★ R11：把「净化 HTML」（R8 起块载荷的形态）还原成纯文本 —— **只去标签**，空白交给 `compactText`。
  //
  //   为什么必须去标签再比较：回显判据是「块文本是否**包含**我方提示词」。R8 之前块载荷是纯文本，
  //   `compactText(块).indexOf(need)` 成立；R8 之后块载荷是净化 HTML，**标签会插在提示词中间**
  //   打断连续性 —— 例如提示词「请解释下面的内容：正文」被渲染成
  //   `<p>请解释下面的内容：</p><p>正文</p>` 时，`compactText(HTML)` 得到
  //   `请解释下面的内容：</p><p>正文`，**不再包含** `请解释下面的内容：正文`。
  //
  //   真机证据（2026-09-15）：长划词（多行）→ `未接受会话容器（cand=2 echo=0）` → 60s 超时 /
  //   0 次采样 → 卡片空白；而单段短划词正常（标签恰好都在提示词之前）—— 这就是「有时候」的来源。
  function stripTags(s) {
    return String(s == null ? '' : s).replace(/<[^>]*>/g, '');
  }

  // 容器的**尾窗里**是否含我方提示词 —— 即「该容器确实承载了本轮对话」这一**结构事实**。
  // ★ 这是 R1 真机缺陷之二的核心判据（见 locateConversation 的 B3 与 snapshotConversation 的缓存校验）：
  //   「含不含我方刚发出的那段文字」是无法被启发式偏置污染的事实，用它排除「不含提示词的容器」
  //   （侧栏历史、输入区工具栏、欢迎页）比任何打分都可靠。
  //
  // ★ R11：比较前**先去标签**（见 `stripTags` 的说明）—— 否则长文/多行划词会因 HTML 标签
  //   打断提示词的连续性而恒判「不含回显」，把正确容器一并拒掉。
  function htmlPlain(html) {
    // 这里只解码我方生成的净化 HTML；不写入 DOM，兼容站点强制 Trusted Types。
    const named = {amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' '};
    return String(html || '').replace(/<[^>]*>/g, '').replace(/&(#x[0-9a-f]+|#[0-9]+|amp|lt|gt|quot|apos|nbsp);/gi, function(all, key) {
      if (key[0] !== '#') return named[key.toLowerCase()] || all;
      const hex = key[1].toLowerCase() === 'x';
      const cp = parseInt(key.slice(hex ? 2 : 1), hex ? 16 : 10);
      return cp > 0 && cp <= 0x10ffff && !(cp >= 0xd800 && cp <= 0xdfff) ? String.fromCodePoint(cp) : all;
    });
  }
  function tailHasEcho(el, need) {
    if (!need) return true; // 无提示词可依（诊断退化路径）→ 不过滤，行为与旧版一致
    const texts = tailBlocks(el);
    for (let i = 0; i < texts.length; i++) {
      if (compactText(htmlPlain(texts[i].ans)).indexOf(need) >= 0) return true;
    }
    return false;
  }

  function locateConversation(need) {
    const res = {
      el: null,
      bl: null,
      sc: null,
      sh: 0,
      ch: 0,
      hops: 0,
      bestEl: null,
      scrollables: [],
      blockLists: [],
      scanPass: 0,
      candBefore: 0,
      echoCand: 0,
      // ★ 取证字段（2026-09-17 划词回报 cand=0 → 0 字符超时）：页面上到底有没有
      //   「含我方提示词的块」、它装在哪个滚动容器里。它分辨「定位器瞎了」与「回显未落盘」。
      echoAny: false,
      echoHolder: '',
      // ★ 诊断（批次2′）：本次下钻是否走了「择优兜底」（= 命中 ChatGPT 形态的多分支候选）。
      //   0 = 走原「单子承载 ≥90%」路径（DeepSeek）；>0 = 走了择优路径。换容器回归一眼可见。
      viaDrillFallback: 0,
      // ★ 诊断（批次2′ 回归排查）：下钻逐层读数（`h0:cc/tl→cc/tl` / `h0:STOP(原因)`）。
      drillTrace: [],
    };

    // A) 可滚动容器（会话区几乎一定在其中）。`scrollsEl` 已包含「整页滚动」兜底，
    //    故不再单独 push `scrollingElement` —— 否则会与祖先回溯的口径不一致（P2-1）。
    //    按 clientHeight 降序：视口最大的那个最可能是会话区，避免遍历预算被侧栏/代码块吃光。
    const scrollers = [];
    const all = document.querySelectorAll('*');
    for (let i = 0; i < all.length; i++) {
      if (scrollsEl(all[i])) scrollers.push(all[i]);
    }
    scrollers.sort(function (a, b) {
      return (b.clientHeight || 0) - (a.clientHeight || 0);
    });

    res.scrollables = scrollers.slice(0, 6).map(function (el) {
      return {
        tag: el.tagName,
        cls: elCls(el),
        size: elSize(el),
        scrollTop: Math.round(el.scrollTop || 0),
        scrollHeight: el.scrollHeight,
        clientHeight: el.clientHeight,
        childCount: (el.children || []).length,
      };
    });

    // ★ 取证：页面上有没有「含我方提示词」的滚动容器？（回答未被读到但面板明明有回复时，
    //   它＝「定位器把活会话判没了」的铁证。取前 8 个按视口降序的滚动容器抽样。）
    if (need) {
      const cap = Math.min(scrollers.length, 8);
      for (let i = 0; i < cap; i++) {
        if (tailHasEcho(scrollers[i], need)) {
          res.echoAny = true;
          res.echoHolder = elCls(scrollers[i]);
          break;
        }
      }
    }

    // B) 第一遍：从可滚动容器出发（口径与 R0 探针**完全一致**，本遍行为不变）。
    let lists = scanLists(scrollers, CONV_MIN_TEXTLEN, 240);
    res.scanPass = lists.length ? 1 : 0;

    // B2) ★ 兜底第二遍（R1 真机修复）：**短会话 / 全新会话**既不滚动
    //     （scrollHeight-clientHeight < CONV_SCROLL_EPS）总文本量也 < CONV_MIN_TEXTLEN，
    //     于是消息列表既不是「可滚动容器」、也过不了文本下限；而侧栏（`ds-virtual-list`）
    //     反而是页面上唯一的滚动容器 → 第一遍**必然**找不到消息列表 →
    //     回读恒报「未定位到会话容器」（真机缺陷 2026-09-14）。
    //     第二遍改以输入框为锚（见 composerSeeds），绕开这两个门槛。
    if (!lists.length) {
      lists = scanLists(composerSeeds(), CONV_MIN_TEXTLEN_RELAXED, 200);
      res.scanPass = lists.length ? 2 : 0;
    }

    // B3) ★★ 结构性硬过滤（R1 真机缺陷之二，2026-09-14）：只接受**尾窗里确实含我方提示词**的候选。
    //
    //   真机证据：pass=2 兜底曾选中一个「末块含提示词、其后紧跟装饰页脚」的容器 →
    //   回读把**页脚**（「深度思考智能搜索内容由 AI 生成，请仔细甄别」，23 字符）当回答交付，
    //   且被判定为 complete=true —— 假通过（W-V3/W-V4 同时失守）。
    //
    //   判据用**事实**而非打分：我方刚发出的那段文字是否出现在该容器的尾窗里。
    //   不含 → 该容器与本轮对话无关（侧栏历史 / 输入区工具栏 / 欢迎页）→ 直接排除。
    //   ★ 取舍：宁可不交付（走超时告警，W-V5 不静默）也**绝不交付错内容** ——
    //     而且过滤掉错误候选后，循环会继续等，一旦回显落盘即自然命中正确容器。
    const lists_before_b3 = lists; // ★ 保留 B3 前的原始候选，供 B3-B 结构兜底回退（见下）。
    res.candBefore = lists.length;
    if (need) {
      lists = lists.filter(function (l) {
        return tailHasEcho(l.el, need);
      });
    }
    res.echoCand = lists.length;

    // B3-B) ★★ 结构兜底（2026-09-17 划词回报「面板有回复、回读 0 字符 / 卡满 60s 超时」）：
    //   B3 的「含提示词」硬门槛在**长会话虚拟化**下会误杀活会话 —— virtua 会把视口上方的
    //   用户回显块**卸载出 DOM**（于是 echoAny=0 / echo=0），而答案块仍挂在底部。
    //   此时「输入框存在」是活会话的**最强结构信号**（输入框永不卸载）：不再强求提示词文本。
    //   锚点优先取 **pre-B3 候选里包含输入框的那个容器** —— 它已通过 scanLists 的 struct 门槛，
    //   且「含输入框」天然排除侧栏，是最可靠的可滚动会话容器；次选输入框最近可滚动祖先。
    //   ★ 不禁 documentElement：DeepSeek 新布局里 composer 常独立于虚拟列表，其最近可滚动祖先
    //     就是整页（document.scrollingElement）—— 上一版把它排除，恰好让本兜底在真实失败场景
    //     失效、整轮干等满 60s（#922）。故此处只禁 body。
    //   ★ 提取答案无需提示词 —— Rust 侧 `locate_answer_and_thinking` 对「提示词不在块列表」
    //     有 `texts.last()` 兜底（readback.rs L484）→ 读到末块即答案；页脚/装饰由既有否决逻辑把关。
    if (!lists.length && need && findInput()) {
      const inp = findInput();
      let near = null;
      for (const c of lists_before_b3) {
        if (c.el.contains(inp)) { near = c.el; break; }
      }
      if (!near) near = nearestScroller(inp);
      if (near && near !== document.body) {
        const bl = blocksOf(near);
        if (bl.childCount >= 1 && bl.textLen >= CONV_MIN_TEXTLEN_RELAXED) {
          lists.push({ el: near, bl: bl });
          res.scanPass = res.scanPass || 3;
        }
      }
    }

    // ★★ 2026-09-20（首轮回流不全）：候选按「块文本量」降序后再取最大者。
    //   根因：全新会话里 B2 兜底（composerSeeds）会把**回显所在的 168px 小折叠容器**
    //   （ds-collapsible-text，只含一句提示词）送进 lists，且因 `tailHasEcho` 通过而留在 B3
    //   之后 —— 旧代码取 lists[0]，于是容器锁死在小题块上；而答案在**外层真正的消息列表**
    //   里生成 ⇒ 回读盯着的容器永不出现新 ds-markdown ⇒ done 恒 false ⇒ 干等到被新划词取代
    //   （真机 `#41/#71`）。消息列表的块文本量必然 ≥ 末轮回显小题块，故按 textLen 降序可
    //   稳定指向真正的会话容器，同时保留诊断读数。
    lists.sort(function (a, b) {
      return (b.bl.textLen || 0) - (a.bl.textLen || 0);
    });
    res.blockLists = lists.slice(0, 5).map(function (l) {
      return {
        tag: l.el.tagName,
        cls: elCls(l.el),
        size: elSize(l.el),
        childCount: l.bl.childCount,
        textLen: l.bl.textLen,
      };
    });

    const best = lists[0];
    if (!best) return res;

    // 所属的滚动容器（自身或最近的可滚动祖先）→ 用于算虚拟化比 / 定位读数。
    // ★ 先算（不再等下钻后）：下面下钻要用 `sh` 做「只下钻进高跨度容器」的护栏。
    const sc = nearestScroller(best.el);
    const sh = sc ? sc.scrollHeight || 0 : 0;
    const ch = sc ? sc.clientHeight || 0 : 0;

    // ★ 下钻「包装层」（2026-09-14 真机后补）：若某个直接子节点承载了本候选 ≥90% 的文本，
    //   说明本候选是**包装层**而非消息列表（真机实例：DeepSeek 的
    //   `DIV.ds-virtual-list-visible-items`，本身高 2495 而容器高仅 540）→ 下钻到该子节点。
    //   否则 virtRatio / 尾窗读数会算在 spacer / 包装层上，得到无意义的读数。
    //   ★ 护栏（QA P2-2）：目标须是**高跨度**容器（高 ≥ 0.5×scrollHeight）—— 防止会话高度偏斜
    //     （某一条超长回答独占 ≥90% 文本）时**下钻进单条消息内部**，把条目误算成段落的子块。
    //
    // ★★ 择优兜底下钻（2026-10-03 · 批次2′，修 ChatGPT 站点「回读交付 UI 外壳」）：
    //   真机证据（pid=3228，chatgpt.com）：`container=@w-sm/main:[…] pass=1 hops=0`，回读交付
    //   2323 字符**混入 UI 外壳**（`<h4>你说：</h4>`）与用户回显。骨架 dump（#78）显示选中的是
    //   整个 `<main>`（`h:548`）而非消息列表。
    //   根因：**上面这条「单子承载 ≥90%」判据隐含假设「候选是单链包装层」**——该假设在 DeepSeek
    //   成立（`容器 → ds-virtual-list → visible-items → 消息列表` 逐层单子），但在 ChatGPT **不成立**：
    //   `<main>` 的直接子节点是**多分支**（导航 / 消息区 / composer），文本**分散**，
    //   **无任一子节点 ≥90%** ⇒ 内层循环一个都不选 ⇒ `dom=null` ⇒ `hops=0` ⇒ pickEl 停在 `<main>`
    //   ⇒ 尾窗块 = `<main>` 的直接子块（= 整个页面区域）⇒ 交付内容混入 UI 外壳。
    //   修法：原判据（①）**一字不改**；仅在其落空时**追加**一条「择优」路径（②）——
    //   在同带候选里挑**最像消息列表**的直接子节点下钻。判据（缺一不可）：
    //     a) 自身是块列表：`childCount ≥ 2` 且 `withText ≥ 2`（排除单条消息 / 空壳）；
    //     b) **与输入框同带**（水平重叠 ≥50%，复用 composerSeeds.sameBand 的语义）——**主护栏**，
    //        排除侧栏/导航（它们与输入框水平几乎不重叠）；输入框不可用时保守放行；
    //     c) 同带候选内取 `textLen` 最大者 —— **仅作排序，不作绝对门槛**。
    //        （曾试「≥ 父的 50%」当门槛：侧栏文本 410 vs 消息区 99 时消息区被误拒 ⇒ 该门槛已废弃。）
    //   ★ 不回归保证：DeepSeek 的单子承载 ≥90% **恒命中 ①** ⇒ 永不进入 ② ⇒ 行为逐位不变。
    //     已用 jsdom 探针验证：DeepSeek 形态 `viaFallback=false`；单条超长消息（`withText=1`）不下钻。
    let pickEl = best.el;
    let pickBl = best.bl;
    let hops = 0;
    let viaFallback = 0; // 诊断：>0 表示本次下钻走了「择优兜底」路径（即命中了 ChatGPT 形态）
    const drillTrace = []; // 诊断：每层一条 `起点cc/tl → 结果(选中|停因)`
    // 下钻用的「同带」判据：候选与输入框水平重叠 ≥50%。输入框不可用 → 保守放行（不因缺数据而漏）。
    const drillInput = findInput();
    const drillIr = drillInput && drillInput.getBoundingClientRect ? drillInput.getBoundingClientRect() : null;
    function inInputBand(el) {
      if (!drillIr || !drillIr.width) return true;
      const r = el.getBoundingClientRect ? el.getBoundingClientRect() : null;
      // ★ 无几何信息 → **保守放行**（与 `composerSeeds.sameBand` 同语义）。
      //   真机教训（批次2′ 回归）：ChatGPT 的候选是 `display:contents` 包装层，`getBoundingClientRect()`
      //   恒为 `{w:0,h:0}` —— 旧写法在此 `return false`，把「无法判定」误当「不在同带」⇒
      //   唯一合格候选被护栏拦掉 ⇒ 兜底路径整体落空（真机 `drill=0`，回读超时 0 字符）。
      if (!r || !r.width) return true;
      const ov = Math.min(r.right, drillIr.right) - Math.max(r.left, drillIr.left);
      return ov >= Math.min(r.width, drillIr.width) * 0.5;
    }
    // ★★ 2026-10-03 三次真机修复：`isMsgListLike(el, bl)` —— 判定「这是真消息列表」。
    //   旧判据只有 `cc≥2 && withText≥2`，**太弱**：ChatGPT 的 `composer-parent`
    //   （消息区 + composer 的父级）也满足 —— composer 里若有我方**残留输入文本**，
    //   它的 `withText` 就 ≥2 ⇒ 被误判为消息列表 ⇒ 下钻提前终止在 `composer-parent` 上
    //   ⇒ 尾窗块混入 composer / 以回显开头 ⇒ 交付 0 字符（真机 `[L]` 停错层）。
    //   关键区分：**消息列表绝不含输入框本身**（输入框在它下方/外部）；而 `composer-parent`
    //   必然 `contains(input)`。故据此排除。
    //   ★ 该判据**下钻终止判定与缓存校验共用** —— 避免「下钻对了但缓存校验放行了错容器」。
    function isMsgListLike(el, bl) {
      if (!(bl.childCount >= 2 && bl.withText >= 2)) return false;
      if (drillInput && el.contains && el.contains(drillInput)) return false;
      // ★★ 六次真机（A14）：尾块是装饰条（免责页脚）⟹ 本层是「消息区 + 页脚」合体层，
      //   **不是**消息列表（真消息列表的尾块是最后一条消息，不是页脚）。
      if (tailIsDecor(bl)) return false;
      return true;
    }
    // ★★ 2026-10-03 四次真机修复（`chain=` 取数确证）：**剔除 composer 的文本量基准**。
    //
    //   真机数据（chatgpt.com · `#108/#110` chain=）：
    //     `self=DIV.composer-parent fl[2/480]`，`down=DIV.relative basis-aut[1/93] , DIV.sticky bottom-0 z-[4/387]`
    //     ⇒ **消息区只有 93 字符**（助手回答尚未渲染 / 懒加载），而 **composer（sticky）有 387 字符**
    //       （我方刚填入的提示词**残留在输入框**里）。
    //     `trace=h0:3/496→1/480[fb], …, h4:1/480→2/480, h5:STOP(no-cand n=2 fbtl=480)`
    //
    //   根因（三条，缺一不成）：
    //     ① composer 的**残留输入文本**（387）**远大于**消息区文本（93）；
    //     ② 于是 `composer-parent`（cc=2）被当作"中转层"采纳（出口二 `cc<2` 不触发）；
    //     ③ 下一轮在它内部选子节点时，`msgArea`(93) 与 `composer`(387) 都低于
    //        `480×0.9=432` 的门槛 ⟹ `no-cand` ⟹ 停死在 `composer-parent` ⟹
    //        尾窗块 = `<h4>你说：</h4>`（回显）⟹ 交付 0 字符。
    //
    //   修法（两条，缺一不可）：
    //     ① **排除含输入框的子节点**（composer 侧）—— 它永远不是"下钻目标"；
    //     ② 基准改用**剔除输入框后的文本量**（`bandTl`）—— 否则 composer 的残留文本
    //        会把门槛抬到消息区永远过不了的高度（93 vs 432）。
    //   ★ 不回归：DeepSeek 的候选/子节点**不含输入框**（composer 独立于虚拟列表）⟹
    //     `bandTl === pickBl.textLen`、排除项为空 ⟹ 判据逐位等价于原式。
    //   ★★ 2026-10-03 五次真机修复（`#74` 回归：`bandTextLen` 削过头，钻进顶部工具栏）：
    //   真机数据（同一次运行，另一轮，`#74/#76`）：
    //     `self=HEADER.draggable no-draggable-children sticky top-0[3/16]`
    //     `chain=up=DIV.@container/main re[2/496] < DIV.@w-sm/main:[scroll[3/496]`
    //           ` < DIV.contents[1/16] < HEADER.draggable[3/16]`
    //     `trace=h0:3/496→1/16[fb],h1:1/16→3/16[fb][L]` ⟹ 骨架 `[]`、尾窗「(空)」⟹ 交付 0 字符。
    //   根因：`@w-sm/main`（`cc=3 tl=496`）的直接子 = `div.contents`（**只含顶部 HEADER**，`cc=1 tl=16`）
    //     + `MAIN.not-keyboard-focus`（**含消息区 + composer 的整棵对话树**，`tl=480`）。
    //     **输入框在 `MAIN` 内部深处** ⟹ 旧 `bandTextLen` 把 `MAIN`(480) 整棵削掉 ⟹
    //     `bandTl = 496 − 480 = 16` ⟹ 本该被门槛 446 排除的 `div.contents`(16)
    //     **恰好满足 `isWrapper`**（`cc==1 && 16 ≥ 16×0.9`）⟹ **抢先选中** ⟹ 钻进 HEADER。
    //
    //   ★★ 收紧判据（**与 `isWrapper` 对偶**，物理含义自洽）：
    //     「含输入框的子节点」中，**只有当它 `tl < 父 × 0.9`（即它是**兄弟并列**的 composer 侧，
    //       父里还有别的东西）时**才削 / 才排除**；
    //     若它 `tl ≥ 父 × 0.9`（**独子包装层** —— 父里几乎只有它），则**不削、不排除**，
    //       与 `isWrapper` 一样**照常下钻**（哪怕输入框在它内部深处）。
    //     - `#108` `composer`(387) vs 父 `composer-parent`(480)：387/480 = 80.6% < 90%
    //       ⟹ 兄弟并列的 composer 侧 ⟹ **削** ⟹ `bandTl = 480 − 387 = 93` ⟹
    //       消息区(93) ≥ 83.7 ⟹ **可选中**（正是要治的缺陷）✓
    //     - `#74` `MAIN.not-keyboard-focus`(480) vs 父 `@w-sm/main`(496)：480/496 = 96.8% ≥ 90%
    //       ⟹ 独子包装层 ⟹ **不削、不排除** ⟹ `bandTl = 496` ⟹ 门槛 446 ⟹
    //       `MAIN`(480) 胜出、`contents`(16) 被排除 ⟹ **不再钻 header** ✓
    //     - 中间各层（`MAIN → contents → group/thread → composer-parent`，皆独子 480，
    //       父也 480）：占比 100% ≥ 90% ⟹ 不削 ⟹ 门槛 432 ⟹ 单子链**照常下钻**（与修复前一致）✓
    //   ★ 不回归：DeepSeek 的候选/子节点**不含输入框** ⟹ `cut=0`、
    //     `bandTl === pickBl.textLen`、排除项为空 ⟹ 判据逐位等价于原式。
    const BAND_CUT_RATIO = 0.9; // 「含输入框的子节点是否**兄弟并列**（而非独子包装层）」的分界
    // ★ 「该子节点是**兄弟并列的 composer 侧**」= 含输入框 **且** 文本量**不到父的 90%**。
    //   用于两处：① `bandTextLen` 决定能否削；② 两个候选循环决定能否排除。
    //   ★ 必须同时满足两个条件（五次真机 `#74` 教训）：
    //     - 只按「含输入框」排除 ⟹ 会把 `MAIN`（含整棵对话树）一并排除 ⟹ 误选页眉；
    //     - 只看占比不看「含输入框」⟹ 任意「矮子节点」都会被排除 ⟹ 过度收敛。
    function isComposerSibling(c, bl) {
      if (!drillInput || !c || !(c.contains && c.contains(drillInput))) return false;
      const ctl = blocksOf(c).textLen;
      return ctl > 0 && bl.textLen > 0 && ctl < bl.textLen * BAND_CUT_RATIO;
    }
    function bandTextLen(el, bl) {
      if (!drillInput || !el.children) return bl.textLen;
      let cut = 0;
      for (let i = 0; i < el.children.length; i++) {
        const c = el.children[i];
        // ★ 仅削「兄弟并列的 composer 侧」（见上方 `#74` 教训）：独子包装层**不削**，
        //   否则它下面（深处）的对话内容会被一并抹平 ⟹ 基准被削成「只含页眉的小 wrapper」的量级
        //   ⟹ 误选页眉（五次真机回归）。
        if (isComposerSibling(c, bl)) cut += blocksOf(c).textLen;
      }
      return Math.max(0, bl.textLen - cut);
    }
    // ★ hop 上限 12（3 → 4 → 8 → 12）：DeepSeek 在 3 层内必到底（多跑空轮由下方两个 `break` 兜住，无害）；
    //   ChatGPT 的包装层链更深 —— 真机实测 `root → msgArea → grow → w0 → w1 → w2 → msgList`
    //   已达 6 层，且站点改版可能再加深。上限取 12：覆盖真机 + 充分改版余量。
    //   ★ 上限提高**不会**改变 DeepSeek 行为：它每轮都在第 3~4 轮就 `break`
    //     （命中原判据①到达 list，或 `db.childCount < 2`），根本用不到后面。
    for (let hop = 0; hop < 12; hop++) {
      const ks = pickEl.children || [];
      let dom = null;
      let reachedList = 0; // 诊断：本层选中的子节点是否为「真消息列表」（≥2 个有文本子块）
      // ★ 本层「基准文本量」= 剔除输入框所在子节点后的文本量（见 bandTextLen 注释）：
      //   composer 的**残留输入文本**会把门槛抬高到消息区永远过不了的高度。
      //   DeepSeek 的 pickEl 不含输入框 ⟹ bandTl === pickBl.textLen ⟹ 逐位等价于原式（不回归）。
      const bandTl = bandTextLen(pickEl, pickBl);
      for (let i = 0; i < ks.length && i < 20; i++) {
        // ★ 排除「兄弟并列的 composer 侧」子节点（文本几乎不到父的 90% 且含输入框）——
        //   它永远不是消息列表，也不是下钻目标。★ 判据必须**双条件**（五次真机 `#74` 教训）：
        //   `MAIN.not-keyboard-focus`（含整棵对话树 + 深藏其中的输入框，占比 96.8%）**不得**排除，
        //   否则 `@w-sm/main` 层只剩「只含页眉的 `div.contents`(16)」可选 ⇒ 钻进 HEADER。
        //   DeepSeek 无此情形（子节点不含输入框）⟹ 该 `continue` 永不触发（不回归）。
        if (isComposerSibling(ks[i], pickBl)) continue;
        // ★ 用 `blocksOf` 的口径（子块文本长之和 / 最大子块文本长）替代裸 textContent：
        //   护栏要看的是「这个子节点**自己**是不是块列表」，光有文本总长不够。
        const kb = blocksOf(ks[i]);
        if (kb.textLen < bandTl * 0.9) continue;
        const kh = ks[i].getBoundingClientRect ? ks[i].getBoundingClientRect().height : 0;
        // 护栏：目标必须是**列表包装层**而非单条消息 —— 两种证据任一成立即可：
        //   ① 高跨度（高 ≥ 0.5×scrollHeight）；
        //   ② 该子节点自身仍是块列表：子块 ≥2 **且** 文本分散（最大子块 < 自身 90%）。
        //   ★ ② 是 2026-09-15 真机缺陷的关键补充：`ds-virtual-list-visible-items` 高 9050
        //     而 scrollHeight 19000+，单靠 ① 会被放行不下钻。
        //   ★★ 六次真机（A14 · 定向放宽）：追加「**尾块是装饰条**」这一条证据 ——
        //     ChatGPT 的 `flex min-h-0 grow`（消息区 + 页脚合体层，高 91）**既不高跨度**
        //     （91 < scrollHeight×0.5），**也不满足文本分散**（回答占比高时
        //     `maxTextLen ≥ textLen×0.9`）⟹ 原 ② 判据会把它拦下 ⟹ 无法继续下钻到 `qMYqUG`。
        //     而「尾块是装饰条」恰好是「这是**包装层**而非单条消息」的**独立证据**
        //     （单条消息的尾块不会是免责页脚）⟹ 放行它继续下钻，下一层即真内容容器。
        //   ★ 定向性（与 A13 的全局放宽区别）：**只有当尾块是装饰时才放宽**；
        //     尾块非装饰的普通层**逐位保持原判据** `maxTextLen < textLen×0.9` ⟹ 回归面最小。
        //   ★ 不回归：DeepSeek 的候选层尾块是最后一条消息（非装饰）⟹ 本项恒不触发 ⟹ 判据等价原式。
        const spread =
          kb.childCount >= 2 &&
          (kb.maxTextLen < kb.textLen * 0.9 || tailIsDecor(kb));
        if (sh > 0 && kh < sh * 0.5 && !spread) continue;
        dom = ks[i];
        break;
      }
      if (!dom) {
        // ② 择优兜底（见函数头注释「择优兜底下钻」）。两条准入判据（任一成立，缺其余不可）：
        //   a) **块列表**：`childCount ≥ 2` 且 `withText ≥ 2`（≥2 个有文本子块 —— 消息列表）；
        //   b) **单子包装层**：`childCount === 1` 且 `textLen ≥ 父的 90%` —— ChatGPT 的中间包装层正是
        //      这种「矮（< 0.5×scrollHeight）且单子」的形态：原判据①因高度护栏放行不了，②又要求
        //      `childCount ≥ 2`，两个都不满足 ⇒ 卡住（真机 `hops=0`）。
        //   共同护栏：**与输入框同带**（主护栏，排除侧栏/导航）；同带候选内取 `textLen` 最大者（仅排序）。
        //
        //   ★★ 2026-10-03 二次真机修复（方案A：加「终止信号」，治「钻过头/钻不到」）：
        //   真机证据（pid=44892，chatgpt.com）修复 `inInputBand` 后仍失败：
        //     `hops=4 drill=2 trace=h0:3/495→1/479[fb],h1:1/479→…,h3:1/479→1/479`，交付 0 字符。
        //   根因：`isWrapper` 判据（`childCount===1 && textLen ≥ 父×0.9`）在**单子包装层上恒真**
        //     （子块文本量恒等于父）⇒ 每层无脑下钻**唯一的那个子节点**，且 `hop` 上限 4 小于
        //     ChatGPT 的实际包装深度（≥6）⇒ 停在**空包装壳中段**，尾窗块 = 用户回显
        //     （`<h4>你说：</h4>`）⇒ 永远等不到回答 ⇒ 120s 超时 0 字符。
        //   修法：**优先找「真消息列表」**（`isList`：`childCount ≥ 2 && withText ≥ 2`）——
        //     命中即**终止下钻**（`reachedList=1`，见循环末尾）；只有**无 list 可选**时，才用
        //     `isWrapper` 作**中转**（继续下一轮）。这样「钻到消息列表就停」，不再钻过头、
        //     也不会因 hop 用尽停在半路的包装壳上。
        //   ★ 排序：list 优先于 wrapper（list 命中即停，不参与比较）；同类型内取 `textLen` 最大者。
        //
        //   ★★ 2026-10-03 三次真机修复（方案A-2：`isList` 太弱，误把 composer 父级当消息列表）：
        //   真机证据（chatgpt.com，两轮）：
        //     `hops=4 drill=3 trace=h0:3/1415→1/1399[fb],h1:…,h3:1/1399→2/1399[fb][L]`
        //     骨架 `cls:"composer-parent flex fle", cc:2`，
        //     out=[relative basis-auto(消息区, h=1885), sticky bottom-0(composer, h=248, n=66)]。
        //   ⇒ `[L]` 确实触发了，但停在 **`composer-parent`（消息区 + composer 的父级）** ——
        //     它 `cc=2 && withText≥2`（msgArea 有文本 + **composer 里有我方残留输入文本**）
        //     ⇒ 满足旧 `isList` 判据 ⇒ 被误判为「消息列表」⇒ 提前终止 ⇒ 尾窗块仍以
        //     `<h4>你说：</h4>` 开头 ⇒ 交付 0 字符。
        //   修法：`isList` 必须**排除「包含输入框本身」的容器**（那是 composer 的祖先，
        //     消息列表绝不含输入框）。新增 `isMsgListLike()` 统一该判据（下钻与缓存校验共用）。
        let cand = null;
        let candLen = 0;
        let candIsList = false;
        let why = 'no-cand'; // 诊断：停因（cand 为空时）
        for (let i = 0; i < ks.length && i < 20; i++) {
          // ★★ 2026-10-03 四次真机修复：**排除「就是一个 composer」的子节点**（composer 侧）。
          //   真机 `chain=`：`composer-parent` 的子 = 消息区(93) + composer(387)；
          //   composer 的子节点含输入框（我方残留输入文本所在子树）—— 它永远不是
          //   消息列表、也不是下钻目标。若不排除，它既可能被 `isWrapper` 误选（单子包装壳），
          //   又会用 387 的文本量**抢走候选排序**。DeepSeek 子节点不含输入框 ⟹ 该 `continue` 永不触发。
          //   ★★ 五次真机收紧：判据用**双条件**（`isComposerSibling`：含输入框 **且** 不到父的 90%）——
          //   `MAIN.not-keyboard-focus`（含整棵对话树）**不得**排除（`#74` 教训）。
          if (isComposerSibling(ks[i], pickBl)) continue;
          const kb = blocksOf(ks[i]);
          const isList = isMsgListLike(ks[i], kb); // ★ 强判据：排除含输入框的容器（composer 祖先）
          // ★★ 2026-10-03 四次真机修复：`isWrapper` 基准改用 `bandTl`（剔除输入框后的父文本量）。
          //   原用 `pickBl.textLen`（含 composer 残留 387）⟹ 门槛 = 387×0.9 = 348 ⟹
          //   消息区(93) 永远过不了 ⟹ `no-cand` 卡死在 `composer-parent`。
          //   改用 `bandTl`（= 480 − 387 = 93）⟹ 消息区(93) 恰好吃到门槛 ⟹ 可被采纳 ⟹ 继续下钻。
          //   ★ 不回归：DeepSeek 无输入框子节点 ⟹ `bandTl === pickBl.textLen` ⟹ 逐位等价原式。
          const isWrapper = kb.childCount === 1 && kb.textLen >= bandTl * 0.9;
          if (!isList && !isWrapper) continue;
          if (kb.textLen <= 0) continue;
          if (!inInputBand(ks[i])) {
            why = 'band-out'; // 有形状合格的子节点，但被「同带」护栏拦下
            continue;
          }
          // ★ list 绝对优先：一旦有 list 候选，wrapper 不再参与竞争（避免"单子中转层文本量更大"时抢走）。
          if (isList && !candIsList) {
            cand = ks[i];
            candLen = kb.textLen;
            candIsList = true;
            continue;
          }
          if (isList === candIsList && kb.textLen > candLen) {
            cand = ks[i];
            candLen = kb.textLen;
          }
        }
        if (cand) {
          dom = cand;
          viaFallback++;
        } else {
          drillTrace.push('h' + hop + ':STOP(' + why + ' n=' + ks.length + ' fbtl=' + bandTl + ')');
        }
      }
      if (!dom) break;
      const db = blocksOf(dom);
      // ★ 出口一判定（方案A 核心）：**无论 dom 来自原判据① 还是兜底②**，只要它本身是
      //   「真消息列表」形态即视为已到达终点。
      //   ★ 必须在此统一判定，不能只在兜底里设：到达 list 常由原判据①（`spread`）命中，
      //     若只在兜底设 `reachedList`，这条路径就不会触发出口一（漏判）。
      //   ★★ 判据必须用 `isMsgListLike`（含「不含输入框」护栏）—— 只用 `cc≥2 && withText≥2`
      //     会把 `composer-parent`（消息区 + composer）误判为消息列表（三次真机缺陷）。
      if (isMsgListLike(dom, db)) reachedList = 1;
      // ★ 单子包装层**不是终点**（2026-09-15 真机根因）：DeepSeek 的 `ds-virtual-list-items`
      //   只有 `ds-virtual-list-visible-items` **一个**子节点 —— 旧代码在这里 `break`，
      //   于是 pickEl 停在外层 `ds-virtual-list`，尾窗块 = 「整段对话」+「免责页脚」，
      //   「回显块的后一块」取到的正是**页脚**（回读交付「内容由 AI 生成，请仔细甄别」，15 字符）。
      //   恰好一个子节点时**继续往下钻一层** —— 下一轮会落进真正的消息列表。
      // ★ 诊断：记录本层「起点 → 选中」读数（含选中来源：原① 还是兜底②、是否到达 list）。
      drillTrace.push(
        'h' + hop + ':' + pickBl.childCount + '/' + pickBl.textLen + '→' +
        db.childCount + '/' + db.textLen +
        (viaFallback > 0 && hops < viaFallback + 1 ? '[fb]' : '') +
        (reachedList ? '[L]' : '')
      );
      // ★ 出口一（方案A 核心）：兜底命中了「真消息列表」⟹ **立即采纳并终止**，不再往下钻。
      //   理由：再钻一层必然进入**第一条消息内部**（`msgList` 的子块只有 ≥2 时才被选为 list，
      //   继续下钻会把单条消息当成新容器），尾窗块会退化成单条消息 ⇒ 交付不全。
      if (reachedList) {
        pickEl = dom;
        pickBl = db;
        hops = hop + 1;
        break;
      }
      // ★ 出口二（原行为，逐位保留）：非「可继续下钻」形态 ⟹ **不采纳** dom，保持在外层。
      if (db.childCount < 2 && !(dom.children && dom.children.length === 1)) break;
      pickEl = dom;
      pickBl = db;
      hops = hop + 1;
    }
    res.viaDrillFallback = viaFallback;
    // ★ 下钻诊断（批次2′ 回归排查，2026-10-03）：把下钻每一层的「起点 / 选中 / 为何停」留成读数，
    //   一次真机运行即可判定卡在哪一条判据（原① 还是兜底②）—— 避免再靠推理。
    res.drillTrace = drillTrace;

    res.el = pickEl;
    res.bl = pickBl;
    res.sc = sc;
    res.sh = sh;
    res.ch = ch;
    res.hops = hops;
    res.bestEl = best.el;
    return res;
  }

  function inspectConversation() {
    try {
      const out = {
        url: location.href,
        viewport: (window.innerWidth || 0) + 'x' + (window.innerHeight || 0),
        dpr: window.devicePixelRatio || 1,
      };

      // 容器定位走**共用**实现（与 R1 的 snapshotConversation 同一份，见上方 locateConversation）。
      const loc = locateConversation();
      out.scrollables = loc.scrollables;
      out.blockLists = loc.blockLists;
      if (!loc.el) {
        out.pick = null;
        out.note = '未找到候选块列表（会话可能尚未渲染，或本页不是会话页）';
        return out;
      }
      const pickEl = loc.el;
      const pickBl = loc.bl;
      const sc = loc.sc;
      const sh = loc.sh;
      const ch = loc.ch;
      const hops = loc.hops;
      const bestEl = loc.bestEl;

      // ★ 结构性虚拟化信号（**主判据**，不依赖高度估算）：在「滚动容器 sc …→ pickEl」这条链上，
      //   若存在一层 E 满足「E 高 ≥ 0.9×scrollHeight、直接子节点 1..8 个、各子高之和 < 0.5×E 高」，
      //   即为 spacer / 撑高层 —— react-window / virtua 的典型结构。
      //   （真机实例：`ds-virtual-list-visible-items` 高 2495，而它的两个子块高之和仅 ~176。）
      //   与 virtRatio 互补：后者是**高度估算**，块高偏斜时会系统性偏低（非虚拟化容器也可能 <0.5）；
      //   本信号是**结构事实**，不受块高分布影响，故可单独定论（见方案 §12.6 与 QA P1-1）。
      const virtStruct = (function () {
        const o = { hit: false, tag: null, cls: null, h: 0, kids: 0, kidH: 0 };
        let node = pickEl;
        for (let up = 0; up < 5 && node; up++) {
          const ks2 = node.children || [];
          let sum = 0;
          for (let i = 0; i < ks2.length; i++) {
            sum += ks2[i].getBoundingClientRect ? ks2[i].getBoundingClientRect().height : 0;
          }
          const hh = node.getBoundingClientRect ? node.getBoundingClientRect().height : 0;
          if (sh > 0 && ks2.length >= 1 && ks2.length <= 8 && hh >= sh * 0.9 && sum < hh * 0.5) {
            o.hit = true;
            o.tag = node.tagName;
            o.cls = elCls(node);
            o.h = Math.round(hh);
            o.kids = ks2.length;
            o.kidH = Math.round(sum);
            return o;
          }
          node = node.parentElement;
        }
        return o;
      })();

      // ★ 虚拟化判定（**条目数**基准，见文件上方说明）：sizer（撑高块）先被排除，再算
      //   expectedItems = scrollHeight / medianItemHeight，virtRatio = renderedItems / expectedItems。
      const itemH = [];
      for (let i = 0; i < pickBl.itemHeights.length; i++) {
        if (ch > 0 && pickBl.itemHeights[i] > ch * CONV_SIZER_MAX) continue; // 撑高块，不算条目
        itemH.push(pickBl.itemHeights[i]);
      }
      const medianItemH = median(itemH);
      const expectedItems = medianItemH > 0 ? Math.round(sh / medianItemH) : 0;
      const virtRatio =
        expectedItems > 0 ? Math.round((itemH.length / expectedItems) * 1000) / 1000 : null;

      // 诊断用：直接子节点的「高/文本长」序列 —— 一眼可辨 spacer 与包装层。
      const kids = pickEl.children || [];
      const childHT = [];
      for (let i = 0; i < kids.length && i < 10; i++) {
        const k = kids[i];
        const kh = k.getBoundingClientRect ? k.getBoundingClientRect().height : 0;
        const kt = norm((k.textContent || '').slice(0, 2000));
        childHT.push(Math.round(kh) + '/' + kt.length);
      }

      out.pick = {
        tag: pickEl.tagName,
        cls: elCls(pickEl),
        size: elSize(pickEl),
        hoppedFrom: pickEl === bestEl ? null : elCls(bestEl), // 下钻前的包装层（诊断用）
        childCount: pickBl.childCount,
        withText: pickBl.withText,
        textLen: pickBl.textLen,
        maxTextLen: pickBl.maxTextLen,
        // —— 虚拟化三要素（人工可交叉核对）——
        renderedItems: itemH.length,
        medianItemHeight: medianItemH,
        expectedItems: expectedItems,
        // ≈1 → 非虚拟化（全部条目常驻 DOM）；<<1 → 只渲染可视区 → 虚拟化
        virtRatio: virtRatio,
        // ★ 结构性虚拟化信号（主判据）＋ 下钻深度（供人工剔除误下钻）
        virtStruct: virtStruct,
        hops: hops,
        childHT: childHT,
        scrollTag: sc ? sc.tagName : null,
        scrollTop: sc ? Math.round(sc.scrollTop || 0) : null,
        scrollHeight: sh,
        clientHeight: sc ? ch : null,
        firstTexts: pickBl.firstTexts,
        lastTexts: pickBl.lastTexts,
        // P1/P2 性能记忆化命中率（审计报告 §5 第 2 批）：稳定块越多越接近 1。
        splitCache: splitCacheStats(),
      };
      out.note =
        '判据（见方案 §12.6）：virtStruct.hit=true → 直接判虚拟化（结构事实，与是否滚动无关）；否则看滚动后的 Δ，并把 virtRatio 作提示（该比值系统性偏低，不单独定论）';
      return out;
    } catch (e) {
      return { error: String((e && e.message) || e) };
    }
  }

  // ---------------------------------------------------------------------------
  // 「结果回流」R1 回读桥的数据源（设计文档 §5.1 / §12.6）—— **纯观测**
  //
  // ★ 定位策略由 R0 真机结论**决定**（§12.6：DeepSeek 会话列表是虚拟化实现 → 风险 R3 触发）：
  //   虚拟化只会卸载**整条消息**，而新回答永远在列表底部、必然处于渲染窗口内
  //   ⇒ 直接读「**尾窗块**」即可，**不需要**「整段会话前缀差集」（该路线已被否掉）。
  //   「本轮回答」的最终定位在 Rust 侧完成（`readback::locate_answer`），本函数只如实回传尾窗。
  //
  // 长度闸门（风险 R7）：单块 ≤ CONV_SNAP_BLOCK 字符、尾窗 ≤ CONV_SNAP_TAIL 块、
  //   块内遍历提前收敛（读到 cap 即停）→ 避免 eval 回传爆量。
  //
  // 文本采集：跳过 `button` / `[role=button]` / `svg` / `aria-hidden` 子树 ——
  //   那些是操作按钮与图标（「复制」「重新生成」等 UI 文案），不是回答正文；
  //   若不跳过，回读结果会被 UI 文案污染（W-V3 要求「与面板实际回答一致」）。
  //
  // 纯观测：只读 DOM / 计算样式，不改动、不派发事件、不点击、不滚动。
  // ---------------------------------------------------------------------------
  // ★ 2026-09-20：单块回传上限 8000 → 60000。真机「Python 内存管理机制」长回答的末块
  // 恰好卡在 8000（`#2(8000)`），页内 `slice(0,CONV_SNAP_BLOCK)` 把完整回答尾部硬切掉 →
  // 回流不全（净文本 7961）。60000 覆盖绝大多数长回答；纯调大是治标，未来若需根治可改「多块聚合」。
  // `readableText` 的遍历收敛上限 = 本值 ×2 亦随之放大，避免超长块被提前跳过。
  const CONV_SNAP_BLOCK = 60000; // 单块回传上限（字符）；R7 起回流改为 Markdown，体积更大，翻倍以容纳完整回答
  const CONV_SNAP_TAIL = 8; // 尾窗保留的块数
  const CONV_SNAP_SCAN = CONV_SNAP_TAIL * 2; // 先只遍历末尾这么多个子块（见 tailBlocks）
  const CONV_SNAP_TTL = 40; // 缓存最多复用 40 次（≈10s）后强制重新定位（防「永久用错容器」）

  // 同会话页内复用容器定位 —— 回读是每 250ms 轮询的，缓存可省掉每轮一次全量 DOM 扫描。
  // 页面 reload 后脚本整体重跑、闭包重建 → 缓存自然重置；会话切换由 `url` 比对兜底。
  // ★ TTL（QA P2-1）：仅凭 url+isConnected 无法排除「React 复用同一节点但改变其角色」→
  //   加 age 计数，到期强制重新定位（重新定位的代价是一次全量扫描，10s 一次可忽略）。
  let CONV_CACHE = null; // { url, el, bl, hops, bestEl, age, scanPass, min }

  // 「正在生成」的只读信号（**结构事实**，非启发式）：流式回答期间 DeepSeek 会把发送按钮
  // 替换为「停止生成」类控件 → 存在**可见**的 stop/停止 控件即视为仍在生成。
  // 用途：Rust 侧终态判定在 `streaming` 为真时**否决**「静默即完成」，
  // 消除「回答中途停顿 > 静默阈值 → 把半截当完成」的误判（QA P1-1）。
  //
  // ★ 2026-09-26（E2 修复：流式信号失灵）：
  //   ① **作用域收敛到会话容器内**扫描（`scope` = `loc.el`）—— 原实现扫全文档，DeepSeek
  //      侧栏 / 设置 / 工具条上的按钮会把候选堆满，「停止生成」控件常排在很后面而**探不到**。
  //   ② **去掉 200 上限**（原 `i < 200` 正是漏判根因）：容器内候选有界，无需人为截断。
  //   ③ **兜底**：容器内没探到才退化到全文档扫描（**同样不设上限**）—— 因「停止生成」控件是
  //      替换 composer 的发送按钮，可能落在会话容器**之外**；该兜底保证「不因收敛作用域而漏判」。
  //   全程**纯观测**：只 `querySelectorAll` + 读属性/文本/可见性，不点击、不改动、不派发事件。
  const CONV_STOP_WORDS = /(stop|abort|停止|中止|终止)/i;
  function scanStopControl(root) {
    const cands = root.querySelectorAll('button, [role="button"], [aria-label], [title]');
    for (let i = 0; i < cands.length; i++) {
      const el = cands[i];
      const label = norm(
        [
          el.getAttribute && el.getAttribute('aria-label'),
          el.getAttribute && el.getAttribute('title'),
          el.textContent,
        ]
          .filter(Boolean)
          .join(' ')
      );
      if (!label || !CONV_STOP_WORDS.test(label)) continue;
      if (isVisibleControl(el)) return true;
    }
    return false;
  }
  function pageStreaming(scope) {
    // ① 容器内（有界、廉价）；② 容器外兜底（composer 的「停止生成」按钮可能在容器之外）。
    if (scope && scope.querySelectorAll && scanStopControl(scope)) return true;
    return scanStopControl(document);
  }

  // 「深度思考」折叠区是否存在（**仅供诊断记录**，暂不参与判据）：真机证据不足，
  // 先只读回传、由 §13.5 真机清单确认「思考正文是否与本轮回答同处一块」后再定策略（QA P1-2）。
  // ★ 必须含「已思考」（真机折叠头是「**已思考**（用时 1 秒）」，旧表只有「已深度思考」→ 恒不命中，
  //   剥离逻辑形同虚设 —— 这正是「回读文本带上整段思考正文」的直接原因）。
  const CONV_THINK_WORDS = /(深度思考|思考过程|已思考|思考中|thinking|reasoning)/i;

  // ★ 「已写完」正向信号（2026-09-20）：DeepSeek 只有在一条消息**完整流式结束**后才在其下方
  //   渲染操作行（复制 / 重新生成 / 点赞等控件）。流式中该操作行不存在（或被子树替换为「停止」）。
  //   用途：Rust 侧终态判定以此**正向信号**为「完成」的必要条件 —— 而非仅靠「静默 + 稳定采样」
  //   启发式（后者在 DeepSeek 分段生成、落尾段前瞬间停顿且无 streaming 标记时必赛输，丢最后一段）。
  //   只匹配复制/重新生成/点赞类文案，绝不匹配「发送」。
  const CONV_DONE_WORDS = /(复制|copy|重新生成|regenerat|点赞|赞成|unlike|👍|👎)/i;
  function blockDone(blockEl) {
    if (!blockEl || !blockEl.querySelectorAll) return false;
    const cands = blockEl.querySelectorAll('button,[role="button"],[aria-label],[title]');
    // DeepSeek 实测完成操作行：复制和重新生成图标均无文字/aria-label。
    // 限定域名、真实回答元素和两个独立图标；单有正文或单个复制图标不算完成。
    if (typeof location !== 'undefined' && location.hostname === 'chat.deepseek.com' && blockEl.querySelector('.ds-assistant-message-main-content')) {
      const paths = Array.from(blockEl.querySelectorAll('[role="button"].ds-button'))
        .filter(isVisibleControl).flatMap(el => Array.from(el.querySelectorAll('svg path')).map(p => p.getAttribute('d') || ''));
      if (paths.some(d => d.startsWith('M6.14929 4.02032')) && paths.some(d => d.startsWith('M7.92136 0.349152'))) return true;
    }
    for (let i = 0; i < cands.length && i < 50; i++) {
      const el = cands[i];
      const label = norm(
        [
          el.getAttribute && el.getAttribute('aria-label'),
          el.getAttribute && el.getAttribute('title'),
          el.textContent,
        ]
          .filter(Boolean)
          .join(' ')
      );
      if (label && CONV_DONE_WORDS.test(label) && isVisibleControl(el)) return true;
    }
    return false;
  }
  function hasReasoningFold() {
    const cands = document.querySelectorAll('[aria-expanded]');
    for (let i = 0; i < cands.length && i < 100; i++) {
      const label = norm(String(cands[i].textContent || '').slice(0, 60));
      if (label && CONV_THINK_WORDS.test(label)) return true;
    }
    return false;
  }

  // 只读遍历取「可读文本」：跳过按钮 / 图标 / 隐藏子树（它们不是回答正文）。
  function readableText(el) {
    let out = '';
    const cap = CONV_SNAP_BLOCK * 2; // 提前收敛：超长块不再继续遍历整棵子树
    (function walk(node) {
      const kids = node.childNodes || [];
      for (let i = 0; i < kids.length; i++) {
        if (out.length >= cap) return;
        const c = kids[i];
        if (c.nodeType === 3) {
          out += c.nodeValue || '';
          continue;
        }
        if (c.nodeType !== 1) continue;
        const tag = c.tagName;
        if (tag === 'BUTTON' || tag === 'SVG' || tag === 'SCRIPT' || tag === 'STYLE') continue;
        if (
          c.getAttribute &&
          (c.getAttribute('role') === 'button' || c.getAttribute('aria-hidden') === 'true')
        ) {
          continue;
        }
        walk(c);
      }
    })(el);
    return norm(out);
  }

  // ---------------------------------------------------------------------------
  // 回答块 → 已净化 HTML（**R8：取代 R7 的 HTML→Markdown→HTML 往返**）
  //
  // ★ 真机回归（R7 用户反馈）：HTML→Markdown→HTML 往返在 DeepSeek 真实 DOM 下丢格式
  //   （尤其表格、嵌套列表），结果卡仍只显示纯文本。R8 改走「**直出净化 HTML**」：
  //   页面端按白名单把回答子树序列化成安全 HTML（剥离 <script>/<style>、所有 on* 事件、
  //   javascript:/非图片 data: URL、未授权属性），结果卡用 DOMParser 解析 + 二次净化后挂入
  //   —— 忠实保留 DeepSeek 全部排版（标题/加粗/列表/代码/引用/表格/链接），且绝不执行远端脚本。
  const SAFE_TAGS = {
    P:1, DIV:1, SPAN:1, BR:1, H1:1, H2:1, H3:1, H4:1, H5:1, H6:1,
    UL:1, OL:1, LI:1, BLOCKQUOTE:1, PRE:1, CODE:1, STRONG:1, B:1, EM:1, I:1,
    U:1, S:1, SUB:1, SUP:1, A:1, IMG:1, TABLE:1, THEAD:1, TBODY:1, TR:1, TD:1,
    TH:1, HR:1, MARK:1, SMALL:1, DEL:1, KBD:1, FIGURE:1, FIGCAPTION:1
  };
  const VOID_TAGS = { BR:1, HR:1, IMG:1 };
  function escapeHtmlText(s) {
    // ★ P3/SEC2 纵深防御：补全双引号与单引号转义。下游 result.html::cloneSafe 会二次净化
    //   （只拷贝白名单属性），即便本函数漏转义也不会被利用；此处补齐转义集，缩小对下游的依赖面。
    return String(s == null ? '' : s)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');   // SEC2：单引号转义（纵深防御，防未来有人把属性改单引号包裹）
  }
  // 仅放行 http(s) / mailto / # / 相对路径；javascript: 等一律丢弃。
  function safeUrl(s) {
    if (typeof s !== 'string') return null;
    return /^\s*(https?:|mailto:|#|\/)/i.test(s) ? s : null;
  }
  // 图片仅放行 http(s) 与内联 data:image；其余（含可执行 data:、blob:）丢弃。
  function safeImgSrc(s) {
    if (typeof s !== 'string') return null;
    return /^\s*(https?:|data:image\/)/i.test(s) ? s : null;
  }

  // ★ 方案A：代码块高亮的「白名单 token」正则（与 result.html::safeClass 同口径）。
  //   HLJS_CLASS_RE：仅匹配 hljs 整体类或 hljs-<token>（如 hljs-keyword / hljs-built_in ——
  //   注意 built_in 含下划线，故字符类含 `_`；这是与任务示例清单一致的最小修正）。
  //   LANG_CLASS_RE：仅匹配 language-<name>（如 language-python，挂在 <code> 上做语言标签）。
  const HLJS_CLASS_RE = /^hljs(-[a-z0-9_-]+)?$/;
  const LANG_CLASS_RE = /^language-[\w+#.-]+$/;

  // 从 <code> 的 class 里提取语言标签：仅接受 language-<name>；若含白名单外类名则保守不挂。
  function codeLangClass(v) {
    if (typeof v !== 'string') return '';
    const tokens = v.split(/\s+/).filter(Boolean);
    if (!tokens.length) return '';
    let lang = '';
    for (let i = 0; i < tokens.length; i++) {
      if (LANG_CLASS_RE.test(tokens[i])) {
        if (!lang) lang = tokens[i];
      } else if (!HLJS_CLASS_RE.test(tokens[i])) {
        return ''; // 含白名单外类名 → 不挂任何语言标签
      }
    }
    return lang;
  }

  // ★ 方案A：代码块语法高亮序列化（极窄 class 白名单）。
  //   文本节点 → 转义后原样保留（含 \n 换行文本节点，highlight.js 行间以其分隔）；
  //   <br> → \n；<span>：逐 token 校验，**仅保留**匹配 HLJS_CLASS_RE 的 hljs-* token，
  //   丢弃其余（如 highlight.js 的裸 function_ / class_ 子 token、或远端其它类名）；
  //   无任何白名单 token 时 unwrap（丢 class、保留子内容，含换行）。其他标签一律 unwrap。
  function serializeCodeText(node) {
    if (!node) return '';
    if (node.nodeType === 3) return escapeHtmlText(node.nodeValue || '');
    if (node.nodeType !== 1) return '';
    const tag = node.tagName;
    if (tag === 'BR') return '\n';
    if (tag === 'SPAN') {
      const cls = node.getAttribute ? node.getAttribute('class') : null;
      let kept = '';
      if (cls && typeof cls === 'string') {
        const tokens = cls.split(/\s+/).filter(Boolean);
        const good = [];
        for (let i = 0; i < tokens.length; i++) {
          if (HLJS_CLASS_RE.test(tokens[i])) good.push(tokens[i]);
        }
        if (good.length) kept = good.join(' ');
      }
      let inner = '';
      const cs = node.childNodes || [];
      for (let i = 0; i < cs.length; i++) inner += serializeCodeText(cs[i]);
      if (!kept) return inner; // 无白名单 token → unwrap：仅保留子内容（含换行）
      return '<span class="' + escapeHtmlText(kept) + '">' + inner + '</span>';
    }
    // 其他标签（含 CODE/PRE 自身，由其父层处理）：unwrap，递归子内容。
    let s = '';
    const cs = node.childNodes || [];
    for (let i = 0; i < cs.length; i++) s += serializeCodeText(cs[i]);
    return s;
  }

  function serializeHtml(node, exclude) {
    if (!node) return '';
    if (node === exclude) return '';
    if (node.nodeType === 3) {
      const text = escapeHtmlText(node.nodeValue || '');
      const parent = node.parentElement;
      // ChatGPT 的正文逐字 span 把原文换行保存在文本中；去掉站点样式后 HTML
      // 默认会把它折成空格。转换为标准 br，结果卡及 Markdown 导出都能保留。
      if (parent?.ownerDocument?.location?.hostname === 'chatgpt.com' &&
          parent.getAttribute('data-d-text-flow') === 'word') return text.replace(/\r\n?|\n/g, '<br>');
      return text;
    }
    if (node.nodeType !== 1) return '';
    const tag = node.tagName;
    const get = node.getAttribute ? node.getAttribute.bind(node) : function () { return null; };
    if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'SVG' || tag === 'BUTTON') return '';
    if (get('role') === 'button' || get('aria-hidden') === 'true') return '';
    // ★ 2026-10-03 七次真机（问题②）：视觉隐藏的「角色头」（`<h4 class="sr-only">ChatGPT 说：</h4>`）
    //   不是回答正文 —— 与 `aria-hidden` 同列跳过（见 `isSrOnly` 注释）。
    if (isSrOnly(node)) return '';
    // ChatGPT 的新代码块用 CodeMirror 行节点代替 pre；只转换回答中的明确代码块，
    // 保留空行/缩进，并排除语言栏、运行按钮和编辑器辅助公告。
    if (node.ownerDocument?.location?.hostname === 'chatgpt.com' && get('data-code-block') === 'true') {
      const editor = node.querySelector('.cm-content');
      if (editor) {
        const lines = Array.from(editor.querySelectorAll('.cm-line'));
        const text = lines.length ? lines.map(line => line.textContent || '').join('\n') : editor.textContent || '';
        const lang = editor.getAttribute('data-language') || '';
        const cls = /^[\w+#.-]+$/.test(lang) ? ' class="language-' + escapeHtmlText(lang) + '"' : '';
        return '<pre><code' + cls + '>' + escapeHtmlText(text) + '</code></pre>';
      }
    }
    if (!SAFE_TAGS[tag]) {
      // 白名单外标签：去掉标签、保留子内容（unwrap），避免丢失文本。
      let s = '';
      const cs = node.childNodes || [];
      for (let i = 0; i < cs.length; i++) s += serializeHtml(cs[i], exclude);
      return s;
    }
    let attrs = '';
    if (tag === 'PRE') {
      // ★ 2026-09-23 基础修复 + 方案A（语法高亮还原）：
      //   换行铁律优先——无论是否高亮，代码块**必须按行显示**，绝不能退化成压成一行。
      //   做法分两段：
      //   ① 优先用 `serializeCodeText` 抽取**带 hljs-* span 的高亮结构**（在白名单内才保留 class）；
      //   ② 若结果里**不含真实 \n**（远端没用 \n 文本节点分隔行，例如块级 span / 非 hljs 高亮器），
      //      则**回退**到 `innerText`（再 `textContent`）纯文本方案，并清空 code 上的 class ——
      //      高亮是尽力而为，换行是强制保证。
      //   语言标签：仅取 <code> 上形如 `language-<name>` 的白名单 token（其它一律忽略），
      //   挂在输出 `<code>` 上做语言提示（卡片侧 safeClass 会二次过滤）。
      const codeEl = node.querySelector && node.querySelector('code');
      const srcEl = codeEl || node;
      let langClass = codeLangClass(srcEl.getAttribute ? srcEl.getAttribute('class') : null);
      let codeHtml = serializeCodeText(srcEl);
      if (!/\n/.test(codeHtml)) {
        // 回退：纯文本换行靠 innerText（对未渲染元素不可靠 → 再退回 textContent），
        // 并**清空 code 上的 class**（高亮失败则连语言标签也不挂，避免卡片侧误上色）。
        langClass = '';
        let codeText = '';
        try { codeText = srcEl.innerText || ''; } catch (_) { codeText = ''; }
        if (!/\n/.test(codeText)) codeText = srcEl.textContent || ''; // innerText 不可靠时的兜底
        codeHtml = escapeHtmlText(codeText);
      }
      const langAttr = langClass ? ' class="' + escapeHtmlText(langClass) + '"' : '';
      return '<pre><code' + langAttr + '>' + codeHtml.replace(/^\n+|\n+$/g, '') + '</code></pre>';
    }
    if (tag === 'A') {
      const href = safeUrl(get('href'));
      if (href) attrs += ' href="' + escapeHtmlText(href) + '" rel="noopener noreferrer" target="_blank"';
    } else if (tag === 'IMG') {
      const src = safeImgSrc(get('src'));
      if (src) {
        attrs += ' src="' + escapeHtmlText(src) + '"';
        const alt = get('alt');
        if (alt) attrs += ' alt="' + escapeHtmlText(alt) + '"';
      }
    }
    const cs = node.childNodes || [];
    let inner = '';
    for (let i = 0; i < cs.length; i++) inner += serializeHtml(cs[i], exclude);
    if (VOID_TAGS[tag]) return '<' + tag.toLowerCase() + attrs + '>';
    return '<' + tag.toLowerCase() + attrs + '>' + inner + '</' + tag.toLowerCase() + '>';
  }
  // 抽取回答的净化 HTML；净化结果空（极端结构）时退回 toMarkdown，至少不丢文本。
  //
  // ★ R10（真机 2026-09-15 复现）：**无可见内容**（`"<div></div>"` 这类纯空壳标记）一律返回
  //   空串。否则空块会因 HTML 标记而「变成非空」（11 字符），**骗过 Rust 侧的最短长度闸门**
  //   （`min_len=1`），把「回答还没开始生成的空占位块」当回答交付 —— 真机日志：
  //   `回读完成（11 字符 / 7 次采样 / 1606ms）：「<div></div>」`（卡片空白 = 用户说的「没有回流」）。
  //   返回空串后，尾窗会跳过该块（`tailBlocks` 的 `if(!p.ans)continue`），Rust 侧继续等真回答。
  function hasVisibleHtml(html) {
    if (/<img\b/i.test(html)) return true; // 图片无文本但确实可见
    return /\S/.test(String(html).replace(/<[^>]*>/g, ''));
  }
  function answerHtml(blockEl, exclude) {
    const html = serializeHtml(blockEl, exclude);
    if (!hasVisibleHtml(html)) return '';
    return html.trim() || toMarkdown(blockEl, exclude || null);
  }

  // 列表项内层正文抽取（与 `toMarkdown` 同口径：遍历子节点递归转 MD）。
  // 单独抽出，避免 UL/OL 分支里 `toMarkdown(li)` 命中 LI 分支再叠一次 "- " 前缀（双重前缀 bug）。
  function liText(li, exclude) {
    let s = '';
    const cs = li.childNodes || [];
    for (let i = 0; i < cs.length; i++) s += toMarkdown(cs[i], exclude);
    return s;
  }

  // 历史实现（**R7，保留为兜底**）：HTML → Markdown（标题/加粗/列表/代码/引用/表格/链接…）。
  // 正常路径用上面的 `answerHtml`（净化 HTML）；本函数仅在上述净化结果为空时兜底。
  function toMarkdown(node, exclude) {
    if (!node) return '';
    if (node === exclude) return '';
    if (node.nodeType === 3) return node.nodeValue || '';
    if (node.nodeType !== 1) return '';
    const tag = node.tagName;
    const get = node.getAttribute ? node.getAttribute.bind(node) : function () { return null; };
    if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'SVG' || tag === 'BUTTON') return '';
    if (get('role') === 'button' || get('aria-hidden') === 'true') return '';
    // ★ 2026-10-03 七次真机（问题②）：与 `serializeHtml` 同口径跳过视觉隐藏的角色头（`sr-only`）。
    if (isSrOnly(node)) return '';
    const kids = function () {
      let s = '';
      const cs = node.childNodes || [];
      for (let i = 0; i < cs.length; i++) s += toMarkdown(cs[i], exclude);
      return s;
    };
    switch (tag) {
      case 'H1': case 'H2': case 'H3': case 'H4': case 'H5': case 'H6': {
        const lvl = parseInt(tag.charAt(1), 10) || 1;
        return '\n' + '#'.repeat(lvl) + ' ' + kids().trim() + '\n';
      }
      case 'P':
      case 'DIV':
        return '\n' + kids().trim() + '\n';
      case 'BR':
        return '\n';
      case 'HR':
        return '\n---\n';
      case 'BLOCKQUOTE': {
        const lines = kids().split('\n');
        return '\n' + lines.map(function (l) { return '> ' + l; }).join('\n') + '\n';
      }
      case 'PRE': {
        // ★ 与 serializeHtml 的 PRE 修复同源：textContent 丢结构性换行 → 用 innerText 取渲染后文本。
        const codeEl = node.querySelector && node.querySelector('code');
        const srcEl = codeEl || node;
        let code = '';
        try { code = srcEl.innerText || ''; } catch (_) { code = ''; }
        if (!/\n/.test(code)) code = srcEl.textContent || '';
        return '\n```\n' + code.replace(/^\s+|\s+$/g, '') + '\n```\n';
      }
      case 'CODE':
        return '`' + (node.textContent || '').replace(/^\s+|\s+$/g, '') + '`';
      case 'STRONG': case 'B':
        return '**' + kids() + '**';
      case 'EM': case 'I':
        return '*' + kids() + '*';
      case 'A': {
        const href = get('href') || '';
        return '[' + kids() + '](' + href + ')';
      }
      case 'IMG': {
        const alt = get('alt') || '';
        const src = get('src') || '';
        return src ? '![' + alt + '](' + src + ')' : alt;
      }
      case 'UL': case 'OL': {
        const ordered = tag === 'OL';
        const items = [];
        const cs = node.children || [];
        let n = 0;
        for (let i = 0; i < cs.length; i++) {
          if (cs[i].tagName !== 'LI') continue;
          n++;
          // 直接取 LI 内层正文，不可再调 toMarkdown(li)（会命中 LI 分支再叠一个 "- " 前缀）。
          items.push((ordered ? n + '. ' : '- ') + liText(cs[i], exclude).trim());
        }
        return '\n' + items.join('\n') + '\n';
      }
      case 'LI':
        // 独立调用时带 "- " 前缀；处于 UL/OL 容器内时由 UL/OL 分支负责加前缀（见上）。
        return '- ' + liText(node, exclude).trim() + '\n';
      case 'TABLE':
        return '\n' + tableToMarkdown(node) + '\n';
      default:
        return kids();
    }
  }

  // 把 <table> 转成「管道式 Markdown」（与 `renderMarkdown` 的表格解析对齐）。
  // thead/tbody/直接 tr 都识别；列以首行列数对齐；第 2 行插入 `---` 分隔行。
  function tableToMarkdown(table) {
    const rowCells = function (tr) {
      const out = [];
      const cs = tr.children || [];
      for (let i = 0; i < cs.length; i++) {
        const t = cs[i].tagName;
        if (t === 'TH' || t === 'TD') out.push(toMarkdown(cs[i]).replace(/\s+/g, ' ').trim());
      }
      return out;
    };
    const trs = [];
    (function collect(n) {
      const ch = n.children || [];
      for (let i = 0; i < ch.length; i++) {
        if (ch[i].tagName === 'TR') trs.push(ch[i]);
        else collect(ch[i]);
      }
    })(table);
    if (!trs.length) return '';
    const lines = trs.map(function (tr) { return '| ' + rowCells(tr).join(' | ') + ' |'; });
    const cols = lines.length ? (lines[0].match(/\|/g) || []).length - 1 : 0;
    const sep = '| ' + new Array(cols > 0 ? cols : 1).fill('---').join(' | ') + ' |';
    if (lines.length > 1) lines.splice(1, 0, sep);
    else lines.push(sep);
    return lines.join('\n');
  }

  // 尾窗（tailing window）：末 CONV_SNAP_TAIL 个「有文本的直接子块」的 {ans, think}。
  // 直接子节点即「消息块」——与 `blocksOf` 的条目口径一致（同一容器、同一层级）。
  // ★ 先截窗口再遍历（QA P2-2）：非虚拟化容器可能有数百块，全量遍历的代价随块数线性增长；
  //   先只取末尾 CONV_SNAP_SCAN(=2×TAIL) 个候选（留 2 倍余量以跳过空块/占位块），再取末 TAIL 个。
  const MESSAGE_IDS = new WeakMap();
  let nextMessageId = 0;
  function messageId(el) {
    if (!MESSAGE_IDS.has(el)) MESSAGE_IDS.set(el, ++nextMessageId);
    return MESSAGE_IDS.get(el);
  }
  function tailBlocks(el) {
    const kids = el.children || [];
    const from = Math.max(0, kids.length - CONV_SNAP_SCAN);
    const acc = [];
    for (let i = from; i < kids.length; i++) {
      const p = splitBlock(kids[i]);
      if (!p.ans && !p.think) continue;
      const roleNode = kids[i].querySelector('[data-message-author-role]');
      const semantic = kids[i].getAttribute('data-message-author-role') || (roleNode && roleNode.getAttribute('data-message-author-role')) || '';
      acc.push(Object.assign({}, p, {messageId: messageId(kids[i]), semantic: semantic}));
    }
    return acc.length > CONV_SNAP_TAIL ? acc.slice(acc.length - CONV_SNAP_TAIL) : acc;
  }

  // ★ 结构性诊断（纯只读）：把容器尾窗的「骨架」压成一段短 JSON —— 一次真机运行即可看清
  //   「思考正文 / 回答 / 装饰页脚」在 DOM 里到底是**块**还是**子块**，据此决定抽取策略。
  //
  //   为什么必须测而不是猜（真机证据 2026-09-14）：思考折叠的**正文**没有被 `readableText` 跳过
  //   （它不是 button / aria-hidden），于是「已思考（用时 1 秒）<思考正文><回答>」被当作一整块读出
  //   （67 字符）→ 回读 ≠ 面板回答。要剥掉思考正文，必须先知道它与回答是「同块内联」还是
  //   「同一块的兄弟子块」—— 二者对应完全不同的抽取代码，猜错就是又一次「反向误判」。
  //   上限：≤ (TAIL+2) 块 × (类名 24 + 文本头 44 + ≤4 子项 × 57) ≈ 3.4KB。
  // ★ 结构性剥离：从「思考正文 + 回答同处一块」里取出**回答**（R1 真机缺陷之二，2026-09-14）。
  //
  //   真机事实：助手消息块 `_4f9bf79…` 的直接子元素里，承载全部文本的那个 `ds-message`
  //   **以折叠头「已思考（用时 1 秒）」开头**，其后紧接思考正文、再紧接回答 ——
  //   **三者文本首尾相连、没有任何分隔符** → 纯文本切不开，只能靠 DOM 分层。
  //
  //   做法（**保守**，只在结构确凿时才动手）：
  //     ① 仅当**整块文本的开头**就是思考标记时进入（否则原样返回，零开销零风险）；
  //     ② 在块内（深度 ≤4）找一个元素 F，使 `readableText(F)` 是整块文本的**前缀**，
  //        且**严格短于**整块文本 → 取其中**最长**的那个 F（= 折叠区，含折叠头 + 思考正文）；
  //     ③ 回答 = 去掉该前缀后的剩余文本。
  //   找不到这样的 F（如思考就是纯内联文本、无可分层）→ **原样返回**：
  //     宁可多带一段思考，也**绝不**把回答截断（截断会让 W-V3 更隐蔽地失败）。
  // ★ A+B（2026-09-20 判终修复）：块内是否已渲染出「答案元素」—— 一个**不属于思考子树**、
  //   且带有可见文本的 `ds-markdown`。
  //
  //   **结构事实 > 按钮外观**：DeepSeek 长思考/暂停时，末块只挂着思考容器（`ds-think-*`），
  //   却在下方渲染了复制/重新生成等操作行按钮 —— 于是 `blockDone()` 的按钮探测误报「已写完」，
  //   而 `full` 文本启发式又把它判成「普通回答块」（pending=false）→ Rust 放行、把思考占位正文
  //   当答案交付（真机证据 `#236` done=true 但末块只有思考容器、`ans=""`，`#238` 交付了「正在思考…」）。
  //
  //   判据（只做**否决/闸门**，绝不参与定位）：只有真实的、已渲染的答案元素才算「可能写完」；
  //   思考占位块因不含任何答案元素而被判 `pending`、`done=false`，让 Rust 继续等到真答案出现。
  function hasAnswerElement(blockEl) {
    if (!blockEl.querySelectorAll) return false;
    const mds = blockEl.querySelectorAll('div.ds-markdown, [class*=" ds-markdown"]');
    for (let i = 0; i < mds.length; i++) {
      const md = mds[i];
      // 思考区内部的 ds-markdown 存的是**思考正文**，不是答案 —— 不算。
      if (md.closest && md.closest('[class*="ds-think"]')) continue;
      if (hasVisibleHtml(answerHtml(md, null))) return true;
    }
    return false;
  }
  // ★ P1 修复（2026-09-25 审计）：class 无关的答案渲染判据 —— 兜底 `done`。
  //
  // `hasAnswerElement` 硬依赖 `ds-markdown` class（站点改版改名即失效 ⇒ 所有回读降级超时）。
  // 此处增加**结构级**判据：末块存在「可见、非思考、非控件」的可读文本元素 ⇒ 答案已渲染，
  // 不依赖任何具体 class。与 `hasAns` 取 OR 作为 `done`（Rust 侧 `decide_done` 仍以 done 为
  // 必要条件，并由 streaming/quiet/stable 否决器兜底，故「提前为真」不会误终）。
  function hasRenderedAnswer(blockEl) {
    if (!blockEl || !blockEl.querySelectorAll) return false;
    const els = blockEl.querySelectorAll('div, p, span, li, pre, code');
    for (let i = 0; i < els.length; i++) {
      const el = els[i];
      if (el.closest && el.closest('[class*="ds-think"]')) continue; // 思考正文不算
      if (el.closest && el.closest('button, svg, [role="button"]')) continue; // 操作行控件不算
      const t = (el.textContent || '').trim();
      if (t.length > 1) return true;
    }
    return false;
  }
  // P1/P2 性能：下方 splitBlock() 对稳定块做记忆化；此为纯计算实现（接收已算好的 full）。
  function splitBlockRaw(blockEl, full) {
    // ★ A（2026-09-20 改）：`done` 以「答案元素已渲染」为准，**不依赖操作行按钮**。
    //   曾用 `hasAns && blockDone(blockEl)` —— 但带联网搜索的长回答页底操作行按钮 hover 才显示
    //   或语义不同，`blockDone` 恒 false ⇒ done 永远 false ⇒ Rust 卡满超时降级交付（真机 #108）。
    //   思考占位块（hasAns=false）原本就由 B 闸门以 `pending=true` 否决，**不需要按钮配合**；
    //   答案渲染中则由 `streaming=true` + 文本继续增长天然不判终。故按钮探测已无必要。
    const hasAns = hasAnswerElement(blockEl);
    // ★ P1 修复（2026-09-25 审计）：class 无关的答案渲染判据兜底 `done`（见 hasRenderedAnswer）。
    const hasRendered = hasRenderedAnswer(blockEl);
    const done = blockDone(blockEl);
    // ★ B：无答案元素 + 形态像思考/助手块 ⇒ 此刻**只有思考过程**、回答还未渲染 →
    //   `pending` 硬否决，绝不当答案交付（Rust 据此继续等真答案）。
    const thinkingLike =
      (blockEl.className && /ds-think/i.test(String(blockEl.className))) ||
      !!blockEl.querySelector('[class*="ds-think"], [data-testid="reasoning"]');
    // ★ #004：**助手标记**（= 有答案元素 或 形态像思考/助手块）。★ 它是 **prompt 无关**的，
    //   故可安全随块进 splitBlock 的记忆化缓存（缓存键不含 prompt）；页内据此产出块角色 `assistant`。
    const asst = hasAns || thinkingLike;
    if (!hasAns && thinkingLike) {
      return { ans: '', think: full, pending: true, done: false, asst: asst };
    }
    if (!thinkingLike) {
      // 无思考折叠头：整块即回答（净化 HTML，保留标题/列表/表格/换行等排版）。
      return {
        ans: answerHtml(blockEl, null),
        think: '',
        pending: false,
        done: done,
        asst: asst,
      };
    }
    let best = 0;
    let bestEl = null;
    (function scan(node, depth) {
      if (depth > 4) return;
      const ks = node.children || [];
      for (let i = 0; i < ks.length; i++) {
        const c = ks[i];
        const ct = readableText(c);
        if (ct.length > best && ct.length < full.length && full.indexOf(ct) === 0) {
          best = ct.length;
          bestEl = c;
        }
        scan(c, depth + 1);
      }
    })(blockEl, 0);
    if (bestEl) {
      // ★ 思考区仍以**纯文本**回传（卡片思考区走 textContent，塞 Markdown 会显示字面量标记）；
      //   回答则把思考子树从整块里排除后，转成净化 HTML（保留标题/加粗/列表/代码块/表格等排版）。
      const think = readableText(bestEl);
      const ans = answerHtml(blockEl, bestEl).trim();
      if (ans)
        return { ans: ans, think: think, pending: false, done: done, asst: asst };
      // ★ `pending`：命中折叠头却**切不出回答** ⇒ 这一块此刻**只有思考过程**，回答还没开始。
      //   这是结构事实（不是启发式）—— Rust 侧据此否决终态判定，避免「思考完毕、正文未起」
      //   这段停顿（真机可超过静默阈值）被误判成「回答已写完」而把思考正文当回答交付。
      return { ans: '', think: think, pending: true, done: done, asst: asst };
    }
    // 命中折叠头但找不到可下钻的思考子树：原样返回整块（净化 HTML），并标 pending 让 Rust 继续等。
    return {
      ans: answerHtml(blockEl, null),
      think: '',
      pending: true,
      done: done,
      asst: asst,
    };
  }

  // ---------------------------------------------------------------------------
  // P1 / P2 性能记忆化（审计报告 §5 第 2 批）：对**稳定**消息块缓存 splitBlock 结果，
  // 避免每 250ms 轮询都重复跑 answerHtml（重正则 serializeHtml）+ hasAnswerElement（querySelectorAll）。
  //
  // 键 = DOM 元素（WeakMap：节点被 React 卸载即自动回收，**零泄漏**）+ 文本指纹
  //   （FNV-1a over readableText(full) + childElementCount）。
  // 流式末块文本每轮变化 → 指纹变 → 必然重算（**无陈旧风险**）；已定稿的块指纹不变 → 命中缓存。
  // 指纹同时纳入 childElementCount，覆盖「文本不变但结构变了（如新增子节点）」的情况。
  const SPLIT_CACHE = new WeakMap();
  let SPLIT_CACHE_HITS = 0;
  let SPLIT_CACHE_MISSES = 0;
  function splitBlock(blockEl) {
    const full = readableText(blockEl);
    let fp = 2166136261 >>> 0; // FNV-1a offset basis
    for (let i = 0; i < full.length; i++) {
      fp ^= full.charCodeAt(i);
      fp = Math.imul(fp, 16777619) >>> 0;
    }
    fp ^= (blockEl.childElementCount || 0);
    fp = Math.imul(fp, 16777619) >>> 0;
    const structure = blockEl.outerHTML;
    const prev = SPLIT_CACHE.get(blockEl);
    if (prev && prev.fp === fp && prev.structure === structure) {
      SPLIT_CACHE_HITS++;
      return prev.res;
    }
    SPLIT_CACHE_MISSES++;
    const res = splitBlockRaw(blockEl, full);
    SPLIT_CACHE.set(blockEl, { fp: fp, structure: structure, res: res });
    return res;
  }
  function splitCacheStats() {
    return {
      hits: SPLIT_CACHE_HITS,
      misses: SPLIT_CACHE_MISSES,
      // 命中率：稳定块越多（多轮历史消息）越接近 1；纯流式单块会话接近 0。
      ratio:
        SPLIT_CACHE_HITS + SPLIT_CACHE_MISSES > 0
          ? SPLIT_CACHE_HITS / (SPLIT_CACHE_HITS + SPLIT_CACHE_MISSES)
          : 0,
    };
  }

  // 只取回答（剥离思考前缀后的部分）—— structDump 的诊断读数沿用这个口径。
  function answerOf(blockEl) {
    return splitBlock(blockEl).ans;
  }

  // ★ 临时诊断（2026-10-03 三次真机排查）：打印 `pickEl` 的**祖先链 + 自身 + 一层后代**，
  //   每级读 `tag/cls/cc/tl/h`。用于一次性确认真机层级 —— 前几轮因 `trace` 与骨架读数
  //   无法对齐（疑似缓存复用旧容器），必须直接看链条才能定位，不再靠推理。
  function chainDump(el) {
    try {
      const up = [];
      let n = el;
      for (let i = 0; i < 4 && n; i++) {
        const bl = blocksOf(n);
        up.unshift(
          n.tagName + '.' + elCls(n).slice(0, 18) + '[' + bl.childCount + '/' + bl.textLen + ']'
        );
        n = n.parentElement;
      }
      const down = [];
      const ks = (el && el.children) || [];
      for (let i = 0; i < ks.length && i < 4; i++) {
        const bl = blocksOf(ks[i]);
        down.push(
          ks[i].tagName + '.' + elCls(ks[i]).slice(0, 18) + '[' + bl.childCount + '/' + bl.textLen + ']'
        );
      }
      return 'up=' + up.join(' < ') + ' | self=' + el.tagName + '.' + elCls(el).slice(0, 18) +
        '[' + blocksOf(el).childCount + '/' + blocksOf(el).textLen + '] | down=' + down.join(' , ');
    } catch (e) {
      return '';
    }
  }

  // 只读轮廓：某元素的直接子元素（≤6）→ tag/类名/高/文本长/文本头/aria；再对其中**文本最长**的
  // 下钻一层（≤5）。目的：一次真机运行看清「思考正文与回答」到底分不分层、分在哪一层。
  function outline(root) {
    const ks = root.children || [];
    const arr = [];
    let deepest = null;
    let deepLen = -1;
    for (let i = 0; i < ks.length && arr.length < 6; i++) {
      const c = ks[i];
      const ct = readableText(c);
      const h = c.getBoundingClientRect ? Math.round(c.getBoundingClientRect().height) : 0;
      arr.push({
        tag: c.tagName,
        cls: elCls(c).slice(0, 22),
        h: h,
        n: ct.length,
        t: ct.slice(0, 22),
        a: (c.getAttribute && (c.getAttribute('aria-hidden') || '') + (c.getAttribute('aria-expanded') || '')) || '',
      });
      if (ct.length > deepLen) {
        deepLen = ct.length;
        deepest = c;
      }
    }
    if (deepest) {
      const gs = deepest.children || [];
      const g = [];
      for (let i = 0; i < gs.length && g.length < 5; i++) {
        const c = gs[i];
        const ct = readableText(c);
        const h = c.getBoundingClientRect ? Math.round(c.getBoundingClientRect().height) : 0;
        g.push({ tag: c.tagName, cls: elCls(c).slice(0, 22), h: h, n: ct.length, t: ct.slice(0, 26) });
      }
      arr.push({ gcOf: elCls(deepest).slice(0, 22), gc: g });
    }
    return arr;
  }

  function structDump(el) {
    try {
      const kids = el.children || [];
      const from = Math.max(0, kids.length - CONV_SNAP_SCAN);
      const arr = [];
      for (let i = from; i < kids.length && arr.length < CONV_SNAP_TAIL + 2; i++) {
        const k = kids[i];
        const t = readableText(k);
        if (!t) continue;
        const h = k.getBoundingClientRect ? Math.round(k.getBoundingClientRect().height) : 0;
        arr.push({
          h: h,
          cc: blocksOf(k).childCount,
          n: t.length,
          cls: elCls(k).slice(0, 24),
          t: t.slice(0, 40),
          // 剥离后的文本头（应等于回答）：与 `t` 对照，一眼看出剥离是否生效。
          ans: answerOf(k).slice(0, 40),
          out: outline(k),
        });
      }
      const s = JSON.stringify(arr);
      return s.length > 3600 ? s.slice(0, 3600) + '…' : s;
    } catch (e) {
      return '';
    }
  }

  let SEND_CONTEXT = null;
  function prepareSend(prompt, refresh) {
    const request = window.__dsfloatRequest || 0;
    if (!refresh && SEND_CONTEXT && SEND_CONTEXT.request === request) return true;
    SEND_CONTEXT = null;
    const before = snapshotConversation('', false);
    // 短历史也要保留有明确角色的基线，防止复用消息槽时读取到旧答案。
    if (!before.ok) {
      const nodes = Array.from(document.querySelectorAll('[data-message-author-role]'))
        .filter(el => !el.parentElement?.closest('[data-message-author-role]'));
      before.texts = nodes.map(el => splitBlock(el).ans.slice(0, CONV_SNAP_BLOCK));
      before.messageIds = nodes.map(el => messageId(el));
      before.roles = nodes.map(el => el.getAttribute('data-message-author-role'));
    }
    SEND_CONTEXT = {request: request, prompt: prompt, submitted: false,
      baseline: before.texts || [], ids: before.messageIds || [], roles: before.roles || [], bound: null};
    return true;
  }
  function restrictToRequest(parts, prompt) {
    const ctx = SEND_CONTEXT;
    if (!ctx || ctx.request !== (window.__dsfloatRequest || 0) || ctx.prompt !== prompt) return parts;
    const need = compactText(prompt);
    const plain = p => compactText(htmlPlain(p.ans));
    const question = p => p.semantic !== 'assistant' && !p.asst && plain(p) === need;
    let index = parts.findIndex(p => p.messageId === ctx.bound && question(p));
    // 已确认过的本轮被 React 重建：只接受相同前序消息和本轮位置，不能仅凭问题相同回退。
    if (ctx.bound && index < 0 && ctx.boundPrefix) {
      const i = ctx.boundPrefix.length;
      if (parts[i] && question(parts[i]) && parts.slice(0,i).every((p,j) => plain(p) === ctx.boundPrefix[j])) {
        ctx.bound = parts[i].messageId;
        index = i;
      }
      if (index < 0) return [];
    }
    if (!ctx.bound) {
      const current = parts.map(p => p.ans.slice(0, CONV_SNAP_BLOCK));
      if (JSON.stringify(current) === JSON.stringify(ctx.baseline)) return [];
      const baselineCount = ctx.baseline.filter((html,i) => ctx.roles[i] !== 'assistant' && compactText(htmlPlain(html)) === need).length;
      const currentCount = parts.filter(question).length;
      const oldTail = parts.findIndex(p => p.messageId === ctx.ids[ctx.ids.length - 1]);
      for (let i = parts.length - 1; i >= 0; i--) {
        const p = parts[i];
        const fresh = ctx.ids.length === 0 || currentCount > baselineCount || (oldTail >= 0 && i > oldTail);
        if (question(p) && fresh) {
          ctx.bound = p.messageId;
          ctx.appendedQuestion = oldTail >= 0 && i > oldTail;
          ctx.boundPrefix = parts.slice(0,i).map(plain);
          index = i;
          break;
        }
      }
    }
    if (index < 0) return [];
    return parts.slice(index).filter((p,j) => {
      if (j === 0) return true;
      // 回答身份按本轮相对位置保留，DOM 节点重建不改变它。
      // 原会话尾部之后新增的提问及全新回答节点可返回相同答案；旧槽位复用仍需
      // 先观察到新内容，避免把尚未替换的基线答案交付到本轮。
      if (!ctx.answerSlots) ctx.answerSlots = new Set();
      if (ctx.answerSlots.has(j)) return true;
      const text = compactText(htmlPlain(p.ans.slice(0, CONV_SNAP_BLOCK)));
      const stale = ctx.baseline.some((html,k) => ctx.roles[k] !== 'user' && compactText(htmlPlain(html)) === text);
      const appendedAnswer = ctx.appendedQuestion && p.semantic === 'assistant' && !ctx.ids.includes(p.messageId);
      if (stale && !appendedAnswer) return false;
      ctx.answerSlots.add(j);
      return true;
    });
  }
  function sendEvidence(prompt) {
    if (!SEND_CONTEXT || SEND_CONTEXT.request !== (window.__dsfloatRequest || 0)) return 'unknown';
    const snap = snapshotConversation(prompt, false);
    return SEND_CONTEXT.bound && snap.ok && snap.texts.length ? 'sent' : 'unknown';
  }
  // 真实站点的语义消息边界优先；已知站点结构变化时拒绝全页猜测。
  function siteConversation() {
    if (location.hostname === 'chatglm.cn') {
      const parts = [];
      for (const item of document.querySelectorAll('.conversation-item')) {
        const question = item.querySelector('.question .question-txt');
        const answer = item.querySelector('.answer');
        if (question) parts.push(Object.assign({}, splitBlock(question), {messageId: messageId(question), semantic: 'user', asst: false, done: false}));
        const body = answer && Array.from(answer.querySelectorAll('.answer-content-wrap')).find(el => !el.closest('.thinking-content'));
        if (!body) continue;
        const copy = answer.querySelector('.shim.copy.canuse');
        const redo = answer.querySelector('.shim.regenerate.canuse');
        parts.push(Object.assign({}, splitBlock(body), {messageId: messageId(answer), semantic: 'assistant', asst: true,
          done: !!copy && !!redo && isVisibleControl(copy) && isVisibleControl(redo)}));
      }
      return parts.slice(-CONV_SNAP_TAIL);
    }
    if (location.hostname === 'www.kimi.com' || location.hostname === 'kimi.com') {
      return Array.from(document.querySelectorAll('.segment-user, .segment-assistant')).map(node => {
        const user = node.classList.contains('segment-user');
        const body = user ? node.querySelector('.user-content__text') : Array.from(node.querySelectorAll('.markdown-container'))
          .find(el => !el.closest('.toolcall-flow') && !el.classList.contains('toolcall-content-text'));
        if (!body) return null;
        const copy = node.querySelector('.segment-assistant-actions svg[name="Copy"]');
        const redo = node.querySelector('.segment-assistant-actions svg[name="Refresh"]');
        return Object.assign({}, splitBlock(body), {messageId: messageId(node), semantic: user ? 'user' : 'assistant',
          asst: !user, done: !user && !!copy && !!redo});
      }).filter(Boolean).slice(-CONV_SNAP_TAIL);
    }
    if (location.hostname === 'www.qianwen.com') {
      return Array.from(document.querySelectorAll('[data-chat-question-wrap], [data-chat-answers-wrap]'))
        .map(node => {
          const user = node.hasAttribute('data-chat-question-wrap');
          const body = node.querySelector(user ? '.question-text-card' : '.answer-common-card .qk-markdown');
          if (!body) return null;
          return Object.assign({}, splitBlock(body), {messageId: messageId(node), semantic: user ? 'user' : 'assistant',
            asst: !user, done: !user && body.classList.contains('qk-markdown-complete')});
        }).filter(Boolean).slice(-CONV_SNAP_TAIL);
    }
    if (location.hostname === 'chatgpt.com') {
      return Array.from(document.querySelectorAll('[data-user-message-bubble], [data-dil-message-id]'))
        .filter(node => !node.parentElement?.closest('[data-dil-message-id]'))
        .map(node => {
          const user = node.hasAttribute('data-user-message-bubble');
          const part = splitBlock(node);
          const complete = node.closest('[data-talvt-turn-state="complete"]');
          // 模型结束不等于前端渲染结束：隐藏 WebView 的逐字呈现可能仍停在标题。
          // CodeMirror 的 gap 也表示代码行未完整挂入 DOM，不能把可见片段交付为完成。
          const rendering = node.hasAttribute('data-d-streaming-reveal') ||
            !!node.querySelector('[data-d-streaming-reveal], [data-code-block] .cm-gap');
          return Object.assign({}, part, {messageId: messageId(node), semantic: user ? 'user' : 'assistant',
            asst: !user, done: !user && !!complete && !rendering});
        }).slice(-CONV_SNAP_TAIL);
    }
    if (location.hostname !== 'gemini.google.com') return null;
    const parts = [];
    for (const node of document.querySelectorAll('user-query, model-response')) {
      const user = node.tagName === 'USER-QUERY';
      const body = user ? node.querySelector('.query-text') || node.querySelector('.query-text-line') : node.querySelector('message-content');
      if (!body) continue;
      const part = splitBlock(body);
      if (user) {
        const text = Array.from(node.querySelectorAll('.query-text-line')).map(n => n.textContent).join('\n') || body.textContent;
        const escaped = document.createElement('div'); escaped.textContent = text;
        part.ans = escaped.innerHTML;
      }
      const copy = node.querySelector('button[aria-label="复制"],button[aria-label="Copy"]');
      const redo = node.querySelector('button[aria-label="重做"],button[aria-label="Redo"]');
      parts.push(Object.assign({}, part, {messageId: messageId(node), semantic: user ? 'user' : 'assistant',
        asst: !user, done: !user && !!copy && !!redo && isVisibleControl(copy) && isVisibleControl(redo)}));
    }
    return parts.slice(-CONV_SNAP_TAIL);
  }
  function siteSnapshot(parts, prompt) {
    parts = restrictToRequest(parts, prompt);
    const last = parts[parts.length - 1];
    return {url: location.href, ok: true, splitV: 6, requestBound: !!(SEND_CONTEXT && SEND_CONTEXT.bound && parts.length),
      blocks: parts.length, childCount: parts.length, total: parts.reduce((n,p) => n + p.ans.length, 0),
      container: 'site-semantic', streaming: pageStreaming(document.body), pending: !!last?.pending,
      done: !!last && last.semantic === 'assistant' && !last.pending && !!last.done,
      reasoning: hasReasoningFold(), messageIds: parts.map(p => p.messageId),
      texts: parts.map(p => p.ans.slice(0,CONV_SNAP_BLOCK)), thinking: parts.map(p => p.think.slice(0,CONV_SNAP_BLOCK)),
      roles: parts.map(p => p.semantic)};
  }
  function snapshotConversation(prompt, wantDump) {
    try {
      // ★ 结构性硬过滤的锚：我方刚发出的提示词（紧凑归一）。空则退化为「不过滤」（兼容旧调用）。
      const need = compactText(prompt);
      const site = siteConversation();
      if (site !== null) return siteSnapshot(site, prompt);
      let loc = null;
      // 0) 缓存命中：容器仍挂在文档上、仍像消息列表、**仍含我方提示词**、且未超 TTL → 复用。
      //    ★ 增「仍含提示词」这一项（真机缺陷 2026-09-14）：仅凭 url + isConnected + TTL 无法排除
      //      「React 复用同一节点、却把它换成了另一个区域的容器」→ 曾整轮读错容器（假通过）。
      if (
        CONV_CACHE &&
        CONV_CACHE.url === location.href &&
        CONV_CACHE.age < CONV_SNAP_TTL &&
        CONV_CACHE.el &&
        CONV_CACHE.el.isConnected &&
        tailHasEcho(CONV_CACHE.el, need)
      ) {
        const bl = blocksOf(CONV_CACHE.el);
        // ★ 阈值必须随「当初命中哪一遍」而变：第 1 遍用 120、B2 兜底第 2 遍用 20。
        //   曾一律用 `CONV_MIN_TEXTLEN`(120) 校验 → 短会话消息列表 textLen≈80 永远过不了 →
        //   缓存**恒不命中** → 每 250ms 轮询都跑一次全量 DOM 扫描（`querySelectorAll('*')`），
        //   且 `container=` 诊断读数里的 pass 恒为 0（缓存路径没有 scanPass）。
        // ★★ 2026-10-03 三次真机修复：缓存校验也须加「消息列表」强判据 ——
        //   旧判据 `childCount≥2 && textLen≥min` 会**放行 `composer-parent`**
        //   （msgArea + composer，textLen 很大）⇒ 一旦缓存了错容器，**后续每轮都复用它**
        //   ⇒ 整轮读错容器（真机表现为：骨架恒为 `composer-parent`，交付 0 字符）。
        //   与下钻出口一**共用同一判据**（`isMsgListLike` 的语义）：cc≥2 && withText≥2
        //   且**不含输入框本身**（消息列表绝不含输入框）。
        // ★★ 2026-10-03 六次真机（A14）：缓存校验同样须加「尾块是装饰条 ⟹ 不是消息列表」——
        //   否则一旦缓存了「消息区 + 页脚」合体层，后续每轮都复用它（与上面 composer-parent 同病）。
        const cinp = findInput();
        const cacheOk =
          bl.childCount >= 2 &&
          bl.withText >= 2 &&
          bl.textLen >= CONV_CACHE.min &&
          !(cinp && CONV_CACHE.el.contains && CONV_CACHE.el.contains(cinp)) &&
          !tailIsDecor(bl);
        if (cacheOk) {
          CONV_CACHE.bl = bl;
          CONV_CACHE.age++;
          loc = {
            el: CONV_CACHE.el,
            bl: bl,
            hops: CONV_CACHE.hops,
            bestEl: CONV_CACHE.bestEl,
            scanPass: CONV_CACHE.scanPass,
            viaDrillFallback: CONV_CACHE.viaDrillFallback || 0,
            drillTrace: CONV_CACHE.drillTrace || [],
          };
        }
      }
      if (!loc) {
        const l = locateConversation(need);
        if (!l.el) {
          // 未接受任何容器：不写入缓存（可能是回显尚未落盘的正常空窗），让下一轮重试。
          // ★ 必须带上诊断数字（真机排障的唯一线索）：scrollers=0 → 页面里没有任何可滚动容器；
          //   cand=0 → 两遍扫描都没找到候选；echo=0 → 候选**全部**因「不含我方提示词」被 B3 排除
          //   （正是要观察的「假通过」拦截点）；input=0 → 连输入框都没找到（页面还没渲染完）。
          return {
            url: location.href,
            ok: false,
            error:
              '未接受会话容器（scrollers=' +
              l.scrollables.length +
              ' cand=' +
              (l.candBefore || 0) +
              ' echo=' +
              (l.echoCand || 0) +
              ' pass=' +
              (l.scanPass || 0) +
              ' input=' +
              (findInput() ? 1 : 0) +
              ' echoAny=' +
              (l.echoAny ? 1 : 0) +
              ' echoHolder=' +
              (l.echoHolder || '') +
              '）',
          };
        }
        loc = l;
        CONV_CACHE = {
          url: location.href,
          el: l.el,
          bl: l.bl,
          hops: l.hops,
          bestEl: l.bestEl,
          age: 0,
          scanPass: l.scanPass || 0,
          viaDrillFallback: l.viaDrillFallback || 0,
          drillTrace: l.drillTrace || [],
          // 记下这一次命中所用的文本下限，供后续缓存命中校验（见 K2 注释里的缺陷）。
          min: (l.scanPass || 0) === 1 ? CONV_MIN_TEXTLEN : CONV_MIN_TEXTLEN_RELAXED,
        };
      }

      const parts = restrictToRequest(tailBlocks(loc.el), prompt);
      const texts = parts.map(function (p) {
        return p.ans.slice(0, CONV_SNAP_BLOCK);
      });
      // ★ 与 `texts` **逐项对齐**的思考正文（Rust 侧据此把「思考过程」搬上结果卡）。
      //   对齐是硬要求：Rust 的 locate_answer_and_thinking 按下标取同一块。
      const thinking = parts.map(function (p) {
        return p.think.slice(0, CONV_SNAP_BLOCK);
      });
      // ★ #004：**块角色**（与 texts/thinking 逐项对齐）—— 供 Rust 侧「结构化拒绝回显」。
      //   ① assistant 优先：即便该块同时含提示词也判 assistant（避免长回答**引用提示词**时被误拦）；
      //      判据只用**已算好的、prompt 无关的** `p.asst`（来自 splitBlockRaw、随块记忆化缓存）。
      //   ② 非 assistant 且块文本含我方提示词（= `need`）→ user（= 我方那条消息的**回显**）。
      //   ③ 其余 → unknown。
      //   ★ role 依赖 `need`，故**不**把它塞进 splitBlock 缓存（缓存键 = 元素 + 文本指纹，不含 prompt，
      //     塞进去会跨轮「串味」）—— role 在此处（snapshotConversation）**现算**。
      const roles = parts.map(function (p) {
        if (p.semantic === 'assistant' || p.asst) return 'assistant';
        if (p.semantic === 'user' || (need && compactText(htmlPlain(p.ans)) === need)) return 'user';
        return 'unknown';
      });
      // ★ 「已写完」正向信号：末块（最新一条消息）是否已渲染操作行 = 整条消息已完整流式结束。
      //   只在末块真为「回答块」时算数 —— 若末块仍是思考（pending）则 done 恒为 false。
      const done = parts.length
        ? !parts[parts.length - 1].pending && !!parts[parts.length - 1].done
        : false;
      return {
        url: location.href,
        ok: true,
        requestBound: !!(SEND_CONTEXT && SEND_CONTEXT.bound && parts.length),
        // ★ 注入脚本版本探针：Rust 侧据此自证「新分块逻辑已真正运行」。
        //   曾因重启 app 后 WebView 会话被复用、旧脚本仍挂在页上，导致修复看似无效。
        //   日志只要出现 splitV=5 即证明新脚本在线；缺它 ⇒ 强制刷新面板页面 / 重建 WebView。
        //   （4 → 5：#004 起页内契约新增 `roles` 块角色字段 —— 任何页内契约变更都应升版。）
        splitV: 5,
        blocks: loc.bl.withText,
        childCount: loc.bl.childCount,
        total: loc.bl.textLen,
        hops: loc.hops,
        // 诊断：实际选中的容器标识（cls + 命中遍次 + 下钻深度）—— 换成侧栏之类的回归一眼可见。
        container:
          elCls(loc.el) +
          ' pass=' +
          (loc.scanPass || 0) +
          ' hops=' +
          loc.hops +
          ' drill=' +
          (loc.viaDrillFallback || 0) +
          (loc.drillTrace && loc.drillTrace.length ? ' trace=' + loc.drillTrace.join(',') : '') +
          // ★ 临时诊断（2026-10-03 三次真机排查）：容器链条（祖先 4 级 + 自身 + 后代 4 个）。
          //   前几轮 `trace` 与骨架读数无法对齐（疑似缓存复用旧容器）——有此读数即可一次确认层级。
          ' chain=' + chainDump(loc.el),
        // ★ 结构性诊断（只读，**按需**）：容器尾窗骨架（块/子块 + 类名 + 文本头）——
        //   一次真机运行即可判定「思考正文与回答是同块内联还是同块的兄弟子块」，据此决定抽取策略。
        //   `wantDump` 为假时返回空串：调用方（Rust）只在起始与临交付时索取，避免每 250ms 的
        //   轮询都为诊断多跑一遍尾窗子树遍历。
        dump: wantDump ? structDump(loc.el) : '',
        // ★ 结构信号（供 Rust 侧终态判定）：是否仍在流式生成（存在可见的「停止生成」控件）。
        //   ★ E2：优先在**会话容器内**扫描（`loc.el`），容器内没有再退化到全文档兜底。
        streaming: pageStreaming(loc.el),
        // ★ 结构信号：**末块此刻只有思考过程**（切不出回答）→ 回答还没开始，否决终态。
        pending: parts.length ? !!parts[parts.length - 1].pending : false,
        // ★ 「已写完」正向信号：Rust 侧以它为终态「完成」的必要条件（见 readback::decide_done）。
        //   无它（即便静默够久）也**不得**判终 —— 是修复「缺最后一段」的决定性输入。
        done: done,
        // 诊断信号：是否存在「深度思考」折叠区（暂不参与判据，见 §13.5 真机清单）。
        reasoning: hasReasoningFold(),
        // ★ R1 主数据：尾窗各块的可读文本（末项 = 最新一条消息）。
        // Rust 侧据此定位「本轮回声 → 其后一块」得本轮回答，见 readback::locate_answer。
        messageIds: parts.map(p => p.messageId),
        texts: texts,
        // R3：思考过程（与 texts 逐项对齐）。见 readback::locate_answer_and_thinking。
        thinking: thinking,
        // ★ #004：块角色（与 texts/thinking 逐项对齐，取值 assistant/user/unknown）。见
        //   readback::analyze::locate_answer_and_thinking（Rust 侧只在正向识别 user 时拒绝交付）。
        roles: roles,
      };
    } catch (e) {
      return { url: location.href, ok: false, error: String((e && e.message) || e) };
    }
  }

  // ★ R8-④：开新会话 —— 定位并点击 DeepSeek 网页端的「新对话」按钮。
  //   由 Rust 侧 `result::new_session` 经 `panel.eval("...__DSFLOAT__.newChat()")` 触发：
  //   面板窗不能回调 Tauri（约束 G5），故只能由 Rust 反向 eval，无法让面板脚本自发调用。
  //   返回 true = 找到并点击了按钮；false = 没找到（页面结构变了 / 当前不在对话页）。
  //
  //   ★ R9 加固：DeepSeek 的「新对话」入口实测**可能不含任何文案**（纯图标按钮，无 aria-label），
  //   单靠文案匹配会漏。故补两路：① `<a>` 的 href 指向 `/` 或含 `chat.deepseek.com`（首页即新会话）；
  //   ② 全部未命中时**导航到首页**兜底（`location.href = '/'` 等价于点「新对话」）。
  function newChat() {
    const candidates = Array.from(
      document.querySelectorAll('button, a, [role="button"]')
    );
    const pat = /新对话|开启新对话|新建对话|新会话|new\s*chat/i;
    for (const el of candidates) {
      const txt = (el.textContent || '').trim();
      const title = (el.getAttribute && el.getAttribute('title')) || '';
      const aria = (el.getAttribute && el.getAttribute('aria-label')) || '';
      if (pat.test(txt + ' ' + title + ' ' + aria)) {
        try {
          el.click();
          return true;
        } catch (_e) {
          /* 点击失败则继续找下一个候选 */
        }
      }
    }
    // ① 文案未命中 → 找指向首页的链接（DeepSeek 侧栏「新对话」常是 `<a href="/">`）。
    for (const a of Array.from(document.querySelectorAll('a'))) {
      const href = (a.getAttribute && a.getAttribute('href')) || '';
      if (href === '/' || href === '' || /^https?:\/\/chat\.deepseek\.com\/?$/i.test(href)) {
        try {
          a.click();
          return true;
        } catch (_e) {
          /* 继续找 */
        }
      }
    }
    // ② 全部未命中 → 导航到首页兜底（DeepSeek 首页即新会话）。
    try {
      if (location.pathname !== '/' || location.search) {
        location.href = '/';
        return true;
      }
    } catch (_e) {
      /* 导航失败 */
    }
    return false;
  }

  // ★ R11（S3）：截图投递 —— 分块接收 PNG 的 base64，拼成 `File` 挂到 DeepSeek 的
  //   `input[type=file]`（设计文档 §5.6 路线①：页内 DataTransfer 注入，不碰系统剪贴板、
  //   不依赖前台焦点）。由 Rust 侧 `screenshot::deliver_png` 经 `eval_with_callback` 驱动。
  //
  //   为什么分块：4 MiB PNG → 5.4 MiB base64，单条 `eval` 脚本过大。每块带 `seq` 校验，
  //   乱序/重复一律丢弃 —— 否则字节错位会产出**损坏图片**且不报错。
  let _shotChunks = [];
  let attachmentBaseline = new Set();
  function attachmentRoot() {
    const input = findInput();
    const root = input ? composerRootOf(input) : null;
    if (location.hostname === 'chatgpt.com' && input) {
      const shell = input.closest('[data-composer-surface-variant]');
      if (shell) return shell;
    }
    // DeepSeek 的附件列表是输入/发送控件的兄弟节点；只扩一层，且不能包含历史消息
    // 或第二个编辑器。发送按钮仍使用原来更窄的 composerRootOf 边界。
    const parent = root && root.parentElement;
    if (location.hostname === 'chat.deepseek.com' && parent && !['BODY', 'HTML', 'MAIN'].includes(parent.tagName) &&
        parent.querySelectorAll('textarea,[contenteditable="true"]').length === 1 &&
        !parent.querySelector('.ds-message,[data-message-author-role]')) return parent;
    return root;
  }
  function shotReset() {
    _shotChunks = [];
    const root = attachmentRoot();
    attachmentBaseline = new Set(root ? Array.from(root.querySelectorAll('img')).map(el => el.getAttribute('src')) : []);
    return true;
  }
  function shotPush(chunk, seq) {
    if (typeof chunk !== 'string') return false;
    if (seq !== _shotChunks.length) return false; // 顺序/去重校验
    _shotChunks.push(chunk);
    return true;
  }

  // 选「最像图片上传」的 file input：accept 含 image/图片扩展名者优先。
  function findFileInput() {
    const root = attachmentRoot();
    if (!root) return null;
    const list = root.querySelectorAll('input[type=file]');
    let best = null;
    let bestScore = -1;
    for (let i = 0; i < list.length; i++) {
      const el = list[i];
      const accept = String(el.getAttribute('accept') || '');
      if (el.hasAttribute('capture')) continue;
      if (el.disabled || (accept && accept !== '*' && accept !== '*/*' && !/image|\.png|\.jpe?g|\.webp|\.gif|\.bmp/i.test(accept))) continue;
      let score = 0;
      if (/image|\.png|\.jpe?g|\.webp|\.gif|\.bmp/i.test(accept)) score += 3;
      if (/image/i.test(accept) && !/video/i.test(accept)) score += 1;
      if (accept === '' || accept === '*') score += 1;
      if (el.multiple) score += 0;
      if (score > bestScore) {
        bestScore = score;
        best = el;
      }
    }
    return best;
  }
  function base64ToBytes(b64) {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i) & 0xff;
    return out;
  }
  function shotCommit(mime) {
    try {
      const input = findFileInput();
      if (!input) return false;
      const bytes = base64ToBytes(_shotChunks.join(''));
      _shotChunks = [];
      const file = new File([bytes], 'shot.png', { type: mime || 'image/png' });
      const dt = new DataTransfer();
      dt.items.add(file);
      input.files = dt.files;
      // React 受控组件靠 input/change 事件感知（同 `setNativeValue` 的口径）。
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    } catch (e) {
      return false;
    }
  }
  // 复核信号：输入区是否出现图片附件缩略图（blob: / data:image）。仅用于诊断日志。
  function hasAttachment() {
    const root = attachmentRoot();
    if (!root || root.querySelector('[aria-busy="true"], [role="progressbar"]')) return false;
    if (location.hostname === 'chat.deepseek.com' && root.querySelector('.ds-loading')) return false;
    const imgs = root.querySelectorAll('img');
    for (let i = 0; i < imgs.length; i++) {
      const src = String(imgs[i].getAttribute('src') || '');
      if (src && !attachmentBaseline.has(src) && /^(blob:|data:image\/|https?:)/i.test(src)) {
        const r = imgs[i].getBoundingClientRect();
        if (r.width > 8 && r.height > 8) return true;
      }
    }
    return false;
  }

  // ★ 安全硬化（P3）：以**不可枚举**属性挂在 window 上，避免污染 window 枚举
  // （for..in / Object.keys(window) 不可见），降低与远端页同名全局冲突、或被脚本探到的概率。
  // 仍可用 window.__DSFLOAT__ 直接访问（Rust 侧 eval 走的就是属性访问，不受 enumerable 影响，
  // 因此 inject.rs 里的 window.__DSFLOAT__.fill/submit 调用路径无需改动）。
  const api = {
    prepareSend: prepareSend,
    sendEvidence: sendEvidence,
    fill: fill,
    submit: submit,
    pressEnter: pressEnter,
    stillHasText: stillHasText,
    inspect: inspect,
    inspectUpload: inspectUpload,
    inspectConversation: inspectConversation,
    snapshotConversation: snapshotConversation,
    readText: function () { return readText(findInput()); },
    newChat: newChat,
    shotReset: shotReset,
    shotPush: shotPush,
    shotCommit: shotCommit,
    hasAttachment: hasAttachment,
  };
  Object.defineProperty(window, '__DSFLOAT__', {
    value: api,
    enumerable: false,
    writable: false,
    configurable: false,
  });
})();
