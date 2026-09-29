//! One copy of the Node SDK per process (Decision 20).
//!
//! The native library is loaded once per process, and each package copy
//! registers its callback tables in it when it initializes. A second copy,
//! for example one loaded through CommonJS next to an ESM copy, or one loaded
//! by a worker thread, would replace the first copy's tables. The generated
//! index claims the process in the native library before it initializes the
//! binding; a later copy fails at load with a public error, before it
//! registers anything. A JavaScript marker would not work: each worker thread
//! has its own `globalThis`.

use anyhow::{Result, bail};

const INITIALIZE: &str =
    "let initialized = false;\nif (!initialized) {\n  xmtp_sdk.default.initialize();";

const GUARD: &str = r#"import { XmtpError as PublicXmtpError } from './public-values.gen';
// The native callback tables are process-wide. Only the first package copy in
// the process, on any thread, may register them; the native claim is atomic.
if (!xmtp_sdk.sdkClaimJsHost())
  throw new PublicXmtpError.Unknown({ code: "Unknown", category: "unknown", retryable: false, message: "the XMTP SDK was loaded twice in one process; load it once, from one thread" });

"#;

/// Insert the guard before the index initializes the binding.
pub(crate) fn guard(index: &str) -> Result<String> {
    if index.matches(INITIALIZE).count() != 1 {
        bail!("generated TypeScript index no longer initializes the binding in one place");
    }
    Ok(index.replacen(INITIALIZE, &format!("{GUARD}{INITIALIZE}"), 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn guard_runs_before_initialization() {
        let index = format!(
            "import * as xmtp_sdk from './xmtp_sdk';\n{INITIALIZE}\n  initialized = true;\n}}\n"
        );
        let guarded = guard(&index)?;
        let claim = guarded.find("xmtp_sdk.sdkClaimJsHost()").unwrap();
        let initialize = guarded.find("xmtp_sdk.default.initialize();").unwrap();
        assert!(claim < initialize, "the claim must precede initialization");
        assert!(guarded.contains("loaded twice in one process"));
        assert!(guard("no initialization").is_err());
    }
}
