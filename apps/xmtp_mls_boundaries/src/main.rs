//! `check-boundaries`: keeps the code in `crates/xmtp_mls` from adding
//! dependency edges that the planned crate split forbids.

mod check;
mod config;
mod graph;
mod scip;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use clap::Parser;
use config::{Config, Tier};
use graph::{Build, Reference};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "check-boundaries")]
struct Cli {
    /// The workspace root.
    #[arg(long, default_value = ".")]
    workspace: PathBuf,
    /// The boundaries file, relative to the workspace.
    #[arg(long, default_value = "crates/xmtp_mls/boundaries.toml")]
    config: PathBuf,
    /// The boundaries file of the base branch. Its waivers must not be raised
    /// or added to.
    #[arg(long)]
    base_config: Option<PathBuf>,
    /// Also run the `call` tier: the rust-analyzer SCIP index of a native
    /// build with `test` on. Needs `--scip-wasm`.
    #[arg(long, requires = "scip_wasm")]
    scip: Option<PathBuf>,
    /// The index of a `wasm32-unknown-unknown` build with `test` off.
    #[arg(long, requires = "scip")]
    scip_wasm: Option<PathBuf>,
    /// Print the measured references of every edge between target crates.
    #[arg(long)]
    report: bool,
}

/// What one run found: the problems, and the references each tier measured.
struct Outcome {
    problems: Vec<String>,
    tiers: Vec<(Tier, Vec<Reference>)>,
    summary: Option<String>,
}

fn run(
    workspace: &Path,
    config: &Config,
    base: Option<&Config>,
    indexes: Option<&BTreeMap<Build, PathBuf>>,
) -> Result<Outcome> {
    let mut problems = vec![];
    if let Some(base) = base {
        problems.extend(config.loosened_from(base));
    }
    problems.extend(check::check_names(
        config,
        &check::workspace_packages(workspace)?,
    ));
    let krate = graph::Crate::load(&workspace.join(&config.source).join("lib.rs"))?;
    let references = krate.references(config, workspace);
    problems.extend(check::check_edges(config, Tier::Path, &references));
    let mut tiers = vec![(Tier::Path, references)];
    let mut summary = None;
    if let Some(indexes) = indexes {
        let calls = scip::references(config, &krate, workspace, indexes)?;
        problems.extend(check::check_edges(config, Tier::Call, &calls.references));
        summary = Some(calls.summary);
        tiers.push((Tier::Call, calls.references));
    }
    Ok(Outcome {
        problems,
        tiers,
        summary,
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let workspace = cli
        .workspace
        .canonicalize()
        .with_context(|| format!("resolving {}", cli.workspace.display()))?;
    let config = Config::load(&workspace.join(&cli.config))?;
    let base = cli.base_config.as_deref().map(Config::load).transpose()?;
    let indexes = cli.scip.zip(cli.scip_wasm).map(|(native, wasm)| {
        BTreeMap::from([(Build::NativeTest, native), (Build::WasmProduction, wasm)])
    });
    let outcome = run(&workspace, &config, base.as_ref(), indexes.as_ref())?;
    if let Some(summary) = &outcome.summary {
        println!("{summary}");
    }
    if cli.report {
        for (tier, references) in &outcome.tiers {
            for ((from, to), references) in check::edges(references) {
                let allowed = if config.is_allowed(&from, &to) {
                    "allowed"
                } else {
                    "not allowed"
                };
                println!("{tier}\t{from} -> {to}\t{}\t{allowed}", references.len());
            }
        }
    }
    if outcome.problems.is_empty() {
        println!("check-boundaries: no unwaived edges");
        return Ok(());
    }
    for problem in &outcome.problems {
        eprintln!("error: {problem}");
    }
    std::process::exit(1);
}
