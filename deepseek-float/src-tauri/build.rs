fn main() {
    tauri_build::build();

    // ★★ 硬性约束（E1，2026-09-23 审计排雷）：release 构建必须开启 `custom-protocol`，
    //    否则 `tauri::generate_context!` 走 dev 模式 —— 前端资源不内嵌、exe 脱离 dev server
    //    即白屏（R20 真机事故）。详见 Cargo.toml 的 [features] 注释。
    //    漏开时直接 panic! 阻断构建并给出明确提示，而不是发布一个白屏 exe。
    if std::env::var("PROFILE").as_deref() == Ok("release")
        && std::env::var("CARGO_FEATURE_CUSTOM_PROTOCOL").is_err()
    {
        panic!(
            "release 构建必须启用 --features custom-protocol，否则前端资源无法加载（白屏）。\n\
             正确命令：cargo build --release --features custom-protocol\n\
             或使用 Tauri CLI：pnpm build:release / tauri build"
        );
    }
}
