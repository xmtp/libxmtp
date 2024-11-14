//! App Argument Options
use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_verbosity_flag::{InfoLevel, Verbosity};

/// Debug & Generate data on the XMTP Networ
#[derive(Parser, Debug)]
pub struct AppOpts {
    #[command(subcommand)]
    pub cmd: Commands,
    #[command(flatten)]
    pub log: LogOptions,
    #[command(flatten)]
    pub backend: BackendOpts,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Generate(Generate),
    Inspect(Inspect),
    Query(Query),
}

/// Generate Groups/Messages/Users
#[derive(Args, Debug)]
pub struct Generate {
    /// Specify an entity to generate
    #[arg(value_enum, long, short)]
    pub entity: EntityKind,
    /// How many entities to generate
    #[arg(long, short)]
    pub amount: usize,
}

/// Inspect Payloads
#[derive(Args, Debug)]
pub struct Inspect {}

/// Query for Information about a Group or Message or User
#[derive(Args, Debug)]
pub struct Query {}

#[derive(ValueEnum, Debug, Clone)]
pub enum EntityKind {
    Group,
    Message,
    Identity,
}

/// specify the log output
#[derive(Args, Debug)]
pub struct LogOptions {
    /// Output logs in a structured, ndJSON format
    #[arg(long, conflicts_with = "logfmt")]
    pub json: bool,
    /// Output into logfmt format
    #[arg(long, conflicts_with = "json")]
    pub logfmt: bool,
    /// Show key-value fields. Default on for JSON & logfmt and off for stdout
    #[arg(short, long)]
    pub show_fields: Option<bool>,
    /// Specify verbosity of logs, default ERROR
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
}

/// Specify which backend to use
#[derive(Args, Clone, Debug)]
pub struct BackendOpts {
    #[arg(
        value_enum,
        short,
        long,
        group = "constant-backend",
        conflicts_with = "custom-backend"
    )]
    pub backend: BackendKind,
    #[arg(
        short,
        long,
        group = "custom-backend",
        conflicts_with = "constant-backend"
    )]
    pub url: Option<url::Url>,
}

impl From<BackendOpts> for url::Url {
    fn from(value: BackendOpts) -> Self {
        let BackendOpts { backend, url } = value;
        url.unwrap_or(backend.into())
    }
}

#[derive(ValueEnum, Debug, Copy, Clone, Default)]
pub enum BackendKind {
    #[default]
    Dev,
    Production,
    Local,
}

impl From<BackendKind> for url::Url {
    fn from(value: BackendKind) -> Self {
        use BackendKind::*;
        match value {
            Dev => (*crate::constants::XMTP_DEV).clone(),
            Production => (*crate::constants::XMTP_PRODUCTION).clone(),
            Local => (*crate::constants::XMTP_LOCAL).clone(),
        }
    }
}
