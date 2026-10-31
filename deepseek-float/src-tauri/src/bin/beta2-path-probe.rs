//! Console-only path probe; never calls application run(), Builder or WebView creation.
fn main() {
    let result = (|| -> Result<(), String> {
        let mut context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        deepseek_float_lib::isolation::initialize(
            std::env::args_os(),
            &context.config().identifier,
        )?;
        deepseek_float_lib::isolation::configure_context(&mut context)?;
        let manifest = deepseek_float_lib::isolation::probe_manifest(&context)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?
        );
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("PATH_PROBE_REJECTED: {error}");
        std::process::exit(78);
    }
}
