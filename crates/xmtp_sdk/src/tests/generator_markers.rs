//! The SDK generator trusts the metadata markers that `xmtp_macro::sdk_export`
//! writes. A doc comment that spells one out skips the macro's checks, and
//! the macro sees only the items it exports, so this test reads every source
//! file of the façade.

use std::path::{Path, PathBuf};

/// The markers that only the macro writes. Only `@xmtp-worker` and
/// `@xmtp-internal` are written by hand.
const MACRO_MARKERS: &[&str] = &[
    "@xmtp-immutable",
    "@xmtp-kind",
    "@xmtp-pure",
    "@xmtp-redact",
];

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

/// A doc comment, or an attribute that may hold `doc = "..."`.
fn documentation(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("///") || line.starts_with("//!") || line.starts_with("#[")
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
fn facade_doc_comments_write_no_macro_marker() {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    )?;
    assert!(files.len() > 100, "found {} source files", files.len());
    let mut written = Vec::new();
    for file in files {
        let source = std::fs::read_to_string(&file)?;
        for (index, line) in source.lines().enumerate() {
            if documentation(line) && MACRO_MARKERS.iter().any(|marker| line.contains(marker)) {
                written.push(format!("{}:{}: {}", file.display(), index + 1, line.trim()));
            }
        }
    }
    assert!(
        written.is_empty(),
        "write the sdk option instead of the marker:\n{}",
        written.join("\n")
    );
}
