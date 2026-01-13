use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

#[derive(Parser, Copy, Clone, Debug)]
pub struct AppArgs {
    #[arg(long)]
    pub version: bool,
    #[command(subcommand)]
    pub cmd: Option<Commands>,
    #[command(flatten)]
    pub log: LogOptions,
}

#[derive(Subcommand, Copy, Clone, Debug)]
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
}

#[derive(Subcommand, Debug, Copy, Clone)]
pub enum Node {
    Add,
    Remove,
}

#[derive(Args, Debug, Copy, Clone)]
pub struct Info {}

#[derive(Args, Debug, Copy, Clone)]
pub struct Migrate {}

/// specify the log output
#[derive(Args, Debug, Default, Copy, Clone)]
pub struct LogOptions {
    /// Specify verbosity of logs, default ERROR
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
}
