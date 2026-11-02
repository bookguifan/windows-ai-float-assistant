(() => {
  const allowed = new Set([
    "document-start",
    "dom-content-loaded",
    "script-start",
    "ipc-available",
    "ipc-unavailable",
    "first-frame-callback",
    "ready-invoke-start",
    "ready-invoke-ok",
    "ready-invoke-rejected",
    "javascript-error",
    "unhandled-rejection",
  ]);
  const record = (stage) => {
    if (!allowed.has(stage)) return;
    const bridge = window.__TAURI_INTERNALS__;
    if (!bridge || typeof bridge.invoke !== "function") return;
    try {
      bridge.invoke("ball_diagnostic", { stage }).catch(() => {});
    } catch (_) {}
  };
  Object.defineProperty(window, "__DSFLOAT_BALL_DIAG__", {
    value: record,
    configurable: false,
  });
  window.addEventListener("error", () => record("javascript-error"));
  window.addEventListener("unhandledrejection", () => record("unhandled-rejection"));
  document.addEventListener("DOMContentLoaded", () => record("dom-content-loaded"), { once: true });
  record("document-start");
})();
