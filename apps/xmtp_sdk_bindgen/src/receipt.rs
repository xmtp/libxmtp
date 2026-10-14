//! The receipt, `sdk-contract.json`, that `render` writes beside each tree.
//!
//! Staging and the product checks read it. `files` names every generated file
//! with its SHA-256; `artifact` describes the library that the tree binds; and
//! `contract` digests the library's bytes and the generator identity, so a
//! check can compare a tree with the library it came from. The generator is
//! named by its identity, not its bytes: a cache can hold one realization of
//! the bindgen while a tree was rendered by another.

use std::{collections::BTreeMap, fmt::Write as _, fs};

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

pub(crate) const NAME: &str = "sdk-contract.json";

/// The library a tree binds, as its build recorded it.
pub(crate) struct Artifact<'a> {
    /// `native`, `wasm`, or `pure`.
    pub role: &'a str,
    pub library: &'a Utf8Path,
    pub source: &'a str,
    pub generator: &'a str,
    pub features: &'a str,
    pub profile: &'a str,
    pub target: &'a str,
}

enum Json {
    Text(String),
    Object(Vec<(String, Json)>),
}

fn text(value: &str) -> Json {
    Json::Text(value.to_owned())
}

/// Write `artifact`'s receipt for the tree at `out`.
pub(crate) fn write(out: &Utf8Path, artifact: &Artifact) -> Result<()> {
    let files = tree_files(out)?;
    let library = digest_file(artifact.library)?;
    // `nix/check-sdk-products.py` recomputes this digest.
    let role = |(name, checksum): &(String, String)| {
        Json::Object(vec![
            (
                "files".into(),
                Json::Object(vec![(name.clone(), text(checksum))]),
            ),
            ("generator".into(), text(artifact.generator)),
        ])
    };
    let roles = vec![(artifact.role.to_owned(), role(&library))];
    let contract = format!(
        "{:x}",
        Sha256::digest(python_json(&Json::Object(roles)).as_bytes())
    );
    let record = Json::Object(vec![
        ("contract".into(), Json::Text(contract)),
        ("generator".into(), text(artifact.generator)),
        (
            "artifact".into(),
            Json::Object(vec![
                ("source".into(), text(artifact.source)),
                ("generator".into(), text(artifact.generator)),
                ("features".into(), text(artifact.features)),
                ("profile".into(), text(artifact.profile)),
                ("target".into(), text(artifact.target)),
                (
                    "files".into(),
                    Json::Object(vec![(library.0, Json::Text(library.1))]),
                ),
            ]),
        ),
        (
            "files".into(),
            Json::Object(
                files
                    .into_iter()
                    .map(|(name, checksum)| (name, Json::Text(checksum)))
                    .collect(),
            ),
        ),
    ]);
    let mut body = String::new();
    pretty(&record, 0, &mut body);
    body.push('\n');
    fs::write(out.join(NAME), body).with_context(|| format!("write {}", out.join(NAME)))
}

/// A file's name and SHA-256. Receipts name files, never local or store paths.
fn digest_file(path: &Utf8Path) -> Result<(String, String)> {
    let name = path
        .file_name()
        .with_context(|| format!("{path} has no file name"))?;
    let bytes = fs::read(path).with_context(|| format!("read {path}"))?;
    Ok((name.to_owned(), format!("{:x}", Sha256::digest(bytes))))
}

/// Every file of the tree by its `/`-separated relative path, except the
/// receipt and installed `node_modules`.
fn tree_files(root: &Utf8Path) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![Utf8PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let directory = root.join(&relative);
        for entry in directory
            .read_dir_utf8()
            .with_context(|| format!("read {directory}"))?
        {
            let entry = entry?;
            let name = entry.file_name();
            let path = relative.join(name);
            if name == "node_modules" || path == NAME {
                continue;
            }
            if entry.path().is_dir() {
                pending.push(path);
            } else {
                let key = path
                    .components()
                    .map(|part| part.as_str())
                    .collect::<Vec<_>>();
                files.insert(key.join("/"), digest_file(entry.path())?.1);
            }
        }
    }
    Ok(files)
}

/// JSON text as Python's `json.dumps(value)` writes it: `, ` and `: `
/// separators and ASCII-only strings.
fn python_json(value: &Json) -> String {
    match value {
        Json::Text(value) => quote(value),
        Json::Object(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(key, value)| format!("{}: {}", quote(key), python_json(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn pretty(value: &Json, depth: usize, out: &mut String) {
    match value {
        Json::Text(value) => out.push_str(&quote(value)),
        Json::Object(fields) if fields.is_empty() => out.push_str("{}"),
        Json::Object(fields) => {
            out.push_str("{\n");
            for (index, (key, value)) in fields.iter().enumerate() {
                let _ = write!(out, "{}{}: ", "  ".repeat(depth + 1), quote(key));
                pretty(value, depth + 1, out);
                out.push_str(if index + 1 < fields.len() {
                    ",\n"
                } else {
                    "\n"
                });
            }
            let _ = write!(out, "{}}}", "  ".repeat(depth));
        }
    }
}

fn quote(value: &str) -> String {
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\u{8}' => quoted.push_str("\\b"),
            '\u{c}' => quoted.push_str("\\f"),
            ' '..='~' => quoted.push(character),
            _ => {
                for unit in character.encode_utf16(&mut [0; 2]) {
                    let _ = write!(quoted, "\\u{unit:04x}");
                }
            }
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn receipt_lists_every_generated_file() {
        let directory = tempfile::tempdir()?;
        let root = Utf8Path::from_path(directory.path()).context("UTF-8 path")?;
        for (path, body) in [
            ("index.ts", "index"),
            ("runtime/public/client.ts", "client"),
            ("node_modules/@ubjs/core/index.js", "installed"),
            (NAME, "earlier receipt"),
        ] {
            fs::create_dir_all(root.join(path).parent().context("parent")?)?;
            fs::write(root.join(path), body)?;
        }
        let files = tree_files(root)?;
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            ["index.ts", "runtime/public/client.ts"]
        );
        assert_eq!(files["index.ts"], format!("{:x}", Sha256::digest(b"index")));
    }

    // `nix/check-sdk-products.py` recomputes the contract with `json.dumps`.
    #[xmtp_common::test(unwrap_try = true)]
    fn contract_text_matches_python_json() {
        let value = Json::Object(vec![
            ("a".into(), text("é\"")),
            ("b".into(), Json::Object(vec![("c".into(), text("d"))])),
        ]);
        // python3 -c 'import json; print(json.dumps({"a": "é\"", "b": {"c": "d"}}))'
        assert_eq!(python_json(&value), r#"{"a": "\u00e9\"", "b": {"c": "d"}}"#);
    }
}
