mod discovery;
mod extraction;
mod markdown;
mod model;

use std::path::PathBuf;

use clap::Parser;
use model::CrateInfo;

#[derive(Parser)]
#[command(name = "error-glossary", about = "Generate XMTP error code glossary")]
struct Cli {
    /// Path to the workspace root
    #[arg(long, default_value = ".")]
    workspace: PathBuf,

    /// Output markdown file path
    #[arg(long, default_value = "docs/error_glossary.md")]
    output: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    let workspace = cli.workspace.canonicalize().unwrap_or_else(|e| {
        eprintln!("error: cannot resolve workspace path {:?}: {}", cli.workspace, e);
        std::process::exit(1);
    });

    // Discover source files grouped by crate
    let crate_files = discovery::discover_source_files(&workspace);

    let mut crates: Vec<CrateInfo> = Vec::new();
    let mut total_types = 0usize;
    let mut total_variants = 0usize;

    for (crate_name, files) in &crate_files {
        let mut error_types = Vec::new();

        for file_path in files {
            let Ok(source) = std::fs::read_to_string(file_path) else {
                continue;
            };
            let mut types = extraction::extract_error_types(&source, file_path, &workspace);
            error_types.append(&mut types);
        }

        if error_types.is_empty() {
            continue;
        }

        // Filter out internal types
        error_types.retain(|t| !t.internal);

        if error_types.is_empty() {
            continue;
        }

        // Sort types by name within each crate
        error_types.sort_by(|a, b| a.name.cmp(&b.name));

        total_types += error_types.len();
        total_variants += error_types
            .iter()
            .flat_map(|t| &t.variants)
            .filter(|v| !v.inherit)
            .count();

        crates.push(CrateInfo {
            name: crate_name.clone(),
            error_types,
        });
    }

    if crates.is_empty() {
        eprintln!("warning: no ErrorCode types found");
        std::process::exit(0);
    }

    let md = markdown::render(&crates);

    // Ensure output directory exists
    if let Some(parent) = cli.output.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("error: cannot create output directory: {}", e);
            std::process::exit(1);
        });
    }

    std::fs::write(&cli.output, md).unwrap_or_else(|e| {
        eprintln!("error: cannot write output file: {}", e);
        std::process::exit(1);
    });

    eprintln!(
        "Generated error glossary: {} crates, {} types, {} variants -> {}",
        crates.len(),
        total_types,
        total_variants,
        cli.output.display()
    );
}
