//! The checks: edges against the allowed set and the waivers, waivers
//! against the base branch, and the names of new workspace packages.

use crate::config::{Config, Tier};
use crate::graph::Reference;
use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// How many references of an unwaived edge a failure lists.
const LISTED: usize = 20;

/// Count the references of each edge between two target crates.
pub fn edges(references: &[Reference]) -> BTreeMap<(String, String), Vec<&Reference>> {
    let mut edges: BTreeMap<(String, String), Vec<&Reference>> = BTreeMap::new();
    for reference in references {
        edges
            .entry((reference.from.clone(), reference.to.clone()))
            .or_default()
            .push(reference);
    }
    edges
}

/// Problems with the edges `tier` measured: an edge that is neither allowed
/// nor waived, a waiver whose count differs from the measured count, and a
/// waiver of this tier with no reference left.
pub fn check_edges(config: &Config, tier: Tier, references: &[Reference]) -> Vec<String> {
    let edges = edges(references);
    let mut problems = vec![];
    for ((from, to), references) in &edges {
        if config.is_allowed(from, to) {
            continue;
        }
        let measured = references.len();
        let listing = || {
            references
                .iter()
                .take(LISTED)
                .map(|reference| format!("\n    {} {}", reference.file.display(), reference.path))
                .collect::<String>()
        };
        match config.waiver(from, to, tier) {
            None => problems.push(format!(
                "edge {from} -> {to} is not allowed ({tier} tier, {measured} references):{}",
                listing()
            )),
            Some(waiver) if waiver.count < measured => problems.push(format!(
                "edge {from} -> {to} has {measured} {tier} references but its waiver allows {}:{}",
                waiver.count,
                listing()
            )),
            Some(waiver) if waiver.count > measured => problems.push(format!(
                "waiver {from} -> {to} ({tier}) is {} but {measured} references remain; lower it to {measured}",
                waiver.count
            )),
            Some(_) => {}
        }
    }
    for waiver in config.waivers.iter().filter(|waiver| waiver.tier == tier) {
        if !edges.contains_key(&(waiver.from.clone(), waiver.to.clone())) {
            problems.push(format!(
                "waiver {} -> {} ({tier}) has no references left; remove it",
                waiver.from, waiver.to
            ));
        }
    }
    problems
}

/// The package names of the workspace members under `workspace`, read from
/// the root manifest's `members` and `exclude`.
pub fn workspace_packages(workspace: &Path) -> Result<BTreeSet<String>> {
    let manifest = workspace.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    let root: toml::Table = toml::from_str(&text)?;
    let list = |key: &str| -> Vec<String> {
        root.get("workspace")
            .and_then(|workspace| workspace.get(key))
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let excluded: BTreeSet<_> = list("exclude")
        .iter()
        .map(|path| workspace.join(path))
        .collect();
    let mut directories = vec![];
    for member in list("members") {
        match member.strip_suffix("/*") {
            Some(parent) => {
                let parent = workspace.join(parent);
                for entry in std::fs::read_dir(&parent)
                    .with_context(|| format!("listing {}", parent.display()))?
                {
                    directories.push(entry?.path());
                }
            }
            None => directories.push(workspace.join(member)),
        }
    }
    let mut names = BTreeSet::new();
    for directory in directories {
        let manifest = directory.join("Cargo.toml");
        if excluded.contains(&directory) || !manifest.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&manifest)
            .with_context(|| format!("reading {}", manifest.display()))?;
        let package: toml::Table =
            toml::from_str(&text).with_context(|| format!("parsing {}", manifest.display()))?;
        if let Some(name) = package
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(|name| name.as_str())
        {
            names.insert(name.to_string());
        }
    }
    Ok(names)
}

/// A package outside the naming exceptions whose name lacks the prefix.
pub fn check_names(config: &Config, packages: &BTreeSet<String>) -> Vec<String> {
    let naming = &config.naming;
    packages
        .iter()
        .filter(|name| {
            !name.starts_with(&naming.prefix)
                && !naming.existing.contains(*name)
                && !naming.exempt.contains(*name)
        })
        .map(|name| {
            format!(
                "package {name:?} must start with {:?}: the log filter matches xmtp_mls targets by prefix",
                naming.prefix
            )
        })
        .collect()
}
