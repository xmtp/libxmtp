use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Walk `crates/` and `bindings/` directories for `.rs` files.
/// Returns a map of crate name -> list of source file paths, sorted by crate name.
pub fn discover_source_files(workspace: &Path) -> BTreeMap<String, Vec<PathBuf>> {
    let mut crate_files: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();

    let dirs_to_scan = ["crates", "bindings"];

    for dir in &dirs_to_scan {
        let scan_root = workspace.join(dir);
        if !scan_root.is_dir() {
            continue;
        }

        for entry in WalkDir::new(&scan_root)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }

            // Determine crate name from the nearest ancestor that contains Cargo.toml
            if let Some(crate_name) = find_crate_name(path, workspace) {
                crate_files
                    .entry(crate_name)
                    .or_default()
                    .push(path.to_path_buf());
            }
        }
    }

    crate_files
}

/// Walk up from `file_path` to find the nearest `Cargo.toml` and use its directory name as the crate name.
fn find_crate_name(file_path: &Path, workspace_root: &Path) -> Option<String> {
    let mut dir = file_path.parent()?;

    loop {
        if dir.join("Cargo.toml").is_file() {
            return dir.file_name().and_then(|n| n.to_str()).map(String::from);
        }
        if dir == workspace_root || dir.parent().is_none() {
            return None;
        }
        dir = dir.parent()?;
    }
}
