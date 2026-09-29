//! One copy of the Node SDK per process (Decision 20).
//!
//! The native library is loaded once per process, and each package copy
//! registers its callback tables in it when it initializes. A second copy, for
//! example one loaded through CommonJS next to an ESM copy, would replace the
//! first copy's tables and crash later. The generated index records a
//! process-global marker before it initializes the binding; a different copy
//! then fails at load with a public error, before it registers anything.

use anyhow::{Result, bail};

const INITIALIZE: &str =
    "let initialized = false;\nif (!initialized) {\n  xmtp_sdk.default.initialize();";

const GUARD: &str = r#"import { XmtpError as PublicXmtpError } from './public-values.gen';
// One copy of this package per process: its binding namespace is the marker.
const loadedKey = Symbol.for("xmtp.sdk.node.loaded");
const loadedCopy: unknown = Reflect.get(globalThis, loadedKey);
if (loadedCopy !== undefined && loadedCopy !== xmtp_sdk)
  throw new PublicXmtpError.Unknown({ code: "Unknown", category: "unknown", retryable: false, message: "the XMTP SDK was loaded twice in one process; load it once" });
Reflect.set(globalThis, loadedKey, xmtp_sdk);

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
        let marker = guarded
            .find("Reflect.set(globalThis, loadedKey, xmtp_sdk);")
            .unwrap();
        let initialize = guarded.find("xmtp_sdk.default.initialize();").unwrap();
        assert!(
            marker < initialize,
            "the marker must precede initialization"
        );
        assert!(guarded.contains("loaded twice in one process"));
        assert!(guard("no initialization").is_err());
    }
}
