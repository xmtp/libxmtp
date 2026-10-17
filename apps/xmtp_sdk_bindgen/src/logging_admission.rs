//! The log queue keeps transport credit until the host admits the app call.
//! These two narrow safety hooks add no public API and leave stock casing intact.
use anyhow::{Result, bail};

fn replace_once(source: &str, old: &str, new: &str) -> Result<String> {
    let count = source.matches(old).count();
    if count != 1 {
        bail!("log admission: expected one {old:?}, found {count}");
    }
    Ok(source.replacen(old, new, 1))
}

pub fn swift(source: &str) -> Result<String> {
    let private = replace_once(
        source,
        "public func sdkLogSinkHandoff()",
        "private func sdkLogSinkHandoff()",
    )?;
    replace_once(
        &private,
        "                return try await uniffiObj.log(\n                     record: try FfiConverterTypeLogRecord_lift(record)\n                )",
        "                let logRecord = try FfiConverterTypeLogRecord_lift(record)\n                guard sdkLogSinkHandoff() else { return }\n                return try await uniffiObj.log(record: logRecord)",
    )
}

pub fn kotlin(source: &str) -> Result<String> {
    let private = replace_once(
        source,
        "fun `sdkLogSinkHandoff`()",
        "private fun `sdkLogSinkHandoff`()",
    )?;
    replace_once(
        &private,
        "                uniffiObj.`log`(\n                    FfiConverterTypeLogRecord.lift(`record`),\n                )",
        "                val logRecord = FfiConverterTypeLogRecord.lift(`record`)\n                if (sdkLogSinkHandoff()) { uniffiObj.`log`(logRecord) }",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[xmtp_common::test(unwrap_try = true)]
    fn refuses_missing_or_ambiguous_log_admission_anchors() {
        assert!(swift("public func sdkLogSinkHandoff() {}").is_err());
        assert!(kotlin("no helper").is_err());
        assert!(kotlin("fun `sdkLogSinkHandoff`() fun `sdkLogSinkHandoff`()").is_err());
    }
}
