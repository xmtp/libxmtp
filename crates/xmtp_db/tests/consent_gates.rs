//! One place spells a group's consent key in SQL, and the deleted consent
//! writers and readers stay deleted: a reader keyed by group id alone would
//! read a DM's own record instead of its peer's.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

/// Words no Rust source in `crates/` or `apps/` may hold, with the one file
/// a word may appear in. The legacy converter reads pre-8.0 rows and keeps
/// their spellings.
const BANNED: [(&str, Option<&str>); 8] = [
    (
        "lower(hex(",
        Some("crates/xmtp_db/src/encrypted_store/group.rs"),
    ),
    ("get_consent_record", None),
    ("record_app_act", None),
    ("insert_newer_consent_record", None),
    ("PreferenceOrigin", None),
    ("beyond_received_bound", None),
    ("stitch_dm_consent", None),
    ("store_hmac_key", None),
];

const SKIPPED: [&str; 4] = [
    "crates/xmtp_legacy_migration",
    "target",
    "node_modules",
    ".git",
];

fn rust_sources(root: &Path, dir: PathBuf, sources: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if SKIPPED
            .iter()
            .any(|skip| relative == *skip || relative.ends_with(&format!("/{skip}")))
        {
            continue;
        }
        if path.is_dir() {
            rust_sources(root, path, sources);
        } else if relative.ends_with(".rs") && relative != file!() {
            sources.push((relative, std::fs::read_to_string(&path).unwrap()));
        }
    }
}

// verifies: CONS-052
#[xmtp_common::test]
fn deleted_consent_spellings_stay_deleted() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut sources = vec![];
    for dir in ["crates", "apps"] {
        rust_sources(&root, root.join(dir), &mut sources);
    }
    assert!(sources.iter().any(|(path, _)| path.starts_with("apps/")));
    let found: Vec<_> = sources
        .iter()
        .flat_map(|(path, text)| {
            BANNED
                .iter()
                .filter(move |(word, allowed)| {
                    text.contains(word) && *allowed != Some(path.as_str())
                })
                .map(move |(word, _)| format!("{path}: {word}"))
        })
        .collect();
    assert!(found.is_empty(), "{found:#?}");
}
