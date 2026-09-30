//! Guard for the error walk (`error/wrappers.rs`). The walk follows
//! `source()` and opens the core variants marked `#[error(transparent)]`,
//! which forward `source()` past their inner error. This test reads every
//! error enum in the core crates and, from every type passed to `from_core`
//! (the `CoreError` implementors), follows every variant the walk can reach:
//!
//! - a variant whose field has `#[from]`, `#[source]`, or the name `source`
//!   is followed through `source()`;
//! - a transparent variant that is, or hides, a type the walk classifies
//!   must be opened in the table;
//! - a single-field variant with no source whose inner type is, or leads to,
//!   a classified type fails: the walk cannot see its cause.
//!
//! Known limits: the scan resolves the last path segment of a field type and
//! unwraps `Option`, `Box`, and `Arc`. It does not follow generics, type
//! aliases, or `Box<dyn Error>`. Hand-written `source()` implementations are
//! listed in `SOURCE_IMPLS`. Preflight failures and credential errors inside
//! boxed or type-erased network errors are found by the special cases in
//! `XmtpError::classify` (`xmtp_api::preflight::failure`) and
//! `delivery::auth_cause`, not by this scan.

use crate::error::{CORE_ERROR_ROOTS, OPENED_WRAPPERS};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

/// The core crates whose error enums can reach `XmtpError::from_core`.
const CRATES: &[&str] = &[
    "xmtp_mls",
    "xmtp_db",
    "xmtp_api",
    "xmtp_api_backend",
    "xmtp_archive",
    "xmtp_id",
    "xmtp_proto",
    "xmtp_content_types",
    "xmtp_mls_validation",
    "xmtp_cryptography",
    "xmtp_common",
];

/// Entry points that map an error without `from_core`.
const EXTRA_ROOTS: &[&str] = &["ApiError", "NotificationError"];

/// Types that the walk classifies at their own level: every value of one
/// maps to a code, so the guard does not follow their variants.
const TERMINAL: &[&str] = &[
    "PlatformStorageError",
    "ApiError",
    "AuthError",
    "StorageError",
    "ConnectionError",
    "diesel::result::Error",
    "PreflightError",
    "StorageLocationError",
    // Mapped by `XmtpError::from_notification`, not by the walk.
    "NotificationError",
];

/// Types that the walk classifies. A transparent variant whose inner type is
/// one of these, or hides one, must be opened.
const CLASSIFIED: &[&str] = &[
    "PlatformStorageError",
    "GroupError",
    "ClientError",
    "ApiError",
    "AuthError",
    "StorageError",
    "ConnectionError",
    "diesel::result::Error",
    "PreflightError",
    "StorageLocationError",
];

/// Hand-written `source()` implementations: (type, the type it returns).
const SOURCE_IMPLS: &[(&str, &str)] = &[("SyncSummary", "GroupError")];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Link {
    /// `#[error(transparent)]`: the walk sees the inner error only if opened.
    Transparent,
    /// `#[from]`, `#[source]`, or a field named `source`.
    Source,
    /// A single field with no source: the walk cannot see it.
    Hidden,
}

type Edge = (String, String, Link);

/// The type name of a field, without attributes, wrappers, generics, or path.
fn inner_name(field: &str) -> String {
    let mut field = field.trim();
    while let Some(rest) = field.strip_prefix("#[") {
        field = rest.split_once(']').map_or("", |(_, rest)| rest).trim();
    }
    // Diesel's type names collide with core names (diesel::ConnectionError),
    // so a diesel type keeps its path.
    if let Some(at) = field.find("diesel::") {
        return field[at..]
            .trim_end_matches(|c: char| c == '>' || c.is_whitespace())
            .to_string();
    }
    let mut field = field.to_string();
    loop {
        let before = field.clone();
        for wrapper in ["Option<", "Box<", "Arc<", "std::sync::Arc<"] {
            if let Some(rest) = field.strip_prefix(wrapper) {
                field = rest.strip_suffix('>').unwrap_or(rest).trim().to_string();
            }
        }
        if field == before {
            break;
        }
    }
    let field = field.split('<').next().unwrap_or("").trim();
    field
        .rsplit("::")
        .next()
        .unwrap_or(field)
        .trim()
        .to_string()
}

/// Split at commas outside brackets and string literals.
fn split_top(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut current = String::new();
    let mut previous = ' ';
    for character in text.chars() {
        if character == '"' && previous != '\\' {
            in_string = !in_string;
        }
        if !in_string {
            match character {
                '(' | '{' | '[' | '<' => depth += 1,
                ')' | '}' | ']' | '>' => depth -= 1,
                ',' if depth == 0 => {
                    parts.push(std::mem::take(&mut current));
                    previous = character;
                    continue;
                }
                _ => {}
            }
        }
        current.push(character);
        previous = character;
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

/// The leading `#[...]` attributes of `text`, and the rest.
fn attributes(text: &str) -> (String, &str) {
    let mut attributes = String::new();
    let mut rest = text.trim_start();
    while rest.starts_with("#[") {
        let mut depth = 0;
        let mut end = rest.len();
        for (offset, character) in rest.char_indices() {
            match character {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = offset + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        attributes.push_str(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    (attributes, rest)
}

/// The text between the bracket at the start of `text` and its match.
fn bracketed(text: &str) -> &str {
    let mut depth = 0;
    for (offset, character) in text.char_indices() {
        match character {
            '(' | '{' => depth += 1,
            ')' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[1..offset];
                }
            }
            _ => {}
        }
    }
    ""
}

/// Every error enum's variant links.
fn scan(source: &str, index: &mut BTreeMap<String, BTreeSet<Edge>>) {
    // Drop line comments, which may hold brackets or commas.
    let source: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut rest = source.as_str();
    while let Some(at) = rest.find("enum ") {
        let before = &rest[..at];
        rest = &rest[at + "enum ".len()..];
        if !before.ends_with("pub ") && !before.ends_with("pub(crate) ") && !before.ends_with('\n')
        {
            continue;
        }
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let Some(open) = rest.find('{') else { break };
        if rest[..open].contains(';') {
            continue;
        }
        let body = bracketed(&rest[open..]);
        for variant in split_top(body) {
            let (attrs, variant) = attributes(&variant);
            let transparent = attrs.contains("#[error(transparent)]");
            let variant_name: String = variant
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let after = variant[variant_name.len()..].trim_start();
            let entry = index.entry(name.clone()).or_default();
            if after.starts_with('(') {
                let fields = split_top(bracketed(after));
                if fields.len() != 1 {
                    continue;
                }
                let (field_attrs, field) = attributes(&fields[0]);
                let link = if transparent {
                    Link::Transparent
                } else if field_attrs.contains("#[from]") || field_attrs.contains("#[source]") {
                    Link::Source
                } else {
                    Link::Hidden
                };
                entry.insert((variant_name.clone(), inner_name(field), link));
            } else if after.starts_with('{') {
                for field in split_top(bracketed(after)) {
                    let (field_attrs, field) = attributes(&field);
                    let Some((field_name, field_type)) = field.split_once(':') else {
                        continue;
                    };
                    if field_attrs.contains("#[from]")
                        || field_attrs.contains("#[source]")
                        || field_name.trim() == "source"
                    {
                        entry.insert((variant_name.clone(), inner_name(field_type), Link::Source));
                    }
                }
            }
        }
        rest = &rest[open..];
    }
}

fn sources(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name != "tests" && name != "test" {
                sources(&path, found);
            }
        } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            found.push(path);
        }
    }
}

fn short(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path).trim()
}

/// The walk's edges out of `name`, including hand-written `source()`.
fn edges<'a>(
    name: &str,
    index: &'a BTreeMap<String, BTreeSet<Edge>>,
) -> Vec<(String, String, Link)> {
    let mut found: Vec<Edge> = index.get(name).into_iter().flatten().cloned().collect();
    for (owner, inner) in SOURCE_IMPLS {
        if *owner == name {
            found.push(("source()".into(), inner.to_string(), Link::Source));
        }
    }
    found
}

/// Whether `name` hides a classified type behind transparent variants.
fn hides(
    name: &str,
    index: &BTreeMap<String, BTreeSet<Edge>>,
    seen: &mut BTreeSet<String>,
) -> bool {
    if !seen.insert(name.to_string()) {
        return false;
    }
    edges(name, index)
        .into_iter()
        .filter(|(_, _, link)| *link == Link::Transparent)
        .any(|(_, inner, _)| CLASSIFIED.contains(&inner.as_str()) || hides(&inner, index, seen))
}

/// Whether `name` is, or leads by any link to, a classified type.
fn leads_to_classified(
    name: &str,
    index: &BTreeMap<String, BTreeSet<Edge>>,
    seen: &mut BTreeSet<String>,
) -> bool {
    if CLASSIFIED.contains(&name) {
        return true;
    }
    if !seen.insert(name.to_string()) {
        return false;
    }
    edges(name, index)
        .into_iter()
        .any(|(_, inner, _)| leads_to_classified(&inner, index, seen))
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
fn every_reachable_core_error_cause_is_visible_to_the_walk() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut index = BTreeMap::new();
    for krate in CRATES {
        let mut files = Vec::new();
        sources(&crates.join(krate).join("src"), &mut files);
        for file in files {
            scan(&std::fs::read_to_string(file)?, &mut index);
        }
    }
    let opened: BTreeSet<(String, String)> = OPENED_WRAPPERS
        .iter()
        .map(|(ty, variant)| (short(ty).to_string(), variant.to_string()))
        .collect();
    let roots: Vec<String> = CORE_ERROR_ROOTS
        .iter()
        .map(|root| short(root).to_string())
        .chain(EXTRA_ROOTS.iter().map(|root| root.to_string()))
        .collect();

    // Each item is (type, whether the walk visits that level). A skipped
    // transparent variant's inner level is not visited, but the walk still
    // continues from that inner error's source(), so its links still count.
    let mut queue: VecDeque<(String, bool)> = roots.into_iter().map(|root| (root, true)).collect();
    let mut visited = BTreeSet::new();
    let mut reached = BTreeSet::new();
    let mut problems = Vec::new();
    while let Some((name, visible)) = queue.pop_front() {
        if TERMINAL.contains(&name.as_str()) || !visited.insert((name.clone(), visible)) {
            continue;
        }
        for (variant, inner, link) in edges(&name, &index) {
            match link {
                Link::Source => queue.push_back((inner, true)),
                Link::Transparent => {
                    let key = (name.clone(), variant.clone());
                    reached.insert(key.clone());
                    if visible && opened.contains(&key) {
                        queue.push_back((inner, true));
                    } else if CLASSIFIED.contains(&inner.as_str())
                        || hides(&inner, &index, &mut BTreeSet::new())
                    {
                        problems.push(format!(
                            "{name}::{variant} ({inner}) is transparent and is or hides a classified type; open it"
                        ));
                    } else {
                        queue.push_back((inner, false));
                    }
                }
                Link::Hidden => {
                    if leads_to_classified(&inner, &index, &mut BTreeSet::new()) {
                        problems.push(format!(
                            "{name}::{variant} ({inner}) has no source, so the walk cannot see its cause; mark the field #[source]"
                        ));
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "error walk gaps: {problems:#?}");
    let stale: Vec<_> = opened
        .iter()
        .filter(|key| !reached.contains(*key))
        .collect();
    assert!(
        stale.is_empty(),
        "opened entries that no root reaches: {stale:?}"
    );
}
