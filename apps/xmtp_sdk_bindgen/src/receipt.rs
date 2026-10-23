//! The receipt, `sdk-contract.json`, that `render` writes beside each tree.
//!
//! Staging and the product checks read it. `files` names every generated file
//! with its SHA-256; `artifact` describes the library that the tree binds; and
//! `contract` digests the bytes of the library that this render read and the
//! generator identity. Checks compare identities, not library or bindgen
//! bytes: another build of the same derivation can write other bytes, and a
//! cache keeps only one of them.

use std::{collections::BTreeMap, fmt::Write as _, fs};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

pub(crate) const NAME: &str = "sdk-contract.json";

/// Embed the lock at compilation. A stale executable must report its own fork,
/// even when it renders inside a checkout whose dependency pin has changed.
pub(crate) fn compiled_fork_revision() -> Result<String> {
    fork_revision(include_str!("../../../Cargo.lock"))
}

fn fork_revision(lock: &str) -> Result<String> {
    let lock: toml::Value = toml::from_str(lock)?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .context("compiled Cargo.lock has no packages")?;
    let matches = packages
        .iter()
        .filter(|package| package.get("name").and_then(toml::Value::as_str) == Some("ubrn_bindgen"))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!("compiled Cargo.lock needs one ubrn_bindgen package");
    }
    let source = matches[0]
        .get("source")
        .and_then(toml::Value::as_str)
        .context("compiled ubrn_bindgen must have a Git source")?;
    let (repository, revision) = source
        .rsplit_once('#')
        .context("compiled ubrn_bindgen has no resolved revision")?;
    if !repository.starts_with("git+")
        || revision.len() != 40
        || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("compiled ubrn_bindgen revision is not a Git commit");
    }
    Ok(revision.to_owned())
}

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
    pub fork_revision: Option<&'a str>,
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
    let mut fields = vec![
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
    ];
    if let Some(revision) = artifact.fork_revision {
        fields.push(("forkRevision".into(), text(revision)));
    }
    let record = Json::Object(fields);
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

    #[xmtp_common::test(unwrap_try = true)]
    fn fork_receipt_uses_resolved_commit_and_rejects_ambiguous_or_unpinned_input() {
        let revision = "1234567890123456789012345678901234567890";
        let source = format!(
            "[[package]]\nname = \"ubrn_bindgen\"\nsource = \"git+https://example.test/fork?rev=branch#{revision}\"\n"
        );
        assert_eq!(fork_revision(&source)?, revision);
        assert!(fork_revision(&source.repeat(2)).is_err());
        assert!(fork_revision(&source.replace(revision, "main")).is_err());
        assert!(fork_revision(&source.replace("git+", "registry+")).is_err());
        assert_eq!(compiled_fork_revision()?.len(), 40);
    }

    // Published contracts digest this `json.dumps` form; keep it stable.
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
