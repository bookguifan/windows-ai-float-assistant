//! 首版公开构建的集中能力约束。配置、备份、环境变量和IPC都不能解锁。
//! UI Automation读取和用户主动复制不属于自动复制回退，不受此约束影响。

pub(crate) const fn automatic_copy_allowed(_requested: bool) -> bool {
    false
}

pub(crate) fn validate_automatic_copy_request(enabled: bool) -> Result<(), String> {
    if enabled && !automatic_copy_allowed(enabled) {
        return Err("本公开构建已禁用自动复制回退；请使用UIA划词或主动复制".into());
    }
    Ok(())
}

/// New entry only: native consent and transaction context are independent gates.
pub(crate) fn safe_copy_allowed(
    authorized: bool,
    target_valid: bool,
    transaction_live: bool,
) -> bool {
    cfg!(target_os = "windows") && authorized && target_valid && transaction_live
}

#[cfg(test)]
mod tests {
    #[test]
    fn enabling_is_rejected_and_disabling_is_accepted() {
        assert!(super::validate_automatic_copy_request(true).is_err());
        assert!(super::validate_automatic_copy_request(false).is_ok());
    }
}
