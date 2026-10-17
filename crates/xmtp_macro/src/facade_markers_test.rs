//! The SDK generator trusts the metadata markers that `sdk_export` writes.
//! One written by hand skips the macro's checks, and the macro sees only the
//! items it exports, so this test reads every source file of the
//! `xmtp_sdk` façade. It reads each file as Rust tokens, where every doc
//! comment, `///` or `/** */`, is a `doc` attribute, so no line break can
//! hide anything. It flags a string literal that holds a macro-owned marker,
//! and a `doc` attribute whose value is not a string literal, such as
//! `concat!(...)`, which may expand into one.

use std::{
    error::Error,
    path::{Path, PathBuf},
};

use proc_macro2::{Delimiter, TokenStream, TokenTree};

use crate::sdk_member::WRITTEN_BY_OPTIONS;

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

fn punct(token: Option<&TokenTree>, char: char) -> bool {
    matches!(token, Some(TokenTree::Punct(punct)) if punct.as_char() == char)
}

/// Whether a token is a string literal: `"..."`, `r"..."` or `r#"..."#`.
fn string_literal(token: Option<&TokenTree>) -> bool {
    let Some(TokenTree::Literal(literal)) = token else {
        return false;
    };
    let text = literal.to_string();
    text.starts_with('"') || text.starts_with("r\"") || text.starts_with("r#")
}

/// Push what `tokens` write that the macro would reject. `attribute` says
/// whether they sit inside `#[...]` or `#![...]`, where `doc = value` is a
/// doc attribute, also inside `cfg_attr`.
fn scan(tokens: TokenStream, attribute: bool, found: &mut Vec<String>) {
    let tokens = tokens.into_iter().collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        let before = |back: usize| index.checked_sub(back).map(|at| &tokens[at]);
        match token {
            TokenTree::Group(group) => {
                let opens = group.delimiter() == Delimiter::Bracket
                    && (punct(before(1), '#') || (punct(before(1), '!') && punct(before(2), '#')));
                scan(group.stream(), attribute || opens, found);
            }
            TokenTree::Literal(literal) => {
                let text = literal.to_string();
                if WRITTEN_BY_OPTIONS
                    .iter()
                    .any(|(marker, _)| text.contains(marker))
                {
                    found.push(text);
                }
            }
            TokenTree::Ident(ident) if attribute && ident == "doc" => {
                // `doc == x` compares; `doc = x` assigns.
                let assigns =
                    punct(tokens.get(index + 1), '=') && !punct(tokens.get(index + 2), '=');
                if assigns && !string_literal(tokens.get(index + 2)) {
                    let value = tokens
                        .get(index + 2)
                        .map_or_else(String::new, ToString::to_string);
                    found.push(format!("doc = {value}"));
                }
            }
            _ => {}
        }
    }
}

/// What `source` writes that the macro would reject.
fn written_markers(source: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let mut found = Vec::new();
    scan(source.parse()?, false, &mut found);
    Ok(found)
}

// A block doc comment reaches UniFFI's docstring like a `///` one does, and
// a computed doc value may expand into a marker however the attribute is
// split across lines.
#[test]
fn every_doc_comment_form_is_scanned() -> Result<(), Box<dyn Error>> {
    for (source, flagged) in [
        ("/** @xmtp-redact */ struct A;", true),
        ("/**\n * A key.\n * @xmtp-redacted\n */\nstruct A;", true),
        (
            "#[cfg_attr(all(), doc = \"@xmtp-kind=lagged\")] struct A;",
            true,
        ),
        (
            "/// Made by the worker. @xmtp-worker @xmtp-internal\nstruct A;",
            false,
        ),
        ("#[doc = concat!(\"@xmtp-red\", \"act\")] struct A;", true),
        (
            "#[cfg_attr(all(), doc=include_str!(\"key.md\"))] struct A;",
            true,
        ),
        ("#[doc\n = concat!(\"@xmtp-red\", \"act\")] struct A;", true),
        (
            "#[doc =\n    concat!(\"@xmtp-red\", \"acted\")] struct A;",
            true,
        ),
        ("#![doc = concat!(\"@xmtp-red\", \"act\")]", true),
        (
            "macro_rules! m { ($d:expr) => { #[doc =$d] struct A; } }",
            true,
        ),
        (
            "#[doc = \"A literal.\"] #[doc = r\"Raw.\"] #[doc = r#\"Raw.\"#] #[doc(hidden)] struct A;",
            false,
        ),
        (
            "fn f() { let doc = g(); let docs = 1; if doc == 3 {} }",
            false,
        ),
    ] {
        let found = written_markers(source)?;
        assert_eq!(!found.is_empty(), flagged, "{source}: {found:?}");
    }
    Ok(())
}

#[test]
fn facade_writes_no_macro_marker() -> Result<(), Box<dyn Error>> {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../xmtp_sdk/src"),
        &mut files,
    )?;
    assert!(files.len() > 100, "found {} source files", files.len());
    let mut written = Vec::new();
    for file in files {
        let source = std::fs::read_to_string(&file)?;
        for found in written_markers(&source)? {
            written.push(format!("{}: {found}", file.display()));
        }
    }
    assert!(
        written.is_empty(),
        "write the sdk option instead of the marker, and doc values as string literals:\n{}",
        written.join("\n")
    );
    Ok(())
}
