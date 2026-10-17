//! Process-based, bounded chaos testing for a local self-hosted backend.
mod check;
mod counters;
mod evidence;
mod faults;
mod inspect;
mod instance;
mod ledger;
mod population;
mod process;
mod protocol;
mod recipes;
mod report;
mod schedule;
mod supervisor;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Local XMTP fork and liveness chaos suite")]
struct Cli {
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    Baseline(RunArgs),
    Run(RunArgs),
    Soak(RunArgs),
    Status {
        #[arg(long, default_value = ".chaos")]
        directory: PathBuf,
    },
    Inspect {
        bundle: Option<PathBuf>,
        /// Compare saved state and commit history without starting clients.
        #[arg(long, conflicts_with = "db")]
        forks: bool,
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        db: Option<String>,
    },
    #[command(hide = true)]
    Instance {
        config: PathBuf,
    },
}

#[derive(Clone, clap::Args)]
pub(crate) struct RunArgs {
    #[arg(long, default_value_t = 50)]
    pub rounds: u64,
    #[arg(long)]
    pub hours: Option<f64>,
    #[arg(long)]
    pub seed: Option<u64>,
    #[arg(long, default_value = "all")]
    pub faults: String,
    #[arg(long)]
    pub strict: bool,
    #[arg(long, default_value = ".chaos")]
    pub directory: PathBuf,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    xmtp_cryptography::install_crypto_provider();
    let cli = Cli::parse();
    let result = match cli.command {
        Action::Instance { config } => instance::run(&config).await.map(|_| 0),
        Action::Baseline(mut args) => {
            args.faults = "none".into();
            supervisor::run(args).await
        }
        Action::Run(args) => supervisor::run(args).await,
        Action::Soak(mut args) => {
            args.hours.get_or_insert(8.0);
            supervisor::run(args).await
        }
        Action::Status { directory } => {
            println!("{}", inspect::status(&supervisor::newest(&directory)));
            Ok(0)
        }
        Action::Inspect {
            bundle,
            group,
            db,
            forks,
        } => {
            let path = bundle.unwrap_or_else(|| PathBuf::from(".chaos"));
            let path = supervisor::newest(&path);
            println!(
                "{}",
                if forks {
                    inspect::forks::inspect(&path, group.as_deref())
                } else {
                    inspect::inspect(&path, group.as_deref(), db.as_deref())
                }
            );
            Ok(0)
        }
    };
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("HARNESS: {error:#}");
            3
        }
    };
    std::process::exit(code);
}
