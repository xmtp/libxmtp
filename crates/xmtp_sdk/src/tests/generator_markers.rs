//! The SDK generator trusts the metadata markers that `xmtp_macro::sdk_export`
//! writes. One written by hand skips the macro's checks, and the macro sees
//! only the items it exports, so this test reads every source file of the
//! façade. It flags a marker on any line, whatever carries it: a `///` or
//! `/** */` doc comment, a continuation line of a block comment, or a
//! `#[doc]` attribute. It also flags a `doc` value that is not a string
//! literal, such as `concat!(...)`, which may expand into a marker.

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

/// Whether `line` gives a `doc` attribute a value that is not a string
/// literal, or leaves the value to the next line.
fn computed_doc(line: &str) -> bool {
    line.match_indices("doc").any(|(index, _)| {
        let word = !line[..index]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || before == '_');
        let value = line[index + "doc".len()..]
            .trim_start()
            .strip_prefix('=')
            .filter(|value| !value.starts_with('='))
            .map(str::trim_start);
        word && value.is_some_and(|value| {
            !["\"", "r\"", "r#"]
                .iter()
                .any(|literal| value.starts_with(literal))
        })
    })
}

/// The lines of `source` that spell out a macro-owned marker or compute a
/// doc value, numbered from 1.
fn written_markers(source: &str) -> Vec<(usize, &str)> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            MACRO_MARKERS.iter().any(|marker| line.contains(marker)) || computed_doc(line)
        })
        .map(|(index, line)| (index + 1, line.trim()))
        .collect()
}

// A block doc comment reaches UniFFI's docstring like a `///` one does, and
// a computed doc value may expand into a marker.
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
#[doc = concat!(\"@xmtp-red\", \"act\")]
#[cfg_attr(all(), doc=include_str!(\"key.md\"))]
#[doc =
    concat!(\"@xmtp-red\", \"acted\")]
#[doc = \"A literal.\"] #[doc = r\"A raw literal.\"] #[doc(hidden)]
let docs = 1; let rustdoc = 2; if doc == 3 {}
fn read() {}
";
    let lines = written_markers(source)
        .into_iter()
        .map(|(line, _)| line)
        .collect::<Vec<_>>();
    assert_eq!(lines, [1, 4, 6, 8, 9, 10]);
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
        "write the sdk option instead of the marker, and doc values as string literals:\n{}",
        written.join("\n")
    );
}
