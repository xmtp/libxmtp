//! Guard: every `#[error(transparent)]` core error variant that the SDK can
//! reach, and that hides a type the error walk classifies, is opened by the
//! walk (`error/wrappers.rs`). A transparent variant forwards `source()` past
//! its inner error, so a new one that nobody opens would silently turn a typed
//! cause into `Unknown`. The test scans every error enum in the core crates.

use crate::error::OPENED_WRAPPERS;
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

/// The error types that the SDK maps. The guard follows every transparent
/// variant reachable from these.
const ROOTS: &[&str] = &[
    "GroupError",
    "ClientError",
    "ClientBuilderError",
    "IdentityError",
    "ApiError",
    "StorageError",
    "ConnectionError",
    "LocalDeliveryError",
    "NotificationError",
    "ArchiveError",
    "SubscribeError",
    "DeviceSyncError",
    "EnrichMessageError",
    "IncomingError",
];

/// Types that the walk classifies at their own level: every value of one
/// maps to a code, so the guard does not follow their variants.
const TERMINAL: &[&str] = &[
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

/// The type name of a variant field, without wrappers, generics, or path.
fn inner_name(field: &str) -> String {
    let mut field = field.trim();
    while let Some(rest) = field.strip_prefix("#[") {
        field = rest.split_once(']').map_or("", |(_, rest)| rest).trim();
    }
    if field.contains("diesel::result::Error") {
        return "diesel::result::Error".into();
    }
    let mut field = field.to_string();
    for wrapper in ["Box<", "Arc<", "std::sync::Arc<"] {
        if let Some(rest) = field.strip_prefix(wrapper) {
            field = rest.trim_end_matches('>').to_string();
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

/// One single-field variant: its name, its inner type name, and whether it
/// is marked `#[error(transparent)]`.
type Variant = (String, String, bool);

/// Every enum's single-field variants.
fn scan(source: &str, index: &mut BTreeMap<String, BTreeSet<Variant>>) {
    let mut rest = source;
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
        let mut depth = 0;
        let mut end = open;
        for (offset, character) in rest[open..].char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &rest[open + 1..end];
        let mut attributes = String::new();
        let mut lines = body.lines();
        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[") || trimmed.starts_with("//") {
                attributes.push_str(trimmed);
                continue;
            }
            if !trimmed.starts_with(|c: char| c.is_ascii_uppercase()) {
                continue;
            }
            let variant: String = trimmed
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let after = trimmed[variant.len()..].trim_start();
            if let Some(field) = after.strip_prefix('(') {
                // Join the field across lines until its closing parenthesis.
                let mut field = field.to_string();
                while field.matches('(').count() >= field.matches(')').count() {
                    let Some(next) = lines.next() else { break };
                    field.push_str(next.trim());
                }
                let mut depth = 1;
                let mut close = field.len();
                for (offset, character) in field.char_indices() {
                    match character {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                close = offset;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let field = &field[..close];
                if !field.contains(',') {
                    let transparent = attributes.contains("#[error(transparent)]");
                    index.entry(name.clone()).or_default().insert((
                        variant,
                        inner_name(field),
                        transparent,
                    ));
                }
            }
            attributes.clear();
        }
        rest = &rest[end..];
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

/// Whether `name` hides a classified type behind its own transparent
/// variants, directly or further down.
fn hides(
    name: &str,
    index: &BTreeMap<String, BTreeSet<Variant>>,
    seen: &mut BTreeSet<String>,
) -> bool {
    if !seen.insert(name.to_string()) {
        return false;
    }
    index
        .get(name)
        .into_iter()
        .flatten()
        .filter(|(_, _, transparent)| *transparent)
        .any(|(_, inner, _)| CLASSIFIED.contains(&inner.as_str()) || hides(inner, index, seen))
}

/// From the SDK's root error types, the walk reaches every variant's inner
/// error: a non-transparent one through `source()`, a transparent one only if
/// the table opens it. Every reachable transparent variant whose inner type
/// is, or hides, a type that the walk classifies must be opened. A skipped
/// transparent variant that hides no classified type loses nothing, because
/// the walk continues from its inner error's source.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
fn every_transparent_core_wrapper_that_hides_a_classified_type_is_opened() {
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

    let mut queue: VecDeque<String> = ROOTS.iter().map(|root| root.to_string()).collect();
    let mut visited = BTreeSet::new();
    let mut reached = BTreeSet::new();
    let mut problems = Vec::new();
    while let Some(name) = queue.pop_front() {
        if TERMINAL.contains(&name.as_str()) || !visited.insert(name.clone()) {
            continue;
        }
        for (variant, inner, transparent) in index.get(&name).into_iter().flatten() {
            if !transparent {
                // The walk visits a non-transparent variant's inner error
                // through source(), so it is reachable too.
                queue.push_back(inner.clone());
                continue;
            }
            let key = (name.clone(), variant.clone());
            reached.insert(key.clone());
            if opened.contains(&key) {
                queue.push_back(inner.clone());
            } else if CLASSIFIED.contains(&inner.as_str())
                || hides(inner, &index, &mut BTreeSet::new())
            {
                problems.push(format!(
                    "{name}::{variant} ({inner}) is or hides a classified type; open it"
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "transparent core wrappers: {problems:#?}"
    );
    let stale: Vec<_> = opened
        .iter()
        .filter(|key| !reached.contains(*key))
        .collect();
    assert!(stale.is_empty(), "entries that no root reaches: {stale:?}");
}
