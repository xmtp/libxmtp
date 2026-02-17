use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use color_eyre::eyre::{Result, eyre};

#[derive(Parser, Clone, Debug)]
pub struct AppArgs {
    #[arg(long)]
    pub version: bool,
    #[command(subcommand)]
    pub cmd: Option<Commands>,
    #[command(flatten)]
    pub log: LogOptions,
    /// Path to a TOML configuration to use for xnet
    #[arg(long, short)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Commands {
    /// Bring XNet services Up. Initialize them if they have not yet been initialized.
    Up,
    /// Bring XNet Services Down
    Down,
    Delete,
    /// Node Operations (Add, Remove XMTPD Nodes)
    #[command(subcommand)]
    Node(Node),
    /// Print Information about the network
    Info(Info),
    /// Set a migration time
    Migrate(Migrate),
    /// Query the current d14n cutover timestamp from node-go
    Cutover,
}

#[derive(Subcommand, Debug, Copy, Clone)]
pub enum Node {
    Add(AddNode),
    Remove,
}

#[derive(Args, Debug, Copy, Clone)]
pub struct AddNode {
    /// make this node a migrator node
    #[arg(long, short)]
    pub migrator: bool,
}

#[derive(Args, Debug, Copy, Clone)]
pub struct Info {}

#[derive(Args, Debug, Clone)]
pub struct Migrate {
    /// Cutover time: a duration offset like "30s", "5m", "1h" (computed as now + offset),
    /// OR a raw nanosecond timestamp. If omitted, uses "now".
    #[arg(long, short)]
    pub cutover: Option<String>,
}

impl Migrate {
    pub fn cutover_ns(&self) -> Result<i64> {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| eyre!("system time error: {}", e))?
            .as_nanos() as i64;

        match &self.cutover {
            None => Ok(now_ns),
            Some(val) => {
                // Try raw integer first (nanosecond timestamp)
                if let Ok(ns) = val.parse::<i64>() {
                    return Ok(ns);
                }
                // Parse as human-readable duration via humantime
                let duration: std::time::Duration = val
                    .parse::<humantime::Duration>()
                    .map_err(|e| eyre!("invalid cutover '{}': {}", val, e))?
                    .into();
                Ok(now_ns + duration.as_nanos() as i64)
            }
        }
    }
}

/// specify the log output
#[derive(Args, Debug, Default, Copy, Clone)]
pub struct LogOptions {
    /// Specify verbosity of logs, default ERROR
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
}
