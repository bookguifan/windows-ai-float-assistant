//! Automatic copy is available only through the guarded, explicitly consented path.
//! Legacy config/backup and the old capture_selection entry point remain locked.

pub(crate) const fn automatic_copy_allowed(_requested: bool) -> bool {
    false
}

pub(crate) fn validate_automatic_copy_request(enabled: bool) -> Result<(), String> {
    if enabled && !automatic_copy_allowed(enabled) {
        return Err("旧配置不能授权自动复制回退；请在设置页确认本次运行授权".into());
    }
    Ok(())
}

/// Research-only switch; release builds never treat this as authorization.
pub(crate) const fn experimental_copy_available() -> bool {
    cfg!(all(
        debug_assertions,
        not(feature = "custom-protocol"),
        feature = "experimental-clipboard-fallback",
        target_os = "windows"
    ))
}

/// The guarded transaction ships in ordinary Windows Release. Debug research
/// builds may opt in separately; neither build mode is runtime authorization.
pub(crate) const fn safe_copy_available() -> bool {
    cfg!(all(
        not(debug_assertions),
        feature = "custom-protocol",
        target_os = "windows"
    )) || experimental_copy_available()
}

/// Native consent and live target/transaction context are all required.
pub(crate) fn safe_copy_allowed(
    authorized: bool,
    target_valid: bool,
    transaction_live: bool,
) -> bool {
    safe_copy_available() && authorized && target_valid && transaction_live
}

#[cfg(test)]
mod tests {
    #[test]
    fn enabling_is_rejected_and_disabling_is_accepted() {
        assert!(super::validate_automatic_copy_request(true).is_err());
        assert!(super::validate_automatic_copy_request(false).is_ok());
    }
}
