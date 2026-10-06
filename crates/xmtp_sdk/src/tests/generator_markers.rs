//! The SDK generator trusts the metadata markers that `xmtp_macro::sdk_export`
//! writes. One written by hand skips the macro's checks, and the macro sees
//! only the items it exports, so this test reads every source file of the
//! façade. It flags a marker on any line, whatever carries it: a `///` or
//! `/** */` doc comment, a continuation line of a block comment, or a
//! `#[doc]` attribute.

#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

/// The markers that only the macro writes. Only `@xmtp-worker` and
/// `@xmtp-internal` are written by hand.
const MACRO_MARKERS: &[&str] = &[
    "@xmtp-immutable",
    "@xmtp-kind",
    "@xmtp-pure",
    "@xmtp-redact",
    "@xmtp-redacted",
];

#[cfg(not(target_arch = "wasm32"))]
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

/// The lines of `source` that spell out a macro-owned marker, numbered from 1.
fn written_markers(source: &str) -> Vec<(usize, &str)> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| MACRO_MARKERS.iter().any(|marker| line.contains(marker)))
        .map(|(index, line)| (index + 1, line.trim()))
        .collect()
}

// A block doc comment reaches UniFFI's docstring like a `///` one does.
#[xmtp_common::test(unwrap_try = true)]
fn every_doc_comment_form_is_scanned() {
    let source = "\
/** @xmtp-redact */
/**
 * A key.
 * @xmtp-redacted
 */
#[cfg_attr(all(), doc = \"@xmtp-kind=lagged\")]
/// Made by the worker. @xmtp-worker @xmtp-internal
fn read() {}
";
    let lines = written_markers(source)
        .into_iter()
        .map(|(line, _)| line)
        .collect::<Vec<_>>();
    assert_eq!(lines, [1, 4, 6]);
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
        // This file names the markers it looks for.
        if file.ends_with("tests/generator_markers.rs") {
            continue;
        }
        let source = std::fs::read_to_string(&file)?;
        for (line, text) in written_markers(&source) {
            written.push(format!("{}:{line}: {text}", file.display()));
        }
    }
    assert!(
        written.is_empty(),
        "write the sdk option instead of the marker:\n{}",
        written.join("\n")
    );
}
