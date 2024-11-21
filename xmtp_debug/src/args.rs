//! App Argument Options
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use xxhash_rust::xxh3;
mod types;
pub use types::*;

/// Debug & Generate data on the XMTP Network
#[derive(Parser, Debug)]
pub struct AppOpts {
    #[command(subcommand)]
    pub cmd: Option<Commands>,
    #[command(flatten)]
    pub log: LogOptions,
    #[command(flatten)]
    pub backend: BackendOpts,
    /// Clear ALL local app data & state kept by xdbg
    /// Runs at the end of execution, so operations will still be carried out
    #[arg(long)]
    pub clear: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Generate(Generate),
    Modify(Modify),
    Inspect(Inspect),
    Query(Query),
    Info(InfoOpts),
    Export(ExportOpts),
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
    /// Specify amount of random identities to invite to group
    #[arg(long, short)]
    pub invite: Option<usize>,
    #[command(flatten)]
    pub message_opts: MessageGenerateOpts,
}

#[derive(Args, Debug, Clone)]
pub struct MessageGenerateOpts {
    /// Continuously generate & send messages
    #[arg(long, short)]
    pub r#loop: bool,
    /// Interval to send messages on (default every second)
    #[arg(long, short, default_value_t = MillisecondInterval::default())]
    pub interval: MillisecondInterval,
    /// Max variable message size, in words.
    #[arg(long, short, default_value = "100")]
    pub max_message_size: u32,
}

/// Modify state of local clients & groups
#[derive(Args, Debug)]
pub struct Modify {
    /// action to take
    #[arg(value_enum)]
    pub action: MemberModificationKind,

    /// group to modify
    pub group_id: GroupId,

    /// InboxID to add or remove
    #[arg(long, short)]
    pub inbox_id: Option<InboxId>,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum MemberModificationKind {
    /// Remove a member from a group
    Remove,
    /// Add a random member to a group
    AddRandom,
    /// Add an external id the group
    AddExternal,
}

/// Inspect Local State
#[derive(Args, Debug)]
pub struct Inspect {
    /// The InboxId of the Client to Inspect
    pub inbox_id: InboxId,

    /// Kind of inspection to perform
    pub kind: InspectionKind,
}

#[derive(ValueEnum, Default, Debug, Clone)]
pub enum InspectionKind {
    /// Inspect the associations this client has
    Associations,
    /// Inspect the groups this client is apart of
    #[default]
    Groups,
}

/// Query for Information about a Group or Message or User
#[derive(Args, Debug)]
pub struct Query {}

/// Print information about the local generated state
#[derive(Args, Debug)]
pub struct InfoOpts {
    /// Show a random identity
    #[arg(long)]
    pub random: bool,
    /// Show information about the app
    #[arg(long)]
    pub app: bool, // /// Show information about a specific identity, like its id and storage
                   // pub identity: IdentityInfo
}

/// Export information to JSON
#[derive(Args, Debug)]
pub struct ExportOpts {
    /// Entity to export
    #[arg(long, short)]
    pub entity: EntityKind,
    /// File to write to
    #[arg(long, short)]
    pub out: Option<PathBuf>,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum EntityKind {
    Group,
    Message,
    Identity,
}

/// specify the log output
#[derive(Args, Debug)]
pub struct LogOptions {
    /// Output libxmtp logs into file with a structured, ndJSON format
    #[arg(long)]
    pub json: bool,
    /// Output libxmtp into file with logfmt format
    #[arg(long)]
    pub logfmt: bool,
    /// Output libxmtp logs to file in a human-readable format
    #[arg(long)]
    pub human: bool,
    /// Show key-value fields. Default on for JSON & logfmt and off for human
    #[arg(short, long, action)]
    pub show_fields: bool,
    /// Specify verbosity of logs, default ERROR
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
}

/// Specify which backend to use
#[derive(Args, Clone, Debug, Default)]
pub struct BackendOpts {
    #[arg(
        value_enum,
        short,
        long,
        group = "constant-backend",
        conflicts_with = "custom-backend",
        default_value_t = BackendKind::Local
    )]
    pub backend: BackendKind,
    /// URL Pointing to a backend. Conflicts with `backend`
    #[arg(
        short,
        long,
        group = "custom-backend",
        conflicts_with = "constant-backend"
    )]
    pub url: Option<url::Url>,
}

impl<'a> From<&'a BackendOpts> for u64 {
    fn from(value: &'a BackendOpts) -> Self {
        use BackendKind::*;

        if let Some(ref url) = value.url {
            xxh3::xxh3_64(url.as_str().as_bytes())
        } else {
            match value.backend {
                Production => 2,
                Dev => 1,
                Local => 0,
            }
        }
    }
}

impl From<BackendOpts> for u64 {
    fn from(value: BackendOpts) -> Self {
        (&value).into()
    }
}

impl From<BackendOpts> for url::Url {
    fn from(value: BackendOpts) -> Self {
        let BackendOpts { backend, url } = value;
        url.unwrap_or(backend.into())
    }
}

#[derive(ValueEnum, Debug, Copy, Clone, Default)]
pub enum BackendKind {
    Dev,
    Production,
    #[default]
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
