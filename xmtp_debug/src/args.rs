//! App Argument Options
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use color_eyre::eyre::{self, Result};
use directories::ProjectDirs;
use xxhash_rust::xxh3;
mod types;
pub use types::*;

/// Debug & Generate data on the XMTP Network
#[derive(Parser, Debug)]
pub struct AppOpts {
    // Print Version
    #[arg(long)]
    pub version: bool,
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
    /// Override the directory to store xdbg's data
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    /// Override the directory where sqlite dbs of clients are stored
    #[arg(long)]
    pub sqlite_dir: Option<PathBuf>,
}

impl AppOpts {
    pub(super) fn data_directory(&self) -> Result<PathBuf> {
        let data = if let Some(d) = &self.data_dir {
            Ok(d.clone())
        } else {
            if let Some(dir) = ProjectDirs::from("org", "xmtp", "xdbg") {
                Ok::<_, eyre::Report>(dir.data_dir().to_path_buf())
            } else {
                Err(eyre::eyre!("No Home Directory Path could be retrieved"))
            }
        }?;
        Ok(data)
    }

    pub(super) fn db_directory(&self, network: impl Into<u64>) -> Result<PathBuf> {
        let data = if let Some(o) = &self.sqlite_dir {
            o
        } else {
            &self.data_directory()?
        };
        let dir = data.join("sqlite").join(network.into().to_string());
        Ok(dir)
    }

    pub(super) fn redb(&self) -> Result<PathBuf> {
        let data = self.data_directory()?;
        let mut dir = data.join("xdbg");
        dir.set_extension("redb");
        Ok(dir)
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Generate(Generate),
    Modify(Modify),
    Inspect(Inspect),
    Send(Send),
    Query(Query),
    Info(InfoOpts),
    Export(ExportOpts),
    Stream(Stream),
}

/// output stream of messages from a group to stdout
#[derive(Args, Debug)]
pub struct Stream {
    /// inboxId of user to stream with
    #[arg(long, short)]
    pub inbox_id: InboxId,
    /// import identity from a file instead of pulling from a database.
    /// useful if multiple instances of xdbg running.
    #[arg(long)]
    pub import: Option<PathBuf>,
}

/// Send Data on the network
#[derive(Args, Debug)]
pub struct Send {
    pub action: ActionKind,
    pub data: String,
    pub group_id: GroupId,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum ActionKind {
    Message,
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
    #[arg(long)]
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
    /// Inbox Id to export, if any
    #[arg(long, short)]
    pub inbox_id: Option<String>,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum EntityKind {
    Group,
    Message,
    Identity,
    SingleIdentity,
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use EntityKind::*;
        match self {
            Group => write!(f, "group"),
            Message => write!(f, "message"),
            Identity => write!(f, "identity"),
            SingleIdentity => write!(f, "single identity"),
        }
    }
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
    #[arg(
        short,
        long,
        group = "custom-backend",
        conflicts_with = "constant-backend"
    )]
    pub payer_url: Option<url::Url>,
    /// Enable the decentralization backend
    #[arg(short, long)]
    pub d14n: bool,
}

impl BackendOpts {
    pub fn payer_url(&self) -> eyre::Result<url::Url> {
        use BackendKind::*;

        if let Some(p) = &self.payer_url {
            return Ok(p.clone());
        }

        match (self.backend, self.d14n) {
            (Dev, false) => eyre::bail!("No payer for V3"),
            (Production, false) => eyre::bail!("No payer for V3"),
            (Local, false) => eyre::bail!("No payer for V3"),
            (Dev, true) => Ok((*crate::constants::XMTP_DEV_PAYER).clone()),
            (Production, true) => Ok((*crate::constants::XMTP_PRODUCTION_PAYER).clone()),
            (Local, true) => Ok((*crate::constants::XMTP_LOCAL_PAYER).clone()),
        }
    }

    pub fn network_url(&self) -> url::Url {
        use BackendKind::*;

        if let Some(n) = &self.url {
            return n.clone();
        }

        match (self.backend, self.d14n) {
            (Dev, false) => (*crate::constants::XMTP_DEV).clone(),
            (Production, false) => (*crate::constants::XMTP_PRODUCTION).clone(),
            (Local, false) => (*crate::constants::XMTP_LOCAL).clone(),
            (Dev, true) => (*crate::constants::XMTP_DEV_D14N).clone(),
            (Production, true) => (*crate::constants::XMTP_PRODUCTION_D14N).clone(),
            (Local, true) => (*crate::constants::XMTP_LOCAL_D14N).clone(),
        }
    }

    pub async fn connect(&self) -> eyre::Result<crate::DbgClientApi> {
        let network = self.network_url();
        let is_secure = network.scheme() == "https";

        if self.d14n {
            let payer = self.payer_url()?;
            trace!(url = %network, payer = %payer, is_secure, "create grpc");

            Ok(Arc::new(
                xmtp_api_grpc::replication_client::ClientV4::create(
                    network.as_str().to_string(),
                    payer.as_str().to_string(),
                    is_secure,
                )
                .await?,
            ))
        } else {
            trace!(url = %network, is_secure, "create grpc");
            Ok(Arc::new(
                crate::GrpcClient::create(network.as_str().to_string(), is_secure).await?,
            ))
        }
    }
}

impl<'a> From<&'a BackendOpts> for u64 {
    fn from(value: &'a BackendOpts) -> Self {
        use BackendKind::*;

        if let Some(ref url) = value.url {
            xxh3::xxh3_64(url.as_str().as_bytes())
        } else {
            match (value.backend, value.d14n) {
                (Production, false) => 2,
                (Dev, false) => 1,
                (Local, false) => 0,
                (Production, true) => 5,
                (Dev, true) => 4,
                (Local, true) => 3,
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
        let BackendOpts {
            backend, url, d14n, ..
        } = value;
        url.unwrap_or(backend.to_network_url(d14n))
    }
}

#[derive(ValueEnum, Debug, Copy, Clone, Default)]
pub enum BackendKind {
    Dev,
    Production,
    #[default]
    Local,
}

impl BackendKind {
    fn to_network_url(self, d14n: bool) -> url::Url {
        use BackendKind::*;
        match (self, d14n) {
            (Dev, false) => (*crate::constants::XMTP_DEV).clone(),
            (Production, false) => (*crate::constants::XMTP_PRODUCTION).clone(),
            (Local, false) => (*crate::constants::XMTP_LOCAL).clone(),
            (Dev, true) => (*crate::constants::XMTP_DEV_D14N).clone(),
            (Production, true) => (*crate::constants::XMTP_PRODUCTION_D14N).clone(),
            (Local, true) => (*crate::constants::XMTP_LOCAL_D14N).clone(),
        }
    }
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
